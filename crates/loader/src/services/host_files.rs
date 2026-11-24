// SPDX-License-Identifier: GPL-3.0-or-later

//! Guest drive letters backed by explicitly configured host directories.
//!
//! Each mount holds a descriptor for its root. Guest paths are resolved one
//! component at a time with `openat` and `O_NOFOLLOW` from that descriptor,
//! matching names case-insensitively against the real directory listing. A
//! path can therefore never name anything outside the root: `..` and `.` are
//! rejected as components, symbolic links are refused wherever they appear,
//! and absolute host paths cannot be expressed. Mounts are read-only unless
//! configured writable.

use std::ffi::{CStr, CString};
use std::fs::File;
use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::path::Path;
use std::sync::Arc;

pub(super) const ERROR_FILE_NOT_FOUND: u32 = 2;
pub(super) const ERROR_PATH_NOT_FOUND: u32 = 3;
pub(super) const ERROR_ACCESS_DENIED: u32 = 5;
pub(super) const ERROR_FILE_EXISTS: u32 = 80;
const ERROR_GEN_FAILURE: u32 = 31;
const ERROR_DISK_FULL: u32 = 112;
const ERROR_DIR_NOT_EMPTY: u32 = 145;
const ERROR_FILENAME_EXCED_RANGE: u32 = 206;
const ERROR_IO_DEVICE: u32 = 1117;
/// Largest number of entries read from one host directory.
const MAX_DIRECTORY_ENTRIES: usize = 4096;

/// A guest drive letter backed by a host directory.
pub struct HostMount {
    drive: u8,
    root: Arc<OwnedFd>,
    writable: bool,
}

impl HostMount {
    /// Mount `root` as guest drive `drive`. The directory is opened once; later
    /// changes to the host path name do not move the mount.
    pub fn new(drive: char, root: &Path, writable: bool) -> io::Result<Self> {
        let drive = drive.to_ascii_lowercase();
        if !drive.is_ascii_lowercase() || drive == 'c' {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "host mounts use drive letters D: to Z:",
            ));
        }
        let path = CString::new(root.as_os_str().as_encoded_bytes())
            .map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))?;
        // SAFETY: `path` is NUL-terminated; the result is checked.
        let fd = unsafe {
            libc::open(
                path.as_ptr(),
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC,
            )
        };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(Self {
            drive: drive as u8,
            // SAFETY: `fd` is a newly opened descriptor owned by nothing else.
            root: Arc::new(unsafe { OwnedFd::from_raw_fd(fd) }),
            writable,
        })
    }

    pub fn drive(&self) -> char {
        self.drive as char
    }

    pub fn writable(&self) -> bool {
        self.writable
    }
}

/// Win32 error for a host failure. Unknown errors keep a distinct generic code.
pub(super) fn win32_error(error: &io::Error) -> u32 {
    match error.raw_os_error() {
        Some(libc::ENOENT) => ERROR_FILE_NOT_FOUND,
        Some(libc::ENOTDIR) => ERROR_PATH_NOT_FOUND,
        Some(libc::EACCES | libc::EPERM | libc::EROFS | libc::ELOOP | libc::EISDIR) => {
            ERROR_ACCESS_DENIED
        }
        Some(libc::EEXIST) => ERROR_FILE_EXISTS,
        Some(libc::ENOSPC | libc::EDQUOT | libc::EFBIG) => ERROR_DISK_FULL,
        Some(libc::ENOTEMPTY) => ERROR_DIR_NOT_EMPTY,
        Some(libc::ENAMETOOLONG) => ERROR_FILENAME_EXCED_RANGE,
        Some(libc::EIO) => ERROR_IO_DEVICE,
        _ => ERROR_GEN_FAILURE,
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Kind {
    File,
    Directory,
}

pub(super) struct Entry {
    /// The host name, with its original case.
    pub(super) name: String,
    pub(super) kind: Kind,
    pub(super) size: u64,
}

fn c_name(name: &str) -> io::Result<CString> {
    CString::new(name).map_err(|_| io::Error::from_raw_os_error(libc::EINVAL))
}

fn open_at(directory: &OwnedFd, name: &str, flags: i32, mode: u32) -> io::Result<OwnedFd> {
    let name = c_name(name)?;
    // SAFETY: `name` is NUL-terminated and `directory` is an open descriptor.
    let fd = unsafe {
        libc::openat(
            directory.as_raw_fd(),
            name.as_ptr(),
            flags | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            mode,
        )
    };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: `fd` is a newly opened descriptor owned by nothing else.
    Ok(unsafe { OwnedFd::from_raw_fd(fd) })
}

/// Regular files and directories in `directory`. Symbolic links, devices and
/// names that are not UTF-8 are omitted, so the guest never sees them.
pub(super) fn list(directory: &OwnedFd) -> io::Result<Vec<Entry>> {
    // fdopendir takes ownership of its descriptor, so give it a duplicate.
    // SAFETY: `directory` is open; the duplicate is checked.
    let duplicate = unsafe { libc::fcntl(directory.as_raw_fd(), libc::F_DUPFD_CLOEXEC, 0) };
    if duplicate < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: `duplicate` is a directory descriptor owned by nothing else.
    let stream = unsafe { libc::fdopendir(duplicate) };
    if stream.is_null() {
        let error = io::Error::last_os_error();
        // SAFETY: fdopendir failed, so the duplicate is still ours to close.
        unsafe { libc::close(duplicate) };
        return Err(error);
    }
    // SAFETY: fdopendir shares the descriptor's offset with every duplicate.
    unsafe { libc::rewinddir(stream) };
    let mut entries = Vec::new();
    let result = loop {
        // SAFETY: `stream` is a valid directory stream until closedir below.
        let entry = unsafe { libc::readdir(stream) };
        if entry.is_null() {
            break Ok(());
        }
        // SAFETY: readdir returned a valid entry with a NUL-terminated name.
        let raw = unsafe { CStr::from_ptr((*entry).d_name.as_ptr()) };
        let Ok(name) = raw.to_str() else { continue };
        if name == "." || name == ".." {
            continue;
        }
        let mut status = std::mem::MaybeUninit::<libc::stat>::uninit();
        // SAFETY: `raw` is NUL-terminated and `status` has room for a stat.
        let found = unsafe {
            libc::fstatat(
                directory.as_raw_fd(),
                raw.as_ptr(),
                status.as_mut_ptr(),
                libc::AT_SYMLINK_NOFOLLOW,
            )
        };
        if found != 0 {
            continue;
        }
        // SAFETY: fstatat succeeded and initialized `status`.
        let status = unsafe { status.assume_init() };
        let kind = match status.st_mode & libc::S_IFMT {
            libc::S_IFREG => Kind::File,
            libc::S_IFDIR => Kind::Directory,
            _ => continue,
        };
        if entries.len() >= MAX_DIRECTORY_ENTRIES {
            break Err(io::Error::from_raw_os_error(libc::ENOMEM));
        }
        entries.push(Entry {
            name: name.to_owned(),
            kind,
            size: status.st_size as u64,
        });
    };
    // SAFETY: `stream` is valid and closed exactly once.
    unsafe { libc::closedir(stream) };
    result.map(|()| entries)
}

/// The entry in `directory` whose name matches `component` ignoring ASCII
/// case. Two entries that differ only by case are ambiguous and refused.
fn find(directory: &OwnedFd, component: &str) -> io::Result<Option<Entry>> {
    let mut matches = list(directory)?
        .into_iter()
        .filter(|entry| entry.name.eq_ignore_ascii_case(component));
    let first = matches.next();
    if matches.next().is_some() {
        return Err(io::Error::from_raw_os_error(libc::EACCES));
    }
    Ok(first)
}

/// A guest path on a mounted drive: the mount and its validated components.
pub(super) struct Resolved<'a> {
    pub(super) mount: &'a HostMount,
    pub(super) components: Vec<String>,
}

impl Resolved<'_> {
    /// The handle-table key, used for sharing checks.
    pub(super) fn key(&self) -> String {
        let mut key = format!("{}:", self.mount.drive as char);
        for component in &self.components {
            key.push('\\');
            key.push_str(&component.to_ascii_lowercase());
        }
        key
    }

    /// Open the directory holding the final component.
    pub(super) fn parent(&self) -> Result<OwnedFd, u32> {
        let parents = self
            .components
            .split_last()
            .map_or(&[][..], |(_, parents)| parents);
        self.walk(parents)
    }

    /// Open the directory the whole path names.
    pub(super) fn directory(&self) -> Result<OwnedFd, u32> {
        self.walk(&self.components)
    }

    fn walk(&self, components: &[String]) -> Result<OwnedFd, u32> {
        let mut directory = open_at(&self.mount.root, ".", libc::O_RDONLY | libc::O_DIRECTORY, 0)
            .map_err(|error| win32_error(&error))?;
        for component in components {
            match find(&directory, component).map_err(|error| win32_error(&error))? {
                Some(entry) if entry.kind == Kind::Directory => {
                    directory = open_at(
                        &directory,
                        &entry.name,
                        libc::O_RDONLY | libc::O_DIRECTORY,
                        0,
                    )
                    .map_err(|error| win32_error(&error))?;
                }
                _ => return Err(ERROR_PATH_NOT_FOUND),
            }
        }
        Ok(directory)
    }

    /// The final component's directory and existing entry, if any. The root
    /// itself resolves to a directory with no name.
    pub(super) fn lookup(&self) -> Result<(OwnedFd, Option<Entry>), u32> {
        let parent = self.parent()?;
        let Some(name) = self.components.last() else {
            let root = Entry {
                name: String::new(),
                kind: Kind::Directory,
                size: 0,
            };
            return Ok((parent, Some(root)));
        };
        let entry = find(&parent, name).map_err(|error| win32_error(&error))?;
        Ok((parent, entry))
    }
}

/// Resolve `path`, which keeps its case, when it names a mounted drive.
/// `valid` checks each component against Windows naming rules.
pub(super) fn resolve<'a>(
    mounts: &'a [HostMount],
    path: &str,
    valid: impl Fn(&str) -> bool,
) -> Option<Result<Resolved<'a>, u32>> {
    let bytes = path.as_bytes();
    if bytes.len() < 2 || bytes[1] != b':' {
        return None;
    }
    let drive = bytes[0].to_ascii_lowercase();
    let mount = mounts.iter().find(|mount| mount.drive == drive)?;
    let Some(rest) = path[2..].strip_prefix('\\') else {
        // Drive-relative paths such as "D:file" depend on a current directory.
        return Some(Err(ERROR_PATH_NOT_FOUND));
    };
    let components: Vec<String> = rest
        .split('\\')
        .filter(|component| !component.is_empty())
        .map(str::to_owned)
        .collect();
    if !components
        .iter()
        .all(|component| valid(&component.to_ascii_lowercase()))
    {
        return Some(Err(123)); // ERROR_INVALID_NAME
    }
    Some(Ok(Resolved { mount, components }))
}

pub(super) struct OpenRequest {
    pub(super) read: bool,
    pub(super) write: bool,
    pub(super) create: bool,
    pub(super) truncate: bool,
}

/// Open `name` in `directory`, creating it exclusively when it did not exist.
pub(super) fn open_file(
    directory: &OwnedFd,
    name: &str,
    request: &OpenRequest,
) -> io::Result<File> {
    let mut flags = match (request.read, request.write) {
        (_, false) => libc::O_RDONLY,
        (false, true) => libc::O_WRONLY,
        (true, true) => libc::O_RDWR,
    };
    if request.create {
        flags |= libc::O_CREAT | libc::O_EXCL;
    }
    if request.truncate {
        flags |= libc::O_TRUNC;
    }
    let fd = open_at(directory, name, flags, 0o644)?;
    Ok(File::from(fd))
}

pub(super) fn create_directory(directory: &OwnedFd, name: &str) -> io::Result<()> {
    let name = c_name(name)?;
    // SAFETY: `name` is NUL-terminated and `directory` is open.
    if unsafe { libc::mkdirat(directory.as_raw_fd(), name.as_ptr(), 0o755) } != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// Remove a file, or an empty directory when `directory_entry` is set.
pub(super) fn remove(directory: &OwnedFd, name: &str, directory_entry: bool) -> io::Result<()> {
    let name = c_name(name)?;
    let flags = if directory_entry {
        libc::AT_REMOVEDIR
    } else {
        0
    };
    // SAFETY: `name` is NUL-terminated and `directory` is open.
    if unsafe { libc::unlinkat(directory.as_raw_fd(), name.as_ptr(), flags) } != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

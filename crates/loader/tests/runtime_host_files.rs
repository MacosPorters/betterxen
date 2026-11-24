// SPDX-License-Identifier: GPL-3.0-or-later

//! Guest drives backed by temporary host directories, used through the guest
//! import table. Each test builds its own directory tree, including files
//! outside the mount and symbolic links that point to them, and checks that
//! the guest can neither read nor change anything outside the mounted root.
//!
//! Error codes and dispositions follow the public CreateFileW, ReadFile,
//! FindFirstFileW and System Error Codes documentation.

mod support;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use support::*;
use xenora_loader::services::host_files::HostMount;
use xenora_loader::services::{MockClock, ServicesState};

const KERNEL32: &str = "kernel32.dll";
const FILE_API: &str = "api-ms-win-core-file-l1-1-0.dll";
const UCRT: &str = "ucrtbase.dll";

const IMPORTS: &[(&str, &str)] = &[
    (KERNEL32, "GetLastError"),
    (FILE_API, "CreateFileW"),
    (FILE_API, "GetTempFileNameW"),
    (KERNEL32, "ReadFile"),
    (KERNEL32, "WriteFile"),
    (KERNEL32, "CloseHandle"),
    (KERNEL32, "GetFileSizeEx"),
    (KERNEL32, "SetFilePointerEx"),
    (KERNEL32, "SetEndOfFile"),
    (FILE_API, "GetFileAttributesA"),
    (FILE_API, "GetFileAttributesW"),
    (KERNEL32, "FindFirstFileW"),
    (KERNEL32, "FindNextFileW"),
    (KERNEL32, "FindClose"),
    (KERNEL32, "CreateDirectoryW"),
    (KERNEL32, "RemoveDirectoryW"),
    (KERNEL32, "DeleteFileW"),
    (KERNEL32, "MoveFileW"),
    (UCRT, "fopen"),
    (UCRT, "fclose"),
    (UCRT, "fread"),
    (UCRT, "fwrite"),
];

const GENERIC_READ: u64 = 0x8000_0000;
const GENERIC_WRITE: u64 = 0x4000_0000;
const CREATE_NEW: u64 = 1;
const CREATE_ALWAYS: u64 = 2;
const OPEN_EXISTING: u64 = 3;
const OPEN_ALWAYS: u64 = 4;
const INVALID: u64 = u64::MAX;

const PATH: u64 = SCRATCH;
const OTHER_PATH: u64 = SCRATCH + 0x300;
const BUFFER: u64 = SCRATCH + 0x600;
const COUNT: u64 = SCRATCH + 0x700;
const FIND_DATA: u64 = SCRATCH + 0x800;

/// A fresh directory, removed on drop. It lives under `XENORA_TEST_TMP` or
/// `/tmp`, which must support hard links and POSIX permissions.
struct Tree(PathBuf);

impl Tree {
    fn new(name: &str) -> Self {
        let base = std::env::var_os("XENORA_TEST_TMP")
            .map_or_else(|| PathBuf::from("/tmp"), PathBuf::from);
        let path = base.join(format!("xenora-host-files-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    fn path(&self, relative: &str) -> PathBuf {
        self.0.join(relative)
    }
}

impl Drop for Tree {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn guest_with(mounts: Vec<HostMount>) -> Guest {
    let mut state = ServicesState::new(Arc::new(MockClock::new(0)));
    for mount in mounts {
        assert!(state.mount(mount).is_ok());
    }
    Guest::with_state(IMPORTS, &[], state)
}

fn put_path(guest: &mut Guest, address: u64, path: &str) {
    for (index, unit) in path.encode_utf16().chain([0]).enumerate() {
        guest
            .memory
            .write_u16(address + 2 * index as u64, unit)
            .unwrap();
    }
}

fn open(guest: &mut Guest, path: &str, access: u64, share: u64, disposition: u64) -> u64 {
    put_path(guest, PATH, path);
    guest.call(
        "CreateFileW",
        &[PATH, access, share, 0, disposition, 0x80, 0],
    )
}

fn read(guest: &mut Guest, handle: u64, count: u64) -> Option<Vec<u8>> {
    guest.memory.write_u32(COUNT, u32::MAX).unwrap();
    if guest.call("ReadFile", &[handle, BUFFER, count, COUNT, 0]) == 0 {
        return None;
    }
    let read = guest.u32_at(COUNT) as usize;
    let mut bytes = vec![0; read];
    guest.memory.read_bytes(BUFFER, &mut bytes).unwrap();
    Some(bytes)
}

fn write(guest: &mut Guest, handle: u64, bytes: &[u8]) -> bool {
    guest.memory.write_bytes(BUFFER, bytes).unwrap();
    let written = guest.call("WriteFile", &[handle, BUFFER, bytes.len() as u64, COUNT, 0]);
    written == 1 && guest.u32_at(COUNT) as usize == bytes.len()
}

fn attributes(guest: &mut Guest, path: &str) -> u64 {
    put_path(guest, PATH, path);
    guest.call("GetFileAttributesW", &[PATH])
}

fn path_call(guest: &mut Guest, symbol: &str, path: &str) -> u64 {
    put_path(guest, PATH, path);
    match symbol {
        "CreateDirectoryW" => guest.call(symbol, &[PATH, 0]),
        _ => guest.call(symbol, &[PATH]),
    }
}

fn found_name(guest: &Guest) -> String {
    let units: Vec<u16> = (0..260)
        .map(|index| guest.u16_at(FIND_DATA + 44 + 2 * index))
        .take_while(|&unit| unit != 0)
        .collect();
    String::from_utf16(&units).unwrap()
}

fn symlink(target: &Path, link: &Path) {
    std::os::unix::fs::symlink(target, link).unwrap();
}

#[test]
fn temporary_files_use_the_mounted_directory_and_skip_existing_names() {
    let tree = Tree::new("temp-file");
    let root = tree.path("root");
    std::fs::create_dir_all(root.join("Temp")).unwrap();
    std::fs::write(root.join("Temp/PRE0001.tmp"), b"original").unwrap();
    let writable = HostMount::new('D', &root, true).unwrap();
    let readonly = HostMount::new('E', &root, false).unwrap();
    let mut guest = guest_with(vec![writable, readonly]);
    let prefix = PATH + 0x200;
    put_path(&mut guest, prefix, "prefix");
    put_path(&mut guest, PATH, r"D:\temp");
    assert_eq!(guest.call("GetTempFileNameW", &[PATH, prefix, 0, 1]), 0);
    assert_eq!(guest.error(), 998);
    assert_eq!(guest.call("GetTempFileNameW", &[PATH, prefix, 0, BUFFER]), 2);
    let name: Vec<_> = (0..32)
        .map(|i| guest.u16_at(BUFFER + 2 * i))
        .take_while(|unit| *unit != 0)
        .collect();
    assert_eq!(String::from_utf16(&name).unwrap(), r"D:\temp\pre0002.TMP");
    assert_eq!(std::fs::metadata(root.join("Temp/pre0002.TMP")).unwrap().len(), 0);
    assert_eq!(std::fs::read(root.join("Temp/PRE0001.tmp")).unwrap(), b"original");

    put_path(&mut guest, PATH, r"E:\Temp");
    assert_eq!(guest.call("GetTempFileNameW", &[PATH, prefix, 0, BUFFER]), 0);
    assert_eq!(guest.error(), 5);
    assert_eq!(guest.call("GetTempFileNameW", &[PATH, prefix, 3, BUFFER]), 3);
    assert!(!root.join("Temp/pre0003.TMP").exists());
}

#[test]
fn host_mount_non_ascii_spelling_and_native_create_collisions() {
    let tree = Tree::new("non-ascii-case");
    let root = tree.path("root");
    std::fs::create_dir_all(root.join("Över")).unwrap();
    std::fs::write(root.join("État.txt"), b"original").unwrap();
    let file_collision = root.join("état.txt").exists();
    let directory_collision = root.join("över").exists();
    let mount = HostMount::new('D', &root, true).unwrap();
    let mut guest = guest_with(vec![mount]);

    let file = open(&mut guest, r"D:\ÉTAT.TXT", GENERIC_READ, 0, OPEN_EXISTING);
    assert_ne!(file, INVALID);
    assert_eq!(read(&mut guest, file, 8).unwrap(), b"original");
    assert_eq!(guest.call("CloseHandle", &[file]), 1);
    assert_eq!(
        open(&mut guest, r"D:\état.txt", GENERIC_READ, 0, OPEN_EXISTING),
        INVALID
    );
    assert_eq!(guest.error(), 2);

    let file = open(&mut guest, r"D:\état.txt", GENERIC_WRITE, 0, CREATE_NEW);
    if file_collision {
        assert_eq!(file, INVALID);
        assert_eq!(guest.error(), 80);
    } else {
        assert_ne!(file, INVALID);
        assert_eq!(guest.call("CloseHandle", &[file]), 1);
    }
    assert_eq!(std::fs::read(root.join("État.txt")).unwrap(), b"original");

    let created = path_call(&mut guest, "CreateDirectoryW", r"D:\över");
    if directory_collision {
        assert_eq!(created, 0);
        assert_eq!(guest.error(), 183);
    } else {
        assert_eq!(created, 1);
        assert!(root.join("över").is_dir());
    }
    assert!(root.join("Över").is_dir());
}

#[test]
fn read_only_mounts_serve_files_without_escaping_the_root() {
    let tree = Tree::new("read-only");
    std::fs::create_dir_all(tree.path("root/Data")).unwrap();
    std::fs::write(tree.path("root/Data/Level1.bin"), b"XENORA-LEVEL").unwrap();
    std::fs::write(tree.path("outside.txt"), b"secret").unwrap();
    symlink(&tree.path("outside.txt"), &tree.path("root/Data/escape"));
    symlink(&tree.0, &tree.path("root/link"));
    let mount = HostMount::new('D', &tree.path("root"), false).unwrap();
    let mut guest = guest_with(vec![mount]);

    guest.memory.write_bytes(PATH, b"D:\\data\\LEVEL1.BIN\0").unwrap();
    assert_eq!(guest.call("GetFileAttributesA", &[PATH]), 1);
    guest.memory.write_bytes(PATH, b"D:\\data\\missing.bin\0").unwrap();
    assert_eq!(guest.call("GetFileAttributesA", &[PATH]), u64::from(u32::MAX));
    assert_eq!(guest.error(), 2);
    guest.memory.write_bytes(PATH, b"D:\\data\\\xff\0").unwrap();
    assert_eq!(guest.call("GetFileAttributesA", &[PATH]), u64::from(u32::MAX));
    assert_eq!(guest.error(), 123);

    // Names match case-insensitively against the host listing.
    let file = open(
        &mut guest,
        "D:\\data\\LEVEL1.BIN",
        GENERIC_READ,
        1,
        OPEN_EXISTING,
    );
    assert_ne!(file, INVALID);
    assert_eq!(read(&mut guest, file, 6).unwrap(), b"XENORA");
    guest.memory.write_u64(COUNT, 0).unwrap();
    assert_eq!(guest.call("GetFileSizeEx", &[file, COUNT]), 1);
    assert_eq!(guest.u64_at(COUNT), 12);
    assert_eq!(
        guest.call("SetFilePointerEx", &[file, (-2i64) as u64, 0, 2]),
        1
    );
    assert_eq!(read(&mut guest, file, 16).unwrap(), b"EL");
    // Reading at the end of the file succeeds with zero bytes.
    assert_eq!(read(&mut guest, file, 16).unwrap(), b"");
    assert!(!write(&mut guest, file, b"x"));
    assert_eq!(guest.error(), 5);

    // Sharing modes apply to host files: the open handle allows only readers.
    let reader = open(
        &mut guest,
        "d:/Data/level1.bin",
        GENERIC_READ,
        1,
        OPEN_EXISTING,
    );
    assert_ne!(reader, INVALID);
    assert_eq!(guest.call("CloseHandle", &[reader]), 1);
    let exclusive = open(
        &mut guest,
        "D:\\Data\\Level1.bin",
        GENERIC_READ,
        0,
        OPEN_EXISTING,
    );
    assert_eq!(exclusive, INVALID);
    assert_eq!(guest.error(), 32); // ERROR_SHARING_VIOLATION
    assert_eq!(guest.call("CloseHandle", &[file]), 1);

    // The mount refuses changes.
    assert_eq!(
        open(
            &mut guest,
            "D:\\Data\\Level1.bin",
            GENERIC_WRITE,
            0,
            OPEN_EXISTING
        ),
        INVALID
    );
    assert_eq!(guest.error(), 5);
    assert_eq!(
        open(&mut guest, "D:\\new.txt", GENERIC_READ, 0, OPEN_ALWAYS),
        INVALID
    );
    assert_eq!(guest.error(), 5);
    assert_eq!(path_call(&mut guest, "CreateDirectoryW", "D:\\Saves"), 0);
    assert_eq!(guest.error(), 5);

    // Nothing outside the root is reachable: traversal is an invalid name,
    // symbolic links are invisible, and drive-relative paths are refused.
    for (path, error) in [
        ("D:\\..\\outside.txt", 123),
        ("D:\\Data\\..\\..\\outside.txt", 123),
        ("D:\\Data\\escape", 2),
        ("D:\\link\\outside.txt", 3),
        ("D:outside.txt", 3),
    ] {
        assert_eq!(
            open(&mut guest, path, GENERIC_READ, 1, OPEN_EXISTING),
            INVALID,
            "{path}"
        );
        assert_eq!(guest.error(), error, "{path}");
    }

    assert_eq!(attributes(&mut guest, "D:\\"), 0x10);
    assert_eq!(attributes(&mut guest, "D:\\DATA"), 0x10);
    assert_eq!(attributes(&mut guest, "D:\\Data\\Level1.bin"), 0x01); // read-only
    assert_eq!(
        attributes(&mut guest, "D:\\Data\\missing"),
        u64::from(u32::MAX)
    );
    assert_eq!(guest.error(), 2);

    // Enumeration reports the host names and hides the symbolic link.
    put_path(&mut guest, PATH, "D:\\data\\*");
    let search = guest.call("FindFirstFileW", &[PATH, FIND_DATA]);
    assert_ne!(search, INVALID);
    assert_eq!(found_name(&guest), "Level1.bin");
    assert_eq!(guest.u32_at(FIND_DATA), 0x01);
    assert_eq!(guest.u32_at(FIND_DATA + 32), 12); // nFileSizeLow
    assert_eq!(guest.call("FindNextFileW", &[search, FIND_DATA]), 0);
    assert_eq!(guest.error(), 18); // ERROR_NO_MORE_FILES
    assert_eq!(guest.call("FindClose", &[search]), 1);
    put_path(&mut guest, PATH, "D:\\*.*");
    let search = guest.call("FindFirstFileW", &[PATH, FIND_DATA]);
    assert_eq!(found_name(&guest), "Data");
    assert_eq!(guest.call("FindNextFileW", &[search, FIND_DATA]), 0);

    assert_eq!(std::fs::read(tree.path("outside.txt")).unwrap(), b"secret");
}

#[test]
fn writable_mounts_create_change_and_remove_only_inside_the_root() {
    let tree = Tree::new("writable");
    std::fs::create_dir_all(tree.path("root")).unwrap();
    std::fs::write(tree.path("outside.txt"), b"secret").unwrap();
    symlink(&tree.path("outside.txt"), &tree.path("root/escape"));
    let mount = HostMount::new('f', &tree.path("root"), true).unwrap();
    let mut guest = guest_with(vec![mount]);
    let saves = tree.path("root/Saves");

    assert_eq!(
        open(
            &mut guest,
            "F:\\Saves\\Slot.dat",
            GENERIC_READ | GENERIC_WRITE,
            0,
            CREATE_NEW
        ),
        INVALID
    );
    assert_eq!(guest.error(), 3);
    assert_eq!(path_call(&mut guest, "CreateDirectoryW", "F:\\Saves"), 1);
    assert_eq!(path_call(&mut guest, "CreateDirectoryW", "F:\\saves"), 0);
    assert_eq!(guest.error(), 183);
    assert!(saves.is_dir());

    let file = open(
        &mut guest,
        "F:\\saves\\Slot.dat",
        GENERIC_READ | GENERIC_WRITE,
        0,
        CREATE_NEW,
    );
    assert_ne!(file, INVALID);
    assert!(write(&mut guest, file, b"hello"));
    // The guest's spelling of a new name is kept on the host.
    assert_eq!(std::fs::read(saves.join("Slot.dat")).unwrap(), b"hello");
    assert_eq!(
        path_call(&mut guest, "DeleteFileW", "F:\\Saves\\slot.dat"),
        0
    );
    assert_eq!(guest.error(), 32);
    assert_eq!(guest.call("SetFilePointerEx", &[file, 2, 0, 0]), 1);
    assert_eq!(guest.call("SetEndOfFile", &[file]), 1);
    assert_eq!(std::fs::read(saves.join("Slot.dat")).unwrap(), b"he");
    assert_eq!(guest.call("CloseHandle", &[file]), 1);

    assert_eq!(
        open(
            &mut guest,
            "F:\\Saves\\slot.dat",
            GENERIC_READ,
            0,
            CREATE_NEW
        ),
        INVALID
    );
    assert_eq!(guest.error(), 80); // ERROR_FILE_EXISTS
    let reopened = open(
        &mut guest,
        "F:\\Saves\\SLOT.DAT",
        GENERIC_READ,
        0,
        OPEN_ALWAYS,
    );
    assert_ne!(reopened, INVALID);
    assert_eq!(guest.error(), 183); // existing file opened
    assert_eq!(read(&mut guest, reopened, 8).unwrap(), b"he");
    assert_eq!(guest.call("CloseHandle", &[reopened]), 1);

    // CREATE_ALWAYS truncates an existing host file and reports ALREADY_EXISTS.
    let truncated = open(
        &mut guest,
        "F:\\Saves\\SLOT.DAT",
        GENERIC_READ | GENERIC_WRITE,
        0,
        CREATE_ALWAYS,
    );
    assert_ne!(truncated, INVALID);
    assert_eq!(guest.error(), 183); // ERROR_ALREADY_EXISTS
    assert_eq!(std::fs::metadata(saves.join("Slot.dat")).unwrap().len(), 0);
    assert!(write(&mut guest, truncated, b"new"));
    assert_eq!(std::fs::read(saves.join("Slot.dat")).unwrap(), b"new");
    assert_eq!(guest.call("CloseHandle", &[truncated]), 1);

    assert_eq!(path_call(&mut guest, "RemoveDirectoryW", "F:\\Saves"), 0);
    assert_eq!(guest.error(), 145); // ERROR_DIR_NOT_EMPTY
    assert_eq!(
        path_call(&mut guest, "DeleteFileW", "F:\\Saves\\slot.dat"),
        1
    );
    assert_eq!(path_call(&mut guest, "RemoveDirectoryW", "F:\\Saves"), 1);
    assert!(!saves.exists());

    // A symbolic link cannot be written through or replaced, and moves on
    // host mounts are not supported yet.
    assert_eq!(
        open(&mut guest, "F:\\escape", GENERIC_WRITE, 0, OPEN_ALWAYS),
        INVALID
    );
    assert_eq!(guest.error(), 80);
    assert_eq!(path_call(&mut guest, "DeleteFileW", "F:\\escape"), 0);
    assert_eq!(guest.error(), 2);
    put_path(&mut guest, PATH, "F:\\a.txt");
    put_path(&mut guest, OTHER_PATH, "F:\\b.txt");
    assert_eq!(guest.call("MoveFileW", &[PATH, OTHER_PATH]), 0);
    assert_eq!(guest.error(), 50);

    assert_eq!(std::fs::read(tree.path("outside.txt")).unwrap(), b"secret");
    assert!(tree.path("root/escape").is_symlink());
}

#[test]
fn mounts_are_explicit_and_unique_per_drive() {
    let tree = Tree::new("mounts");
    for drive in ['A', 'b', 'C', 'c', '1', ':', 'é'] {
        assert!(HostMount::new(drive, &tree.0, false).is_err(), "{drive}");
    }
    for drive in ['D', 'd', 'Z', 'z'] {
        assert!(HostMount::new(drive, &tree.0, false).is_ok(), "{drive}");
    }
    assert!(HostMount::new('D', &tree.path("missing"), false).is_err());
    let mut state = ServicesState::new(Arc::new(MockClock::new(0)));
    assert!(
        state
            .mount(HostMount::new('D', &tree.0, false).unwrap())
            .is_ok()
    );
    assert!(
        state
            .mount(HostMount::new('d', &tree.0, true).unwrap())
            .is_err()
    );

    // Without a mount, other drive letters stay unreachable.
    let mut guest = guest_with(Vec::new());
    assert_eq!(
        open(&mut guest, "D:\\anything", GENERIC_READ, 1, OPEN_EXISTING),
        INVALID
    );
    assert_ne!(guest.error(), 0);
}

#[test]
fn hard_links_are_hidden_so_they_cannot_alias_outside_files() {
    let tree = Tree::new("hard-links");
    std::fs::create_dir_all(tree.path("root")).unwrap();
    std::fs::write(tree.path("outside.txt"), b"secret").unwrap();
    std::fs::hard_link(tree.path("outside.txt"), tree.path("root/alias.txt")).unwrap();
    std::fs::write(tree.path("root/plain.txt"), b"plain").unwrap();
    let mount = HostMount::new('D', &tree.path("root"), true).unwrap();
    let mut guest = guest_with(vec![mount]);

    assert_eq!(
        open(&mut guest, r"D:\alias.txt", GENERIC_READ, 1, OPEN_EXISTING),
        INVALID
    );
    assert_eq!(guest.error(), 2);
    // A writable mount cannot replace or delete the alias either.
    assert_eq!(
        open(&mut guest, r"D:\alias.txt", GENERIC_WRITE, 0, OPEN_ALWAYS),
        INVALID
    );
    assert_eq!(guest.error(), 80);
    assert_eq!(path_call(&mut guest, "DeleteFileW", r"D:\alias.txt"), 0);
    assert_eq!(guest.error(), 2);
    put_path(&mut guest, PATH, r"D:\*");
    let search = guest.call("FindFirstFileW", &[PATH, FIND_DATA]);
    assert_eq!(found_name(&guest), "plain.txt");
    assert_eq!(guest.call("FindNextFileW", &[search, FIND_DATA]), 0);
    assert_eq!(std::fs::read(tree.path("outside.txt")).unwrap(), b"secret");
}

#[test]
fn sharing_follows_the_host_file_rather_than_its_name() {
    let tree = Tree::new("identity");
    std::fs::create_dir_all(tree.path("root")).unwrap();
    std::fs::write(tree.path("root/a.txt"), b"first").unwrap();
    let mount = HostMount::new('F', &tree.path("root"), true).unwrap();
    let mut guest = guest_with(vec![mount]);

    let file = open(
        &mut guest,
        r"F:\a.txt",
        GENERIC_READ | GENERIC_WRITE,
        0,
        OPEN_EXISTING,
    );
    assert_ne!(file, INVALID);
    // The host renames the file while the guest holds it exclusively.
    std::fs::rename(tree.path("root/a.txt"), tree.path("root/b.txt")).unwrap();
    assert_eq!(
        open(&mut guest, r"F:\b.txt", GENERIC_READ, 3, OPEN_EXISTING),
        INVALID
    );
    assert_eq!(guest.error(), 32);
    assert_eq!(path_call(&mut guest, "DeleteFileW", r"F:\b.txt"), 0);
    assert_eq!(guest.error(), 32);
    // The handle still refers to the same file under its new name.
    assert!(write(&mut guest, file, b"SECOND"));
    assert_eq!(std::fs::read(tree.path("root/b.txt")).unwrap(), b"SECOND");
    // A different file now using the old name is unrelated to the handle.
    std::fs::write(tree.path("root/a.txt"), b"other").unwrap();
    let other = open(&mut guest, r"F:\a.txt", GENERIC_READ, 0, OPEN_EXISTING);
    assert_ne!(other, INVALID);
    assert_eq!(guest.call("CloseHandle", &[other]), 1);
    assert_eq!(guest.call("CloseHandle", &[file]), 1);
    assert_eq!(path_call(&mut guest, "DeleteFileW", r"F:\b.txt"), 1);
}

#[test]
fn host_changes_between_guest_calls_cannot_redirect_paths() {
    let tree = Tree::new("concurrent");
    std::fs::create_dir_all(tree.path("root/Data")).unwrap();
    std::fs::create_dir_all(tree.path("elsewhere")).unwrap();
    std::fs::write(tree.path("elsewhere/level.bin"), b"secret").unwrap();
    std::fs::write(tree.path("root/Data/level.bin"), b"data").unwrap();
    let mount = HostMount::new('D', &tree.path("root"), false).unwrap();
    let mut guest = guest_with(vec![mount]);
    let file = open(
        &mut guest,
        r"D:\Data\level.bin",
        GENERIC_READ,
        1,
        OPEN_EXISTING,
    );
    assert_ne!(file, INVALID);

    // The directory is replaced by a link to a directory outside the root.
    std::fs::rename(tree.path("root/Data"), tree.path("root/Old")).unwrap();
    symlink(&tree.path("elsewhere"), &tree.path("root/Data"));
    assert_eq!(
        open(
            &mut guest,
            r"D:\Data\level.bin",
            GENERIC_READ,
            1,
            OPEN_EXISTING
        ),
        INVALID
    );
    assert_eq!(guest.error(), 3);
    put_path(&mut guest, PATH, r"D:\Data\*");
    assert_eq!(guest.call("FindFirstFileW", &[PATH, FIND_DATA]), INVALID);
    assert_eq!(guest.error(), 3);
    // The open handle keeps reading the file it opened.
    assert_eq!(read(&mut guest, file, 8).unwrap(), b"data");

    // A file replaced by a directory is refused as a file.
    std::fs::create_dir_all(tree.path("root/Old/level2.bin")).unwrap();
    assert_eq!(
        open(
            &mut guest,
            r"D:\Old\level2.bin",
            GENERIC_READ,
            1,
            OPEN_EXISTING
        ),
        INVALID
    );
    assert_eq!(guest.error(), 5);
    assert_eq!(
        std::fs::read(tree.path("elsewhere/level.bin")).unwrap(),
        b"secret"
    );
}

#[test]
fn read_only_mounts_refuse_every_change() {
    let tree = Tree::new("refuse");
    std::fs::create_dir_all(tree.path("root/Dir")).unwrap();
    std::fs::write(tree.path("root/file.bin"), b"keep").unwrap();
    let mount = HostMount::new('D', &tree.path("root"), false).unwrap();
    let mut guest = guest_with(vec![mount]);
    let reader = open(&mut guest, r"D:\file.bin", GENERIC_READ, 3, OPEN_EXISTING);
    assert_ne!(reader, INVALID);
    assert_eq!(guest.call("SetEndOfFile", &[reader]), 0);
    assert_eq!(guest.error(), 5);
    assert!(!write(&mut guest, reader, b"x"));
    for disposition in [1, 2, 4, 5] {
        assert_eq!(
            open(
                &mut guest,
                r"D:\file.bin",
                GENERIC_READ | GENERIC_WRITE,
                3,
                disposition
            ),
            INVALID
        );
        assert_eq!(
            guest.error(),
            if disposition == 1 { 80 } else { 5 },
            "{disposition}"
        );
    }
    for (symbol, path) in [
        ("DeleteFileW", r"D:\file.bin"),
        ("RemoveDirectoryW", r"D:\Dir"),
        ("CreateDirectoryW", r"D:\New"),
    ] {
        assert_eq!(path_call(&mut guest, symbol, path), 0, "{symbol}");
        assert_eq!(guest.error(), 5, "{symbol}");
    }
    assert_eq!(std::fs::read(tree.path("root/file.bin")).unwrap(), b"keep");
    assert!(tree.path("root/Dir").is_dir());
    assert!(!tree.path("root/New").exists());
}

#[test]
fn enumeration_errors_and_limits_are_reported() {
    use std::os::unix::fs::PermissionsExt;
    let tree = Tree::new("limits");
    std::fs::create_dir_all(tree.path("root/Big")).unwrap();
    std::fs::create_dir_all(tree.path("root/Locked")).unwrap();
    for index in 0..=4096 {
        std::fs::write(tree.path(&format!("root/Big/{index}.bin")), b"").unwrap();
    }
    std::fs::set_permissions(
        tree.path("root/Locked"),
        std::fs::Permissions::from_mode(0o000),
    )
    .unwrap();
    let mount = HostMount::new('D', &tree.path("root"), false).unwrap();
    let mut guest = guest_with(vec![mount]);

    // 4,097 entries exceed the 4,096-entry listing bound.
    put_path(&mut guest, PATH, r"D:\Big\*");
    assert_eq!(guest.call("FindFirstFileW", &[PATH, FIND_DATA]), INVALID);
    assert_eq!(guest.error(), 8); // ERROR_NOT_ENOUGH_MEMORY
    assert_eq!(
        open(&mut guest, r"D:\Big\1.bin", GENERIC_READ, 1, OPEN_EXISTING),
        INVALID
    );
    assert_eq!(guest.error(), 8);
    // A directory the host cannot read reports access denied, not emptiness.
    put_path(&mut guest, PATH, r"D:\Locked\*");
    assert_eq!(guest.call("FindFirstFileW", &[PATH, FIND_DATA]), INVALID);
    assert_eq!(guest.error(), 5);
    std::fs::set_permissions(
        tree.path("root/Locked"),
        std::fs::Permissions::from_mode(0o755),
    )
    .unwrap();
}

#[test]
fn writable_mounts_bound_how_large_guest_writes_make_files() {
    let tree = Tree::new("quota");
    std::fs::create_dir_all(tree.path("root")).unwrap();
    let mount = HostMount::new('F', &tree.path("root"), true)
        .unwrap()
        .with_max_file_size(8);
    let mut guest = guest_with(vec![mount]);
    let file = open(
        &mut guest,
        r"F:\save.bin",
        GENERIC_READ | GENERIC_WRITE,
        0,
        CREATE_NEW,
    );
    assert_ne!(file, INVALID);
    assert!(write(&mut guest, file, b"12345"));
    assert!(!write(&mut guest, file, b"6789"));
    assert_eq!(guest.error(), 112); // ERROR_DISK_FULL
    assert_eq!(std::fs::read(tree.path("root/save.bin")).unwrap(), b"12345");
    // Seeking far ahead and extending is bounded the same way.
    assert_eq!(guest.call("SetFilePointerEx", &[file, 1 << 40, 0, 0]), 1);
    assert_eq!(guest.call("SetEndOfFile", &[file]), 0);
    assert_eq!(guest.error(), 112);
    assert_eq!(guest.call("SetFilePointerEx", &[file, 8, 0, 0]), 1);
    assert_eq!(guest.call("SetEndOfFile", &[file]), 1);
    assert_eq!(
        std::fs::metadata(tree.path("root/save.bin")).unwrap().len(),
        8
    );
    assert_eq!(guest.call("CloseHandle", &[file]), 1);
}

#[test]
fn crt_fopen_writes_through_a_writable_host_mount() {
    let tree = Tree::new("crt-fopen");
    std::fs::create_dir_all(tree.path("root")).unwrap();
    let mount = HostMount::new('D', &tree.path("root"), true).unwrap();
    let mut guest = guest_with(vec![mount]);
    let path = BUFFER;
    let mode_w = BUFFER + 0x40;
    let mode_r = BUFFER + 0x50;
    let payload = BUFFER + 0x60;
    let out = BUFFER + 0x80;
    guest.memory.write_bytes(path, b"D:\\crt.bin\0").unwrap();
    guest.memory.write_bytes(mode_w, b"wb\0").unwrap();
    guest.memory.write_bytes(mode_r, b"rb\0").unwrap();
    guest.memory.write_bytes(payload, b"stdio\0").unwrap();
    let file = guest.call("fopen", &[path, mode_w]);
    assert_ne!(file, 0);
    assert_eq!(guest.call("fwrite", &[payload, 1, 5, file]), 5);
    assert_eq!(guest.call("fclose", &[file]), 0);
    assert_eq!(std::fs::read(tree.path("root/crt.bin")).unwrap(), b"stdio");
    let reader = guest.call("fopen", &[path, mode_r]);
    assert_ne!(reader, 0);
    assert_eq!(guest.call("fread", &[out, 1, 5, reader]), 5);
    let mut got = [0u8; 5];
    guest.memory.read_bytes(out, &mut got).unwrap();
    assert_eq!(&got, b"stdio");
    assert_eq!(guest.call("fclose", &[reader]), 0);
}

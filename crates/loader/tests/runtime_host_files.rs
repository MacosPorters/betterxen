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

const IMPORTS: &[(&str, &str)] = &[
    (KERNEL32, "GetLastError"),
    (FILE_API, "CreateFileW"),
    (KERNEL32, "ReadFile"),
    (KERNEL32, "WriteFile"),
    (KERNEL32, "CloseHandle"),
    (KERNEL32, "GetFileSizeEx"),
    (KERNEL32, "SetFilePointerEx"),
    (KERNEL32, "SetEndOfFile"),
    (FILE_API, "GetFileAttributesW"),
    (KERNEL32, "FindFirstFileW"),
    (KERNEL32, "FindNextFileW"),
    (KERNEL32, "FindClose"),
    (KERNEL32, "CreateDirectoryW"),
    (KERNEL32, "RemoveDirectoryW"),
    (KERNEL32, "DeleteFileW"),
    (KERNEL32, "MoveFileW"),
];

const GENERIC_READ: u64 = 0x8000_0000;
const GENERIC_WRITE: u64 = 0x4000_0000;
const CREATE_NEW: u64 = 1;
const OPEN_EXISTING: u64 = 3;
const OPEN_ALWAYS: u64 = 4;
const INVALID: u64 = u64::MAX;

const PATH: u64 = SCRATCH;
const OTHER_PATH: u64 = SCRATCH + 0x300;
const BUFFER: u64 = SCRATCH + 0x600;
const COUNT: u64 = SCRATCH + 0x700;
const FIND_DATA: u64 = SCRATCH + 0x800;

/// A fresh directory under the system temporary directory, removed on drop.
struct Tree(PathBuf);

impl Tree {
    fn new(name: &str) -> Self {
        let path =
            std::env::temp_dir().join(format!("xenora-host-files-{name}-{}", std::process::id()));
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
fn read_only_mounts_serve_files_without_escaping_the_root() {
    let tree = Tree::new("read-only");
    std::fs::create_dir_all(tree.path("root/Data")).unwrap();
    std::fs::write(tree.path("root/Data/Level1.bin"), b"XENORA-LEVEL").unwrap();
    std::fs::write(tree.path("outside.txt"), b"secret").unwrap();
    symlink(&tree.path("outside.txt"), &tree.path("root/Data/escape"));
    symlink(&tree.0, &tree.path("root/link"));
    let mount = HostMount::new('D', &tree.path("root"), false).unwrap();
    let mut guest = guest_with(vec![mount]);

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
    assert!(HostMount::new('C', &tree.0, false).is_err());
    assert!(HostMount::new('1', &tree.0, false).is_err());
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

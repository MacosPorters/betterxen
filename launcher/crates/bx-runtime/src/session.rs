//! Coordinate BetterXen runners for one bottle. This does not lock out CrossOver's UI or
//! other launchers: maintainers must dedicate the bottle to BetterXen tests.

use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::Path;

pub struct BottleSession {
    file: File,
}

impl BottleSession {
    pub fn acquire(prefix: &Path, recovery: bool) -> Result<Self, String> {
        let path = prefix.join(".betterxen-session");
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(&path)
            .map_err(|e| format!("cannot open {}: {e}", path.display()))?;
        let metadata = file.metadata().map_err(|e| e.to_string())?;
        // SAFETY: geteuid takes no pointers and has no memory preconditions.
        let uid = unsafe { libc::geteuid() };
        if !metadata.is_file() || metadata.nlink() != 1 || metadata.uid() != uid {
            return Err(format!(
                "{} must be a regular, singly linked file owned by the current user",
                path.display()
            ));
        }
        // SAFETY: the descriptor remains owned by `file`; flock changes no Rust memory.
        if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
            return Err(
                "bottle has an active BetterXen runner; stop that runner with SIGTERM first".into(),
            );
        }
        let mut previous = String::new();
        Read::by_ref(&mut file)
            .take(4096)
            .read_to_string(&mut previous)
            .map_err(|e| e.to_string())?;
        if !recovery && !previous.is_empty() {
            return Err(format!(
                "bottle has an unfinished session ({previous:?}); run `bx-cli stop --bottle B` (or `--prefix P`) before another run"
            ));
        }
        file.set_len(0).map_err(|e| e.to_string())?;
        file.seek(SeekFrom::Start(0)).map_err(|e| e.to_string())?;
        writeln!(file, "pid={} recovery={recovery}", std::process::id())
            .map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        Ok(Self { file })
    }

    /// Clear only after supervision or explicit bottle recovery completed successfully.
    pub fn complete(&mut self) -> Result<(), String> {
        self.file
            .set_len(0)
            .and_then(|()| self.file.sync_all())
            .map_err(|e| e.to_string())?;
        Ok(())
    }
}

// The kernel releases the lock on close, including SIGKILL. Without `complete`, the
// nonempty marker survives so a new runner cannot hide a surviving guest.
impl Drop for BottleSession {
    fn drop(&mut self) {
        // A concurrent fork briefly inherits the open file description until exec closes
        // its CLOEXEC descriptor. Explicit unlock prevents that transient child reference
        // from keeping a completed parent session locked after this object is dropped.
        // SAFETY: the descriptor is still owned and open; flock changes no Rust memory.
        unsafe { libc::flock(self.file.as_raw_fd(), libc::LOCK_UN) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn active_and_abandoned_sessions_require_distinct_recovery() {
        let dir = std::env::temp_dir().join(format!("bx-session-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let first = BottleSession::acquire(&dir, false).unwrap();
        assert!(BottleSession::acquire(&dir, false).is_err());
        assert!(BottleSession::acquire(&dir, true).is_err());
        drop(first); // models a supervisor disappearing without completing cleanup
        assert!(BottleSession::acquire(&dir, false).is_err());
        BottleSession::acquire(&dir, true)
            .unwrap()
            .complete()
            .unwrap();
        BottleSession::acquire(&dir, false)
            .unwrap()
            .complete()
            .unwrap();
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_marker_symlink_does_not_truncate_its_target() {
        let dir = std::env::temp_dir().join(format!("bx-session-link-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let target = dir.join("valuable");
        std::fs::write(&target, "preserve me").unwrap();
        std::os::unix::fs::symlink(&target, dir.join(".betterxen-session")).unwrap();
        assert!(BottleSession::acquire(&dir, true).is_err());
        assert_eq!(std::fs::read_to_string(target).unwrap(), "preserve me");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_hardlinked_marker_is_never_truncated_during_recovery() {
        let dir = std::env::temp_dir().join(format!("bx-session-hardlink-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let target = dir.join("valuable");
        std::fs::write(&target, "preserve me").unwrap();
        std::fs::hard_link(&target, dir.join(".betterxen-session")).unwrap();
        assert!(BottleSession::acquire(&dir, true).is_err());
        assert_eq!(std::fs::read_to_string(target).unwrap(), "preserve me");
        std::fs::remove_dir_all(dir).unwrap();
    }
}

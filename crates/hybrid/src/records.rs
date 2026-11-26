//! Exclusive report creation: previous evidence is never replaced.

use crate::json::Json;
use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// Save a run or benchmark report under a unique name, reporting all I/O errors.
pub fn save(dir: &Path, kind: &str, report: &Json) -> io::Result<PathBuf> {
    if !matches!(kind, "run" | "bench") {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "unknown report kind",
        ));
    }
    std::fs::create_dir_all(dir)?;
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(io::Error::other)?
        .as_nanos();
    let bytes = report.render();
    loop {
        let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let path = dir.join(format!(
            "{kind}-{nanos}-{}-{sequence}.json",
            std::process::id()
        ));
        match write_new(&path, bytes.as_bytes(), |file, bytes| file.write_all(bytes)) {
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
            Ok(()) => return Ok(path),
        }
    }
}

fn write_new(
    path: &Path,
    bytes: &[u8],
    write: impl FnOnce(&mut File, &[u8]) -> io::Result<()>,
) -> io::Result<()> {
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    if let Err(error) = write(&mut file, bytes).and_then(|()| file.sync_all()) {
        drop(file);
        // Remove only the incomplete file this call exclusively created.
        return match std::fs::remove_file(path) {
            Ok(()) => Err(error),
            Err(cleanup) => Err(io::Error::new(
                error.kind(),
                format!(
                    "{error}; cannot remove incomplete report {}: {cleanup}",
                    path.display()
                ),
            )),
        };
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn directory() -> PathBuf {
        let id = SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("bx-records-{}-{id}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn repeated_run_and_bench_reports_preserve_prior_evidence() {
        let dir = directory();
        std::fs::write(dir.join("bench.json"), "previous evidence").unwrap();
        let first = save(&dir, "run", &Json::str("first")).unwrap();
        let second = save(&dir, "run", &Json::str("second")).unwrap();
        let bench1 = save(&dir, "bench", &Json::str("bench one")).unwrap();
        let bench2 = save(&dir, "bench", &Json::str("bench two")).unwrap();
        assert_ne!(first, second);
        assert_ne!(bench1, bench2);
        assert_eq!(
            std::fs::read_to_string(first).unwrap(),
            Json::str("first").render()
        );
        assert_eq!(
            std::fs::read_to_string(second).unwrap(),
            Json::str("second").render()
        );
        assert_eq!(
            std::fs::read_to_string(bench1).unwrap(),
            Json::str("bench one").render()
        );
        assert_eq!(
            std::fs::read_to_string(bench2).unwrap(),
            Json::str("bench two").render()
        );
        assert_eq!(
            std::fs::read_to_string(dir.join("bench.json")).unwrap(),
            "previous evidence"
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn existing_destination_is_never_truncated() {
        let dir = directory();
        let path = dir.join("record.json");
        std::fs::write(&path, "original").unwrap();
        let error =
            write_new(&path, b"replacement", |file, bytes| file.write_all(bytes)).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "original");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn invalid_directory_is_an_error() {
        let dir = directory();
        let path = dir.join("not-a-directory");
        std::fs::write(&path, "original").unwrap();
        assert!(save(&path, "run", &Json::Null).is_err());
        assert_eq!(std::fs::read_to_string(path).unwrap(), "original");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn partial_write_failure_is_reported_and_incomplete_file_removed() {
        let dir = directory();
        let path = dir.join("record.json");
        let error = write_new(&path, b"a complete report", |file, _| {
            file.write_all(b"partial")?;
            Err(io::Error::other("injected write failure"))
        })
        .unwrap_err();
        assert!(error.to_string().contains("injected write failure"));
        assert!(!path.exists());
        std::fs::remove_dir_all(dir).unwrap();
    }
}

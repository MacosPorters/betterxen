// SPDX-License-Identifier: GPL-3.0-or-later

use std::fmt::Write as _;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::unix::fs::DirBuilderExt;
use std::path::{Path, PathBuf};
use std::process::ExitStatus;
use std::sync::atomic::{AtomicUsize, Ordering};

pub(super) const WATCHDOG_EXIT: u8 = 124;
pub(super) const JOURNAL_ERROR_EXIT: u8 = 125;
const VERSION: &str = "1";
const RECORD_SEPARATOR: char = '\t';
const MAX_BYTES: usize = 8192;
const MAX_ERROR_BYTES: usize = 2048;
const STAGES: [&str; 8] = [
    "parsed",
    "mapped",
    "relocated",
    "imports_bound",
    "tls_completed",
    "entry",
    "exit",
    "framebuffer",
];

pub(super) struct Journal(PathBuf);

impl Journal {
    pub(super) fn create() -> io::Result<Self> {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        loop {
            let id = NEXT.fetch_add(1, Ordering::Relaxed);
            let directory =
                std::env::temp_dir().join(format!("betterxen-startup-{}-{id}", std::process::id()));
            match fs::DirBuilder::new().mode(0o700).create(&directory) {
                Ok(()) => return Ok(Self(directory)),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error),
            }
        }
    }

    pub(super) fn path(&self) -> PathBuf {
        self.0.join("events")
    }

    fn read(&self) -> Report {
        let mut bytes = Vec::new();
        match File::open(self.path())
            .and_then(|file| file.take((MAX_BYTES + 1) as u64).read_to_end(&mut bytes))
        {
            Ok(_) => Report::parse(&bytes),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Report::default(),
            Err(_) => Report {
                integrity: "unreadable",
                ..Report::default()
            },
        }
    }

    pub(super) fn finish(
        &self,
        mut destination: File,
        termination: &io::Result<Termination>,
    ) -> io::Result<()> {
        destination.write_all(self.read().json(termination).as_bytes())
    }
}

impl Drop for Journal {
    fn drop(&mut self) {
        let _ = fs::remove_file(self.path());
        let _ = fs::remove_dir(&self.0);
    }
}

pub(super) struct Events(File, Option<io::Error>);

impl Events {
    pub(super) fn open(path: &Path) -> io::Result<Self> {
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .map(|file| Self(file, None))
    }

    pub(super) fn stage(&mut self, stage: &str) {
        self.record(format_args!("{VERSION}\t{stage}"));
    }

    pub(super) fn failure(&mut self, error: &dyn std::error::Error) {
        let message = error.to_string();
        let end = message.floor_char_boundary(MAX_ERROR_BYTES);
        let mut encoded = String::with_capacity(end * 2);
        for byte in message[..end].bytes() {
            write!(encoded, "{byte:02x}").unwrap();
        }
        self.record(format_args!(
            "{VERSION}\tfailure\t{}\t{encoded}",
            u8::from(end < message.len())
        ));
    }
    fn record(&mut self, arguments: std::fmt::Arguments<'_>) {
        if self.1.is_none() {
            self.1 = writeln!(self.0, "{arguments}").err();
        }
    }

    pub(super) fn write_error(&self) -> Option<&io::Error> {
        self.1.as_ref()
    }
}

pub(super) enum Termination {
    Exited(ExitStatus),
    Deadline,
}

struct Report {
    completed: usize,
    error: Option<String>,
    error_truncated: bool,
    integrity: &'static str,
}

impl Default for Report {
    fn default() -> Self {
        Self {
            completed: 0,
            error: None,
            error_truncated: false,
            integrity: "complete",
        }
    }
}

impl Report {
    fn parse(bytes: &[u8]) -> Self {
        let mut report = Self::default();
        let bounded = &bytes[..bytes.len().min(MAX_BYTES)];
        for line in bounded.split_inclusive(|&byte| byte == b'\n') {
            let Some(line) = line.strip_suffix(b"\n") else {
                report.integrity = "truncated";
                break;
            };
            if !std::str::from_utf8(line).is_ok_and(|line| report.record(line)) {
                report.integrity = "malformed";
                break;
            }
        }
        if bytes.len() > MAX_BYTES {
            report.integrity = "oversized";
        }
        report
    }

    fn record(&mut self, line: &str) -> bool {
        let Some((VERSION, record)) = line.split_once(RECORD_SEPARATOR) else {
            return false;
        };
        if self.error.is_some() {
            return false;
        }
        if STAGES.get(self.completed) == Some(&record) {
            self.completed += 1;
            return true;
        }
        let Some((truncated, encoded)) = record
            .strip_prefix("failure\t")
            .and_then(|s| s.split_once('\t'))
        else {
            return false;
        };
        if !matches!(truncated, "0" | "1")
            || encoded.len() > MAX_ERROR_BYTES * 2
            || !encoded.len().is_multiple_of(2)
            || !encoded.bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            return false;
        }
        let (pairs, _) = encoded.as_bytes().as_chunks::<2>();
        let Some(message) = pairs
            .iter()
            .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).ok()?, 16).ok())
            .collect::<Option<Vec<_>>>()
            .and_then(|bytes| String::from_utf8(bytes).ok())
        else {
            return false;
        };
        self.error = Some(message);
        self.error_truncated = truncated == "1";
        true
    }

    fn json(&self, termination: &io::Result<Termination>) -> String {
        let unwritable = matches!(termination, Ok(Termination::Exited(status))
            if status.code() == Some(i32::from(JOURNAL_ERROR_EXIT)));
        let integrity = if unwritable {
            "unwritable"
        } else {
            self.integrity
        };
        let (outcome, status, detail) = match termination {
            Ok(Termination::Deadline) => (
                "timeout",
                None,
                Some("worker exceeded the wall deadline".into()),
            ),
            Err(error) => ("unknown", None, Some(error.to_string())),
            Ok(Termination::Exited(status)) if status.code() == Some(i32::from(WATCHDOG_EXIT)) => (
                "timeout",
                Some(status),
                Some("worker watchdog expired".into()),
            ),
            Ok(Termination::Exited(status)) if unwritable => (
                "unknown",
                Some(status),
                Some("worker could not write its startup journal".into()),
            ),
            Ok(Termination::Exited(status))
                if status.success()
                    && self.error.is_none()
                    && self.completed == STAGES.len()
                    && self.integrity == "complete" =>
            {
                ("verified", Some(status), None)
            }
            Ok(Termination::Exited(status))
                if status.code().is_some() && !status.success() && self.error.is_some() =>
            {
                ("stage_failure", Some(status), None)
            }
            Ok(Termination::Exited(status)) if status.code().is_some() && !status.success() => {
                ("early_exit", Some(status), Some(status.to_string()))
            }
            Ok(Termination::Exited(status)) => ("unknown", Some(status), Some(status.to_string())),
        };
        let stages = &STAGES[..self.completed];
        let records = stages
            .iter()
            .map(|stage| format!(r#"{{"schema_version":{VERSION},"stage":"{stage}"}}"#))
            .collect::<Vec<_>>()
            .join(",");
        let last = json_string(stages.last().copied());
        let error = json_string(self.error.as_deref());
        let detail = json_string(detail.as_deref());
        let code = status
            .and_then(|status| status.code())
            .map_or("null".into(), |code| code.to_string());
        format!(
            "{{\"schema_version\":{VERSION},\"outcome\":\"{outcome}\",\"last_completed_stage\":{last},\"record_integrity\":\"{}\",\"worker_exit_code\":{code},\"termination_detail\":{detail},\"error\":{error},\"error_truncated\":{},\"stages\":[{records}]}}\n",
            integrity, self.error_truncated
        )
    }
}

fn json_string(value: Option<&str>) -> String {
    let Some(value) = value else {
        return String::from("null");
    };
    let mut escaped = String::from("\"");
    for ch in value.chars() {
        match ch {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\0'..='\u{1f}' => write!(escaped, "\\u{:04x}", u32::from(ch)).unwrap(),
            _ => escaped.push(ch),
        }
    }
    escaped.push('"');
    escaped
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;
    use std::time::{Duration, Instant};

    #[test]
    fn journal_write_error_surfaces_in_report() {
        use std::os::unix::process::ExitStatusExt;
        let journal = Journal::create().unwrap();
        fs::write(journal.path(), b"1\tparsed\n").unwrap();
        let mut events = Events(File::open(journal.path()).unwrap(), None);
        events.stage("mapped");
        assert!(events.write_error().is_some());
        let status = ExitStatus::from_raw(i32::from(JOURNAL_ERROR_EXIT) << 8);
        let result = journal.read().json(&Ok(Termination::Exited(status)));
        assert!(
            result.contains(r#""record_integrity":"unwritable""#),
            "{result}"
        );
        assert!(
            result.contains(r#""last_completed_stage":"parsed""#),
            "{result}"
        );
    }

    #[test]
    fn terminal_failure_keeps_signal_detail_after_all_stages() {
        use std::os::unix::process::ExitStatusExt;
        let journal = Journal::create().unwrap();
        let mut events = Events::open(&journal.path()).unwrap();
        for stage in STAGES {
            events.stage(stage);
        }
        events.failure(&io::Error::other("late error"));
        let report = journal.read();
        assert_eq!(report.integrity, "complete");
        let result = report.json(&Ok(Termination::Exited(ExitStatus::from_raw(15))));
        assert!(result.contains(r#""outcome":"unknown""#), "{result}");
        assert!(result.contains(r#""error":"late error""#), "{result}");
        assert!(result.contains("signal: 15"), "{result}");
    }

    #[test]
    fn supervisor_distinguishes_early_exit_watchdog_signal_and_deadline() {
        for (script, limit, outcome, code) in [
            ("exit 9", Duration::from_secs(2), "early_exit", "9"),
            ("exit 124", Duration::from_secs(2), "timeout", "124"),
            ("kill -TERM $$", Duration::from_secs(2), "unknown", "null"),
            (
                "exec sleep 5",
                Duration::from_millis(250),
                "timeout",
                "null",
            ),
        ] {
            let journal = Journal::create().unwrap();
            let started = Instant::now();
            let child = Command::new("/bin/sh")
                .args([
                    "-c",
                    &format!("printf '1\\tparsed\\n' > \"$1\"; {script}"),
                    "worker",
                ])
                .arg(journal.path())
                .spawn()
                .unwrap();
            let termination = super::super::supervise(child, started, limit);
            let report = journal.read().json(&termination);
            assert!(
                report.contains(&format!(r#""outcome":"{outcome}""#)),
                "{report}"
            );
            assert!(
                report.contains(&format!(r#""worker_exit_code":{code}"#)),
                "{report}"
            );
            assert!(
                report.contains(r#""last_completed_stage":"parsed""#),
                "{report}"
            );
        }
    }

    #[test]
    fn invalid_records_keep_only_the_valid_prefix() {
        let mut oversized = b"1\tparsed\n1\tmapped".to_vec();
        oversized.resize(MAX_BYTES + 1, b'x');
        for (bytes, integrity) in [
            (&b"1\tparsed\n1\tmapped"[..], "truncated"),
            (&b"1\tparsed\n1\tentry\n1\tmapped\n"[..], "malformed"),
            (&b"1\tparsed\n1\tfailure\t0\tqq\n"[..], "malformed"),
            (oversized.as_slice(), "oversized"),
        ] {
            let journal = Journal::create().unwrap();
            fs::write(journal.path(), bytes).unwrap();
            let report = journal.read();
            assert_eq!(report.completed, 1);
            assert_eq!(report.integrity, integrity);
            assert!(report.error.is_none());
        }
    }

    #[test]
    fn failure_preserves_original_context_with_bounded_utf8() {
        let journal = Journal::create().unwrap();
        let mut events = Events::open(&journal.path()).unwrap();
        events.stage("parsed");
        let message = format!("line\nwith \"quote\" and \\{}", "é".repeat(MAX_ERROR_BYTES));
        events.failure(&io::Error::other(&message[..]));
        let report = journal.read();
        assert_eq!(
            report.error.as_deref(),
            Some(&message[..message.floor_char_boundary(MAX_ERROR_BYTES)])
        );
        assert!(report.error_truncated);
        let json = report.json(&Ok(Termination::Deadline));
        assert!(
            json.contains(r#"line\u000awith \"quote\" and \\"#),
            "{json}"
        );
    }
}

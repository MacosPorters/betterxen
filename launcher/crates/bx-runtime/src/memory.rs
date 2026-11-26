//! Memory-pressure supervision of a child process.
//!
//! Adapted in concept from the memory guard in `mac/run-victory.sh` of
//! <https://github.com/skakwy/victory-mac> (author skakwy, GPL-3.0; commit 22b3f8e, used with
//! the author's permission): do not start a workload when little memory is free, and stop it
//! when system-wide free memory falls below a limit, because GPU memory is wired under
//! unified memory and a runaway workload can take the whole Mac down. This version is
//! reimplemented in Rust, stops the child with SIGTERM before SIGKILL, and reports why it
//! stopped. It supervises any command; it contains nothing game-specific.
//!
//! The reading is the "System-wide memory free percentage" line of `memory_pressure -Q`.
//! That line was observed on macOS 27.0 (build 26A428); its format is not a documented
//! stable interface, so an unreadable value is an error, never "assume enough memory".

use crate::cmdutil::run;
use std::process::{Child, Command};
use std::time::{Duration, Instant};

pub fn parse_free_percentage(output: &str) -> Option<u8> {
    let line = output
        .lines()
        .find(|l| l.contains("System-wide memory free percentage"))?;
    let digits: String = line
        .split(':')
        .nth(1)?
        .trim()
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    let v: u8 = digits.parse().ok()?;
    (v <= 100).then_some(v)
}

pub fn free_percentage() -> Option<u8> {
    run("/usr/bin/memory_pressure", &["-Q"]).and_then(|o| parse_free_percentage(&o))
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Limits {
    /// Refuse to start below this percent free.
    pub min_start_free: u8,
    /// Stop the child when free memory drops below this percent.
    pub min_free: u8,
    pub poll: Duration,
    /// Time between SIGTERM and SIGKILL.
    pub grace: Duration,
}

#[derive(Debug, PartialEq)]
pub enum Outcome {
    /// The child ran to completion; `None` means it ended by a signal.
    Exited(Option<i32>),
    RefusedToStart {
        free: u8,
        need: u8,
    },
    StoppedLowMemory {
        free: u8,
        limit: u8,
    },
    /// Memory could not be read, so no guarantee can be given.
    MemoryUnreadable,
    SpawnFailed(String),
}

pub(crate) fn terminate(child: &mut Child, grace: Duration) {
    let _ = Command::new("/bin/kill")
        .args(["-TERM", &child.id().to_string()])
        .status();
    let t0 = Instant::now();
    while t0.elapsed() < grace {
        if matches!(child.try_wait(), Ok(Some(_))) {
            return;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let _ = child.kill();
    let _ = child.wait();
}

/// Run `program` under `limits`, reading memory through `read_free`.
pub fn supervise(
    program: &str,
    args: &[String],
    limits: Limits,
    read_free: &mut dyn FnMut() -> Option<u8>,
) -> Outcome {
    match read_free() {
        None => return Outcome::MemoryUnreadable,
        Some(free) if free < limits.min_start_free => {
            return Outcome::RefusedToStart {
                free,
                need: limits.min_start_free,
            };
        }
        Some(_) => {}
    }
    let mut child = match Command::new(program).args(args).spawn() {
        Ok(c) => c,
        Err(e) => return Outcome::SpawnFailed(format!("{program}: {e}")),
    };
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Outcome::Exited(status.code()),
            Ok(None) => {}
            Err(e) => return Outcome::SpawnFailed(e.to_string()),
        }
        match read_free() {
            None => {
                terminate(&mut child, limits.grace);
                return Outcome::MemoryUnreadable;
            }
            Some(free) if free < limits.min_free => {
                terminate(&mut child, limits.grace);
                return Outcome::StoppedLowMemory {
                    free,
                    limit: limits.min_free,
                };
            }
            Some(_) => {}
        }
        std::thread::sleep(limits.poll);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn limits() -> Limits {
        Limits {
            min_start_free: 50,
            min_free: 20,
            poll: Duration::from_millis(20),
            grace: Duration::from_millis(500),
        }
    }

    #[test]
    fn parses_memory_pressure_output() {
        let real = "The system has 25769803776 (1572864 pages with a page size of 16384).\nSystem-wide memory free percentage: 71%\n";
        assert_eq!(parse_free_percentage(real), Some(71));
        assert_eq!(
            parse_free_percentage("System-wide memory free percentage: 100%"),
            Some(100)
        );
        assert_eq!(
            parse_free_percentage("System-wide memory free percentage: 101%"),
            None
        );
        assert_eq!(
            parse_free_percentage("System-wide memory free percentage: x"),
            None
        );
        assert_eq!(parse_free_percentage("nothing"), None);
    }

    #[test]
    fn refuses_to_start_when_low() {
        let out = supervise("/usr/bin/true", &[], limits(), &mut || Some(30));
        assert_eq!(out, Outcome::RefusedToStart { free: 30, need: 50 });
    }

    #[test]
    fn unreadable_memory_is_an_error() {
        assert_eq!(
            supervise("/usr/bin/true", &[], limits(), &mut || None),
            Outcome::MemoryUnreadable
        );
    }

    #[test]
    fn child_exit_code_is_reported() {
        let out = supervise(
            "/bin/sh",
            &["-c".into(), "exit 7".into()],
            limits(),
            &mut || Some(90),
        );
        assert_eq!(out, Outcome::Exited(Some(7)));
    }

    #[test]
    fn stops_child_when_memory_drops() {
        let mut calls = 0;
        let t0 = Instant::now();
        let out = supervise("/bin/sleep", &["30".into()], limits(), &mut || {
            calls += 1;
            Some(if calls < 4 { 90 } else { 10 })
        });
        assert_eq!(
            out,
            Outcome::StoppedLowMemory {
                free: 10,
                limit: 20
            }
        );
        assert!(
            t0.elapsed() < Duration::from_secs(10),
            "child was not stopped promptly"
        );
    }

    #[test]
    fn real_reading_is_available_on_macos() {
        if cfg!(target_os = "macos") {
            assert!(free_percentage().is_some());
        }
    }
}

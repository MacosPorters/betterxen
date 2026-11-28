//! Running the x86_64 probe under Rosetta 2 with an isolated environment.

use std::io::{self, Read};
use std::path::Path;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread;
use std::time::{Duration, Instant};

/// Variables passed through to the probe; everything else is cleared so host
/// credentials and stray settings never reach D3DMetal.
pub const ENV_ALLOWLIST: [&str; 3] = ["HOME", "TMPDIR", "DMN_LOG"];
/// `PATH` given to the probe.
const PROBE_PATH: &str = "/usr/bin:/bin";
const POLL_INTERVAL: Duration = Duration::from_millis(10);
/// Time allowed after the deadline (or a timeout kill) to drain output the
/// probe already wrote.
const DRAIN_GRACE: Duration = Duration::from_millis(200);
const READ_CHUNK: usize = 8192;

/// What a probe process produced.
#[derive(Debug)]
pub struct ProbeRun {
    /// `None` when the probe was killed after the timeout.
    pub status: Option<ExitStatus>,
    pub stdout: String,
    pub stderr: String,
    /// `false` when stdout or stderr was still open when collection stopped,
    /// for example because a descendant of the probe inherited the pipes.
    pub output_complete: bool,
}

#[derive(Clone, Copy)]
enum Stream {
    Stdout,
    Stderr,
}

enum Event {
    Data(Stream, Vec<u8>),
    Closed,
    Failed(io::Error),
}

/// Forwards a pipe to `tx` in chunks. The thread is never joined: if a
/// descendant keeps the pipe open it blocks until that process exits and then
/// ends when its next send fails.
fn pump(mut pipe: impl Read + Send + 'static, stream: Stream, tx: Sender<Event>) {
    thread::spawn(move || {
        let mut buf = [0u8; READ_CHUNK];
        loop {
            let event = match pipe.read(&mut buf) {
                Ok(0) => Event::Closed,
                Ok(n) => Event::Data(stream, buf[..n].to_vec()),
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) => Event::Failed(e),
            };
            let last = !matches!(event, Event::Data(..));
            if tx.send(event).is_err() || last {
                return;
            }
        }
    });
}

/// Collects both streams until they close or `until` passes.
fn collect(rx: &Receiver<Event>, until: Instant) -> io::Result<(Vec<u8>, Vec<u8>, bool)> {
    let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
    let mut open = 2;
    while open > 0 {
        match rx.recv_timeout(until.saturating_duration_since(Instant::now())) {
            Ok(Event::Data(Stream::Stdout, bytes)) => stdout.extend_from_slice(&bytes),
            Ok(Event::Data(Stream::Stderr, bytes)) => stderr.extend_from_slice(&bytes),
            Ok(Event::Closed) => open -= 1,
            Ok(Event::Failed(e)) => return Err(e),
            Err(RecvTimeoutError::Timeout) => return Ok((stdout, stderr, false)),
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }
    Ok((stdout, stderr, true))
}

/// `deadline: None` means the requested timeout is not representable on this clock, so the
/// probe is allowed to run without one (#587).
fn wait_until(child: &mut Child, deadline: Option<Instant>) -> io::Result<Option<ExitStatus>> {
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(Some(status));
        }
        if deadline.is_some_and(|at| Instant::now() >= at) {
            child.kill()?;
            child.wait()?;
            return Ok(None);
        }
        thread::sleep(POLL_INTERVAL);
    }
}

/// Runs `probe --framework <framework_binary>` and collects its output. The
/// whole run, output collection included, ends within `timeout` plus a short
/// drain grace, even if a descendant of the probe holds its pipes open.
pub fn run_probe(probe: &Path, framework_binary: &Path, timeout: Duration) -> io::Result<ProbeRun> {
    let mut command = Command::new(probe);
    command
        .arg("--framework")
        .arg(framework_binary)
        .env_clear()
        .env("PATH", PROBE_PATH)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for key in ENV_ALLOWLIST {
        if let Some(value) = std::env::var_os(key) {
            command.env(key, value);
        }
    }
    // `--timeout` takes any u64 seconds, and `Instant + Duration` panics on overflow, so a
    // huge value would abort the harness with exit 101 instead of one of its verdict codes
    // (#586). Saturate at the platform maximum instead: the probe then simply never times out.
    // `--timeout` takes any u64 seconds, and `Instant + Duration` panics on overflow. When the
    // requested timeout is not representable the probe simply has no deadline; representing
    // that as `None` avoids inventing a fallback duration that might also not fit (#587).
    let deadline = Instant::now().checked_add(timeout);
    let mut child = command.spawn()?;
    let (tx, rx) = mpsc::channel();
    pump(
        child.stdout.take().expect("stdout is piped"),
        Stream::Stdout,
        tx.clone(),
    );
    pump(
        child.stderr.take().expect("stderr is piped"),
        Stream::Stderr,
        tx,
    );
    let status = wait_until(&mut child, deadline)?;
    // With no deadline the probe has already exited, so draining still needs a bound.
    let drain_until = deadline
        .unwrap_or_else(Instant::now)
        .max(Instant::now() + DRAIN_GRACE);
    let (stdout, stderr, output_complete) = collect(&rx, drain_until)?;
    Ok(ProbeRun {
        status,
        stdout: String::from_utf8_lossy(&stdout).into_owned(),
        stderr: String::from_utf8_lossy(&stderr).into_owned(),
        output_complete,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;

    /// A throwaway executable shell script standing in for the probe.
    struct Script(PathBuf);

    impl Script {
        fn new(name: &str, body: &str) -> Self {
            let path =
                std::env::temp_dir().join(format!("bx-harness-{name}-{}.sh", std::process::id()));
            std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
            Self(path)
        }
    }

    impl Drop for Script {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }

    #[test]
    fn an_unrepresentable_deadline_leaves_the_probe_without_a_timeout() {
        // `checked_add` returns None for a timeout the clock cannot represent; the runner
        // must then wait for the probe normally instead of inventing a fallback duration
        // that could itself overflow (#587).
        let script = Script::new("nodeadline", "sleep 0.2; exit 0");
        let run = run_probe(
            &script.0,
            std::path::Path::new("/nonexistent/framework"),
            Duration::from_secs(u64::MAX),
        )
        .expect("an unrepresentable deadline must not abort the run");
        assert_eq!(run.status.and_then(|s| s.code()), Some(0));
        assert!(run.output_complete);
    }

    #[test]
    fn an_overflowing_timeout_does_not_panic() {
        // u64::MAX seconds overflows `Instant + Duration`; before #586 this panicked.
        let script = Script::new("overflow", "exit 0");
        let run = run_probe(
            &script.0,
            std::path::Path::new("/nonexistent/framework"),
            Duration::from_secs(u64::MAX),
        );
        let run = run.expect("an overflowing timeout must still run the probe, not panic");
        assert_eq!(run.status.and_then(|s| s.code()), Some(0));
    }

    #[test]
    fn passes_the_framework_and_only_allowlisted_environment() {
        let script = Script::new("env", "echo \"args=$*\"; env; echo oops >&2; exit 3");
        let run = run_probe(
            &script.0,
            Path::new("/fw/D3DMetal"),
            Duration::from_secs(10),
        )
        .unwrap();
        assert_eq!(run.status.and_then(|s| s.code()), Some(3));
        assert!(run.stdout.contains("args=--framework /fw/D3DMetal"));
        assert_eq!(run.stderr, "oops\n");
        // /bin/sh itself adds PWD, SHLVL and `_`.
        let allowed = ["PATH", "PWD", "SHLVL", "_"];
        for line in run.stdout.lines().skip(1) {
            let key = line.split_once('=').map_or(line, |(k, _)| k);
            assert!(
                allowed.contains(&key) || ENV_ALLOWLIST.contains(&key),
                "unexpected variable {key} reached the probe"
            );
        }
        assert!(run.stdout.contains(&format!("PATH={PROBE_PATH}")));
        assert!(run.output_complete);
    }

    /// Timeout for the tests below whose stand-in probe has to start and print before the
    /// deadline. Starting a freshly written script costs tens of milliseconds here and
    /// occasionally far more, so a few hundred milliseconds made these tests fail now and then
    /// (#479). They check what happens at the deadline, not how fast a script starts.
    const START_MARGIN: Duration = Duration::from_secs(2);

    /// Removes the pid file, and kills the recorded process if a failing test left it running.
    struct PidFile(PathBuf);

    impl Drop for PidFile {
        fn drop(&mut self) {
            if std::thread::panicking()
                && let Ok(pid) = std::fs::read_to_string(&self.0)
            {
                let _ = Command::new("/bin/kill").arg(pid.trim()).status();
            }
            let _ = std::fs::remove_file(&self.0);
        }
    }

    #[test]
    fn kills_a_probe_that_exceeds_the_timeout() {
        let pid_file = PidFile(
            std::env::temp_dir().join(format!("bx-harness-probe's-{}.pid", std::process::id())),
        );
        let _ = std::fs::remove_file(&pid_file.0);
        // Pass the path as the value of --framework. The apostrophe tests literal path handling.
        // `exec` keeps the pid, so the recorded pid is the process that has to be killed.
        let script = Script::new(
            "hang",
            "echo $$ > \"$2\" || exit 1; echo started; exec sleep 30",
        );
        let started = Instant::now();
        let run = run_probe(&script.0, &pid_file.0, START_MARGIN).unwrap();
        let elapsed = started.elapsed();
        assert!(run.status.is_none());
        // Output written before the kill is kept.
        assert_eq!(run.stdout, "started\n");
        // The probe ran until the deadline and no longer: it was killed, not waited for.
        assert!(elapsed >= START_MARGIN, "returned early after {elapsed:?}");
        assert!(
            elapsed < START_MARGIN + DRAIN_GRACE + Duration::from_secs(5),
            "returned after {elapsed:?}"
        );
        // The process is gone. `output_complete` is deliberately not asserted: tests run in
        // parallel, and a child spawned by another test at the wrong moment can inherit this
        // probe's pipe and hold it open (seen in 2 of 200 runs).
        let pid = std::fs::read_to_string(&pid_file.0).unwrap();
        let alive = Command::new("/bin/kill")
            .args(["-0", pid.trim()])
            .stderr(Stdio::null())
            .status()
            .unwrap()
            .success();
        assert!(!alive, "probe {} survived the timeout", pid.trim());
    }

    /// The probe exits at once, but a background descendant inherits stdout and
    /// keeps it open well past the timeout.
    #[test]
    fn a_descendant_holding_the_pipes_cannot_outlast_the_deadline() {
        // The descendant has to outlive the bound asserted below by a wide margin.
        let script = Script::new("orphan", "sleep 6 & echo started");
        let timeout = START_MARGIN;
        let started = Instant::now();
        let run = run_probe(&script.0, Path::new("/fw"), timeout).unwrap();
        let elapsed = started.elapsed();
        assert_eq!(run.status.and_then(|s| s.code()), Some(0));
        assert_eq!(run.stdout, "started\n");
        assert!(!run.output_complete);
        assert!(
            elapsed < timeout + DRAIN_GRACE + Duration::from_secs(1),
            "returned after {elapsed:?}"
        );
    }
}

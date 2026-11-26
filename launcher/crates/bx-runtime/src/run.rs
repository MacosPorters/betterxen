//! Launch a Windows program through an installed CrossOver, supervise it, and write a
//! structured record of what happened.
//!
//! Interface used: `<CrossOver.app>/Contents/SharedSupport/CrossOver/bin/wine --bottle B EXE`
//! and `.../bin/wineserver -k`. These are **not documented by CodeWeavers** for macOS; they
//! exist in the installed build and answer `--help` (cxbottle, cxstart) as observed on build
//! 26.3.0.39832. Every record therefore carries the CrossOver build and a flag saying the
//! launcher interface is vendor-help-only. The graphics backend is selected through the
//! `CX_GRAPHICS_BACKEND` environment variable, which appears in third-party launch scripts and
//! is likewise undocumented; an unrecognised value was observed to be ignored silently, so a
//!
//! Stall detection and the wait for memory to settle after a stop follow the supervision
//! lessons in skakwy's <https://github.com/skakwy/victory-mac> (`mac/PROGRESS.md`,
//! `mac/run-victory.sh`, `crates/victory-core/src/notes.rs` at commit 22b3f8e45a59; his own
//! work, GPL-3.0, used with his permission): detect a hung run by absence of progress, stop a
//! bottle with `wineserver -k` and its prefix, and do not start the next run until memory has
//! been returned. Reimplemented here; none of his code is copied.
//!
//! `CX_GRAPHICS_BACKEND` and `CX_GRAPHICS_BACKEND_VERSION` are removed from the child's
//! environment unless a backend was requested. Inherited values silently changed the backend
//! while the record said none was requested (reproduced by IamHeavyLazer in issue #350).
//!
//! The child does not inherit the caller's environment. Only [`ENV_ALLOWLIST`] and explicit
//! `--env` pairs are passed, because a Windows program run this way could otherwise read any
//! host variable (observed: tokens and `SSH_AUTH_SOCK` are visible to it). This limits what
//! is handed over; it is not a sandbox, and the program can still reach the user's files.
//!
//! The launcher runs in its own process group. SIGINT, SIGTERM or SIGHUP to the supervisor
//! stops the group and the bottle. SIGKILL of the supervisor cannot be handled; use
//! `bx-cli stop --bottle B` (or `--prefix P`) afterwards.
//!
//! A workload may print `gate <name> pass|FAIL <detail>` lines. They are recorded, and a run
//! whose process exits 0 is still not a pass if any gate failed or a required gate is absent:
//! starting a process says nothing about whether it initialised.
//!
//! A record names the backend *requested* and relies on the workload's own diagnostics
//! (adapter name, backend log lines) to show what actually ran.

use crate::frames;
use crate::json::Json;
use crate::memory;
use std::io::Read;
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone)]
pub struct RunConfig {
    pub wine: PathBuf,
    pub bottle: String,
    /// An explicit Wine prefix uses #406's wineloader path instead of named-bottle launch.
    pub prefix: Option<PathBuf>,
    pub backend: Option<String>,
    pub program: String,
    pub args: Vec<String>,
    pub timeout: Duration,
    pub limits: memory::Limits,
    /// Text that must appear in stdout for a zero exit to count as a pass.
    pub expect: Option<String>,
    pub work_dir: PathBuf,
    /// Stop the run when neither stdout nor stderr has grown for this long.
    pub stall_timeout: Option<Duration>,
    /// After a stop, wait at most this long for free memory to return to `min_start_free`.
    pub settle_bound: Duration,
    /// Free-memory reader; `memory::free_percentage` outside tests.
    pub read_free: fn() -> Option<u8>,
    /// Gate names that must appear as passed.
    pub require_gates: Vec<String>,
    /// Stop the run when stdout and stderr together exceed this many bytes.
    pub max_output: u64,
    /// Pass the caller's whole environment instead of the allowlist.
    pub inherit_env: bool,
    /// Extra variables set for the child.
    pub env: Vec<(String, String)>,
    /// Keep the workload's full stdout and stderr files in `work_dir`.
    pub keep_output: bool,
    /// Set by a signal handler; the run is then stopped and classified `interrupted`.
    pub interrupted: &'static AtomicBool,
}

/// Host variables passed to the child unless the whole environment is requested.
pub const ENV_ALLOWLIST: [&str; 12] = [
    "HOME",
    "USER",
    "LOGNAME",
    "TMPDIR",
    "LANG",
    "LC_ALL",
    "LC_CTYPE",
    "__CF_USER_TEXT_ENCODING",
    "TERM",
    "DISPLAY",
    "XPC_FLAGS",
    "XPC_SERVICE_NAME",
];

impl RunConfig {
    pub fn prefix_path(&self) -> PathBuf {
        self.prefix
            .clone()
            .unwrap_or_else(|| bottle_prefix(&self.bottle))
    }
}

/// System directories only: CrossOver's launcher is a script that needs standard tools.
const CHILD_PATH: &str = "/usr/bin:/bin:/usr/sbin:/sbin";

fn isolated_host_env(
    mut lookup: impl FnMut(&str) -> Option<std::ffi::OsString>,
) -> Vec<(&'static str, std::ffi::OsString)> {
    let mut values = vec![("PATH", CHILD_PATH.into())];
    values.extend(
        ENV_ALLOWLIST
            .into_iter()
            .filter_map(|key| lookup(key).map(|value| (key, value))),
    );
    values
}

/// Backend names accepted for `CX_GRAPHICS_BACKEND`. The installed CrossOver ignores an
/// unknown value silently, so the runner refuses it instead.
pub const BACKENDS: [&str; 4] = ["d3dmetal", "dxmt", "dxvk", "wined3d"];

const BACKEND_VARS: [&str; 2] = ["CX_GRAPHICS_BACKEND", "CX_GRAPHICS_BACKEND_VERSION"];

#[derive(Debug, Clone, PartialEq)]
pub struct Gate {
    pub name: String,
    pub passed: bool,
    pub detail: String,
}

pub fn parse_gates(stdout: &str) -> Vec<Gate> {
    stdout
        .lines()
        .filter_map(|l| {
            let mut it = l.trim_end().strip_prefix("gate ")?.splitn(3, ' ');
            let name = it.next()?.to_string();
            // Anything other than the exact word "pass" is not a pass.
            let passed = it.next()? == "pass";
            Some(Gate {
                name,
                passed,
                detail: it.next().unwrap_or("").to_string(),
            })
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq)]
pub enum Class {
    Passed,
    FailedExit(i32),
    MissingExpectedMarker,
    TimedOut,
    /// No output on stdout or stderr for the stall timeout.
    Stalled,
    StoppedLowMemory,
    RefusedToStart,
    /// The Windows loader did not accept the file (for example an ARM64 PE this build cannot run).
    CannotExecute,
    /// The workload reported that no graphics device could be created.
    NoDevice,
    MemoryUnreadable,
    LaunchFailed,
    /// A gate reported failure, or a required gate never reported.
    GateFailed,
    OutputLimit,
    /// The supervisor received SIGINT, SIGTERM or SIGHUP.
    Interrupted,
    /// A bottle-specific stop failed or did not complete within its bound.
    CleanupFailed,
    /// Complete workload output could not be read back for the requested record.
    EvidenceFailed,
}

impl Class {
    pub fn name(&self) -> String {
        match self {
            Class::Passed => "passed".into(),
            Class::FailedExit(c) => format!("failed_exit_{c}"),
            Class::MissingExpectedMarker => "missing_expected_marker".into(),
            Class::TimedOut => "timed_out".into(),
            Class::Stalled => "stalled".into(),
            Class::StoppedLowMemory => "stopped_low_memory".into(),
            Class::RefusedToStart => "refused_to_start_low_memory".into(),
            Class::CannotExecute => "cannot_execute".into(),
            Class::NoDevice => "no_graphics_device".into(),
            Class::MemoryUnreadable => "memory_unreadable".into(),
            Class::LaunchFailed => "launch_failed".into(),
            Class::GateFailed => "gate_failed".into(),
            Class::OutputLimit => "stopped_output_limit".into(),
            Class::Interrupted => "interrupted".into(),
            Class::CleanupFailed => "cleanup_failed".into(),
            Class::EvidenceFailed => "evidence_failed".into(),
        }
    }
}

/// Classify a finished (or stopped) run from observable facts only.
pub fn classify(exit: Option<i32>, stdout: &str, stderr: &str, expect: Option<&str>) -> Class {
    // A successful program may itself mention "cannot execute" (for example about a child).
    if exit != Some(0) && stderr.contains("cannot execute") {
        return Class::CannotExecute;
    }
    if stdout.contains("NO DEVICE") && exit == Some(2) {
        return Class::NoDevice;
    }
    match exit {
        Some(0) => match expect {
            Some(m) if !stdout.contains(m) => Class::MissingExpectedMarker,
            _ => Class::Passed,
        },
        Some(c) => Class::FailedExit(c),
        None => Class::FailedExit(-1),
    }
}

/// Known host-environment noise in CrossOver output, reported as diagnostics instead of
/// silently dropped. Returns (kind, line) for each recognised line.
pub fn diagnostics(text: &str) -> Vec<(&'static str, String)> {
    let mut out = Vec::new();
    for line in text.lines() {
        if line.contains("LibClamAV Error") || line.contains("Can't open file or directory") {
            out.push(("clamav-database-missing", line.to_string()));
        } else if line.starts_with("warn:") && line.contains("DXVK") {
            out.push(("dxvk-warning", line.to_string()));
        }
    }
    out
}

/// Which graphics layer announced itself, judged from the workload's stdout and stderr.
pub fn detected_backend(output: &str) -> &'static str {
    if output.contains("vkd3d:") || output.contains("vkd3d-proton") {
        "vkd3d (D3D12 implementation banner; D3D11 backend selection is separate)"
    } else if output.contains("DXVK:") {
        "dxvk"
    } else if output.contains("DXMT") || output.contains("D3D_FEATURE_LEVEL_11_1\ninfo:  Using") {
        "dxmt"
    } else if output.contains("adapter=AMD Compatibility Mode") {
        "d3dmetal (inferred: Apple-reported adapter identity)"
    } else if output.contains("adapter=Apple M") {
        "wined3d (inferred: native adapter name, no layer banner)"
    } else {
        "unknown"
    }
}

#[derive(Debug, Default, Clone)]
pub struct Samples {
    pub peak_rss_kb: u64,
    pub mean_cpu_percent_of_core: f64,
    pub min_free_percent: Option<u8>,
    pub samples: u32,
    pub peak_workload_rss_kb: u64,
    pub last_workload_rss_kb: u64,
}

/// Process ids of the workload, identified the same way as for memory sampling. Input lines
/// are `ps -axo pid=,command=`.
fn workload_pids(ps: &str, file_name: &str) -> Vec<i32> {
    let needle = format!("\\{file_name}").to_ascii_lowercase();
    ps.lines()
        .filter_map(|l| {
            let (pid, command) = l.trim_start().split_once(char::is_whitespace)?;
            let command = command.trim_start().to_ascii_lowercase();
            let b = command.as_bytes();
            (b.len() > 3 && b[1] == b':' && b[2] == b'\\' && command.contains(&needle))
                .then(|| pid.parse().ok())?
        })
        .filter(|pid| *pid > 1)
        .collect()
}

pub fn running_workload_pids(file_name: &str) -> Vec<i32> {
    let ps = crate::cmdutil::run("/bin/ps", &["-axo", "pid=,command="]).unwrap_or_default();
    workload_pids(&ps, file_name)
}

// Command lines contain no reliable bottle identity after Wine reparents a Windows
// process. A basename match is useful diagnostic evidence, never authority to kill it.

/// Stop the launcher's whole process group: SIGTERM, then SIGKILL after `grace`.
fn stop_group(child: &mut Child, grace: Duration) {
    let group = child.id() as i32;
    // SAFETY: the child was started as leader of its own group, so `group` names only its
    // descendants; killpg takes plain integers.
    unsafe { libc::killpg(group, libc::SIGTERM) };
    let t = Instant::now();
    while t.elapsed() < grace && !matches!(child.try_wait(), Ok(Some(_))) {
        std::thread::sleep(Duration::from_millis(50));
    }
    // SAFETY: as above. A group that has already exited returns ESRCH, which is harmless.
    unsafe { libc::killpg(group, libc::SIGKILL) };
    let _ = child.wait();
}

/// RSS (KB) of the workload itself. Observed on CrossOver 26.3: the Windows program is
/// re-parented to launchd and its command line is a drive-letter path (`Y:\...\name.exe`),
/// so neither the bundle path nor the bottle path appears in it.
fn workload_rss_kb(ps: &str, file_name: &str) -> u64 {
    let needle = format!("\\{file_name}").to_ascii_lowercase();
    ps.lines()
        .filter_map(|l| {
            let l = l.trim_start();
            let (rss, rest) = l.split_once(char::is_whitespace)?;
            let (_cpu, command) = rest.trim_start().split_once(char::is_whitespace)?;
            let command = command.trim_start().to_ascii_lowercase();
            let b = command.as_bytes();
            (b.len() > 3 && b[1] == b':' && b[2] == b'\\' && command.contains(&needle))
                .then(|| rss.parse::<u64>().ok())?
        })
        .sum()
}

/// Sum RSS (KB) and CPU % of processes whose command line mentions the CrossOver bundle or the
/// bottle directory. Heuristic: processes not started from those paths are not counted.
fn sample_processes(out: &str, needles: &[String]) -> (u64, f64) {
    let (mut rss, mut cpu) = (0u64, 0f64);
    for l in out.lines() {
        if !needles.iter().any(|n| l.contains(n.as_str())) {
            continue;
        }
        let mut it = l.split_whitespace();
        rss += it.next().and_then(|v| v.parse::<u64>().ok()).unwrap_or(0);
        cpu += it.next().and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0);
    }
    (rss, cpu)
}

fn thermal() -> Json {
    match crate::cmdutil::run("/usr/bin/pmset", &["-g", "therm"]) {
        Some(t) => Json::Arr(
            t.lines()
                .map(|l| Json::str(l.trim()))
                .filter(|j| *j != Json::str(""))
                .collect(),
        ),
        None => Json::Null,
    }
}

pub struct RunRecord {
    pub class: Class,
    pub duration: Duration,
    pub exit: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub samples: Samples,
    pub json: Json,
    pub gates: Vec<Gate>,
    /// Why the launcher could not be started or supervised, when it could not.
    pub error: Option<String>,
}

fn file_name(program: &str) -> &str {
    program.rsplit(['/', '\\']).next().unwrap_or(program)
}

/// Windows programs and Wine may write bytes that are not UTF-8; losing the whole log over
/// one of them would hide the evidence.
fn read_lossy(path: &Path, limit: u64) -> std::io::Result<(String, bool)> {
    let file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK | libc::O_NOFOLLOW)
        .open(path)?;
    if !file.metadata()?.is_file() {
        return Err(std::io::Error::other(
            "workload output is not a regular file",
        ));
    }
    let mut bytes = Vec::new();
    file.take(limit.saturating_add(1)).read_to_end(&mut bytes)?;
    let truncated = bytes.len() as u64 > limit;
    if truncated {
        bytes.truncate(limit as usize);
    }
    Ok((String::from_utf8_lossy(&bytes).into_owned(), truncated))
}

pub fn execute(cfg: &RunConfig, crossover_build: Option<String>) -> RunRecord {
    let t_wall = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    // Read before the launch so the record holds the ceiling this run started under (#386).
    let gpu_wired = crate::host::gpu_wired();
    let _ = std::fs::create_dir_all(&cfg.work_dir);
    let out_path = cfg
        .work_dir
        .join(format!("stdout-{nanos}-{}.txt", std::process::id()));
    let err_path = cfg
        .work_dir
        .join(format!("stderr-{nanos}-{}.txt", std::process::id()));
    let mut settle: Option<(Duration, bool)> = None;
    let mut leftovers: Option<usize> = None;
    let mut passed_names: Vec<String> = Vec::new();
    let mut output_truncated = false;
    let mut raw_output_bytes = 0u64;
    let thermal_before = thermal();
    let free_start = (cfg.read_free)();

    let mut result = RunRecord {
        class: Class::LaunchFailed,
        duration: Duration::ZERO,
        exit: None,
        stdout: String::new(),
        stderr: String::new(),
        samples: Samples::default(),
        json: Json::Null,
        gates: Vec::new(),
        error: None,
    };
    let inherited_backend: Vec<(&str, String)> = BACKEND_VARS
        .iter()
        .filter_map(|k| std::env::var(k).ok().map(|v| (*k, v)))
        .collect();

    match free_start {
        _ if cfg.interrupted.load(Ordering::SeqCst) => result.class = Class::Interrupted,
        None => result.class = Class::MemoryUnreadable,
        Some(f) if f < cfg.limits.min_start_free => result.class = Class::RefusedToStart,
        Some(_) => {
            let files = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&out_path)
                .and_then(|o| {
                    std::fs::OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .open(&err_path)
                        .map(|e| (o, e))
                });
            if let Err(error) = &files {
                result.error = Some(format!("cannot create output files: {error}"));
            }
            if let Ok((o, e)) = files {
                let mut cmd = Command::new(&cfg.wine);
                if cfg.prefix.is_none() {
                    cmd.arg("--bottle").arg(&cfg.bottle);
                }
                cmd.arg(&cfg.program).args(&cfg.args);
                cmd.stdout(Stdio::from(o)).stderr(Stdio::from(e));
                cmd.process_group(0);
                if cfg.inherit_env {
                    for key in BACKEND_VARS {
                        cmd.env_remove(key);
                    }
                } else {
                    cmd.env_clear()
                        .envs(isolated_host_env(|key| std::env::var_os(key)));
                }
                for (key, value) in &cfg.env {
                    cmd.env(key, value);
                }
                // Even explicit environment inheritance must not redirect the bottle chosen
                // by --bottle while cleanup targets a different prefix.
                cmd.env_remove("WINEPREFIX").env_remove("CX_BOTTLE");
                if let Some(prefix) = &cfg.prefix {
                    let bin = cfg.wine.parent().expect("validated launcher directory");
                    let root = bin.parent().expect("validated CrossOver directory");
                    cmd.env("WINEPREFIX", prefix)
                        .env("WINESERVER", bin.join("wineserver"))
                        .env("WINELOADER", &cfg.wine)
                        .env(
                            "WINEDLLPATH",
                            format!(
                                "{}:{}",
                                root.join("lib/wine").display(),
                                root.join("lib64/wine").display()
                            ),
                        )
                        .env(
                            "DYLD_FALLBACK_LIBRARY_PATH",
                            format!(
                                "{}:{}:/usr/lib",
                                root.join("lib64").display(),
                                root.join("lib").display()
                            ),
                        );
                }
                passed_names = cfg.env.iter().map(|(k, _)| k.clone()).collect();
                if let Some(b) = &cfg.backend {
                    cmd.env("CX_GRAPHICS_BACKEND", b);
                }
                let started = Instant::now();
                let spawned = cmd.spawn();
                if let Err(error) = &spawned {
                    result.error = Some(format!("{}: {error}", cfg.wine.display()));
                }
                if let Ok(mut child) = spawned {
                    let bundle = cfg
                        .wine
                        .to_string_lossy()
                        .split("/Contents/")
                        .next()
                        .unwrap_or("")
                        .to_string();
                    let needles = vec![bundle, format!("{}/", cfg.prefix_path().display())];
                    let (mut cpu_sum, mut n) = (0f64, 0u32);
                    let mut stop: Option<Class> = None;
                    let mut bottle_cleaned = false;
                    let mut last_len = 0u64;
                    let mut last_growth = Instant::now();
                    loop {
                        match child.try_wait() {
                            Ok(Some(st)) => {
                                result.exit = st.code();
                                break;
                            }
                            Ok(None) => {}
                            Err(error) => {
                                result.error = Some(format!("lost track of the launcher: {error}"));
                                stop_group(&mut child, cfg.limits.grace);
                                match stop_prefix(&cfg.wine, &cfg.prefix_path()) {
                                    Ok(()) => bottle_cleaned = true,
                                    Err(error) => {
                                        result.error = Some(error);
                                        stop = Some(Class::CleanupFailed);
                                    }
                                }
                                break;
                            }
                        }
                        let ps = crate::cmdutil::run("/bin/ps", &["-axo", "rss=,pcpu=,command="])
                            .unwrap_or_default();
                        let (rss, cpu) = sample_processes(&ps, &needles);
                        result.samples.peak_rss_kb = result.samples.peak_rss_kb.max(rss);
                        let own = workload_rss_kb(&ps, file_name(&cfg.program));
                        if own > 0 {
                            result.samples.last_workload_rss_kb = own;
                            result.samples.peak_workload_rss_kb =
                                result.samples.peak_workload_rss_kb.max(own);
                        }
                        cpu_sum += cpu;
                        n += 1;
                        match (cfg.read_free)() {
                            Some(f) => {
                                let m = result.samples.min_free_percent.map_or(f, |x| x.min(f));
                                result.samples.min_free_percent = Some(m);
                                if f < cfg.limits.min_free {
                                    stop = Some(Class::StoppedLowMemory);
                                }
                            }
                            None => stop = Some(Class::MemoryUnreadable),
                        }
                        let len = [&out_path, &err_path]
                            .iter()
                            .map(|p| std::fs::metadata(p).map_or(0, |m| m.len()))
                            .sum::<u64>();
                        if len != last_len {
                            last_len = len;
                            last_growth = Instant::now();
                        }
                        if cfg.interrupted.load(Ordering::SeqCst) {
                            stop = Some(Class::Interrupted);
                        }
                        if stop.is_none() && len > cfg.max_output {
                            stop = Some(Class::OutputLimit);
                        }
                        if stop.is_none() && started.elapsed() > cfg.timeout {
                            stop = Some(Class::TimedOut);
                        }
                        if stop.is_none()
                            && cfg.stall_timeout.is_some_and(|d| last_growth.elapsed() > d)
                        {
                            stop = Some(Class::Stalled);
                        }
                        if stop.is_some() {
                            stop_group(&mut child, cfg.limits.grace);
                            match stop_prefix(&cfg.wine, &cfg.prefix_path()) {
                                Ok(()) => bottle_cleaned = true,
                                Err(error) => {
                                    result.error = Some(error);
                                    stop = Some(Class::CleanupFailed);
                                }
                            }
                            break;
                        }
                        std::thread::sleep(cfg.limits.poll);
                    }
                    result.duration = started.elapsed();
                    raw_output_bytes = [&out_path, &err_path]
                        .iter()
                        .filter_map(|p| std::fs::metadata(p).ok().map(|m| m.len()))
                        .sum();
                    if stop.is_none() && raw_output_bytes > cfg.max_output {
                        stop = Some(Class::OutputLimit);
                    }
                    // The launcher may exit before a Wine child. End this dedicated bottle
                    // session through its server, never by a host-wide executable-name match.
                    if !bottle_cleaned
                        && stop != Some(Class::CleanupFailed)
                        && let Err(error) = stop_prefix(&cfg.wine, &cfg.prefix_path())
                    {
                        result.error = Some(error);
                        stop = Some(Class::CleanupFailed);
                    }
                    leftovers = Some(running_workload_pids(file_name(&cfg.program)).len());
                    if stop.is_some() {
                        let t = Instant::now();
                        let need = cfg.limits.min_start_free;
                        let mut recovered = false;
                        loop {
                            if (cfg.read_free)().is_some_and(|f| f >= need) {
                                recovered = true;
                                break;
                            }
                            // a second signal is not needed to leave the wait
                            if t.elapsed() >= cfg.settle_bound || stop == Some(Class::Interrupted) {
                                break;
                            }
                            std::thread::sleep(cfg.limits.poll);
                        }
                        settle = Some((t.elapsed(), recovered));
                    }
                    result.samples.samples = n;
                    result.samples.mean_cpu_percent_of_core =
                        if n > 0 { cpu_sum / n as f64 } else { 0.0 };
                    let mut remaining = cfg.max_output;
                    for (path, text) in [
                        (&out_path, &mut result.stdout),
                        (&err_path, &mut result.stderr),
                    ] {
                        match read_lossy(path, remaining) {
                            Ok((value, truncated)) => {
                                remaining = remaining.saturating_sub(value.len() as u64);
                                *text = value;
                                output_truncated |= truncated;
                            }
                            Err(error) => {
                                result.error = Some(format!(
                                    "cannot read workload output {}: {error}",
                                    path.display()
                                ));
                                if stop.is_none() {
                                    stop = Some(Class::EvidenceFailed);
                                }
                            }
                        }
                    }
                    result.class = stop.unwrap_or_else(|| {
                        classify(
                            result.exit,
                            &result.stdout,
                            &result.stderr,
                            cfg.expect.as_deref(),
                        )
                    });
                    result.gates = parse_gates(&result.stdout);
                    if result.class == Class::Passed {
                        let failed = result.gates.iter().any(|g| !g.passed);
                        let missing = cfg
                            .require_gates
                            .iter()
                            .any(|r| !result.gates.iter().any(|g| &g.name == r && g.passed));
                        if failed || missing {
                            result.class = Class::GateFailed;
                        }
                    }
                }
            }
        }
    }

    let intervals: Vec<f64> = result
        .stdout
        .lines()
        .filter_map(|l| l.strip_prefix("frame_ms "))
        .filter_map(|v| v.trim().parse().ok())
        .collect();
    let frame_stats = frames::analyze(&intervals).ok().map(|s| s.to_json());
    let combined = format!("{}\n{}", result.stdout, result.stderr);
    let diags: Vec<Json> = diagnostics(&combined)
        .into_iter()
        .map(|(k, l)| Json::obj(vec![("kind", Json::str(k)), ("line", Json::str(l))]))
        .collect();
    let tail = |s: &str| -> Json {
        let lines: Vec<&str> = s.lines().filter(|l| !l.starts_with("frame_ms ")).collect();
        let keep = if lines.len() > 40 {
            &lines[lines.len() - 40..]
        } else {
            &lines[..]
        };
        Json::Arr(keep.iter().map(|l| Json::str(*l)).collect())
    };
    let mut fields = vec![
        ("schema", Json::str("betterxen-hybrid-run/0")),
        ("unix_time", Json::Num(t_wall as f64)),
        ("crossover_build", Json::opt_str(crossover_build)),
        (
            "launcher_interface",
            Json::str(if cfg.prefix.is_some() {
                "CrossOver wineloader with explicit prefix (observed interface, not documented macOS automation API)"
            } else {
                "CrossOver bin/wine --bottle (vendor --help only; not documented by CodeWeavers for macOS)"
            }),
        ),
        ("bottle", Json::str(cfg.bottle.clone())),
        ("prefix", Json::str(cfg.prefix_path().display().to_string())),
        (
            "launch_mode",
            Json::str(if cfg.prefix.is_some() {
                "prefix"
            } else {
                "bottle"
            }),
        ),
        ("backend_requested", Json::opt_str(cfg.backend.clone())),
        ("backend_variable_documented", Json::Bool(false)),
        ("backend_detected", Json::str(detected_backend(&combined))),
        ("program", Json::str(cfg.program.clone())),
        (
            "args",
            Json::Arr(cfg.args.iter().map(|a| Json::str(a.clone())).collect()),
        ),
        ("classification", Json::str(result.class.name())),
        (
            "exit_code",
            result.exit.map_or(Json::Null, |c| Json::Num(c as f64)),
        ),
        (
            "duration_ms",
            Json::Num(result.duration.as_secs_f64() * 1000.0),
        ),
        ("timeout_s", Json::Num(cfg.timeout.as_secs_f64())),
        (
            "stall_timeout_s",
            cfg.stall_timeout
                .map_or(Json::Null, |d| Json::Num(d.as_secs_f64())),
        ),
        (
            "memory_settle_after_stop",
            settle.map_or(Json::Null, |(d, ok)| {
                Json::obj(vec![
                    ("waited_ms", Json::Num(d.as_secs_f64() * 1000.0)),
                    ("recovered_to_start_threshold", Json::Bool(ok)),
                    ("bound_s", Json::Num(cfg.settle_bound.as_secs_f64())),
                ])
            }),
        ),
        (
            "free_memory_percent_at_start",
            free_start.map_or(Json::Null, |f| Json::Num(f as f64)),
        ),
        (
            "min_free_memory_percent_during_run",
            result
                .samples
                .min_free_percent
                .map_or(Json::Null, |f| Json::Num(f as f64)),
        ),
        (
            "peak_crossover_rss_kb_sampled",
            Json::Num(result.samples.peak_rss_kb as f64),
        ),
        (
            "mean_crossover_cpu_percent_of_one_core_sampled",
            Json::Num(result.samples.mean_cpu_percent_of_core),
        ),
        ("process_samples", Json::Num(result.samples.samples as f64)),
        (
            "gpu_utilization",
            Json::str("not collected: no unprivileged documented source on this host"),
        ),
        ("thermal_before", thermal_before),
        ("thermal_after", thermal()),
        ("diagnostics", Json::Arr(diags)),
        ("frame_stats", frame_stats.unwrap_or(Json::Null)),
        ("stdout_tail", tail(&result.stdout)),
        ("stderr_tail", tail(&result.stderr)),
        ("raw_output_bytes", Json::Num(raw_output_bytes as f64)),
        ("output_excerpt_truncated", Json::Bool(output_truncated)),
    ];
    fields.extend(vec![
        (
            "peak_workload_rss_kb_sampled",
            Json::Num(result.samples.peak_workload_rss_kb as f64),
        ),
        (
            "last_workload_rss_kb_sampled",
            Json::Num(result.samples.last_workload_rss_kb as f64),
        ),
        ("workload_memory_scope", Json::str("Windows executable basename heuristic; may include other bottles; never used as kill authority")),
        ("crossover_memory_scope", Json::str("installation/bottle path heuristic; may include other runs from this installation")),
        (
            "gates",
            Json::Arr(
                result
                    .gates
                    .iter()
                    .map(|g| {
                        Json::obj(vec![
                            ("name", Json::str(g.name.clone())),
                            ("passed", Json::Bool(g.passed)),
                            ("detail", Json::str(g.detail.clone())),
                        ])
                    })
                    .collect(),
            ),
        ),
        (
            "required_gates",
            Json::Arr(
                cfg.require_gates
                    .iter()
                    .map(|g| Json::str(g.clone()))
                    .collect(),
            ),
        ),
        (
            "inherited_backend_variables_removed",
            Json::Obj(
                inherited_backend
                    .into_iter()
                    .map(|(k, v)| (k.to_string(), Json::Str(v)))
                    .collect(),
            ),
        ),
        ("launcher_error", Json::opt_str(result.error.clone())),
        (
            "environment_policy",
            Json::str(if cfg.inherit_env {
                "inherited"
            } else {
                "allowlist"
            }),
        ),
        (
            "extra_environment_names",
            Json::Arr(passed_names.into_iter().map(Json::Str).collect()),
        ),
        (
            "workload_processes_killed_after_exit",
            leftovers.map_or(Json::Null, |_| Json::Num(0.0)),
        ),
        ("unscoped_matching_processes_after_exit", leftovers.map_or(Json::Null, |n| Json::Num(n as f64))),
        ("cleanup_policy", Json::str("launcher process group and selected bottle only; basename matches are never killed")),
        ("stdout_path", if cfg.keep_output { Json::str(out_path.display().to_string()) } else { Json::Null }),
        ("stderr_path", if cfg.keep_output { Json::str(err_path.display().to_string()) } else { Json::Null }),
    ]);
    fields.extend(gpu_wired.json_fields());
    result.json = Json::obj(fields);
    if !cfg.keep_output {
        let _ = std::fs::remove_file(&out_path);
        let _ = std::fs::remove_file(&err_path);
    }
    result
}

/// Prefix directory of a bottle. CrossOver documents `BottleDir` in its preferences; otherwise
/// the default location under Application Support is used.
pub fn bottle_prefix(bottle: &str) -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_default();
    bottle_base(crate::crossover::preference("BottleDir"), &home).join(bottle)
}

/// The `BottleDir` preference may be written with a leading `~`, which `Path` does not expand.
fn bottle_base(preference: Option<String>, home: &str) -> PathBuf {
    let home = PathBuf::from(home);
    match preference {
        None => home.join("Library/Application Support/CrossOver/Bottles"),
        Some(p) if p == "~" => home,
        Some(p) => p
            .strip_prefix("~/")
            .map_or_else(|| PathBuf::from(&p), |rest| home.join(rest)),
    }
}

/// Stop every process of one bottle with `wineserver -k`, selecting the bottle through
/// `WINEPREFIX`. Setting `CX_BOTTLE` instead was tried first and observed NOT to select the
/// bottle: the Windows process kept running after the supervisor reported it stopped.
pub fn stop_bottle(wine: &Path, bottle: &str) -> Result<(), String> {
    if !crate::preflight::valid_bottle_name(bottle) {
        return Err("invalid bottle name for cleanup".into());
    }
    stop_prefix(wine, &bottle_prefix(bottle))
}

pub fn stop_prefix(wine: &Path, prefix: &Path) -> Result<(), String> {
    let dir = wine
        .parent()
        .ok_or("wine launcher has no parent directory")?;
    for arg in ["-k", "-w"] {
        let mut command = Command::new(dir.join("wineserver"));
        command
            .arg(arg)
            .env_clear()
            .envs(isolated_host_env(|key| std::env::var_os(key)))
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        command.env("WINEPREFIX", prefix);
        let mut child = command
            .spawn()
            .map_err(|e| format!("cannot start bottle cleanup ({arg}): {e}"))?;
        let started = Instant::now();
        loop {
            match child.try_wait() {
                Ok(Some(status)) if status.success() => break,
                Ok(Some(status)) => return Err(format!("bottle cleanup {arg} failed: {status}")),
                Ok(None) if started.elapsed() < Duration::from_secs(15) => {
                    std::thread::sleep(Duration::from_millis(50))
                }
                other => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(format!(
                        "bottle cleanup {arg} did not complete within 15 seconds: {other:?}"
                    ));
                }
            }
        }
    }
    Ok(())
}

pub fn wine_path(app: &Path) -> PathBuf {
    app.join("Contents/SharedSupport/CrossOver/bin/wine")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn isolated_env_strips_secrets_and_pins_path() {
        // Preserves #406's environment contract test against the single shared implementation.
        let host = std::collections::BTreeMap::from([
            ("HOME", "/Users/tester"),
            ("USER", "tester"),
            ("PATH", "/custom/dev/bin"),
            ("GH_TOKEN", "secret_token"),
            ("AWS_SECRET_ACCESS_KEY", "secret_aws"),
            ("CX_GRAPHICS_BACKEND", "wined3d"),
        ]);
        let child: std::collections::BTreeMap<_, _> =
            isolated_host_env(|k| host.get(k).map(|v| std::ffi::OsString::from(*v)))
                .into_iter()
                .collect();
        assert_eq!(child["PATH"], std::ffi::OsStr::new(CHILD_PATH));
        assert_eq!(child["HOME"], std::ffi::OsStr::new("/Users/tester"));
        assert_eq!(child["USER"], std::ffi::OsStr::new("tester"));
        for key in ["GH_TOKEN", "AWS_SECRET_ACCESS_KEY", "CX_GRAPHICS_BACKEND"] {
            assert!(!child.contains_key(key));
        }
    }

    #[test]
    fn classifies_from_observed_facts() {
        assert_eq!(classify(Some(0), "PASS", "", Some("PASS")), Class::Passed);
        assert_eq!(
            classify(Some(0), "ok", "", Some("PASS")),
            Class::MissingExpectedMarker
        );
        assert_eq!(classify(Some(0), "", "", None), Class::Passed);
        assert_eq!(classify(Some(1), "FAIL", "", None), Class::FailedExit(1));
        assert_eq!(
            classify(Some(2), "NO DEVICE hr=0x1", "", None),
            Class::NoDevice
        );
        assert_eq!(classify(Some(2), "other", "", None), Class::FailedExit(2));
        assert_eq!(
            classify(
                Some(1),
                "",
                "winewrapper.exe:error: cannot execute L\"a.exe\"",
                None
            ),
            Class::CannotExecute
        );
    }

    #[test]
    fn recognises_noise_and_backends() {
        let d = diagnostics(
            "LibClamAV Error: x\nERROR: Can't open file or directory\nwarn:  DXVK: No state cache\nfine",
        );
        assert_eq!(d.len(), 3);
        assert_eq!(d[0].0, "clamav-database-missing");
        assert!(detected_backend("info:  DXVK: cxaddon\nadapter=Apple M4").starts_with("dxvk"));
        assert!(detected_backend("adapter=AMD Compatibility Mode").starts_with("d3dmetal"));
        assert_eq!(detected_backend("nothing"), "unknown");
        assert!(
            detected_backend("warn: DXVK: state cache\nvkd3d:0040: device init\nadapter=Apple M4")
                .starts_with("vkd3d")
        );
    }

    fn stub(dir: &Path, body: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        std::fs::create_dir_all(dir).unwrap();
        let p = dir.join("wine");
        std::fs::write(&p, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
        let server = dir.join("wineserver");
        std::fs::write(&server, "#!/bin/sh\nexit 0\n").unwrap();
        std::fs::set_permissions(&server, std::fs::Permissions::from_mode(0o755)).unwrap();
        p
    }

    static NEVER: AtomicBool = AtomicBool::new(false);

    fn cfg(wine: PathBuf, timeout_ms: u64, tag: &str) -> RunConfig {
        RunConfig {
            wine,
            bottle: "t".into(),
            prefix: None,
            backend: None,
            program: "x.exe".into(),
            args: vec![],
            timeout: Duration::from_millis(timeout_ms),
            limits: memory::Limits {
                min_start_free: 0,
                min_free: 0,
                poll: Duration::from_millis(30),
                grace: Duration::from_millis(500),
            },
            expect: Some("PASS".into()),
            work_dir: std::env::temp_dir().join(format!("bxrun-{tag}-{}", std::process::id())),
            stall_timeout: None,
            settle_bound: Duration::from_millis(200),
            read_free: memory::free_percentage,
            require_gates: vec![],
            max_output: 1 << 20,
            inherit_env: false,
            env: vec![],
            keep_output: false,
            interrupted: &NEVER,
        }
    }

    // These use a stub standing in for the launcher: they prove the supervision and record
    // logic, not anything about CrossOver.
    #[test]
    fn stub_run_records_pass_frames_and_env() {
        if !cfg!(target_os = "macos") {
            return;
        }
        let dir = std::env::temp_dir().join(format!("bxrun-stub1-{}", std::process::id()));
        let w = stub(
            &dir,
            "echo \"backend=$CX_GRAPHICS_BACKEND\"; echo 'frame_ms 10'; echo 'frame_ms 20'; echo PASS",
        );
        let mut c = cfg(w, 10_000, "a");
        c.backend = Some("dxmt".into());
        let r = execute(&c, Some("test".into()));
        assert_eq!(r.class, Class::Passed);
        let j = r.json.render();
        assert!(j.contains("\"frames\": 2"), "{j}");
        assert!(j.contains("backend=dxmt"), "{j}");
        assert!(j.contains("\"backend_requested\": \"dxmt\""));
        assert!(
            !j.contains("frame_ms 10"),
            "frame lines belong in frame_stats, not the tail"
        );
        assert!(j.contains("\"gpu_wired_limit_mb\""), "{j}");
        assert!(j.contains("\"gpu_wired_source\""), "{j}");
    }

    #[test]
    fn stub_run_times_out_and_is_stopped() {
        if !cfg!(target_os = "macos") {
            return;
        }
        let dir = std::env::temp_dir().join(format!("bxrun-stub2-{}", std::process::id()));
        let w = stub(&dir, "exec /bin/sleep 30");
        let t0 = Instant::now();
        let r = execute(&cfg(w, 300, "b"), None);
        assert_eq!(r.class, Class::TimedOut);
        assert!(t0.elapsed() < Duration::from_secs(10));
    }

    #[test]
    fn stub_failure_is_classified_with_exit_code() {
        if !cfg!(target_os = "macos") {
            return;
        }
        let dir = std::env::temp_dir().join(format!("bxrun-stub3-{}", std::process::id()));
        let w = stub(&dir, "echo FAIL; exit 3");
        assert_eq!(
            execute(&cfg(w, 10_000, "c"), None).class,
            Class::FailedExit(3)
        );
    }

    #[test]
    fn silent_workload_is_stopped_as_stalled_before_the_timeout() {
        if !cfg!(target_os = "macos") {
            return;
        }
        let dir = std::env::temp_dir().join(format!("bxrun-stall1-{}", std::process::id()));
        let w = stub(&dir, "exec /bin/sleep 30");
        let mut c = cfg(w, 20_000, "d");
        c.stall_timeout = Some(Duration::from_millis(300));
        let t0 = Instant::now();
        let r = execute(&c, None);
        assert_eq!(r.class, Class::Stalled);
        assert!(
            t0.elapsed() < Duration::from_secs(5),
            "stall must fire long before the 20 s timeout"
        );
        assert!(r.json.render().contains("\"classification\": \"stalled\""));
    }

    #[test]
    fn workload_that_keeps_printing_is_not_stalled() {
        if !cfg!(target_os = "macos") {
            return;
        }
        let dir = std::env::temp_dir().join(format!("bxrun-stall2-{}", std::process::id()));
        // Prints every 0.1 s for about 1.2 s: longer than the 0.5 s stall timeout in total.
        let w = stub(
            &dir,
            "i=0; while [ $i -lt 12 ]; do echo tick; i=$((i+1)); sleep 0.1; done; echo PASS",
        );
        let mut c = cfg(w, 20_000, "e");
        c.stall_timeout = Some(Duration::from_millis(500));
        assert_eq!(execute(&c, None).class, Class::Passed);
    }

    static FREE: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(90);
    fn fake_free() -> Option<u8> {
        Some(FREE.load(std::sync::atomic::Ordering::SeqCst))
    }

    // One test owns the shared `FREE` value, so it does not race with the others.
    #[test]
    fn after_a_stop_the_run_waits_for_memory_to_return() {
        use std::sync::atomic::Ordering::SeqCst;
        if !cfg!(target_os = "macos") {
            return;
        }
        let dir = std::env::temp_dir().join(format!("bxrun-settle-{}", std::process::id()));
        let w = stub(&dir, "exec /bin/sleep 30");
        let mut c = cfg(w, 300, "f");
        c.read_free = fake_free;
        c.limits.min_start_free = 50;
        c.limits.min_free = 0; // only the timeout stops the run
        c.settle_bound = Duration::from_secs(5);

        // Memory drops 100 ms in and returns 900 ms in; the timeout stop happens at about 300 ms.
        FREE.store(90, SeqCst);
        let h = std::thread::spawn(|| {
            std::thread::sleep(Duration::from_millis(100));
            FREE.store(10, SeqCst);
            std::thread::sleep(Duration::from_millis(800));
            FREE.store(90, SeqCst);
        });
        let r = execute(&c, None);
        h.join().unwrap();
        assert_eq!(r.class, Class::TimedOut);
        let j = r.json.render();
        assert!(j.contains("\"recovered_to_start_threshold\": true"), "{j}");
        assert!(r.duration >= Duration::from_millis(300));

        // Memory never returns: the wait gives up at the bound and says so.
        FREE.store(90, SeqCst);
        let h = std::thread::spawn(|| {
            std::thread::sleep(Duration::from_millis(100));
            FREE.store(10, SeqCst);
        });
        c.settle_bound = Duration::from_millis(400);
        let t0 = Instant::now();
        let r = execute(&c, None);
        h.join().unwrap();
        assert!(t0.elapsed() < Duration::from_secs(5));
        assert!(
            r.json
                .render()
                .contains("\"recovered_to_start_threshold\": false")
        );
        FREE.store(90, SeqCst);
    }

    #[test]
    fn stopping_a_bottle_selects_it_through_wineprefix() {
        use std::os::unix::fs::PermissionsExt;
        if !cfg!(target_os = "macos") {
            return;
        }
        let dir = std::env::temp_dir().join(format!("bxrun-stop-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let log = dir.join("wineserver.log");
        let ws = dir.join("wineserver");
        std::fs::write(
            &ws,
            format!(
                "#!/bin/sh\necho \"args=$* WINEPREFIX=$WINEPREFIX\" >> '{}'\n",
                log.display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&ws, std::fs::Permissions::from_mode(0o755)).unwrap();
        stop_bottle(&dir.join("wine"), "my-bottle").unwrap();
        let got = std::fs::read_to_string(&log).unwrap();
        assert!(got.contains("args=-k"), "{got}");
        assert!(got.contains("args=-w"), "{got}");
        assert!(
            got.contains("Bottles/my-bottle") || got.contains("/my-bottle"),
            "{got}"
        );
    }

    #[test]
    fn gates_decide_even_when_the_process_exits_zero() {
        if !cfg!(target_os = "macos") {
            return;
        }
        let dir = std::env::temp_dir().join(format!("bxrun-gates-{}", std::process::id()));
        let w = stub(
            &dir,
            "echo 'gate init pass ok'; echo 'gate draw FAIL pixels'; echo PASS",
        );
        assert_eq!(
            execute(&cfg(w, 10_000, "g1"), None).class,
            Class::GateFailed
        );
        let w = stub(&dir, "echo 'gate init pass ok'; echo PASS");
        let mut c = cfg(w, 10_000, "g2");
        c.require_gates = vec!["init".into()];
        assert_eq!(execute(&c, None).class, Class::Passed);
        c.require_gates.push("draw".into());
        let r = execute(&c, None);
        assert_eq!(r.class, Class::GateFailed);
        assert!(r.json.render().contains("\"name\": \"init\""));
    }

    #[test]
    fn only_the_exact_word_pass_passes_a_gate() {
        let g = parse_gates("gate a pass x y\r\ngate b PASS\ngate c skipped\nnot a gate\ngate d");
        assert_eq!(
            g.iter()
                .map(|g| (g.name.as_str(), g.passed))
                .collect::<Vec<_>>(),
            [("a", true), ("b", false), ("c", false)]
        );
        assert_eq!(g[0].detail, "x y");
    }

    #[test]
    fn non_utf8_output_is_kept_and_runaway_output_is_stopped() {
        if !cfg!(target_os = "macos") {
            return;
        }
        let dir = std::env::temp_dir().join(format!("bxrun-bytes-{}", std::process::id()));
        let w = stub(&dir, "printf '\\377\\n'; echo PASS");
        assert_eq!(execute(&cfg(w, 10_000, "u1"), None).class, Class::Passed);
        let w = stub(&dir, "exec /usr/bin/yes");
        let mut c = cfg(w, 20_000, "u2");
        c.max_output = 4096;
        let record = execute(&c, None);
        assert_eq!(record.class, Class::OutputLimit);
        assert!(record.stdout.len() <= c.max_output as usize + 3);
        assert!(
            record
                .json
                .render()
                .contains("\"output_excerpt_truncated\": true")
        );
    }

    #[test]
    fn missing_launcher_reports_why() {
        if !cfg!(target_os = "macos") {
            return;
        }
        let r = execute(&cfg("/nonexistent/wine".into(), 1000, "m"), None);
        assert_eq!(r.class, Class::LaunchFailed);
        assert!(r.error.unwrap().contains("/nonexistent/wine"));
    }

    #[test]
    fn workload_memory_is_found_by_its_windows_command_line() {
        // Shapes copied from `ps` output observed under CrossOver 26.3.
        let ps = " 34444 0.0 /Applications/CrossOver.app/x/winewrapper.exe --run -- /Users/u/w/d3d.exe\n\
                   89960 1.0 Y:\\w\\d3d.exe --frames 128\n\
                   28316 0.0 C:\\windows\\system32\\winedevice.exe\n\
                    1000 0.0 /usr/bin/betterxen-hybrid run -- /Users/u/w/d3d.exe";
        assert_eq!(workload_rss_kb(ps, "d3d.exe"), 89960);
        assert_eq!(workload_rss_kb(ps, "other.exe"), 0);
    }

    #[test]
    fn a_zero_exit_is_not_reclassified_by_child_process_text() {
        assert_eq!(
            classify(Some(0), "PASS", "child: cannot execute", Some("PASS")),
            Class::Passed
        );
    }

    #[test]
    fn host_variables_are_withheld_unless_allowed_or_requested() {
        if !cfg!(target_os = "macos") {
            return;
        }
        // CARGO_MANIFEST_DIR is set by cargo for the test process and is not allowlisted.
        assert!(std::env::var_os("CARGO_MANIFEST_DIR").is_some());
        let dir = std::env::temp_dir().join(format!("bxrun-env-{}", std::process::id()));
        let w = stub(
            &dir,
            "echo \"manifest=${CARGO_MANIFEST_DIR:-absent} extra=${BX_EXTRA:-absent} home=${HOME:+set}\"; echo PASS",
        );
        let mut c = cfg(w, 10_000, "env");
        c.env = vec![("BX_EXTRA".into(), "yes".into())];
        let r = execute(&c, None);
        assert!(
            r.stdout.contains("manifest=absent extra=yes home=set"),
            "{}",
            r.stdout
        );
        assert!(
            r.json
                .render()
                .contains("\"environment_policy\": \"allowlist\"")
        );
        c.inherit_env = true;
        let r = execute(&c, None);
        assert!(!r.stdout.contains("manifest=absent"), "{}", r.stdout);
    }

    #[test]
    fn an_interrupt_stops_the_whole_process_group() {
        if !cfg!(target_os = "macos") {
            return;
        }
        static STOP: AtomicBool = AtomicBool::new(false);
        let dir = std::env::temp_dir().join(format!("bxrun-int-{}", std::process::id()));
        let pidfile = dir.join("grandchild.pid");
        let w = stub(
            &dir,
            &format!("/bin/sleep 30 & echo $! > '{}'; wait", pidfile.display()),
        );
        let mut c = cfg(w, 20_000, "int");
        c.interrupted = &STOP;
        let h = std::thread::spawn(|| {
            std::thread::sleep(Duration::from_millis(400));
            STOP.store(true, Ordering::SeqCst);
        });
        let t = Instant::now();
        let r = execute(&c, None);
        h.join().unwrap();
        assert_eq!(r.class, Class::Interrupted);
        assert!(t.elapsed() < Duration::from_secs(10));
        let pid: i32 = std::fs::read_to_string(pidfile)
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        std::thread::sleep(Duration::from_millis(200));
        // SAFETY: signal 0 only asks whether the process exists.
        assert_ne!(unsafe { libc::kill(pid, 0) }, 0, "grandchild survived");
        // A run requested after the interrupt does not start at all.
        assert_eq!(execute(&c, None).class, Class::Interrupted);
    }

    #[test]
    fn workload_processes_are_identified_by_windows_command_line_only() {
        let ps = "  501 Y:\\w\\d3d.exe --frames 1\n  502 /usr/bin/betterxen hybrid run -- /w/d3d.exe\n  1 C:\\x\\d3d.exe\n  503 Z:\\w\\other.exe";
        assert_eq!(workload_pids(ps, "d3d.exe"), [501]);
    }

    #[test]
    fn failed_bottle_cleanup_is_reported_and_never_certified_as_stopped() {
        let dir = std::env::temp_dir().join(format!("bx-cleanup-fail-{}", std::process::id()));
        let w = stub(&dir, "exec /bin/sleep 30");
        std::fs::write(dir.join("wineserver"), "#!/bin/sh\nexit 7\n").unwrap();
        let record = execute(&cfg(w, 100, "cleanup-fail"), None);
        assert_eq!(record.class, Class::CleanupFailed);
        assert!(record.error.unwrap().contains("cleanup -k failed"));
    }

    #[test]
    fn finishing_a_run_never_kills_an_unrelated_process_with_the_same_basename() {
        let mut unrelated = Command::new("/bin/zsh")
            .args(["-c", "exec -a 'Z:\\another-bottle\\d3d.exe' /bin/sleep 30"])
            .spawn()
            .unwrap();
        std::thread::sleep(Duration::from_millis(100));
        if !running_workload_pids("d3d.exe").contains(&(unrelated.id() as i32)) {
            let _ = unrelated.kill();
            let _ = unrelated.wait();
            panic!("negative-control process did not match the former basename cleanup heuristic");
        }
        let dir = std::env::temp_dir().join(format!("bx-unrelated-{}", std::process::id()));
        let w = stub(&dir, "echo PASS");
        let mut c = cfg(w, 10_000, "unrelated");
        c.program = "d3d.exe".into();
        c.limits.grace = Duration::from_millis(20);
        let record = execute(&c, None);
        let survived = unrelated.try_wait().unwrap().is_none();
        let _ = unrelated.kill();
        let _ = unrelated.wait();
        assert_eq!(record.class, Class::Passed);
        assert!(
            survived,
            "unrelated process was killed solely because its basename matched"
        );
    }

    #[test]
    fn bottle_directory_preference_may_use_a_tilde() {
        assert_eq!(bottle_base(Some("~/B".into()), "/h"), PathBuf::from("/h/B"));
        assert_eq!(bottle_base(Some("~".into()), "/h"), PathBuf::from("/h"));
        assert_eq!(
            bottle_base(Some("/v/B".into()), "/h"),
            PathBuf::from("/v/B")
        );
        assert!(bottle_base(None, "/h").ends_with("CrossOver/Bottles"));
    }
}

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
//! record names the backend *requested* and relies on the workload's own diagnostics
//! (adapter name, backend log lines) to show what actually ran.

use crate::frames;
use crate::json::Json;
use crate::memory;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone)]
pub struct RunConfig {
    pub wine: PathBuf,
    pub bottle: String,
    pub backend: Option<String>,
    pub program: String,
    pub args: Vec<String>,
    pub timeout: Duration,
    pub limits: memory::Limits,
    /// Text that must appear in stdout for a zero exit to count as a pass.
    pub expect: Option<String>,
    pub work_dir: PathBuf,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Class {
    Passed,
    FailedExit(i32),
    MissingExpectedMarker,
    TimedOut,
    StoppedLowMemory,
    RefusedToStart,
    /// The Windows loader did not accept the file (for example an ARM64 PE this build cannot run).
    CannotExecute,
    /// The workload reported that no graphics device could be created.
    NoDevice,
    MemoryUnreadable,
    LaunchFailed,
}

impl Class {
    pub fn name(&self) -> String {
        match self {
            Class::Passed => "passed".into(),
            Class::FailedExit(c) => format!("failed_exit_{c}"),
            Class::MissingExpectedMarker => "missing_expected_marker".into(),
            Class::TimedOut => "timed_out".into(),
            Class::StoppedLowMemory => "stopped_low_memory".into(),
            Class::RefusedToStart => "refused_to_start_low_memory".into(),
            Class::CannotExecute => "cannot_execute".into(),
            Class::NoDevice => "no_graphics_device".into(),
            Class::MemoryUnreadable => "memory_unreadable".into(),
            Class::LaunchFailed => "launch_failed".into(),
        }
    }
}

/// Classify a finished (or stopped) run from observable facts only.
pub fn classify(exit: Option<i32>, stdout: &str, stderr: &str, expect: Option<&str>) -> Class {
    if stderr.contains("cannot execute") {
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
    if output.contains("DXVK:") {
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
}

/// Sum RSS (KB) and CPU % of processes whose command line mentions the CrossOver bundle or the
/// bottle directory. Heuristic: processes not started from those paths are not counted.
fn sample_processes(needles: &[String]) -> (u64, f64) {
    let Some(out) = crate::cmdutil::run("/bin/ps", &["-axo", "rss=,pcpu=,command="]) else {
        return (0, 0.0);
    };
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
}

pub fn execute(cfg: &RunConfig, crossover_build: Option<String>) -> RunRecord {
    let t_wall = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let _ = std::fs::create_dir_all(&cfg.work_dir);
    let out_path = cfg
        .work_dir
        .join(format!("stdout-{t_wall}-{}.txt", std::process::id()));
    let err_path = cfg
        .work_dir
        .join(format!("stderr-{t_wall}-{}.txt", std::process::id()));
    let thermal_before = thermal();
    let free_start = memory::free_percentage();

    let mut result = RunRecord {
        class: Class::LaunchFailed,
        duration: Duration::ZERO,
        exit: None,
        stdout: String::new(),
        stderr: String::new(),
        samples: Samples::default(),
        json: Json::Null,
    };

    match free_start {
        None => result.class = Class::MemoryUnreadable,
        Some(f) if f < cfg.limits.min_start_free => result.class = Class::RefusedToStart,
        Some(_) => {
            let files = std::fs::File::create(&out_path)
                .and_then(|o| std::fs::File::create(&err_path).map(|e| (o, e)));
            if let Ok((o, e)) = files {
                let mut cmd = Command::new(&cfg.wine);
                cmd.arg("--bottle")
                    .arg(&cfg.bottle)
                    .arg(&cfg.program)
                    .args(&cfg.args);
                cmd.stdout(Stdio::from(o)).stderr(Stdio::from(e));
                if let Some(b) = &cfg.backend {
                    cmd.env("CX_GRAPHICS_BACKEND", b);
                }
                let started = Instant::now();
                if let Ok(mut child) = cmd.spawn() {
                    let bundle = cfg
                        .wine
                        .to_string_lossy()
                        .split("/Contents/")
                        .next()
                        .unwrap_or("")
                        .to_string();
                    let needles = vec![bundle, format!("Bottles/{}", cfg.bottle)];
                    let (mut cpu_sum, mut n) = (0f64, 0u32);
                    let mut stop: Option<Class> = None;
                    loop {
                        match child.try_wait() {
                            Ok(Some(st)) => {
                                result.exit = st.code();
                                break;
                            }
                            Ok(None) => {}
                            Err(_) => break,
                        }
                        let (rss, cpu) = sample_processes(&needles);
                        result.samples.peak_rss_kb = result.samples.peak_rss_kb.max(rss);
                        cpu_sum += cpu;
                        n += 1;
                        match memory::free_percentage() {
                            Some(f) => {
                                let m = result.samples.min_free_percent.map_or(f, |x| x.min(f));
                                result.samples.min_free_percent = Some(m);
                                if f < cfg.limits.min_free {
                                    stop = Some(Class::StoppedLowMemory);
                                }
                            }
                            None => stop = Some(Class::MemoryUnreadable),
                        }
                        if stop.is_none() && started.elapsed() > cfg.timeout {
                            stop = Some(Class::TimedOut);
                        }
                        if stop.is_some() {
                            memory::terminate(&mut child, cfg.limits.grace);
                            if let Some(dir) = cfg.wine.parent() {
                                let _ = Command::new(dir.join("wineserver"))
                                    .arg("-k")
                                    .env("CX_BOTTLE", &cfg.bottle)
                                    .status();
                            }
                            break;
                        }
                        std::thread::sleep(cfg.limits.poll);
                    }
                    result.duration = started.elapsed();
                    result.samples.samples = n;
                    result.samples.mean_cpu_percent_of_core =
                        if n > 0 { cpu_sum / n as f64 } else { 0.0 };
                    result.stdout = std::fs::read_to_string(&out_path).unwrap_or_default();
                    result.stderr = std::fs::read_to_string(&err_path).unwrap_or_default();
                    result.class = stop.unwrap_or_else(|| {
                        classify(
                            result.exit,
                            &result.stdout,
                            &result.stderr,
                            cfg.expect.as_deref(),
                        )
                    });
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
    result.json = Json::obj(vec![
        ("schema", Json::str("betterxen-hybrid-run/0")),
        ("unix_time", Json::Num(t_wall as f64)),
        ("crossover_build", Json::opt_str(crossover_build)),
        (
            "launcher_interface",
            Json::str(
                "CrossOver bin/wine --bottle (vendor --help only; not documented by CodeWeavers for macOS)",
            ),
        ),
        ("bottle", Json::str(cfg.bottle.clone())),
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
    ]);
    let _ = std::fs::remove_file(&out_path);
    let _ = std::fs::remove_file(&err_path);
    result
}

pub fn wine_path(app: &Path) -> PathBuf {
    app.join("Contents/SharedSupport/CrossOver/bin/wine")
}

#[cfg(test)]
mod tests {
    use super::*;

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
    }

    fn stub(dir: &Path, body: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        std::fs::create_dir_all(dir).unwrap();
        let p = dir.join("wine");
        std::fs::write(&p, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
        p
    }

    fn cfg(wine: PathBuf, timeout_ms: u64, tag: &str) -> RunConfig {
        RunConfig {
            wine,
            bottle: "t".into(),
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
}

//! Command-line interface of the hybrid tooling.

use crate::{crossover, frames, host, json::Json, memory, pe, preflight, run, toolchain};
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};

const USAGE: &str = "usage: betterxen hybrid <command>
  host                  host receipt (JSON)
  crossover [--app P]   CrossOver installs and documented preferences (JSON)
  toolchain             Windows cross-compilers found on PATH (JSON)
  frames <file>         frame-time statistics; file has one interval in ms per line (JSON)
  guard [--min-start-free N] [--min-free N] -- CMD ARGS...
                        run CMD; refuse to start / stop it when free memory is low
  doctor [--bottle B] [--min-start-free N]
                        check that a run could start here (JSON); non-zero exit if not
  inspect <file.exe>    identify a Windows executable: machine, subsystem, imports (JSON)
  run --bottle B [--backend d3dmetal|dxmt|dxvk|wined3d] [--timeout S] [--stall-timeout S]
      [--settle-bound S] [--expect TEXT] [--require-gates a,b,c] [--out DIR]
      [--min-start-free N] [--min-free N] -- EXE [ARGS...]
                        run a Windows program through CrossOver; write a JSON record
  bench --bottle B --backends a,b,c [--repeat N] [same options] -- EXE [ARGS...]
                        run the program per backend N times; summarise frame times
  stop --bottle B       stop every Windows process in a bottle (recovery)
  receipt               host + crossover + toolchain in one JSON document

run and bench also accept: --env NAME=VALUE (repeatable), --inherit-env true, --keep-output true.
The child otherwise receives only a short allowlist of host variables. A bottle is not a sandbox.";

/// Set by the signal handler; every run polls it.
static INTERRUPTED: AtomicBool = AtomicBool::new(false);

extern "C" fn on_signal(_: libc::c_int) {
    // Storing to an atomic is async-signal-safe.
    INTERRUPTED.store(true, Ordering::SeqCst);
}

fn install_signal_handlers() {
    let handler = on_signal as extern "C" fn(libc::c_int) as libc::sighandler_t;
    for signal in [libc::SIGINT, libc::SIGTERM, libc::SIGHUP] {
        // SAFETY: `on_signal` has the required signature and only touches an atomic.
        unsafe { libc::signal(signal, handler) };
    }
}

fn crossover_json(app: Option<PathBuf>) -> Json {
    let home = PathBuf::from(std::env::var("HOME").unwrap_or_default());
    let cands = match app {
        Some(p) => vec![p],
        None => crossover::candidate_paths(&home),
    };
    let installs = crossover::discover(&cands);
    Json::obj(vec![
        ("installed", Json::Bool(!installs.is_empty())),
        (
            "installs",
            Json::Arr(installs.iter().map(|i| i.to_json()).collect()),
        ),
        (
            "bottle_dir_preference",
            Json::opt_str(crossover::preference("BottleDir")),
        ),
        (
            "managed_bottle_dirs_preference",
            Json::opt_str(crossover::preference("ManagedBottleDirs")),
        ),
        (
            "searched",
            Json::Arr(
                cands
                    .iter()
                    .map(|p| Json::str(p.display().to_string()))
                    .collect(),
            ),
        ),
        (
            "note",
            Json::str(
                "Only Info.plist metadata, lipo -archs and documented defaults keys are read. Bottle graphics backend and bundle internals are not interpreted (undocumented, #350).",
            ),
        ),
    ])
}

fn toolchain_json() -> Json {
    let path = std::env::var("PATH").unwrap_or_default();
    Json::Arr(
        toolchain::probe(&path)
            .iter()
            .map(|c| c.to_json())
            .collect(),
    )
}

fn guard(args: &[String]) -> ExitCode {
    let mut limits = memory::Limits {
        min_start_free: 70,
        min_free: 20,
        poll: std::time::Duration::from_secs(1),
        grace: std::time::Duration::from_secs(10),
    };
    let mut i = 0;
    while i < args.len() && args[i] != "--" {
        let v = args.get(i + 1).and_then(|v| v.parse::<u8>().ok());
        match (args[i].as_str(), v) {
            ("--min-start-free", Some(v)) => limits.min_start_free = v,
            ("--min-free", Some(v)) => limits.min_free = v,
            _ => {
                eprintln!("{USAGE}");
                return ExitCode::from(2);
            }
        }
        i += 2;
    }
    let Some(cmd) = args.get(i + 1) else {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    };
    match memory::supervise(cmd, &args[i + 2..], limits, &mut memory::free_percentage) {
        memory::Outcome::Exited(Some(0)) => ExitCode::SUCCESS,
        memory::Outcome::Exited(c) => ExitCode::from(c.map_or(1, |c| c.clamp(1, 255)) as u8),
        other => {
            eprintln!("guard: {other:?}");
            ExitCode::from(3)
        }
    }
}

struct RunArgs {
    cfg: run::RunConfig,
    backends: Vec<String>,
    repeat: u32,
    out: Option<PathBuf>,
    /// What the program file is, recorded with every run.
    executable: Json,
}

fn parse_run_args(args: &[String]) -> Result<RunArgs, String> {
    let mut bottle = None;
    let mut backend = None;
    let mut backends = Vec::new();
    let mut repeat = 1u32;
    let mut timeout = 300u64;
    let mut stall = None;
    let mut settle = 60u64;
    let mut expect = None;
    let mut require_gates = Vec::new();
    let mut out = None;
    let (mut min_start, mut min_free) = (50u8, 15u8);
    let (mut inherit_env, mut keep_output) = (false, false);
    let mut env = Vec::new();
    let mut i = 0;
    while i < args.len() && args[i] != "--" {
        let v = args
            .get(i + 1)
            .ok_or_else(|| format!("{} needs a value", args[i]))?
            .clone();
        match args[i].as_str() {
            "--bottle" => bottle = Some(v),
            "--backend" => backend = Some(v),
            "--backends" => backends = v.split(',').map(String::from).collect(),
            "--repeat" => repeat = v.parse().map_err(|_| "bad --repeat")?,
            "--timeout" => timeout = v.parse().map_err(|_| "bad --timeout")?,
            "--stall-timeout" => stall = Some(v.parse().map_err(|_| "bad --stall-timeout")?),
            "--settle-bound" => settle = v.parse().map_err(|_| "bad --settle-bound")?,
            "--expect" => expect = Some(v),
            "--env" => {
                let (name, value) = v.split_once('=').ok_or("--env needs NAME=VALUE")?;
                if name.is_empty() || name.contains('\0') {
                    return Err("--env needs NAME=VALUE".into());
                }
                if name.starts_with("CX_GRAPHICS_BACKEND") {
                    return Err("select the backend with --backend, not --env".into());
                }
                if name.starts_with("DYLD_") {
                    return Err("DYLD_ variables are not passed to the launcher".into());
                }
                env.push((name.to_string(), value.to_string()));
            }
            "--inherit-env" => inherit_env = v.parse().map_err(|_| "bad --inherit-env")?,
            "--keep-output" => keep_output = v.parse().map_err(|_| "bad --keep-output")?,
            "--require-gates" => require_gates = v.split(',').map(String::from).collect(),
            "--out" => out = Some(PathBuf::from(v)),
            "--min-start-free" => min_start = v.parse().map_err(|_| "bad --min-start-free")?,
            "--min-free" => min_free = v.parse().map_err(|_| "bad --min-free")?,
            o => return Err(format!("unknown option {o}")),
        }
        i += 2;
    }
    let program = args.get(i + 1).ok_or("missing program after --")?.clone();
    let bottle: String = bottle.ok_or("--bottle is required")?;
    if !preflight::valid_bottle_name(&bottle) {
        return Err(format!("invalid bottle name {bottle:?}"));
    }
    if let Some(unknown) = backend
        .iter()
        .chain(backends.iter().filter(|b| *b != "default"))
        .find(|b| !run::BACKENDS.contains(&b.as_str()))
    {
        return Err(format!(
            "unknown backend {unknown:?} (CrossOver would ignore it silently); use one of {}",
            run::BACKENDS.join(", ")
        ));
    }
    if program.starts_with('-') {
        return Err("the program path must not start with '-' (write ./name.exe)".into());
    }
    let executable = identify(std::path::Path::new(&program))?;
    let home = PathBuf::from(std::env::var("HOME").unwrap_or_default());
    let installs = crossover::discover(&crossover::candidate_paths(&home));
    let install = installs.first().ok_or("no CrossOver installation found")?;
    let wine = run::wine_path(&install.path);
    if !wine.is_file() {
        return Err(format!(
            "{} not found in this CrossOver build",
            wine.display()
        ));
    }
    if !run::bottle_prefix(&bottle).join("drive_c").is_dir() {
        return Err(format!(
            "bottle {bottle:?} not found at {}",
            run::bottle_prefix(&bottle).display()
        ));
    }
    let work = out
        .clone()
        .unwrap_or_else(|| PathBuf::from("artifacts/hybrid-runs"));
    Ok(RunArgs {
        cfg: run::RunConfig {
            wine,
            bottle,
            backend,
            program,
            args: args[i + 2..].to_vec(),
            timeout: std::time::Duration::from_secs(timeout),
            limits: memory::Limits {
                min_start_free: min_start,
                min_free,
                poll: std::time::Duration::from_millis(250),
                grace: std::time::Duration::from_secs(10),
            },
            expect,
            work_dir: work,
            stall_timeout: stall.map(std::time::Duration::from_secs),
            settle_bound: std::time::Duration::from_secs(settle),
            read_free: memory::free_percentage,
            require_gates,
            max_output: 64 << 20,
            inherit_env,
            env,
            keep_output,
            interrupted: &INTERRUPTED,
        },
        backends,
        repeat,
        out,
        executable,
    })
}

/// Refuse anything that is not a launchable Windows program, and describe what is.
fn identify(path: &std::path::Path) -> Result<Json, String> {
    let image = pe::inspect(path)?;
    image
        .check_launchable()
        .map_err(|e| format!("{}: {e}", path.display()))?;
    let sha256 = crate::cmdutil::run(
        "/usr/bin/shasum",
        &["-a", "256", &path.display().to_string()],
    )
    .and_then(|o| o.split_whitespace().next().map(String::from));
    Ok(Json::obj(vec![
        ("path", Json::str(path.display().to_string())),
        ("sha256", Json::opt_str(sha256)),
        ("image", image.to_json()),
    ]))
}

fn with_executable(mut record: Json, executable: &Json) -> Json {
    if let Json::Obj(fields) = &mut record {
        fields.push(("executable".into(), executable.clone()));
    }
    record
}

fn doctor(args: &[String]) -> ExitCode {
    let (mut bottle, mut min_start) = (None, 50u8);
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match (a.as_str(), it.next()) {
            ("--bottle", Some(v)) => bottle = Some(v.clone()),
            ("--min-start-free", Some(v)) if v.parse::<u8>().is_ok() => {
                min_start = v.parse().unwrap_or(min_start)
            }
            _ => {
                eprintln!("{USAGE}");
                return ExitCode::from(2);
            }
        }
    }
    let home = PathBuf::from(std::env::var("HOME").unwrap_or_default());
    let install = crossover::discover(&crossover::candidate_paths(&home))
        .into_iter()
        .next();
    let wine = install.as_ref().map(|i| run::wine_path(&i.path));
    let facts = preflight::gather(
        bottle.as_deref(),
        wine.as_deref()
            .map(|w| (crossover_build().unwrap_or_else(|| "unknown".into()), w)),
        min_start,
    );
    let checks = preflight::evaluate(&facts);
    print!("{}", preflight::to_json(&checks).render());
    if checks.iter().all(|c| c.ok) {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}

fn crossover_build() -> Option<String> {
    let home = PathBuf::from(std::env::var("HOME").unwrap_or_default());
    crossover::discover(&crossover::candidate_paths(&home))
        .first()
        .and_then(|i| {
            Some(format!(
                "{} ({})",
                i.short_version.clone()?,
                i.build_version.clone()?
            ))
        })
}

fn save(dir: &std::path::Path, kind: &str, j: &Json) -> Result<(), String> {
    crate::records::save(dir, kind, j)
        .map(|path| eprintln!("report saved: {}", path.display()))
        .map_err(|error| format!("cannot save {kind} report in {}: {error}", dir.display()))
}

fn finish_report(passed: bool, out: Option<&std::path::Path>, kind: &str, j: &Json) -> ExitCode {
    print!("{}", j.render());
    if let Some(dir) = out
        && let Err(error) = save(dir, kind, j)
    {
        eprintln!("{error}");
        return ExitCode::from(1);
    }
    if passed {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}

fn run_cmd(args: &[String]) -> ExitCode {
    let a = match parse_run_args(args) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("{e}\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    let r = run::execute(&a.cfg, crossover_build());
    if r.class != run::Class::Passed {
        eprintln!("run: {}", r.class.name());
        for g in r.gates.iter().filter(|g| !g.passed) {
            eprintln!("  gate {} failed: {}", g.name, g.detail);
        }
        if let Some(e) = &r.error {
            eprintln!("  {e}");
        }
    }
    finish_report(
        r.class == run::Class::Passed,
        a.out.as_deref(),
        "run",
        &with_executable(r.json, &a.executable),
    )
}

fn bench_cmd(args: &[String]) -> ExitCode {
    let a = match parse_run_args(args) {
        Ok(a) if !a.backends.is_empty() => a,
        Ok(_) => {
            eprintln!("bench needs --backends\n{USAGE}");
            return ExitCode::from(2);
        }
        Err(e) => {
            eprintln!("{e}\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    let build = crossover_build();
    let mut per_backend = Vec::new();
    let mut all_ok = true;
    for b in &a.backends {
        let mut runs = Vec::new();
        for _ in 0..a.repeat {
            if INTERRUPTED.load(Ordering::SeqCst) {
                all_ok = false;
                break;
            }
            let mut cfg = a.cfg.clone();
            cfg.backend = if b == "default" {
                None
            } else {
                Some(b.clone())
            };
            let r = run::execute(&cfg, build.clone());
            all_ok &= r.class == run::Class::Passed;
            runs.push(with_executable(r.json, &a.executable));
        }
        per_backend.push(Json::obj(vec![
            ("backend", Json::str(b.clone())),
            ("runs", Json::Arr(runs)),
        ]));
    }
    let doc = Json::obj(vec![
        ("schema", Json::str("betterxen-hybrid-bench/0")),
        ("repeat", Json::Num(a.repeat as f64)),
        ("host", host::collect().to_json()),
        ("results", Json::Arr(per_backend)),
    ]);
    finish_report(all_ok, a.out.as_deref(), "bench", &doc)
}

fn stop(args: &[String]) -> ExitCode {
    let [flag, bottle] = args else {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    };
    if flag != "--bottle" || !preflight::valid_bottle_name(bottle) {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    }
    let home = PathBuf::from(std::env::var("HOME").unwrap_or_default());
    let Some(install) = crossover::discover(&crossover::candidate_paths(&home))
        .into_iter()
        .next()
    else {
        eprintln!("no CrossOver installation found");
        return ExitCode::from(1);
    };
    if !run::bottle_prefix(bottle).join("drive_c").is_dir() {
        eprintln!("bottle {bottle:?} not found");
        return ExitCode::from(1);
    }
    run::stop_bottle(&run::wine_path(&install.path), bottle);
    ExitCode::SUCCESS
}

/// Entry point shared by `betterxen hybrid` and the `betterxen-hybrid` binary. `args` excludes
/// the program name and the `hybrid` word.
pub fn run_cli(args: &[String]) -> ExitCode {
    let rest = args.get(1..).unwrap_or_default();
    match args.first().map(String::as_str) {
        Some("run") => {
            install_signal_handlers();
            return run_cmd(rest);
        }
        Some("bench") => {
            install_signal_handlers();
            return bench_cmd(rest);
        }
        Some("doctor") => return doctor(rest),
        Some("guard") => return guard(rest),
        Some("stop") => return stop(rest),
        _ => {}
    }
    let result: Result<Json, String> = match args.first().map(String::as_str) {
        Some("host") => Ok(host::collect().to_json()),
        Some("inspect") => match args.get(1) {
            None => Err("inspect needs a file".into()),
            Some(f) => pe::inspect(std::path::Path::new(f)).map(|i| i.to_json()),
        },
        Some("crossover") => {
            let app = match args.get(1).map(String::as_str) {
                Some("--app") => args.get(2).map(PathBuf::from),
                _ => None,
            };
            Ok(crossover_json(app))
        }
        Some("toolchain") => Ok(toolchain_json()),
        Some("frames") => match args.get(1) {
            None => Err("frames needs a file".into()),
            Some(f) => std::fs::read_to_string(f)
                .map_err(|e| format!("{f}: {e}"))
                .and_then(|t| frames::parse_intervals(&t).map_err(|e| e.to_string()))
                .and_then(|v| frames::analyze(&v).map_err(|e| e.to_string()))
                .map(|s| s.to_json()),
        },
        Some("receipt") => Ok(Json::obj(vec![
            ("schema", Json::str("betterxen-hybrid-receipt/0")),
            ("host", host::collect().to_json()),
            ("crossover", crossover_json(None)),
            ("windows_cross_compilers", toolchain_json()),
        ])),
        _ => Err(USAGE.into()),
    };
    match result {
        Ok(j) => {
            print!("{}", j.render());
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("{e}");
            ExitCode::from(2)
        }
    }
}

#[cfg(test)]
mod report_tests {
    use super::*;

    fn args(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }

    // These fail before CrossOver discovery, so they run on hosts without CrossOver.
    #[test]
    fn bad_backend_bottle_or_program_is_refused_before_launch() {
        let err = |a: &[&str]| parse_run_args(&args(a)).err().unwrap();
        assert!(
            err(&["--bottle", "b", "--backend", "dmxt", "--", "x.exe"]).contains("unknown backend")
        );
        assert!(
            err(&["--bottle", "b", "--backends", "default,typo", "--", "x.exe"])
                .contains("unknown backend")
        );
        assert!(err(&["--bottle", "../b", "--", "x.exe"]).contains("invalid bottle"));
        assert!(err(&["--bottle", "b", "--", "/nonexistent/x.exe"]).contains("/nonexistent/x.exe"));
        assert!(err(&["--bottle", "b", "--", "/bin/sh"]).contains("not a PE"));
        assert!(err(&["--bottle", "b", "--", "--help"]).contains("must not start with"));
    }

    #[test]
    fn requested_evidence_failure_overrides_a_passing_workload() {
        let path = std::env::temp_dir().join(format!(
            "bx-report-invalid-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::write(&path, "keep this file").unwrap();
        for kind in ["run", "bench"] {
            assert_eq!(
                finish_report(true, Some(&path), kind, &Json::Null),
                ExitCode::from(1)
            );
        }
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "keep this file");
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn stdout_only_reporting_preserves_workload_status() {
        assert_eq!(
            finish_report(true, None, "run", &Json::Null),
            ExitCode::SUCCESS
        );
        assert_eq!(
            finish_report(false, None, "bench", &Json::Null),
            ExitCode::from(1)
        );
    }
}

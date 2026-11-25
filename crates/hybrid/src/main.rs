use betterxen_hybrid::{crossover, frames, host, json::Json, memory, run, toolchain};
use std::path::PathBuf;
use std::process::ExitCode;

const USAGE: &str = "usage: betterxen-hybrid <command>
  host                  host receipt (JSON)
  crossover [--app P]   CrossOver installs and documented preferences (JSON)
  toolchain             Windows cross-compilers found on PATH (JSON)
  frames <file>         frame-time statistics; file has one interval in ms per line (JSON)
  guard [--min-start-free N] [--min-free N] -- CMD ARGS...
                        run CMD; refuse to start / stop it when free memory is low
  run --bottle B [--backend X] [--timeout S] [--stall-timeout S] [--settle-bound S] [--expect TEXT] [--out DIR]
      [--min-start-free N] [--min-free N] -- EXE [ARGS...]
                        run a Windows program through CrossOver; write a JSON record
  bench --bottle B --backends a,b,c [--repeat N] [same options] -- EXE [ARGS...]
                        run the program per backend N times; summarise frame times
  receipt               host + crossover + toolchain in one JSON document";

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
    let mut out = None;
    let (mut min_start, mut min_free) = (50u8, 15u8);
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
            "--out" => out = Some(PathBuf::from(v)),
            "--min-start-free" => min_start = v.parse().map_err(|_| "bad --min-start-free")?,
            "--min-free" => min_free = v.parse().map_err(|_| "bad --min-free")?,
            o => return Err(format!("unknown option {o}")),
        }
        i += 2;
    }
    let program = args.get(i + 1).ok_or("missing program after --")?.clone();
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
    let work = out
        .clone()
        .unwrap_or_else(|| PathBuf::from("artifacts/hybrid-runs"));
    Ok(RunArgs {
        cfg: run::RunConfig {
            wine,
            bottle: bottle.ok_or("--bottle is required")?,
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
        },
        backends,
        repeat,
        out,
    })
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

fn save(dir: &std::path::Path, name: &str, j: &Json) {
    let _ = std::fs::create_dir_all(dir);
    let _ = std::fs::write(dir.join(name), j.render());
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
    print!("{}", r.json.render());
    if let Some(dir) = &a.out {
        save(
            dir,
            &format!(
                "run-{}.json",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0, |d| d.as_secs())
            ),
            &r.json,
        );
    }
    if r.class == run::Class::Passed {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
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
            let mut cfg = a.cfg.clone();
            cfg.backend = if b == "default" {
                None
            } else {
                Some(b.clone())
            };
            let r = run::execute(&cfg, build.clone());
            all_ok &= r.class == run::Class::Passed;
            runs.push(r.json);
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
    print!("{}", doc.render());
    if let Some(dir) = &a.out {
        save(dir, "bench.json", &doc);
    }
    if all_ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}

fn main() -> ExitCode {
    match std::env::args().nth(1).as_deref() {
        Some("run") => return run_cmd(&std::env::args().skip(2).collect::<Vec<_>>()),
        Some("bench") => return bench_cmd(&std::env::args().skip(2).collect::<Vec<_>>()),
        _ => {}
    }
    if std::env::args().nth(1).as_deref() == Some("guard") {
        return guard(&std::env::args().skip(2).collect::<Vec<_>>());
    }
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result: Result<Json, String> = match args.first().map(String::as_str) {
        Some("host") => Ok(host::collect().to_json()),
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

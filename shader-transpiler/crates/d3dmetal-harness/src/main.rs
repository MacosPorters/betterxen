//! `bx-d3dmetal-harness`: run the D3DMetal isolation probe and report a verdict.
//!
//! Exit status: 0 PASS (or `status` succeeded), 1 FAIL or error, 2 usage,
//! 77 SKIP (no D3DMetal.framework available).

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

use d3dmetal_harness::framework::{self, Discovery};
use d3dmetal_harness::{Evaluation, Framework, binary_archs, evaluate};

const EXIT_FAIL: u8 = 1;
const EXIT_USAGE: u8 = 2;
const EXIT_SKIP: u8 = 77;
const DEFAULT_TIMEOUT_SECS: u64 = 60;

const USAGE: &str = "\
usage: bx-d3dmetal-harness status [--framework <path>]
       bx-d3dmetal-harness run --probe <bx-d3dmetal-probe> [--framework <path>] [--timeout <secs>]

The framework is taken from --framework, then $D3DMETAL_FRAMEWORK_PATH, then CrossOver's bundled copy.
Build the probe with shader-transpiler/crates/d3dmetal-harness/probe/build-probe.sh.";

struct Options {
    probe: Option<PathBuf>,
    framework: Option<PathBuf>,
    timeout: Duration,
}

fn parse_options(args: &[String]) -> Result<Options, String> {
    let mut options = Options {
        probe: None,
        framework: None,
        timeout: Duration::from_secs(DEFAULT_TIMEOUT_SECS),
    };
    let mut iter = args.iter();
    while let Some(flag) = iter.next() {
        let mut value = || iter.next().ok_or_else(|| format!("{flag} needs a value"));
        match flag.as_str() {
            "--probe" => options.probe = Some(PathBuf::from(value()?)),
            "--framework" => options.framework = Some(PathBuf::from(value()?)),
            "--timeout" => {
                let secs: u64 = value()?
                    .parse()
                    .map_err(|_| "--timeout takes whole seconds".to_owned())?;
                if secs == 0 {
                    return Err("--timeout must be at least 1 second".to_owned());
                }
                options.timeout = Duration::from_secs(secs);
            }
            other => return Err(format!("unknown argument '{other}'")),
        }
    }
    Ok(options)
}

/// Resolves the framework, or returns the exit code for a skip or error.
fn find_framework(explicit: Option<&Path>) -> Result<Framework, ExitCode> {
    match framework::discover_on_host(explicit) {
        Discovery::Found(fw) => Ok(fw),
        Discovery::Unresolved { path, source } => {
            eprintln!(
                "error: no D3DMetal binary at {} (from {source})",
                path.display()
            );
            Err(ExitCode::from(EXIT_FAIL))
        }
        Discovery::Absent => {
            println!(
                "SKIP: no D3DMetal.framework (set --framework or ${}, or install CrossOver)",
                framework::FRAMEWORK_ENV
            );
            Err(ExitCode::from(EXIT_SKIP))
        }
    }
}

fn status(options: &Options) -> ExitCode {
    println!("host arch : {}", std::env::consts::ARCH);
    let fw = match find_framework(options.framework.as_deref()) {
        Ok(fw) => fw,
        Err(code) => return code,
    };
    println!("framework : {} ({})", fw.binary.display(), fw.source);
    match binary_archs(&fw.binary) {
        Ok(archs) => {
            let archs: Vec<String> = archs.iter().map(ToString::to_string).collect();
            println!("archs     : {}", archs.join(", "));
            println!("note      : the x86_64 probe hosts D3DMetal under Rosetta 2");
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("error: {err}");
            ExitCode::from(EXIT_FAIL)
        }
    }
}

fn print_evaluation(fw: &Framework, eval: &Evaluation) {
    let archs: Vec<String> = eval
        .framework_archs
        .iter()
        .map(ToString::to_string)
        .collect();
    println!(
        "framework   : {} ({}; {})",
        fw.binary.display(),
        fw.source,
        archs.join(", ")
    );
    if let Some(sm) = eval.report.shader_model {
        println!("shader model: {}.{} (highest reported)", sm >> 4, sm & 0xf);
    }
    let failed = eval.report.steps.iter().filter(|s| s.failed()).count();
    println!(
        "d3d12 steps : {} run, {failed} failed",
        eval.report.steps.len()
    );
    match eval.exit_status {
        Some(status) => println!("probe exit  : {status}"),
        None => println!("probe exit  : killed after timeout"),
    }
    if !eval.verdict.is_pass() && !eval.stderr.trim().is_empty() {
        println!("probe stderr:");
        for line in eval.stderr.lines() {
            println!("  {line}");
        }
    }
    println!("{}", eval.verdict);
}

fn run(options: &Options) -> ExitCode {
    let Some(probe) = options.probe.as_deref() else {
        eprintln!("error: run needs --probe\n\n{USAGE}");
        return ExitCode::from(EXIT_USAGE);
    };
    let fw = match find_framework(options.framework.as_deref()) {
        Ok(fw) => fw,
        Err(code) => return code,
    };
    match evaluate(probe, &fw, options.timeout) {
        Ok(eval) => {
            print_evaluation(&fw, &eval);
            if eval.verdict.is_pass() {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(EXIT_FAIL)
            }
        }
        Err(err) => {
            eprintln!("error: {err}");
            ExitCode::from(EXIT_FAIL)
        }
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some((command, rest)) = args.split_first() else {
        eprintln!("{USAGE}");
        return ExitCode::from(EXIT_USAGE);
    };
    let options = match parse_options(rest) {
        Ok(options) => options,
        Err(message) => {
            eprintln!("error: {message}\n\n{USAGE}");
            return ExitCode::from(EXIT_USAGE);
        }
    };
    match command.as_str() {
        "status" => status(&options),
        "run" => run(&options),
        "-h" | "--help" | "help" => {
            println!("{USAGE}");
            ExitCode::SUCCESS
        }
        other => {
            eprintln!("error: unknown command '{other}'\n\n{USAGE}");
            ExitCode::from(EXIT_USAGE)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn parses_run_options() {
        let o = parse_options(&args(&[
            "--probe",
            "p",
            "--framework",
            "f",
            "--timeout",
            "5",
        ]))
        .unwrap();
        assert_eq!(o.probe.as_deref(), Some(Path::new("p")));
        assert_eq!(o.framework.as_deref(), Some(Path::new("f")));
        assert_eq!(o.timeout, Duration::from_secs(5));
    }

    #[test]
    fn rejects_bad_options() {
        assert!(parse_options(&args(&["--probe"])).is_err());
        assert!(parse_options(&args(&["--timeout", "0"])).is_err());
        assert!(parse_options(&args(&["--timeout", "x"])).is_err());
        assert!(parse_options(&args(&["--bogus"])).is_err());
    }
}

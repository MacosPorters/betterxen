//! Track A (M1-A1): Native ARM64 macOS CLI for BetterXen hybrid execution.

use std::collections::BTreeMap;
use std::env;
use std::fmt::Write as _;
use std::fs;
use std::process::ExitCode;

use bx_probe::{ShimTrack, classify_dll, probe_pe32_plus};
use bx_telemetry::summarize_jsonl;
use dxbc_accc::inspect_dxbc;

fn print_usage() {
    eprintln!(
        "BetterXen Hybrid CLI (bx-cli)\n\
         \n\
         Usage:\n\
           bx-cli doctor                         Inspect Apple Silicon host, Rosetta 2, CrossOver & mingw-w64\n\
           bx-cli probe <pe32+_path>             Classify PE32+ imports by BetterXen track (A-E)\n\
           bx-cli shader <dxbc_blob_path>        Classify a DXBC shader container (DXIL vs ACCC/FSPX)\n\
           bx-cli trace <log.jsonl>              Summarize bx_runtime_veh JSONL diagnostics\n\
           bx-cli run --exe <path> (--bottle <name> | --prefix <dir>) [--shims <dir>] [--profile lockhart|anaconda] [--log <path>] [supervisor options]\n\
           bx-cli run --help                    Show supervised runtime commands and options\n"
    );
}

fn cmd_probe(path: &str) -> ExitCode {
    let data = match fs::read(path) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("error reading {path}: {e}");
            return ExitCode::FAILURE;
        }
    };
    let summary = match probe_pe32_plus(&data) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };
    let total_syms: usize = summary.imports.values().map(Vec::len).sum();
    println!("ImageBase        : 0x{:x}", summary.image_base);
    println!("SizeOfImage      : {} bytes", summary.size_of_image);
    println!("EntryPoint RVA   : 0x{:x}", summary.entry_point_rva);
    println!("Sections         : {}", summary.section_count);
    println!(
        "Static Imports   : {} DLLs, {} symbols",
        summary.imports.len(),
        total_syms
    );

    let mut by_track: BTreeMap<ShimTrack, Vec<(&String, usize)>> = BTreeMap::new();
    for (dll, syms) in &summary.imports {
        by_track
            .entry(classify_dll(dll))
            .or_default()
            .push((dll, syms.len()));
    }
    for (track, dlls) in by_track {
        println!("\n[{track}] ({} DLLs):", dlls.len());
        for (dll, cnt) in dlls {
            println!("  - {dll} ({cnt})");
        }
    }
    ExitCode::SUCCESS
}

fn format_clwb_line(clwb_events: usize, clwb_trap_lower_bound: Option<u64>) -> String {
    if let Some(lower_bound) = clwb_trap_lower_bound {
        format!("clwb VEH traps   : at least {lower_bound} (sampled)")
    } else if clwb_events == 0 {
        "clwb VEH traps   : 0".to_string()
    } else {
        format!("clwb VEH samples : {clwb_events}")
    }
}

fn cmd_trace(path: &str) -> ExitCode {
    let text = match fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error reading {path}: {e}");
            return ExitCode::FAILURE;
        }
    };
    print!("{}", render_trace(&text));
    ExitCode::SUCCESS
}

fn render_trace(text: &str) -> String {
    let mut out = String::new();
    let summary = summarize_jsonl(text);
    let _ = writeln!(out, "Trace events     : {}", summary.total_lines);
    let _ = writeln!(
        out,
        "{}",
        format_clwb_line(summary.clwb_events, summary.clwb_trap_lower_bound)
    );
    let _ = writeln!(out, "ADX illegal ops  : {}", summary.adx_errors);
    if !summary.unhandled_exceptions.is_empty() {
        let _ = writeln!(
            out,
            "\nUnhandled exceptions ({}):",
            summary.unhandled_exceptions.len()
        );
        for crash in summary.unhandled_exceptions {
            let _ = writeln!(
                out,
                "  code={} module_base={} + RVA={} RIP={}",
                crash.exception_code.as_deref().unwrap_or("unknown"),
                crash.module_base.as_deref().unwrap_or("unknown"),
                crash.rva.as_deref().unwrap_or("unknown"),
                crash.rip.as_deref().unwrap_or("unknown"),
            );
        }
    }
    if !summary.unimpl_apis.is_empty() {
        let _ = writeln!(out, "\nUnimplemented APIs:");
        for (api, count) in summary.unimpl_apis {
            let _ = writeln!(out, "  {count:5}x  {api}");
        }
    }
    if !summary.unimpl_vtables.is_empty() {
        let _ = writeln!(out, "\nUnimplemented COM vtable slots:");
        for (slot, count) in summary.unimpl_vtables {
            let _ = writeln!(out, "  {count:5}x  {slot}");
        }
    }
    out
}

/// The `--log` value of a `run` invocation, ignoring anything after `--` (guest arguments).
fn run_log_path(args: &[String]) -> Option<&str> {
    let args = match args.first().map(String::as_str) {
        Some("hybrid") => &args[1..],
        _ => args,
    };
    if args.first().map(String::as_str) != Some("run") {
        return None;
    }
    let mut options = args.iter().take_while(|a| *a != "--");
    options.find(|a| *a == "--log")?;
    options.next().map(String::as_str)
}

/// Runs the supervised engine and, when `--log` grew during the run, prints the log's summary
/// to stderr so stdout stays a single JSON record (#408).
fn cmd_runtime(args: &[String]) -> ExitCode {
    let log = run_log_path(args);
    let size = |path: &str| fs::metadata(path).map_or(0, |m| m.len());
    let before = log.map_or(0, size);
    let code = bx_runtime::cli::run_cli(args);
    if let Some(path) = log
        && size(path) > before
        && let Ok(bytes) = fs::read(path)
    {
        // Guest text may not be valid UTF-8; a summary is still worth printing.
        eprint!(
            "\nTrace summary of {path} (whole log):\n{}",
            render_trace(&String::from_utf8_lossy(&bytes))
        );
    }
    code
}

fn cmd_shader(path: &str) -> ExitCode {
    let data = match fs::read(path) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("error reading {path}: {e}");
            return ExitCode::FAILURE;
        }
    };
    match inspect_dxbc(&data) {
        Ok(info) => {
            println!("Kind             : {:?}", info.kind);
            println!(
                "Stage / SM       : {:?} ({}.{})",
                info.stage, info.shader_model_major, info.shader_model_minor
            );
            println!("Container size   : {} bytes", info.container_size);
            println!("DXIL chunk       : {} bytes", info.dxil_size);
            println!(
                "ACCC chunk       : {} bytes (FSPX={})",
                info.accc_size, info.accc_is_fspx
            );
            println!("Root signature   : {}", info.has_root_signature);
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let Some(sub) = args.first() else {
        print_usage();
        return ExitCode::SUCCESS;
    };
    match sub.as_str() {
        "--help" | "help" => {
            print_usage();
            ExitCode::SUCCESS
        }
        "--version" => {
            println!("bx-cli {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        "doctor" | "host" | "receipt" | "crossover" | "toolchain" | "inspect" | "frames"
        | "guard" | "run" | "bench" | "stop" | "hybrid" => cmd_runtime(&args),
        "probe" if args.len() >= 2 => cmd_probe(&args[1]),
        "shader" if args.len() >= 2 => cmd_shader(&args[1]),
        "trace" if args.len() >= 2 => cmd_trace(&args[1]),
        _ => {
            print_usage();
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_run_log_but_not_guest_arguments() {
        let args = |list: &[&str]| list.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let run = args(&["run", "--bottle", "b", "--log", "x.jsonl", "--exe", "a.exe"]);
        assert_eq!(run_log_path(&run), Some("x.jsonl"));
        let hybrid = args(&["hybrid", "run", "--log", "y.jsonl"]);
        assert_eq!(run_log_path(&hybrid), Some("y.jsonl"));
        let guest = args(&["run", "--bottle", "b", "--", "a.exe", "--log", "z"]);
        assert_eq!(run_log_path(&guest), None);
        assert_eq!(run_log_path(&args(&["bench", "--log", "x"])), None);
    }

    #[test]
    fn renders_a_trace_summary() {
        let out = render_trace("");
        assert!(out.starts_with("Trace events     : 0\n"));
        assert!(out.contains("ADX illegal ops  : 0\n"));
    }

    #[test]
    fn formats_clwb_summary_lines() {
        assert_eq!(format_clwb_line(0, None), "clwb VEH traps   : 0");
        assert_eq!(format_clwb_line(2, None), "clwb VEH samples : 2");
        assert_eq!(
            format_clwb_line(3, Some(4)),
            "clwb VEH traps   : at least 4 (sampled)"
        );
    }
}

//! Track A (M1-A1): Native ARM64 macOS CLI for BetterXen hybrid execution.

use std::collections::BTreeMap;
use std::env;
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
    let summary = summarize_jsonl(&text);
    println!("Trace events     : {}", summary.total_lines);
    println!(
        "{}",
        format_clwb_line(summary.clwb_events, summary.clwb_trap_lower_bound)
    );
    println!("ADX illegal ops  : {}", summary.adx_errors);
    if !summary.unhandled_exceptions.is_empty() {
        println!(
            "\nUnhandled exceptions ({}):",
            summary.unhandled_exceptions.len()
        );
        for crash in summary.unhandled_exceptions {
            println!(
                "  code={} module_base={} + RVA={} RIP={}",
                crash.exception_code.as_deref().unwrap_or("unknown"),
                crash.module_base.as_deref().unwrap_or("unknown"),
                crash.rva.as_deref().unwrap_or("unknown"),
                crash.rip.as_deref().unwrap_or("unknown"),
            );
        }
    }
    if !summary.unimpl_apis.is_empty() {
        println!("\nUnimplemented APIs:");
        for (api, count) in summary.unimpl_apis {
            println!("  {count:5}x  {api}");
        }
    }
    if !summary.unimpl_vtables.is_empty() {
        println!("\nUnimplemented COM vtable slots:");
        for (slot, count) in summary.unimpl_vtables {
            println!("  {count:5}x  {slot}");
        }
    }
    ExitCode::SUCCESS
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
        | "guard" | "run" | "bench" | "stop" | "hybrid" => bx_runtime::cli::run_cli(&args),
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
    fn formats_clwb_summary_lines() {
        assert_eq!(format_clwb_line(0, None), "clwb VEH traps   : 0");
        assert_eq!(format_clwb_line(2, None), "clwb VEH samples : 2");
        assert_eq!(
            format_clwb_line(3, Some(4)),
            "clwb VEH traps   : at least 4 (sampled)"
        );
    }
}

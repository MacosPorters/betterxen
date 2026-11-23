// SPDX-License-Identifier: GPL-3.0-or-later

mod demo;

use std::process::ExitCode;

fn main() -> ExitCode {
    match std::env::args().nth(1).as_deref() {
        Some("info") => {
            println!(
                "Xenora {} ({})",
                env!("CARGO_PKG_VERSION"),
                std::env::consts::ARCH
            );
            ExitCode::SUCCESS
        }
        Some("inspect") => {
            let Some(file_path) = std::env::args().nth(2) else {
                eprintln!("usage: xenora inspect <path-to-pe-binary>");
                return ExitCode::from(2);
            };
            match xenora_loader::triage_file(&file_path) {
                Ok(report) => {
                    print!("{report}");
                    ExitCode::SUCCESS
                }
                Err(err) => {
                    eprintln!("inspect failed: {err}");
                    ExitCode::FAILURE
                }
            }
        }
        Some("demo") => match demo::run() {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("demo: {error}");
                ExitCode::FAILURE
            }
        },
        Some("probe") => {
            let args: Vec<String> = std::env::args().collect();
            let mut pe_path: Option<String> = None;
            let mut mode = "static".to_string();
            let mut report_dir = "diagnostics/gta6-candidate-e95020b2".to_string();

            let mut i = 2;
            while i < args.len() {
                match args[i].as_str() {
                    "--pe" => {
                        if i + 1 < args.len() {
                            pe_path = Some(args[i + 1].clone());
                            i += 2;
                        } else {
                            eprintln!("missing argument for --pe");
                            return ExitCode::from(2);
                        }
                    }
                    "--mode" => {
                        if i + 1 < args.len() {
                            mode = args[i + 1].clone();
                            i += 2;
                        } else {
                            eprintln!("missing argument for --mode");
                            return ExitCode::from(2);
                        }
                    }
                    "--report" => {
                        if i + 1 < args.len() {
                            report_dir = args[i + 1].clone();
                            i += 2;
                        } else {
                            eprintln!("missing argument for --report");
                            return ExitCode::from(2);
                        }
                    }
                    other => {
                        eprintln!("unknown probe argument: {other}");
                        return ExitCode::from(2);
                    }
                }
            }

            let Some(pe) = pe_path else {
                eprintln!(
                    "usage: xenora probe --pe <path> [--mode <static>] [--report <output-dir>]"
                );
                return ExitCode::from(2);
            };

            if mode != "static" {
                eprintln!("error: currently only --mode static is supported");
                return ExitCode::from(2);
            }

            let bytes = match std::fs::read(&pe) {
                Ok(b) => b,
                Err(err) => {
                    eprintln!("error reading PE binary '{pe}': {err}");
                    return ExitCode::FAILURE;
                }
            };

            let config = xenora_loader::ProbeConfig::default();
            match xenora_loader::run_probe_and_export(&bytes, &report_dir, &config) {
                Ok(report) => {
                    println!("Xenora static PE probe complete.");
                    println!("Artifacts exported to: {report_dir}");
                    println!("Sections: {}", report.parsed.sections.len());
                    println!("Relocations: {}", report.relocations.total_entries);
                    println!("TLS Callbacks: {}", report.tls.callback_targets.len());
                    println!("Imported Thunks: {}", report.imports.len());
                    if let Some(insn) = report.first_unsupported_dbt_insn {
                        println!("First DBT Blocker: RVA 0x{:08x} ({})", insn.rva, insn.notes);
                    }
                    ExitCode::SUCCESS
                }
                Err(err) => {
                    eprintln!("probe failed: {err}");
                    ExitCode::FAILURE
                }
            }
        }
        _ => {
            eprintln!("usage: xenora <info|demo|inspect|probe>");
            ExitCode::from(2)
        }
    }
}

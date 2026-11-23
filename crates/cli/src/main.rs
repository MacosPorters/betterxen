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
        _ => {
            eprintln!("usage: xenora <info|demo|inspect>");
            ExitCode::from(2)
        }
    }
}

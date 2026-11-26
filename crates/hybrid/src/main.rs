//! Standalone binary for the hybrid tooling; `betterxen hybrid` runs the same code.

fn main() -> std::process::ExitCode {
    betterxen_hybrid::cli::run_cli(&std::env::args().skip(1).collect::<Vec<_>>())
}

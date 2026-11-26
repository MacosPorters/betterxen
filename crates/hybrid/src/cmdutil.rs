//! Run a system tool and capture trimmed stdout. A missing tool or non-zero exit is `None`,
//! never a guessed value.

use std::process::Command;

pub fn run(program: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(program).args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    Some(s)
}

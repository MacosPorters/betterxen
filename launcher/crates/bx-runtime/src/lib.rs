//! BetterXen hybrid tooling: environment checks, executable identification, supervised
//! launches and measurement around an externally installed CrossOver. Nothing here claims
//! game or console compatibility.

pub mod cli;
pub mod cmdutil;
pub mod crossover;
pub mod frames;
pub mod host;
pub mod json;
pub mod memory;
pub mod pe;
pub mod preflight;
pub mod records;
pub mod run;
pub mod session;
pub mod toolchain;

/// Value of a top-level string key in flat JSON produced by `plutil -convert json`.
pub fn host_string(json: &str, key: &str) -> Option<String> {
    let needle = format!("\"{key}\"");
    let at = json.find(&needle)? + needle.len();
    let rest = json[at..]
        .trim_start()
        .strip_prefix(':')?
        .trim_start()
        .strip_prefix('"')?;
    let mut out = String::new();
    let mut chars = rest.chars();
    while let Some(c) = chars.next() {
        match c {
            '"' => return Some(out),
            '\\' => out.push(chars.next()?),
            c => out.push(c),
        }
    }
    None
}

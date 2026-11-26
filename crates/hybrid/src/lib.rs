//! BetterXen hybrid tooling: read-only inspection and measurement helpers around an
//! externally installed CrossOver. Nothing here launches a game or claims compatibility.

pub mod cmdutil;
pub mod crossover;
pub mod frames;
pub mod host;
pub mod json;
pub mod memory;
pub mod records;
pub mod run;
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

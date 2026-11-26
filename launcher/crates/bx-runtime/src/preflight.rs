//! Read-only environment checks before a run (`betterxen-hybrid doctor`).
//!
//! Each check is something observed on this host. A pass means the pieces the runner needs
//! are present; it does not mean any program will run.

use crate::json::Json;
use std::path::Path;

#[derive(Debug, Clone, PartialEq)]
pub struct Check {
    pub name: &'static str,
    pub ok: bool,
    pub detail: String,
}

#[derive(Debug, Clone)]
pub struct Facts {
    pub host_arch: &'static str,
    /// None when no CrossOver bundle was found.
    pub crossover: Option<(String, std::path::PathBuf)>,
    pub rosetta_runs_x86_64: bool,
    pub bottle: Option<(String, std::path::PathBuf)>,
    pub free_percent: Option<u8>,
    pub min_start_free: u8,
}

pub fn evaluate(f: &Facts) -> Vec<Check> {
    let mut out = vec![Check {
        name: "host_arm64",
        ok: f.host_arch == "aarch64",
        detail: f.host_arch.to_string(),
    }];
    match &f.crossover {
        None => out.push(Check {
            name: "crossover_installed",
            ok: false,
            detail: "no CrossOver bundle found; install it from CodeWeavers".into(),
        }),
        Some((version, wine)) => {
            out.push(Check {
                name: "crossover_installed",
                ok: true,
                detail: version.clone(),
            });
            for (name, path) in [
                ("wine_launcher", wine.clone()),
                ("wineserver", wine.with_file_name("wineserver")),
            ] {
                out.push(Check {
                    name,
                    ok: path.is_file(),
                    detail: path.display().to_string(),
                });
            }
        }
    }
    out.push(Check {
        name: "rosetta",
        ok: f.rosetta_runs_x86_64,
        detail: "whether an x86_64 binary executes (CrossOver 26.3's Wine is x86_64-only)".into(),
    });
    if let Some((name, dir)) = &f.bottle {
        out.push(Check {
            name: "bottle",
            ok: valid_bottle_name(name) && dir.join("drive_c").is_dir(),
            detail: dir.display().to_string(),
        });
        if dir.join("drive_c").is_dir() {
            let marker = dir.join(".betterxen-session");
            let clean = match std::fs::symlink_metadata(&marker) {
                Ok(meta) => meta.is_file() && !meta.file_type().is_symlink() && meta.len() == 0,
                Err(e) => e.kind() == std::io::ErrorKind::NotFound,
            };
            out.push(Check {
                name: "bottle_session",
                ok: clean,
                detail: if clean { "no unfinished BetterXen session".into() } else {
                    "active, unfinished or invalid session marker; SIGTERM an active runner, or use hybrid stop for abandoned sessions".into()
                },
            });
        }
    }
    out.push(match f.free_percent {
        Some(p) => Check {
            name: "free_memory",
            ok: p >= f.min_start_free,
            detail: format!("{p}% free, {}% required", f.min_start_free),
        },
        None => Check {
            name: "free_memory",
            ok: false,
            detail: "could not be read".into(),
        },
    });
    out
}

/// A bottle name becomes a path component and a command argument, so it must be neither a
/// path nor something the launcher could read as an option.
pub fn valid_bottle_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('-')
        && !name.contains(['/', '\\'])
        && !name.chars().any(char::is_control)
        && name != "."
        && name != ".."
}

pub fn rosetta_runs_x86_64() -> bool {
    crate::cmdutil::run("/usr/bin/arch", &["-x86_64", "/usr/bin/true"]).is_some()
}

pub fn gather(bottle: Option<&str>, wine: Option<(String, &Path)>, min_start_free: u8) -> Facts {
    Facts {
        host_arch: std::env::consts::ARCH,
        crossover: wine.map(|(v, p)| (v, p.to_path_buf())),
        rosetta_runs_x86_64: rosetta_runs_x86_64(),
        bottle: bottle.map(|b| (b.to_string(), crate::run::bottle_prefix(b))),
        free_percent: crate::memory::free_percentage(),
        min_start_free,
    }
}

pub fn to_json(checks: &[Check]) -> Json {
    Json::obj(vec![
        ("schema", Json::str("betterxen-hybrid-doctor/0")),
        ("ready", Json::Bool(checks.iter().all(|c| c.ok))),
        (
            "checks",
            Json::Arr(
                checks
                    .iter()
                    .map(|c| {
                        Json::obj(vec![
                            ("name", Json::str(c.name)),
                            ("ok", Json::Bool(c.ok)),
                            ("detail", Json::str(c.detail.clone())),
                        ])
                    })
                    .collect(),
            ),
        ),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn failing(checks: &[Check]) -> Vec<&str> {
        checks.iter().filter(|c| !c.ok).map(|c| c.name).collect()
    }

    #[test]
    fn missing_pieces_are_named() {
        let f = Facts {
            host_arch: "aarch64",
            crossover: None,
            rosetta_runs_x86_64: false,
            bottle: Some(("nope".into(), "/nonexistent/nope".into())),
            free_percent: Some(10),
            min_start_free: 50,
        };
        assert_eq!(
            failing(&evaluate(&f)),
            ["crossover_installed", "rosetta", "bottle", "free_memory"]
        );
    }

    #[test]
    fn complete_synthetic_environment_passes() {
        let dir = std::env::temp_dir().join(format!("bx-preflight-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("bottle/drive_c")).unwrap();
        std::fs::write(dir.join("wine"), "").unwrap();
        std::fs::write(dir.join("wineserver"), "").unwrap();
        let f = Facts {
            host_arch: "aarch64",
            crossover: Some(("1.0".into(), dir.join("wine"))),
            rosetta_runs_x86_64: true,
            bottle: Some(("bottle".into(), dir.join("bottle"))),
            free_percent: Some(80),
            min_start_free: 50,
        };
        assert!(failing(&evaluate(&f)).is_empty());
        std::fs::write(dir.join("bottle/.betterxen-session"), "pid=123").unwrap();
        assert_eq!(failing(&evaluate(&f)), ["bottle_session"]);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn bottle_names_that_are_paths_or_options_are_invalid() {
        for bad in ["", "..", ".", "a/b", "a\\b", "a\nb", "--bottle"] {
            assert!(!valid_bottle_name(bad), "{bad}");
        }
        assert!(valid_bottle_name("betterxen-test"));
    }
}

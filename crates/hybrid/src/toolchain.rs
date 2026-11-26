//! Probe Windows cross-compilers for original test programs (issue #364).
//!
//! Reports what is installed and what each compiler targets; it does not install anything.

use crate::cmdutil::run;
use crate::json::Json;

/// (name, kind). Order is the discovery order only, not a preference.
pub const CANDIDATES: &[&str] = &[
    "aarch64-w64-mingw32-clang",
    "x86_64-w64-mingw32-clang",
    "x86_64-w64-mingw32-gcc",
    "i686-w64-mingw32-gcc",
];

#[derive(Debug, Clone, PartialEq)]
pub struct Compiler {
    pub name: String,
    pub path: String,
    pub version_line: Option<String>,
    pub target_machine: Option<String>,
}

pub fn probe(search_path: &str) -> Vec<Compiler> {
    CANDIDATES
        .iter()
        .filter_map(|name| {
            let path = search_path
                .split(':')
                .map(|d| std::path::Path::new(d).join(name))
                .find(|p| p.is_file())?;
            let p = path.to_str()?.to_string();
            Some(Compiler {
                name: name.to_string(),
                version_line: run(&p, &["--version"])
                    .and_then(|s| s.lines().next().map(String::from)),
                target_machine: run(&p, &["-dumpmachine"]),
                path: p,
            })
        })
        .collect()
}

impl Compiler {
    pub fn to_json(&self) -> Json {
        Json::obj(vec![
            ("name", Json::str(self.name.clone())),
            ("path", Json::str(self.path.clone())),
            ("version", Json::opt_str(self.version_line.clone())),
            ("target_machine", Json::opt_str(self.target_machine.clone())),
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_path_finds_nothing() {
        assert!(probe("").is_empty());
        assert!(probe("/nonexistent-dir").is_empty());
    }

    #[test]
    fn finds_a_stub_compiler_and_reports_its_output() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("bxhybrid-tc-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("x86_64-w64-mingw32-gcc");
        std::fs::write(&f, "#!/bin/sh\nif [ \"$1\" = -dumpmachine ]; then echo x86_64-w64-mingw32; else echo stub 1.0; fi\n").unwrap();
        std::fs::set_permissions(&f, std::fs::Permissions::from_mode(0o755)).unwrap();
        let found = probe(dir.to_str().unwrap());
        assert_eq!(found.len(), 1);
        assert_eq!(
            found[0].target_machine.as_deref(),
            Some("x86_64-w64-mingw32")
        );
        assert_eq!(found[0].version_line.as_deref(), Some("stub 1.0"));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}

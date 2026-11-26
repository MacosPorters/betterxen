//! Locating a user-supplied `D3DMetal.framework`.
//!
//! BetterXen never bundles D3DMetal. The harness looks, in order, at an
//! explicit path, at `$D3DMETAL_FRAMEWORK_PATH` (the variable
//! d3dmetal-native also honours), and at the copy inside an installed
//! CrossOver. An explicit path or environment value that does not resolve is
//! an error, never a silent fallback to the next location.

use std::fmt;
use std::path::{Path, PathBuf};

/// Environment variable naming the framework directory or its binary.
pub const FRAMEWORK_ENV: &str = "D3DMETAL_FRAMEWORK_PATH";
/// D3DMetal shipped inside CrossOver 26.x for macOS.
pub const CROSSOVER_FRAMEWORK: &str = "/Applications/CrossOver.app/Contents/SharedSupport/CrossOver/lib64/apple_gptk/external/D3DMetal.framework";

/// Binary locations tried inside a `.framework` directory.
const BINARY_CANDIDATES: [&str; 3] = [
    "Versions/A/D3DMetal",
    "Versions/Current/D3DMetal",
    "D3DMetal",
];

/// Where the framework path came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameworkSource {
    CommandLine,
    Environment,
    CrossOver,
}

impl fmt::Display for FrameworkSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::CommandLine => "--framework",
            Self::Environment => FRAMEWORK_ENV,
            Self::CrossOver => "CrossOver bundle",
        })
    }
}

/// A resolved framework: the path as given and the Mach-O binary inside it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Framework {
    pub path: PathBuf,
    pub binary: PathBuf,
    pub source: FrameworkSource,
}

/// Result of looking for a framework.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Discovery {
    Found(Framework),
    /// A path was requested explicitly but holds no D3DMetal binary.
    Unresolved {
        path: PathBuf,
        source: FrameworkSource,
    },
    /// Nothing requested and no CrossOver copy present: the harness skips.
    Absent,
}

/// Resolves a framework directory or binary path to the binary, using
/// `is_file` to probe the filesystem.
pub fn resolve_binary(path: &Path, is_file: impl Fn(&Path) -> bool) -> Option<PathBuf> {
    if is_file(path) {
        return Some(path.to_path_buf());
    }
    BINARY_CANDIDATES
        .iter()
        .map(|rel| path.join(rel))
        .find(|candidate| is_file(candidate))
}

/// Finds a framework from an explicit path, the environment value, or the
/// CrossOver default, in that order.
pub fn discover(
    explicit: Option<&Path>,
    env_value: Option<&Path>,
    is_file: impl Fn(&Path) -> bool,
) -> Discovery {
    let requested = explicit
        .map(|p| (p, FrameworkSource::CommandLine))
        .or_else(|| env_value.map(|p| (p, FrameworkSource::Environment)));
    if let Some((path, source)) = requested {
        return match resolve_binary(path, &is_file) {
            Some(binary) => Discovery::Found(Framework {
                path: path.to_path_buf(),
                binary,
                source,
            }),
            None => Discovery::Unresolved {
                path: path.to_path_buf(),
                source,
            },
        };
    }
    let crossover = Path::new(CROSSOVER_FRAMEWORK);
    match resolve_binary(crossover, &is_file) {
        Some(binary) => Discovery::Found(Framework {
            path: crossover.to_path_buf(),
            binary,
            source: FrameworkSource::CrossOver,
        }),
        None => Discovery::Absent,
    }
}

/// [`discover`] against the real filesystem and process environment.
pub fn discover_on_host(explicit: Option<&Path>) -> Discovery {
    let env_value = std::env::var_os(FRAMEWORK_ENV)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from);
    discover(explicit, env_value.as_deref(), Path::is_file)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn files(present: &'static [&'static str]) -> impl Fn(&Path) -> bool {
        move |p| present.iter().any(|f| Path::new(f) == p)
    }

    const CROSSOVER_BINARY: &str = "/Applications/CrossOver.app/Contents/SharedSupport/CrossOver/lib64/apple_gptk/external/D3DMetal.framework/Versions/A/D3DMetal";

    #[test]
    fn resolves_directory_or_binary() {
        let fs = files(&["/sdk/D3DMetal.framework/Versions/A/D3DMetal"]);
        let dir = Path::new("/sdk/D3DMetal.framework");
        let bin = Path::new("/sdk/D3DMetal.framework/Versions/A/D3DMetal");
        assert_eq!(resolve_binary(dir, &fs).as_deref(), Some(bin));
        assert_eq!(resolve_binary(bin, &fs).as_deref(), Some(bin));
        assert_eq!(resolve_binary(Path::new("/elsewhere"), &fs), None);
    }

    #[test]
    fn explicit_path_wins_over_environment_and_crossover() {
        let fs = files(&["/a/D3DMetal", "/b/D3DMetal", CROSSOVER_BINARY]);
        let found = discover(Some(Path::new("/a")), Some(Path::new("/b")), fs);
        let Discovery::Found(fw) = found else {
            panic!("expected Found, got {found:?}")
        };
        assert_eq!(fw.source, FrameworkSource::CommandLine);
        assert_eq!(fw.binary, Path::new("/a/D3DMetal"));
    }

    #[test]
    fn environment_wins_over_crossover() {
        let fs = files(&["/b/D3DMetal", CROSSOVER_BINARY]);
        let found = discover(None, Some(Path::new("/b")), fs);
        let Discovery::Found(fw) = found else {
            panic!("expected Found, got {found:?}")
        };
        assert_eq!(fw.source, FrameworkSource::Environment);
    }

    #[test]
    fn unresolved_request_does_not_fall_back() {
        let fs = files(&[CROSSOVER_BINARY]);
        assert_eq!(
            discover(None, Some(Path::new("/missing")), fs),
            Discovery::Unresolved {
                path: PathBuf::from("/missing"),
                source: FrameworkSource::Environment
            }
        );
    }

    #[test]
    fn crossover_default_then_absent() {
        let found = discover(None, None, files(&[CROSSOVER_BINARY]));
        let Discovery::Found(fw) = found else {
            panic!("expected Found, got {found:?}")
        };
        assert_eq!(fw.source, FrameworkSource::CrossOver);
        assert_eq!(fw.binary, Path::new(CROSSOVER_BINARY));
        assert_eq!(discover(None, None, files(&[])), Discovery::Absent);
    }
}

//! Discover installed CrossOver bundles and read standard bundle metadata.
//!
//! Only documented or standard macOS interfaces are used: the app bundle's `Info.plist`
//! (read through `plutil`), `lipo -archs` on the bundle's main executable, and the
//! `defaults` keys CodeWeavers documents in "Advanced CrossOver Mac Configuration"
//! (`BottleDir`, `ManagedBottleDirs` in `com.codeweavers.CrossOver`). Bundle internals such
//! as `Contents/SharedSupport` are NOT interpreted: they are undocumented and not a stable
//! interface (#350). The graphics backend of a bottle is deliberately not read.

use crate::cmdutil::run;
use crate::json::Json;
use std::path::{Path, PathBuf};

pub const BUNDLE_ID: &str = "com.codeweavers.CrossOver";

#[derive(Debug, Clone, PartialEq)]
pub struct Install {
    pub path: PathBuf,
    pub bundle_id: Option<String>,
    pub short_version: Option<String>,
    pub build_version: Option<String>,
    pub executable_archs: Option<Vec<String>>,
    pub minimum_macos: Option<String>,
}

/// Candidate bundle locations: standard application directories. Spotlight is consulted too.
pub fn candidate_paths(home: &Path) -> Vec<PathBuf> {
    let mut v = vec![
        PathBuf::from("/Applications/CrossOver.app"),
        home.join("Applications/CrossOver.app"),
    ];
    if let Some(found) = run(
        "/usr/bin/mdfind",
        &[&format!("kMDItemCFBundleIdentifier == '{BUNDLE_ID}'")],
    ) {
        for line in found.lines().filter(|l| !l.is_empty()) {
            let p = PathBuf::from(line);
            if !v.contains(&p) {
                v.push(p);
            }
        }
    }
    v
}

pub fn discover(candidates: &[PathBuf]) -> Vec<Install> {
    candidates.iter().filter_map(|p| inspect(p)).collect()
}

/// Inspect one bundle. Returns None if the path has no readable `Contents/Info.plist`.
pub fn inspect(app: &Path) -> Option<Install> {
    let plist = app.join("Contents/Info.plist");
    if !plist.is_file() {
        return None;
    }
    let json = run(
        "/usr/bin/plutil",
        &["-convert", "json", "-o", "-", plist.to_str()?],
    )?;
    let exe = crate::host_string(&json, "CFBundleExecutable");
    let archs = exe.and_then(|e| {
        let p = app.join("Contents/MacOS").join(e);
        run("/usr/bin/lipo", &["-archs", p.to_str()?])
            .map(|s| s.split_whitespace().map(String::from).collect())
    });
    Some(Install {
        path: app.to_path_buf(),
        bundle_id: crate::host_string(&json, "CFBundleIdentifier"),
        short_version: crate::host_string(&json, "CFBundleShortVersionString"),
        build_version: crate::host_string(&json, "CFBundleVersion"),
        executable_archs: archs,
        minimum_macos: crate::host_string(&json, "LSMinimumSystemVersion"),
    })
}

/// A documented CrossOver preference, or None when unset or unreadable.
pub fn preference(key: &str) -> Option<String> {
    run("/usr/bin/defaults", &["read", BUNDLE_ID, key])
}

impl Install {
    pub fn to_json(&self) -> Json {
        Json::obj(vec![
            ("path", Json::str(self.path.display().to_string())),
            ("bundle_id", Json::opt_str(self.bundle_id.clone())),
            ("short_version", Json::opt_str(self.short_version.clone())),
            ("build_version", Json::opt_str(self.build_version.clone())),
            (
                "executable_archs",
                self.executable_archs.as_ref().map_or(Json::Null, |a| {
                    Json::Arr(a.iter().map(|s| Json::str(s.clone())).collect())
                }),
            ),
            ("minimum_macos", Json::opt_str(self.minimum_macos.clone())),
            (
                "bundle_id_is_codeweavers_crossover",
                Json::Bool(self.bundle_id.as_deref() == Some(BUNDLE_ID)),
            ),
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    // A synthetic bundle written by the test: it proves the metadata path, not that
    // any real CrossOver build has this layout.
    fn fake_bundle(dir: &Path) -> PathBuf {
        let app = dir.join("CrossOver.app");
        fs::create_dir_all(app.join("Contents/MacOS")).unwrap();
        fs::write(
            app.join("Contents/Info.plist"),
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleExecutable</key><string>CrossOver</string>
<key>CFBundleIdentifier</key><string>com.codeweavers.CrossOver</string>
<key>CFBundleShortVersionString</key><string>99.1.2</string>
<key>CFBundleVersion</key><string>12345</string>
<key>LSMinimumSystemVersion</key><string>14.0</string>
</dict></plist>"#,
        )
        .unwrap();
        // Copy a real system executable so `lipo -archs` has a Mach-O to read.
        fs::copy("/usr/bin/true", app.join("Contents/MacOS/CrossOver")).unwrap();
        app
    }

    #[test]
    fn reads_bundle_metadata() {
        if !cfg!(target_os = "macos") {
            return;
        }
        let tmp = std::env::temp_dir().join(format!("bxhybrid-co-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();
        let app = fake_bundle(&tmp);
        let i = inspect(&app).expect("inspect");
        assert_eq!(i.short_version.as_deref(), Some("99.1.2"));
        assert_eq!(i.build_version.as_deref(), Some("12345"));
        assert_eq!(i.minimum_macos.as_deref(), Some("14.0"));
        assert_eq!(i.bundle_id.as_deref(), Some(BUNDLE_ID));
        assert!(
            i.executable_archs.as_ref().is_some_and(|a| !a.is_empty()),
            "{:?}",
            i.executable_archs
        );
        assert_eq!(discover(&[app.clone(), tmp.join("Missing.app")]).len(), 1);
        fs::remove_dir_all(&tmp).unwrap();
    }

    #[test]
    fn missing_or_plistless_bundle_is_none() {
        let tmp = std::env::temp_dir().join(format!("bxhybrid-co2-{}", std::process::id()));
        fs::create_dir_all(tmp.join("X.app/Contents")).unwrap();
        assert!(inspect(&tmp.join("X.app")).is_none());
        assert!(inspect(&tmp.join("Nope.app")).is_none());
        fs::remove_dir_all(&tmp).unwrap();
    }
}

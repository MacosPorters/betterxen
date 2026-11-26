//! Host receipt: what this Mac is, from documented system interfaces (issue #353).
//!
//! Every field is `Option` where the probe can fail; absence is reported as null, not guessed.

use crate::cmdutil::run;
use crate::json::Json;

#[derive(Debug, Clone, PartialEq)]
pub struct HostReceipt {
    pub macos_version: Option<String>,
    pub macos_build: Option<String>,
    pub chip: Option<String>,
    pub apple_silicon: Option<bool>,
    pub memory_bytes: Option<u64>,
    pub logical_cpus: Option<u32>,
    pub performance_cores: Option<u32>,
    pub efficiency_cores: Option<u32>,
    /// True only if an x86_64 slice of a system binary actually executed.
    pub rosetta_executes_x86_64: Option<bool>,
    pub gpu_name: Option<String>,
    pub gpu_cores: Option<String>,
    pub metal_support: Option<String>,
    pub host_arch_of_this_binary: &'static str,
}

fn sysctl(name: &str) -> Option<String> {
    run("/usr/sbin/sysctl", &["-n", name])
}

pub fn collect() -> HostReceipt {
    let gpu = run(
        "/usr/sbin/system_profiler",
        &["SPDisplaysDataType", "-json"],
    )
    .and_then(|j| parse_gpu(&j));
    HostReceipt {
        macos_version: run("/usr/bin/sw_vers", &["-productVersion"]),
        macos_build: sysctl("kern.osversion"),
        chip: sysctl("machdep.cpu.brand_string"),
        apple_silicon: sysctl("hw.optional.arm64").map(|v| v == "1"),
        memory_bytes: sysctl("hw.memsize").and_then(|v| v.parse().ok()),
        logical_cpus: sysctl("hw.logicalcpu").and_then(|v| v.parse().ok()),
        performance_cores: sysctl("hw.perflevel0.physicalcpu").and_then(|v| v.parse().ok()),
        efficiency_cores: sysctl("hw.perflevel1.physicalcpu").and_then(|v| v.parse().ok()),
        // Executes the x86_64 slice of /usr/bin/true. Failure means Rosetta cannot run it
        // now; it does not distinguish "not installed" from "disabled".
        rosetta_executes_x86_64: Some(
            run("/usr/bin/arch", &["-x86_64", "/usr/bin/true"]).is_some(),
        ),
        gpu_name: gpu.as_ref().map(|g| g.0.clone()),
        gpu_cores: gpu.as_ref().and_then(|g| g.1.clone()),
        metal_support: gpu.as_ref().and_then(|g| g.2.clone()),
        host_arch_of_this_binary: std::env::consts::ARCH,
    }
}

/// Pull `sppci_model`, `sppci_cores` and `spdisplays_mtlgpufamilysupport` of the first display
/// entry from `system_profiler SPDisplaysDataType -json` output. Returns None if the model
/// key is absent. Uses a small string scan because this crate has no JSON parser dependency.
pub fn parse_gpu(json: &str) -> Option<(String, Option<String>, Option<String>)> {
    let model = string_value(json, "sppci_model")?;
    Some((
        model,
        string_value(json, "sppci_cores"),
        string_value(json, "spdisplays_mtlgpufamilysupport"),
    ))
}

fn string_value(json: &str, key: &str) -> Option<String> {
    let needle = format!("\"{key}\"");
    let at = json.find(&needle)? + needle.len();
    let rest = json[at..].trim_start().strip_prefix(':')?.trim_start();
    let rest = rest.strip_prefix('"')?;
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

impl HostReceipt {
    pub fn to_json(&self) -> Json {
        let n = |v: Option<u32>| v.map_or(Json::Null, |x| Json::Num(x as f64));
        Json::obj(vec![
            ("macos_version", Json::opt_str(self.macos_version.clone())),
            ("macos_build", Json::opt_str(self.macos_build.clone())),
            ("chip", Json::opt_str(self.chip.clone())),
            (
                "apple_silicon",
                self.apple_silicon.map_or(Json::Null, Json::Bool),
            ),
            (
                "memory_bytes",
                self.memory_bytes
                    .map_or(Json::Null, |x| Json::Num(x as f64)),
            ),
            ("logical_cpus", n(self.logical_cpus)),
            ("performance_cores", n(self.performance_cores)),
            ("efficiency_cores", n(self.efficiency_cores)),
            (
                "rosetta_executes_x86_64",
                self.rosetta_executes_x86_64.map_or(Json::Null, Json::Bool),
            ),
            ("gpu_name", Json::opt_str(self.gpu_name.clone())),
            ("gpu_cores", Json::opt_str(self.gpu_cores.clone())),
            ("metal_support", Json::opt_str(self.metal_support.clone())),
            (
                "receipt_binary_arch",
                Json::str(self.host_arch_of_this_binary),
            ),
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_gpu_fixture() {
        let j = r#"{"SPDisplaysDataType":[{"sppci_cores":"10","sppci_model":"Apple M4",
            "spdisplays_mtlgpufamilysupport":"spdisplays_metal4"}]}"#;
        let g = parse_gpu(j).unwrap();
        assert_eq!(g.0, "Apple M4");
        assert_eq!(g.1.as_deref(), Some("10"));
        assert_eq!(g.2.as_deref(), Some("spdisplays_metal4"));
    }

    #[test]
    fn missing_gpu_is_none() {
        assert!(parse_gpu("{}").is_none());
    }

    #[test]
    fn collect_runs_on_this_host() {
        let r = collect();
        if cfg!(target_os = "macos") {
            assert!(r.macos_version.is_some());
            assert!(r.memory_bytes.unwrap() > 0);
        }
        assert!(r.to_json().render().contains("receipt_binary_arch"));
    }
}

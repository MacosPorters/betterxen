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
    pub gpu_wired: GpuWired,
    pub host_arch_of_this_binary: &'static str,
}

fn sysctl(name: &str) -> Option<String> {
    run("/usr/sbin/sysctl", &["-n", name])
}

/// GPU wired-memory sysctls (issue #386), recorded as the raw integers `sysctl -n` prints.
///
/// Read only: nothing in BetterXen writes these keys. `None` means the key is absent (the
/// command failed) or its output is not an unsigned integer. A value of 0 was observed on an
/// M4 and an M5; Apple does not document what 0 means in anything reviewed for #386, so it is
/// reported as observed and never interpreted (for example as "system default" or "no limit").
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct GpuWired {
    /// `iogpu.wired_limit_mb`.
    pub limit_mb: Option<u64>,
    /// `iogpu.wired_lwm_mb`.
    pub lwm_mb: Option<u64>,
}

pub const GPU_WIRED_SOURCE: &str = "sysctl -n iogpu.wired_limit_mb, iogpu.wired_lwm_mb (read-only; raw values, meaning of 0 undocumented)";

/// Read the GPU wired-memory keys through `read`, which returns a key's trimmed output or
/// `None` when the key cannot be read. Injected so tests do not depend on this Mac.
pub fn gpu_wired_with(read: impl Fn(&str) -> Option<String>) -> GpuWired {
    let num = |key: &str| read(key).and_then(|v| v.trim().parse::<u64>().ok());
    GpuWired {
        limit_mb: num("iogpu.wired_limit_mb"),
        lwm_mb: num("iogpu.wired_lwm_mb"),
    }
}

pub fn gpu_wired() -> GpuWired {
    gpu_wired_with(sysctl)
}

impl GpuWired {
    /// The record fields shared by the host receipt and every run record.
    pub fn json_fields(&self) -> Vec<(&'static str, Json)> {
        let n = |v: Option<u64>| v.map_or(Json::Null, |x| Json::Num(x as f64));
        vec![
            ("gpu_wired_limit_mb", n(self.limit_mb)),
            ("gpu_wired_lwm_mb", n(self.lwm_mb)),
            ("gpu_wired_source", Json::str(GPU_WIRED_SOURCE)),
        ]
    }
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
        gpu_wired: gpu_wired(),
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
        let mut fields = vec![
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
        ];
        fields.extend(self.gpu_wired.json_fields());
        fields.push((
            "receipt_binary_arch",
            Json::str(self.host_arch_of_this_binary),
        ));
        Json::obj(fields)
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

    fn reader<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |key| {
            pairs
                .iter()
                .find(|(k, _)| *k == key)
                .map(|(_, v)| v.to_string())
        }
    }

    #[test]
    fn gpu_wired_values_are_reported_raw() {
        let w = gpu_wired_with(reader(&[
            ("iogpu.wired_limit_mb", "24576"),
            ("iogpu.wired_lwm_mb", "0"),
        ]));
        assert_eq!(w.limit_mb, Some(24576));
        // 0 is a value like any other, not "absent".
        assert_eq!(w.lwm_mb, Some(0));
    }

    #[test]
    fn missing_or_unparsable_gpu_wired_keys_are_null() {
        let w = gpu_wired_with(reader(&[("iogpu.wired_limit_mb", "unlimited")]));
        assert_eq!(w, GpuWired::default());
        let j = Json::obj(w.json_fields()).render();
        assert!(j.contains("\"gpu_wired_limit_mb\": null"), "{j}");
        assert!(j.contains("\"gpu_wired_lwm_mb\": null"), "{j}");
        assert!(j.contains("\"gpu_wired_source\": \"sysctl -n"), "{j}");
    }

    #[test]
    fn receipt_carries_gpu_wired_fields() {
        let mut r = collect();
        r.gpu_wired = GpuWired {
            limit_mb: Some(0),
            lwm_mb: None,
        };
        let j = r.to_json().render();
        assert!(j.contains("\"gpu_wired_limit_mb\": 0"), "{j}");
        assert!(j.contains("\"gpu_wired_lwm_mb\": null"), "{j}");
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

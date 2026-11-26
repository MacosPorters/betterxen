//! Track A / F: Structured JSONL shim trace aggregator.
//!
//! Summarizes `bx_runtime_veh.dll` logs (stub API calls, unimplemented COM vtable
//! slots, `clwb` traps, ADX illegal-instruction tripwires, and unhandled exceptions)
//! so tracks can identify guest failures and missing APIs.

use std::collections::BTreeMap;

#[derive(Debug, Default, PartialEq, Eq)]
pub struct TraceSummary {
    pub total_lines: usize,
    /// Number of `clwb_trapped` records in the log.
    pub clwb_events: usize,
    /// Highest cumulative counter in the sampled CLWB records, if available.
    pub clwb_trap_lower_bound: Option<u64>,
    pub adx_errors: usize,
    pub unhandled_exceptions: Vec<UnhandledException>,
    pub unimpl_apis: BTreeMap<String, usize>,
    pub unimpl_vtables: BTreeMap<String, usize>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct UnhandledException {
    pub exception_code: Option<String>,
    pub module_base: Option<String>,
    pub rva: Option<String>,
    pub rip: Option<String>,
}

fn extract_json_str<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    let needle = format!("\"{key}\":\"");
    let start = line.find(&needle)? + needle.len();
    let rest = &line[start..];
    let end = rest.find('"')?;
    Some(&rest[..end])
}

pub fn summarize_jsonl(input: &str) -> TraceSummary {
    let mut summary = TraceSummary::default();
    for line in input.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with('{') {
            continue;
        }
        summary.total_lines += 1;
        let event = extract_json_str(trimmed, "event").unwrap_or("");
        let detail = extract_json_str(trimmed, "detail").unwrap_or("");
        match event {
            "clwb_trapped" => {
                summary.clwb_events += 1;
                let sample = detail
                    .split_ascii_whitespace()
                    .find_map(|field| field.strip_prefix("total_traps="))
                    .and_then(|count| count.parse::<u64>().ok());
                summary.clwb_trap_lower_bound = summary.clwb_trap_lower_bound.max(sample);
            }
            "adx_illegal_instruction" => summary.adx_errors += 1,
            "unhandled_exception" => {
                summary.unhandled_exceptions.push(UnhandledException {
                    exception_code: extract_json_str(trimmed, "exception_code").map(str::to_owned),
                    module_base: extract_json_str(trimmed, "module_base").map(str::to_owned),
                    rva: extract_json_str(trimmed, "rva").map(str::to_owned),
                    rip: extract_json_str(trimmed, "rip").map(str::to_owned),
                });
            }
            "unimpl_api" => *summary.unimpl_apis.entry(detail.to_string()).or_insert(0) += 1,
            "unimpl_vtable" => {
                *summary
                    .unimpl_vtables
                    .entry(detail.to_string())
                    .or_insert(0) += 1
            }
            _ => {}
        }
    }
    summary
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aggregates_stub_and_veh_events() {
        let sample = r#"
{"ts_ms":10,"tid":1,"level":"INFO","sub":"runtime_veh","event":"clwb_trapped","detail":"rip=0x1000 len=4 total_traps=1"}
{"ts_ms":12,"tid":1,"level":"WARN","sub":"stub","event":"unimpl_api","detail":"d3d12_xs.dll!D3D12XboxCreateDevice"}
{"ts_ms":14,"tid":1,"level":"WARN","sub":"stub","event":"unimpl_api","detail":"d3d12_xs.dll!D3D12XboxCreateDevice"}
{"ts_ms":16,"tid":1,"level":"WARN","sub":"com_vtable","event":"unimpl_vtable","detail":"ID3D12Device11[44]::CreatePlacedResourceX"}
"#;
        let s = summarize_jsonl(sample);
        assert_eq!(s.total_lines, 4);
        assert_eq!(s.clwb_events, 1);
        assert_eq!(s.clwb_trap_lower_bound, Some(1));
        assert_eq!(s.adx_errors, 0);
        assert!(s.unhandled_exceptions.is_empty());
        assert_eq!(
            s.unimpl_apis.get("d3d12_xs.dll!D3D12XboxCreateDevice"),
            Some(&2)
        );
        assert_eq!(
            s.unimpl_vtables
                .get("ID3D12Device11[44]::CreatePlacedResourceX"),
            Some(&1)
        );
    }

    #[test]
    fn uses_highest_counter_from_sampled_clwb_records() {
        for (actual_traps, records, expected_bound) in [
            (1, &[1][..], 1),
            (6, &[1, 4, 2], 4),
            (100, &[1, 2, 4, 8, 16, 32, 64], 64),
        ] {
            let mut log = String::new();
            for counter in records {
                log.push_str(&format!(
                    "{{\"event\":\"clwb_trapped\",\"detail\":\"rip=0x1000 len=4 total_traps={counter}\"}}\n"
                ));
            }
            let summary = summarize_jsonl(&log);
            assert_eq!(summary.clwb_events, records.len(), "{actual_traps} traps");
            assert_eq!(
                summary.clwb_trap_lower_bound,
                Some(expected_bound),
                "{actual_traps} traps"
            );
        }
    }

    #[test]
    fn preserves_clwb_record_count_without_valid_counters() {
        let summary = summarize_jsonl(
            r#"{"event":"clwb_trapped","detail":"rip=0x1000 len=4"}
{"event":"clwb_trapped","detail":"rip=0x1000 len=4 total_traps=invalid"}
{"event":"unimpl_api","detail":"total_traps=100"}"#,
        );
        assert_eq!(summary.clwb_events, 2);
        assert_eq!(summary.clwb_trap_lower_bound, None);
    }

    #[test]
    fn collects_unhandled_exceptions_in_log_order() {
        let log = r#"
{"ts_ms":16,"tid":1,"level":"ERROR","sub":"runtime_veh","event":"unhandled_exception","exception_code":"0xc0000005","module_base":"0x0000000140000000","rva":"0x0000000000001234","rip":"0x0000000140001234","rsp":"0x000000000021fd80","registers":{"rax":"0x0123456789abcdef","rbx":"0x0000000000000000","rcx":"0x0000000000000000","rdx":"0x0000000000000000","rsi":"0x0000000000000000","rdi":"0x0000000000000000","rbp":"0x0000000000000000","r8":"0x0000000000000000","r9":"0x0000000000000000","r10":"0x0000000000000000","r11":"0x0000000000000000","r12":"0x0000000000000000","r13":"0x0000000000000000","r14":"0x0000000000000000","r15":"0x0000000000000000"}}
{"ts_ms":20,"tid":1,"level":"ERROR","sub":"runtime_veh","event":"unhandled_exception","exception_code":"0xc000001d","module_base":"0x0000000000000000","rva":"0x0000000000000000","rip":"0x0000000000410000","rsp":"0x000000000021fd80","registers":{}}
"#;
        let summary = summarize_jsonl(log);
        assert_eq!(summary.total_lines, 2);
        assert_eq!(
            summary.unhandled_exceptions,
            [
                UnhandledException {
                    exception_code: Some("0xc0000005".into()),
                    module_base: Some("0x0000000140000000".into()),
                    rva: Some("0x0000000000001234".into()),
                    rip: Some("0x0000000140001234".into()),
                },
                UnhandledException {
                    exception_code: Some("0xc000001d".into()),
                    module_base: Some("0x0000000000000000".into()),
                    rva: Some("0x0000000000000000".into()),
                    rip: Some("0x0000000000410000".into()),
                },
            ]
        );
    }
}

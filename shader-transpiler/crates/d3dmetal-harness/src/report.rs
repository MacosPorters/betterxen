//! Parsing the probe's line protocol and judging its readback.
//!
//! See `probe/bx_d3dmetal_probe.cpp` for the protocol. Lines without the
//! `BXD3DM ` prefix (for example D3DMetal's own logging) are ignored.

use std::fmt;

use crate::kernel::{self, GROUPS, SENTINEL, THREADS_PER_GROUP, WORDS};

/// Protocol version this harness understands.
pub const PROTOCOL_VERSION: u32 = 2;
const PREFIX: &str = "BXD3DM ";

/// One D3D call and the HRESULT it returned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    pub name: String,
    pub hresult: u32,
}

impl Step {
    /// `FAILED(hr)`: the severity bit is set.
    pub fn failed(&self) -> bool {
        self.hresult & 0x8000_0000 != 0
    }
}

/// Dispatch geometry the probe declares before its readback.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Dispatch {
    pub groups: u32,
    pub threads: u32,
    pub words: u32,
    pub sentinel: u32,
}

/// Everything the probe reported.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProbeReport {
    pub framework: Option<String>,
    pub steps: Vec<Step>,
    pub shader_model: Option<u8>,
    pub dispatch: Option<Dispatch>,
    pub readback: Option<Vec<u32>>,
    /// d3dmetal-native could not initialize the framework (`<call>=<code>`).
    pub init_failed: Option<String>,
    /// `GetDeviceRemovedReason()` after the fence reported device removal.
    pub device_removed: Option<u32>,
    pub timeout: Option<String>,
    pub done: bool,
}

/// A protocol line the harness could not accept.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReportError {
    pub line: usize,
    pub reason: String,
}

impl fmt::Display for ReportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "probe output line {}: {}", self.line, self.reason)
    }
}

impl std::error::Error for ReportError {}

fn hex_u32(text: &str) -> Option<u32> {
    u32::from_str_radix(text.strip_prefix("0x")?, 16).ok()
}

fn field<'a>(fields: &[&'a str], key: &str) -> Option<&'a str> {
    fields
        .iter()
        .find_map(|f| f.strip_prefix(key)?.strip_prefix('='))
}

fn parse_step(rest: &str) -> Option<Step> {
    let (name, hr) = rest.rsplit_once(' ')?;
    Some(Step {
        name: name.to_owned(),
        hresult: hex_u32(hr.strip_prefix("hr=")?)?,
    })
}

fn parse_dispatch(rest: &str) -> Option<Dispatch> {
    let fields: Vec<&str> = rest.split(' ').collect();
    Some(Dispatch {
        groups: field(&fields, "groups")?.parse().ok()?,
        threads: field(&fields, "threads")?.parse().ok()?,
        words: field(&fields, "words")?.parse().ok()?,
        sentinel: hex_u32(field(&fields, "sentinel")?)?,
    })
}

fn parse_words(hex: &str) -> Option<Vec<u32>> {
    if hex.is_empty() || hex.len() % 8 != 0 || !hex.is_ascii() {
        return None;
    }
    (0..hex.len())
        .step_by(8)
        .map(|i| u32::from_str_radix(&hex[i..i + 8], 16).ok())
        .collect()
}

/// Parses the probe's stdout.
pub fn parse_report(stdout: &str) -> Result<ProbeReport, ReportError> {
    let mut report = ProbeReport::default();
    let mut protocol_seen = false;
    for (index, raw) in stdout.lines().enumerate() {
        let line = index + 1;
        let Some(record) = raw.strip_prefix(PREFIX) else {
            continue;
        };
        let (kind, rest) = record.split_once(' ').unwrap_or((record, ""));
        let malformed = |what: &str| ReportError {
            line,
            reason: format!("malformed {what} record"),
        };
        match kind {
            "protocol" => {
                let version: u32 = rest.parse().map_err(|_| malformed("protocol"))?;
                if version != PROTOCOL_VERSION {
                    return Err(ReportError {
                        line,
                        reason: format!(
                            "probe speaks protocol {version}, harness expects {PROTOCOL_VERSION}"
                        ),
                    });
                }
                protocol_seen = true;
            }
            "framework" => report.framework = Some(rest.to_owned()),
            "step" => report
                .steps
                .push(parse_step(rest).ok_or_else(|| malformed("step"))?),
            "shader_model" => {
                let sm = hex_u32(rest).ok_or_else(|| malformed("shader_model"))?;
                report.shader_model =
                    Some(u8::try_from(sm).map_err(|_| malformed("shader_model"))?);
            }
            "dispatch" => {
                report.dispatch = Some(parse_dispatch(rest).ok_or_else(|| malformed("dispatch"))?);
            }
            "readback" => {
                report.readback = Some(parse_words(rest).ok_or_else(|| malformed("readback"))?);
            }
            "init_failed" => report.init_failed = Some(rest.to_owned()),
            "device_removed" => {
                let reason = rest
                    .strip_prefix("reason=")
                    .and_then(hex_u32)
                    .ok_or_else(|| malformed("device_removed"))?;
                report.device_removed = Some(reason);
            }
            "timeout" => report.timeout = Some(rest.to_owned()),
            "done" => report.done = true,
            _ => {
                return Err(ReportError {
                    line,
                    reason: format!("unknown record '{kind}'"),
                });
            }
        }
        if !protocol_seen {
            return Err(ReportError {
                line,
                reason: "first record must declare the protocol version".to_owned(),
            });
        }
    }
    Ok(report)
}

/// The harness's judgement of one probe run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// Every word matched the host-computed value.
    Pass,
    /// A framework was found, but d3dmetal-native could not initialize it.
    InitFailed { detail: String },
    /// A D3D call failed.
    StepFailed { step: String, hresult: u32 },
    /// The fence reported device removal before the dispatch completed.
    DeviceRemoved { reason: u32 },
    /// The GPU did not signal completion in time.
    TimedOut { detail: String },
    /// The device reported a highest shader model below the 6.6 the harness kernel
    /// requires. The field was parsed and printed before, but never judged (#567).
    UnsupportedShaderModel { reported: u8 },
    /// Every word still holds the sentinel: the kernel never wrote.
    KernelDidNotRun,
    /// The kernel wrote, but some words differ from the expected values.
    Mismatch {
        mismatched: usize,
        first_index: usize,
        got: u32,
        want: u32,
    },
    /// The report ended early or describes a different dispatch.
    Incomplete { reason: String },
}

impl Verdict {
    pub fn is_pass(&self) -> bool {
        matches!(self, Self::Pass)
    }
}

impl fmt::Display for Verdict {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Pass => write!(f, "PASS: {WORDS}/{WORDS} words match"),
            Self::InitFailed { detail } => {
                write!(
                    f,
                    "FAIL: D3DMetal framework could not be initialized ({detail})"
                )
            }
            Self::StepFailed { step, hresult } => {
                write!(f, "FAIL: {step} returned 0x{hresult:08x}")
            }
            Self::DeviceRemoved { reason } => {
                write!(
                    f,
                    "FAIL: device removed (GetDeviceRemovedReason 0x{reason:08x})"
                )
            }
            Self::TimedOut { detail } => write!(f, "FAIL: GPU timeout ({detail})"),
            Self::UnsupportedShaderModel { reported } => write!(
                f,
                "FAIL: highest shader model {}.{} is below the 6.6 the harness kernel requires",
                reported >> 4,
                reported & 0xf
            ),
            Self::KernelDidNotRun => write!(
                f,
                "FAIL: all {WORDS} words still hold the sentinel 0x{SENTINEL:08x}; the kernel did not write"
            ),
            Self::Mismatch {
                mismatched,
                first_index,
                got,
                want,
            } => write!(
                f,
                "FAIL: {mismatched}/{WORDS} words differ; first at index {first_index}: got 0x{got:08x}, want 0x{want:08x}"
            ),
            Self::Incomplete { reason } => write!(f, "FAIL: incomplete probe report ({reason})"),
        }
    }
}

/// Judges a report against the host-computed kernel values.
pub fn judge(report: &ProbeReport) -> Verdict {
    if let Some(detail) = &report.init_failed {
        return Verdict::InitFailed {
            detail: detail.clone(),
        };
    }
    if let Some(step) = report.steps.iter().find(|s| s.failed()) {
        return Verdict::StepFailed {
            step: step.name.clone(),
            hresult: step.hresult,
        };
    }
    if let Some(reason) = report.device_removed {
        return Verdict::DeviceRemoved { reason };
    }
    if let Some(detail) = &report.timeout {
        return Verdict::TimedOut {
            detail: detail.clone(),
        };
    }
    let expected_dispatch = Dispatch {
        groups: GROUPS,
        threads: THREADS_PER_GROUP,
        words: WORDS as u32,
        sentinel: SENTINEL,
    };
    let incomplete = |reason: &str| Verdict::Incomplete {
        reason: reason.to_owned(),
    };
    // Every kernel the harness runs is SM 6.6. A device that reports less, or a
    // report with no shader_model record, must not pass on a matching readback
    // alone (#567). The live Rust test asserted this; the standalone verdict did not.
    match report.shader_model {
        Some(sm) if sm < 0x66 => return Verdict::UnsupportedShaderModel { reported: sm },
        None => return incomplete("no shader_model record"),
        Some(_) => {}
    }
    match report.dispatch {
        None => return incomplete("no dispatch record"),
        Some(d) if d != expected_dispatch => {
            return incomplete("probe dispatch geometry differs from the harness kernel");
        }
        Some(_) => {}
    }
    let Some(words) = &report.readback else {
        return incomplete("no readback record");
    };
    if !report.done {
        return incomplete("probe did not finish");
    }
    if words.len() != WORDS {
        return incomplete("readback word count differs from the dispatch");
    }
    if words.iter().all(|&w| w == SENTINEL) {
        return Verdict::KernelDidNotRun;
    }
    let mut mismatches = words
        .iter()
        .enumerate()
        .filter(|&(i, &got)| got != kernel::expected_word(i as u32));
    match mismatches.next() {
        None => Verdict::Pass,
        Some((first_index, &got)) => Verdict::Mismatch {
            mismatched: 1 + mismatches.count(),
            first_index,
            got,
            want: kernel::expected_word(first_index as u32),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn output(words: &[u32]) -> String {
        let hex: String = words.iter().map(|w| format!("{w:08x}")).collect();
        format!(
            "BXD3DM protocol 2\n\
             BXD3DM framework /fw/D3DMetal\n\
             D3DMetal: unrelated log line\n\
             BXD3DM step D3D12CreateDevice hr=0x00000000\n\
             BXD3DM step CheckFeatureSupport(SHADER_MODEL) hr=0x00000000\n\
             BXD3DM shader_model 0x66\n\
             BXD3DM dispatch groups=4 threads=64 words=256 sentinel=0xa5a5a5a5\n\
             BXD3DM readback {hex}\n\
             BXD3DM done\n"
        )
    }

    fn good_words() -> Vec<u32> {
        (0..WORDS as u32).map(kernel::expected_word).collect()
    }

    #[test]
    fn parses_a_complete_report() {
        let report = parse_report(&output(&good_words())).unwrap();
        assert_eq!(report.framework.as_deref(), Some("/fw/D3DMetal"));
        assert_eq!(report.steps.len(), 2);
        assert_eq!(report.steps[1].name, "CheckFeatureSupport(SHADER_MODEL)");
        assert_eq!(report.shader_model, Some(0x66));
        assert_eq!(report.readback.as_ref().map(Vec::len), Some(WORDS));
        assert!(report.done);
        assert_eq!(judge(&report), Verdict::Pass);
    }

    #[test]
    fn a_shader_model_below_6_6_fails_even_with_a_matching_readback() {
        // The kernel is SM 6.6; the readback matching proves only that something wrote.
        for reported in ["0x65", "0x60"] {
            let lower = output(&good_words()).replace(
                "BXD3DM shader_model 0x66",
                &format!("BXD3DM shader_model {reported}"),
            );
            let report = parse_report(&lower).unwrap();
            assert_eq!(
                judge(&report),
                Verdict::UnsupportedShaderModel {
                    reported: if reported == "0x65" { 0x65 } else { 0x60 }
                }
            );
        }
        // Without the record the harness cannot say the device can run the shader.
        let absent = output(&good_words()).replace("BXD3DM shader_model 0x66\n", "");
        let report = parse_report(&absent).unwrap();
        assert_eq!(report.shader_model, None);
        assert!(matches!(judge(&report), Verdict::Incomplete { .. }));
    }

    #[test]
    fn untouched_sentinel_means_the_kernel_did_not_run() {
        let report = parse_report(&output(&[SENTINEL; WORDS])).unwrap();
        assert_eq!(judge(&report), Verdict::KernelDidNotRun);
    }

    #[test]
    fn reports_every_mismatch_and_the_first_one() {
        let mut words = good_words();
        words[7] = 0;
        words[200] ^= 1;
        let report = parse_report(&output(&words)).unwrap();
        assert_eq!(
            judge(&report),
            Verdict::Mismatch {
                mismatched: 2,
                first_index: 7,
                got: 0,
                want: kernel::expected_word(7)
            }
        );
    }

    #[test]
    fn init_failure_and_failed_steps_take_precedence() {
        let failed = parse_report(
            "BXD3DM protocol 2\nBXD3DM step CreateComputePipelineState hr=0x80070057\n",
        )
        .unwrap();
        assert_eq!(
            judge(&failed),
            Verdict::StepFailed {
                step: "CreateComputePipelineState".into(),
                hresult: 0x8007_0057
            }
        );
        let init = parse_report("BXD3DM protocol 2\nBXD3DM init_failed dmn_init=-2\n").unwrap();
        assert_eq!(
            judge(&init),
            Verdict::InitFailed {
                detail: "dmn_init=-2".into()
            }
        );
    }

    #[test]
    fn device_removal_is_reported_with_its_reason() {
        let removed = parse_report(
            "BXD3DM protocol 2\nBXD3DM step Signal hr=0x00000000\nBXD3DM device_removed reason=0x887a0006\n",
        )
        .unwrap();
        assert_eq!(
            judge(&removed),
            Verdict::DeviceRemoved {
                reason: 0x887a_0006
            }
        );
        assert!(parse_report("BXD3DM protocol 2\nBXD3DM device_removed 0x887a0006\n").is_err());
    }

    #[test]
    fn unfinished_or_foreign_dispatch_is_incomplete() {
        let truncated = output(&good_words()).replace("BXD3DM done\n", "");
        assert!(matches!(
            judge(&parse_report(&truncated).unwrap()),
            Verdict::Incomplete { .. }
        ));
        let other = output(&good_words()).replace("groups=4", "groups=8");
        assert!(matches!(
            judge(&parse_report(&other).unwrap()),
            Verdict::Incomplete { .. }
        ));
        let empty = parse_report("BXD3DM protocol 2\n").unwrap();
        assert!(matches!(judge(&empty), Verdict::Incomplete { .. }));
    }

    #[test]
    fn rejects_malformed_and_unversioned_output() {
        assert_eq!(parse_report("BXD3DM done\n").unwrap_err().line, 1);
        assert!(parse_report("BXD3DM protocol 1\n").is_err());
        assert!(parse_report("BXD3DM protocol 2\nBXD3DM skip dmn_init=-1\n").is_err());
        assert!(parse_report("BXD3DM protocol 2\nBXD3DM readback 0123\n").is_err());
        assert!(parse_report("BXD3DM protocol 2\nBXD3DM readback zzzzzzzz\n").is_err());
        assert!(parse_report("BXD3DM protocol 2\nBXD3DM step NoHr\n").is_err());
        assert!(parse_report("BXD3DM protocol 2\nBXD3DM shader_model 0x1ff\n").is_err());
        assert!(parse_report("BXD3DM protocol 2\nBXD3DM bogus 1\n").is_err());
    }
}

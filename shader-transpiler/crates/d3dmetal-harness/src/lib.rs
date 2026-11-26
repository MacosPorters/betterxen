//! Track D (M1-D2): opt-in harness that drives a user-supplied
//! `D3DMetal.framework` outside CrossOver/Wine through `utmapp/d3dmetal-native`.
//!
//! Isolating D3DMetal from Wine lets a D3D12 failure be attributed to one of
//! them. D3DMetal ships as x86_64 only, so it cannot be loaded into this arm64
//! process: an x86_64 helper (`probe/`, built separately against
//! d3dmetal-native) runs under Rosetta 2, executes a synthetic SM 6.6 DXIL
//! compute kernel, and prints what it read back. This crate locates the
//! framework, checks both binaries' architectures, runs the probe with an
//! isolated environment, and judges the readback against values computed here.
//!
//! Without a framework the harness reports SKIP; default builds and tests need
//! neither D3DMetal nor the probe. Nothing from D3DMetal is bundled.

pub mod framework;
pub mod kernel;
pub mod macho;
pub mod report;
pub mod runner;

use std::fmt;
use std::io;
use std::path::{Path, PathBuf};
use std::process::ExitStatus;
use std::time::Duration;

pub use framework::{Discovery, Framework, FrameworkSource};
pub use macho::CpuArch;
pub use report::{ProbeReport, Verdict};

/// Why an evaluation could not produce a verdict.
#[derive(Debug)]
pub enum HarnessError {
    Io {
        context: String,
        source: io::Error,
    },
    NotMachO {
        path: PathBuf,
        source: macho::MachOError,
    },
    NoX86_64Slice {
        path: PathBuf,
        archs: Vec<CpuArch>,
    },
    Report(report::ReportError),
}

impl fmt::Display for HarnessError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { context, source } => write!(f, "{context}: {source}"),
            Self::NotMachO { path, source } => write!(f, "{}: {source}", path.display()),
            Self::NoX86_64Slice { path, archs } => {
                let archs: Vec<String> = archs.iter().map(ToString::to_string).collect();
                write!(
                    f,
                    "{} has no x86_64 slice ({}); the probe hosts D3DMetal as x86_64",
                    path.display(),
                    archs.join(", ")
                )
            }
            Self::Report(err) => err.fmt(f),
        }
    }
}

impl std::error::Error for HarnessError {}

/// Architectures of the Mach-O at `path`.
pub fn binary_archs(path: &Path) -> Result<Vec<CpuArch>, HarnessError> {
    macho::read_archs(path)
        .map_err(|source| HarnessError::Io {
            context: format!("reading {}", path.display()),
            source,
        })?
        .map_err(|source| HarnessError::NotMachO {
            path: path.to_path_buf(),
            source,
        })
}

fn require_x86_64(path: &Path) -> Result<Vec<CpuArch>, HarnessError> {
    let archs = binary_archs(path)?;
    if archs.contains(&CpuArch::X86_64) {
        Ok(archs)
    } else {
        Err(HarnessError::NoX86_64Slice {
            path: path.to_path_buf(),
            archs,
        })
    }
}

/// Outcome of one probe run against one framework.
#[derive(Debug)]
pub struct Evaluation {
    pub framework_archs: Vec<CpuArch>,
    pub report: ProbeReport,
    pub verdict: Verdict,
    /// `None` when the probe was killed after the timeout.
    pub exit_status: Option<ExitStatus>,
    pub stderr: String,
}

/// Runs the probe against `framework` and judges the readback.
pub fn evaluate(
    probe: &Path,
    framework: &Framework,
    timeout: Duration,
) -> Result<Evaluation, HarnessError> {
    let framework_archs = require_x86_64(&framework.binary)?;
    require_x86_64(probe)?;
    let run = runner::run_probe(probe, &framework.binary, timeout).map_err(|source| {
        HarnessError::Io {
            context: format!("running {}", probe.display()),
            source,
        }
    })?;
    let parsed = report::parse_report(&run.stdout);
    // Output cut short by a kill or by held-open pipes may end mid-record, so
    // it is judged by how the run ended, not parsed strictly.
    let (report, verdict) = match (run.status, run.output_complete) {
        (None, _) => (
            parsed.unwrap_or_default(),
            Verdict::TimedOut {
                detail: format!("probe killed after {} s", timeout.as_secs()),
            },
        ),
        (Some(_), false) => (
            parsed.unwrap_or_default(),
            Verdict::Incomplete {
                reason: "probe output still open at the deadline; a descendant kept its pipes open"
                    .to_owned(),
            },
        ),
        (Some(status), true) => {
            let report = parsed.map_err(HarnessError::Report)?;
            let verdict = match report::judge(&report) {
                Verdict::Pass if !status.success() => Verdict::Incomplete {
                    reason: format!("probe exited with {status} after a full readback"),
                },
                verdict => verdict,
            };
            (report, verdict)
        }
    };
    Ok(Evaluation {
        framework_archs,
        report,
        verdict,
        exit_status: run.status,
        stderr: run.stderr,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use dxbc_accc::{ShaderKind, ShaderStage, inspect_dxbc};

    /// The DXIL bytes the probe embeds, recovered from the DXC header.
    fn embedded_kernel() -> Vec<u8> {
        let header = include_str!("../probe/shaders/fill_cs66.h");
        let start = header
            .find("g_fill_cs66[] = {")
            .expect("byte array present");
        let body = &header[start..];
        let end = body.find("};").expect("byte array terminated");
        body[..end]
            .split(|c: char| c == ',' || c.is_whitespace() || c == '{')
            .filter_map(|t| t.strip_prefix("0x"))
            .map(|t| u8::from_str_radix(t, 16).expect("hex byte"))
            .collect()
    }

    #[test]
    fn embedded_kernel_is_an_sm_6_6_compute_dxil_container() {
        let blob = embedded_kernel();
        assert_eq!(blob.len(), 3092);
        let inspection = inspect_dxbc(&blob).expect("valid DXBC container");
        assert_eq!(inspection.kind, ShaderKind::Dxil);
        assert_eq!(inspection.stage, ShaderStage::Compute);
        assert_eq!(
            (inspection.shader_model_major, inspection.shader_model_minor),
            (6, 6)
        );
        assert_eq!(inspection.container_size as usize, blob.len());
    }

    #[test]
    fn rejects_a_framework_without_an_x86_64_slice() {
        let path = std::env::temp_dir().join(format!("bx-harness-arm64-{}", std::process::id()));
        let mut thin_arm64 = 0xfeed_facf_u32.to_le_bytes().to_vec();
        thin_arm64.extend_from_slice(&0x0100_000c_u32.to_le_bytes());
        std::fs::write(&path, &thin_arm64).unwrap();
        let result = require_x86_64(&path);
        std::fs::remove_file(&path).unwrap();
        assert!(matches!(
            result,
            Err(HarnessError::NoX86_64Slice { archs, .. }) if archs == [CpuArch::Arm64]
        ));
    }
}

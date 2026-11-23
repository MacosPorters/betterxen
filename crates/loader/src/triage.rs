// SPDX-License-Identifier: GPL-3.0-or-later

//! Read-only PE32+ header and triage inspection.
//!
//! Provides diagnostic reports for executable binaries without loading or executing them.

use crate::pe::{ParsedPe, PeError, PeSection};
use std::fmt;
use std::fs;
use std::path::Path;

/// High-level diagnostic report from inspecting a PE32+ binary.
#[derive(Clone, Debug)]
pub struct TriageReport {
    pub file_size: usize,
    pub machine: u16,
    pub is_amd64: bool,
    pub image_base: u64,
    pub entry_point_rva: u32,
    pub size_of_image: u32,
    pub sections: Vec<PeSection>,
}

impl fmt::Display for TriageReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "PE32+ Binary Triage Report:")?;
        writeln!(f, "  File Size:         {} bytes", self.file_size)?;
        writeln!(
            f,
            "  Machine:           0x{:04x} (AMD64: {})",
            self.machine, self.is_amd64
        )?;
        writeln!(f, "  Image Base:        0x{:016x}", self.image_base)?;
        writeln!(f, "  Entry Point RVA:   0x{:08x}", self.entry_point_rva)?;
        writeln!(f, "  Size of Image:     0x{:08x}", self.size_of_image)?;
        writeln!(f, "  Sections ({}):", self.sections.len())?;
        for sec in &self.sections {
            let perms = format!(
                "{}{}{}",
                if sec.permissions.read { "R" } else { "-" },
                if sec.permissions.write { "W" } else { "-" },
                if sec.permissions.execute { "X" } else { "-" }
            );
            writeln!(
                f,
                "    - {:<8} RVA: 0x{:08x} Size: 0x{:08x} Raw: 0x{:08x} Perms: [{}]",
                sec.name, sec.virtual_address, sec.virtual_size, sec.raw_data_size, perms
            )?;
        }
        Ok(())
    }
}

/// Inspect a PE32+ binary file from disk in read-only mode.
pub fn triage_file<P: AsRef<Path>>(path: P) -> Result<TriageReport, PeError> {
    let bytes = fs::read(path).map_err(|_| PeError::TooSmall)?;
    triage_bytes(&bytes)
}

/// Inspect in-memory PE32+ bytes in read-only mode.
pub fn triage_bytes(bytes: &[u8]) -> Result<TriageReport, PeError> {
    let pe = ParsedPe::parse(bytes)?;
    Ok(TriageReport {
        file_size: bytes.len(),
        machine: pe.machine,
        is_amd64: pe.machine == crate::pe::IMAGE_FILE_MACHINE_AMD64,
        image_base: pe.image_base,
        entry_point_rva: pe.entry_point_rva,
        size_of_image: pe.size_of_image,
        sections: pe.sections,
    })
}

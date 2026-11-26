//! Identify a Windows executable before it is handed to CrossOver.
//!
//! Reads only the headers and the import directory names of a PE/COFF image, following
//! Microsoft's published "PE Format" specification. The file is untrusted input: every
//! offset is checked, counts are capped, and nothing is executed. This answers "is this a
//! launchable x86 Windows program, and which DLLs does it import"; it does not prove that
//! the program will run.

use crate::json::Json;

/// Largest file this will read. Larger programs are refused rather than partly checked.
pub const MAX_BYTES: u64 = 1 << 30;
const MAX_SECTIONS: usize = 96;
const MAX_IMPORTS: usize = 512;
const MAX_NAME: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Machine {
    X86_64,
    I386,
    Arm64,
    Other(u16),
}

impl Machine {
    pub fn name(self) -> String {
        match self {
            Machine::X86_64 => "x86_64".into(),
            Machine::I386 => "i386".into(),
            Machine::Arm64 => "arm64".into(),
            Machine::Other(m) => format!("unknown_0x{m:04x}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Image {
    pub machine: Machine,
    pub pe32_plus: bool,
    pub subsystem: u16,
    pub is_dll: bool,
    /// Imported DLL names, lower-cased, from the (non-delayed) import directory.
    pub imports: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Error {
    NotPe(&'static str),
    Truncated(&'static str),
    Unsupported(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::NotPe(why) => write!(f, "not a PE executable: {why}"),
            Error::Truncated(what) => write!(f, "truncated or inconsistent PE: {what}"),
            Error::Unsupported(why) => write!(f, "unsupported executable: {why}"),
        }
    }
}

fn bytes<'a>(d: &'a [u8], at: usize, n: usize, what: &'static str) -> Result<&'a [u8], Error> {
    at.checked_add(n)
        .and_then(|end| d.get(at..end))
        .ok_or(Error::Truncated(what))
}

fn u16_at(d: &[u8], at: usize, what: &'static str) -> Result<u16, Error> {
    let b = bytes(d, at, 2, what)?;
    Ok(u16::from_le_bytes([b[0], b[1]]))
}

fn u32_at(d: &[u8], at: usize, what: &'static str) -> Result<u32, Error> {
    let b = bytes(d, at, 4, what)?;
    Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

struct Section {
    va: u32,
    size: u32,
    file: u32,
}

fn file_offset(sections: &[Section], rva: u32) -> Option<usize> {
    sections.iter().find_map(|s| {
        let into = rva.checked_sub(s.va)?;
        (into < s.size).then(|| s.file.checked_add(into).map(|v| v as usize))?
    })
}

pub fn parse(d: &[u8]) -> Result<Image, Error> {
    if bytes(d, 0, 2, "DOS magic").map_err(|_| Error::NotPe("file too small"))? != b"MZ" {
        return Err(Error::NotPe("missing MZ magic"));
    }
    let pe = u32_at(d, 0x3c, "e_lfanew")? as usize;
    if bytes(d, pe, 4, "PE signature")? != b"PE\0\0" {
        return Err(Error::NotPe("missing PE signature"));
    }
    let coff = pe.checked_add(4).ok_or(Error::Truncated("COFF header"))?;
    let machine = match u16_at(d, coff, "machine")? {
        0x8664 => Machine::X86_64,
        0x014c => Machine::I386,
        0xaa64 => Machine::Arm64,
        m => Machine::Other(m),
    };
    let section_count = u16_at(d, coff + 2, "section count")? as usize;
    let optional_size = u16_at(d, coff + 16, "optional header size")? as usize;
    let characteristics = u16_at(d, coff + 18, "characteristics")?;
    if characteristics & 0x0002 == 0 {
        return Err(Error::NotPe("not marked as an executable image"));
    }
    if section_count > MAX_SECTIONS {
        return Err(Error::Truncated("implausible section count"));
    }
    let opt = coff + 20;
    let optional = bytes(d, opt, optional_size, "optional header")?;
    let (pe32_plus, count_at, dirs_at) = match u16_at(optional, 0, "optional magic")? {
        0x20b => (true, 108, 112),
        0x10b => (false, 92, 96),
        _ => return Err(Error::NotPe("unknown optional header magic")),
    };
    let subsystem = u16_at(optional, 68, "subsystem")?;

    let mut sections = Vec::with_capacity(section_count);
    let table = opt + optional_size;
    for i in 0..section_count {
        let at = table + i * 40;
        sections.push(Section {
            va: u32_at(d, at + 12, "section address")?,
            size: u32_at(d, at + 16, "section raw size")?,
            file: u32_at(d, at + 20, "section raw pointer")?,
        });
    }

    let mut imports = Vec::new();
    let directories = u32_at(optional, count_at, "directory count")?;
    let import_rva = if directories >= 2 {
        u32_at(optional, dirs_at + 8, "import directory")?
    } else {
        0
    };
    if import_rva != 0 {
        let start = file_offset(&sections, import_rva)
            .ok_or(Error::Truncated("import directory outside sections"))?;
        let mut terminated = false;
        for i in 0..=MAX_IMPORTS {
            let entry = bytes(d, start + i * 20, 20, "import descriptor")?;
            if entry.iter().all(|b| *b == 0) {
                terminated = true;
                break;
            }
            let name_rva = u32_at(entry, 12, "import name")?;
            let at = file_offset(&sections, name_rva)
                .ok_or(Error::Truncated("import name outside sections"))?;
            let rest = d.get(at..).ok_or(Error::Truncated("import name"))?;
            let len = rest
                .iter()
                .take(MAX_NAME)
                .position(|b| *b == 0)
                .ok_or(Error::Truncated("unterminated import name"))?;
            imports.push(String::from_utf8_lossy(&rest[..len]).to_ascii_lowercase());
        }
        if !terminated {
            return Err(Error::Truncated("import directory has no terminator"));
        }
    }

    Ok(Image {
        machine,
        pe32_plus,
        subsystem,
        is_dll: characteristics & 0x2000 != 0,
        imports,
    })
}

impl Image {
    /// Refuse what is known not to launch. ARM64 programs were observed to be rejected by
    /// CrossOver 26.3 on macOS ("cannot execute").
    pub fn check_launchable(&self) -> Result<(), Error> {
        if self.is_dll {
            return Err(Error::Unsupported(
                "the file is a DLL, not a program".into(),
            ));
        }
        if !matches!(self.machine, Machine::X86_64 | Machine::I386) {
            return Err(Error::Unsupported(format!(
                "machine {} (CrossOver on macOS runs x86 and x86_64 Windows programs)",
                self.machine.name()
            )));
        }
        // 2 = Windows GUI, 3 = Windows console.
        if !matches!(self.subsystem, 2 | 3) {
            return Err(Error::Unsupported(format!("subsystem {}", self.subsystem)));
        }
        Ok(())
    }

    pub fn to_json(&self) -> Json {
        Json::obj(vec![
            ("machine", Json::str(self.machine.name())),
            ("pe32_plus", Json::Bool(self.pe32_plus)),
            ("subsystem", Json::Num(self.subsystem as f64)),
            (
                "imports",
                Json::Arr(self.imports.iter().map(|s| Json::str(s.clone())).collect()),
            ),
        ])
    }
}

/// Read and identify a program file, refusing anything that is not a regular file of a
/// plausible size.
pub fn inspect(path: &std::path::Path) -> Result<Image, String> {
    let meta = std::fs::metadata(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if !meta.is_file() {
        return Err(format!("{}: not a regular file", path.display()));
    }
    if meta.len() > MAX_BYTES {
        return Err(format!("{}: larger than {MAX_BYTES} bytes", path.display()));
    }
    let data = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    parse(&data).map_err(|e| format!("{}: {e}", path.display()))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    fn put16(d: &mut [u8], at: usize, v: u16) {
        d[at..at + 2].copy_from_slice(&v.to_le_bytes());
    }
    fn put32(d: &mut [u8], at: usize, v: u32) {
        d[at..at + 4].copy_from_slice(&v.to_le_bytes());
    }

    /// A minimal image written field by field from the PE Format specification: one section
    /// at RVA 0x1000 holding one import descriptor and its name.
    pub(crate) fn synthetic(machine: u16, characteristics: u16) -> Vec<u8> {
        let mut d = vec![0u8; 0x400];
        d[0..2].copy_from_slice(b"MZ");
        put32(&mut d, 0x3c, 0x40);
        d[0x40..0x44].copy_from_slice(b"PE\0\0");
        put16(&mut d, 0x44, machine);
        put16(&mut d, 0x46, 1); // one section
        put16(&mut d, 0x54, 240); // PE32+ optional header with 16 directories
        put16(&mut d, 0x56, characteristics);
        let opt = 0x58;
        put16(&mut d, opt, 0x20b);
        put16(&mut d, opt + 68, 3);
        put32(&mut d, opt + 108, 16);
        put32(&mut d, opt + 120, 0x1000); // directory 1: imports
        let section = opt + 240;
        put32(&mut d, section + 12, 0x1000);
        put32(&mut d, section + 16, 0x100);
        put32(&mut d, section + 20, 0x200);
        put32(&mut d, 0x200 + 12, 0x1080); // name of descriptor 0
        d[0x280..0x28a].copy_from_slice(b"D3D11.dll\0");
        d
    }

    #[test]
    fn reads_machine_subsystem_and_imports() {
        let i = parse(&synthetic(0x8664, 0x0022)).unwrap();
        assert_eq!(i.machine, Machine::X86_64);
        assert!(i.pe32_plus);
        assert_eq!(i.subsystem, 3);
        assert_eq!(i.imports, ["d3d11.dll"]);
        assert_eq!(i.check_launchable(), Ok(()));
    }

    #[test]
    fn refuses_arm64_dlls_and_unknown_machines() {
        let arm = parse(&synthetic(0xaa64, 0x0022)).unwrap();
        assert!(matches!(arm.check_launchable(), Err(Error::Unsupported(_))));
        let dll = parse(&synthetic(0x8664, 0x2022)).unwrap();
        assert!(matches!(dll.check_launchable(), Err(Error::Unsupported(_))));
        let other = parse(&synthetic(0x1f0, 0x0022)).unwrap();
        assert!(other.check_launchable().is_err());
    }

    #[test]
    fn non_pe_input_is_an_error() {
        assert!(matches!(parse(b""), Err(Error::NotPe(_))));
        assert!(matches!(parse(b"#!/bin/sh\n"), Err(Error::NotPe(_))));
        let mut d = synthetic(0x8664, 0x0022);
        d[0x40] = b'X';
        assert!(matches!(parse(&d), Err(Error::NotPe(_))));
        assert!(parse(&synthetic(0x8664, 0)).is_err());
    }

    #[test]
    fn hostile_offsets_and_truncation_never_panic() {
        let good = synthetic(0x8664, 0x0022);
        for n in 0..good.len() {
            let _ = parse(&good[..n]);
        }
        // Every aligned word replaced by extreme values: must return, never panic.
        for at in (0..good.len() - 4).step_by(2) {
            for v in [u32::MAX, 0x7fff_ffff, 0x1000] {
                let mut d = good.clone();
                put32(&mut d, at, v);
                let _ = parse(&d);
            }
        }
        let mut far = good.clone();
        put32(&mut far, 0x58 + 120, 0xdead_0000);
        assert!(matches!(parse(&far), Err(Error::Truncated(_))));
    }
}

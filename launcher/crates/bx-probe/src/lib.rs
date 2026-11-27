//! Track A / F: Clean-room PE32+ static import & compatibility classifier.
//!
//! Categorizes PE32+ imported DLLs by BetterXen ownership track (A-E) without
//! executing the binary or embedding any title-specific metadata.

use std::collections::BTreeMap;
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ShimTrack {
    WineBuiltin,
    TrackAWin32Bridge,
    TrackBMemoryStorage,
    TrackCGraphics,
    TrackEPlatformAudio,
    BundledOrUnknown,
}

impl fmt::Display for ShimTrack {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WineBuiltin => write!(f, "Wine / CrossOver built-in"),
            Self::TrackAWin32Bridge => write!(f, "Track A (win32_bridge)"),
            Self::TrackBMemoryStorage => write!(f, "Track B (xmem / XCRDAPI)"),
            Self::TrackCGraphics => write!(f, "Track C (d3d12_xs / xg_xs)"),
            Self::TrackEPlatformAudio => write!(f, "Track E (xgameruntime / GameInput / audio)"),
            Self::BundledOrUnknown => write!(f, "Bundled / Third-party"),
        }
    }
}

pub fn classify_dll(dll_name: &str) -> ShimTrack {
    let lower = dll_name.to_ascii_lowercase();
    match lower.as_str() {
        "d3d12_xs.dll" | "d3d12_x.dll" | "xg_xs.dll" | "xg_x.dll" => ShimTrack::TrackCGraphics,
        "xmem.dll" | "xcrdapi.dll" | "dstorage.dll" | "dstoragecore.dll" => {
            ShimTrack::TrackBMemoryStorage
        }
        "xgameruntime.dll" | "gameinput.dll" | "acphal.dll" | "xapu.dll" | "chad.dll" => {
            ShimTrack::TrackEPlatformAudio
        }
        s if s.starts_with("api-ms-win-core-psm-appnotify-")
            || s.starts_with("ext-ms-win-rtcore-ntuser-") =>
        {
            ShimTrack::TrackAWin32Bridge
        }
        s if s.starts_with("api-ms-win-")
            || matches!(
                s,
                "kernel32.dll"
                    | "ntdll.dll"
                    | "user32.dll"
                    | "gdi32.dll"
                    | "advapi32.dll"
                    | "bcrypt.dll"
                    | "crypt32.dll"
                    | "ws2_32.dll"
                    | "winhttp.dll"
                    | "ole32.dll"
                    | "rpcrt4.dll"
                    | "mfplat.dll"
                    | "mfreadwrite.dll"
                    | "xaudio2_9.dll"
                    | "msvcp140.dll"
                    | "vcruntime140.dll"
                    | "vcruntime140_1.dll"
                    | "ucrtbase.dll"
            ) =>
        {
            ShimTrack::WineBuiltin
        }
        _ => ShimTrack::BundledOrUnknown,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeSummary {
    pub image_base: u64,
    pub size_of_image: u32,
    pub entry_point_rva: u32,
    pub section_count: u16,
    pub imports: BTreeMap<String, Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProbeError {
    NotPe32Plus(&'static str),
}

impl fmt::Display for ProbeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotPe32Plus(msg) => write!(f, "invalid PE32+ image: {msg}"),
        }
    }
}

impl std::error::Error for ProbeError {}

fn u16_le(b: &[u8], off: usize) -> Option<u16> {
    let s = b.get(off..off + 2)?;
    Some(u16::from_le_bytes([s[0], s[1]]))
}

fn u32_le(b: &[u8], off: usize) -> Option<u32> {
    let s = b.get(off..off + 4)?;
    Some(u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}

fn u64_le(b: &[u8], off: usize) -> Option<u64> {
    let s = b.get(off..off + 8)?;
    Some(u64::from_le_bytes([
        s[0], s[1], s[2], s[3], s[4], s[5], s[6], s[7],
    ]))
}

fn read_cstr(b: &[u8], off: usize) -> Option<String> {
    let tail = b.get(off..)?;
    let end = tail.iter().position(|&c| c == 0)?;
    Some(String::from_utf8_lossy(&tail[..end]).into_owned())
}

struct Section {
    vaddr: u32,
    vsize: u32,
    raw_ptr: u32,
    raw_size: u32,
}

fn rva_to_off(sections: &[Section], rva: u32) -> Option<usize> {
    for s in sections {
        let span = s.vsize.max(s.raw_size);
        if rva >= s.vaddr && rva < s.vaddr.saturating_add(span) {
            let delta = rva - s.vaddr;
            if delta < s.raw_size {
                return (s.raw_ptr as usize).checked_add(delta as usize);
            }
        }
    }
    None
}

/// Parse PE32+ headers and standard import directory from raw image bytes.
pub fn probe_pe32_plus(bytes: &[u8]) -> Result<PeSummary, ProbeError> {
    if bytes.len() < 0x40 || &bytes[0..2] != b"MZ" {
        return Err(ProbeError::NotPe32Plus("missing MZ header"));
    }
    let pe_off = u32_le(bytes, 0x3C).ok_or(ProbeError::NotPe32Plus("missing e_lfanew"))? as usize;
    if bytes.get(pe_off..pe_off + 4) != Some(b"PE\0\0") {
        return Err(ProbeError::NotPe32Plus("missing PE signature"));
    }
    let coff = pe_off + 4;
    let machine = u16_le(bytes, coff).ok_or(ProbeError::NotPe32Plus("truncated COFF"))?;
    if machine != 0x8664 {
        return Err(ProbeError::NotPe32Plus("not x86-64 machine"));
    }
    let section_count = u16_le(bytes, coff + 2).ok_or(ProbeError::NotPe32Plus("sections"))?;
    let opt_size = u16_le(bytes, coff + 16).ok_or(ProbeError::NotPe32Plus("opt_size"))? as usize;
    let opt = coff + 20;
    let magic = u16_le(bytes, opt).ok_or(ProbeError::NotPe32Plus("opt magic"))?;
    if magic != 0x020B {
        return Err(ProbeError::NotPe32Plus("not PE32+ (0x020B)"));
    }

    let entry_point_rva = u32_le(bytes, opt + 16).ok_or(ProbeError::NotPe32Plus("entry"))?;
    let image_base = u64_le(bytes, opt + 24).ok_or(ProbeError::NotPe32Plus("image_base"))?;
    let size_of_image = u32_le(bytes, opt + 56).ok_or(ProbeError::NotPe32Plus("size_of_image"))?;
    let num_rvas = u32_le(bytes, opt + 108).ok_or(ProbeError::NotPe32Plus("num_rvas"))?;

    let sec_base = opt + opt_size;
    let section_table_size = (section_count as usize)
        .checked_mul(40)
        .ok_or(ProbeError::NotPe32Plus("section table size overflow"))?;
    bytes
        .get(sec_base..sec_base + section_table_size)
        .ok_or(ProbeError::NotPe32Plus("truncated section table"))?;
    let mut sections = Vec::with_capacity(section_count as usize);
    for i in 0..section_count as usize {
        let so = sec_base + i * 40;
        let vsize = u32_le(bytes, so + 8).ok_or(ProbeError::NotPe32Plus("sec vsize"))?;
        let vaddr = u32_le(bytes, so + 12).ok_or(ProbeError::NotPe32Plus("sec vaddr"))?;
        let raw_size = u32_le(bytes, so + 16).ok_or(ProbeError::NotPe32Plus("sec raw_size"))?;
        let raw_ptr = u32_le(bytes, so + 20).ok_or(ProbeError::NotPe32Plus("sec raw_ptr"))?;
        sections.push(Section {
            vaddr,
            vsize,
            raw_ptr,
            raw_size,
        });
    }

    let mut imports: BTreeMap<String, Vec<String>> = BTreeMap::new();
    if num_rvas > 1 {
        let imp_rva = u32_le(bytes, opt + 112 + 8)
            .ok_or(ProbeError::NotPe32Plus("truncated import directory"))?;
        if imp_rva != 0
            && let Some(mut desc_off) = rva_to_off(&sections, imp_rva)
        {
            let max_descriptors = bytes.len() / 20 + 1;
            for _ in 0..max_descriptors {
                let (Some(orig_thunk), Some(name_rva), Some(first_thunk)) = (
                    u32_le(bytes, desc_off),
                    u32_le(bytes, desc_off + 12),
                    u32_le(bytes, desc_off + 16),
                ) else {
                    break;
                };
                if orig_thunk == 0 && name_rva == 0 && first_thunk == 0 {
                    break;
                }
                if let Some(name_off) = rva_to_off(&sections, name_rva)
                    && let Some(dll) = read_cstr(bytes, name_off)
                {
                    let thunk_rva = if orig_thunk != 0 {
                        orig_thunk
                    } else {
                        first_thunk
                    };
                    let mut syms = Vec::new();
                    if let Some(mut toff) = rva_to_off(&sections, thunk_rva) {
                        while let Some(entry) = u64_le(bytes, toff) {
                            if entry == 0 {
                                break;
                            }
                            if (entry & (1u64 << 63)) != 0 {
                                syms.push(format!("ORDINAL:{}", entry & 0xFFFF));
                            } else if let Some(hint_off) =
                                rva_to_off(&sections, (entry & 0x7FFF_FFFF) as u32)
                                && let Some(sym) = read_cstr(bytes, hint_off + 2)
                            {
                                syms.push(sym);
                            }
                            toff += 8;
                        }
                    }
                    imports.entry(dll).or_default().extend(syms);
                }
                desc_off = match desc_off.checked_add(20) {
                    Some(off) => off,
                    None => break,
                };
            }
        }
    }

    Ok(PeSummary {
        image_base,
        size_of_image,
        entry_point_rva,
        section_count,
        imports,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const PE_OFF: usize = 0x80;
    const COFF: usize = PE_OFF + 4;
    const OPT: usize = COFF + 20;
    const OPT_SIZE: usize = 0xF0;
    const SEC: usize = OPT + OPT_SIZE;
    const SECTION_RVA: u32 = 0x1000;
    const RAW_PTR: u32 = 0x200;

    fn put_u16(bytes: &mut [u8], off: usize, value: u16) {
        bytes[off..off + 2].copy_from_slice(&value.to_le_bytes());
    }

    fn put_u32(bytes: &mut [u8], off: usize, value: u32) {
        bytes[off..off + 4].copy_from_slice(&value.to_le_bytes());
    }

    fn put_u64(bytes: &mut [u8], off: usize, value: u64) {
        bytes[off..off + 8].copy_from_slice(&value.to_le_bytes());
    }

    fn minimal_pe32_plus() -> Vec<u8> {
        let mut bytes = vec![0u8; 0x400];
        bytes[0..2].copy_from_slice(b"MZ");
        put_u32(&mut bytes, 0x3C, PE_OFF as u32);
        bytes[PE_OFF..PE_OFF + 4].copy_from_slice(b"PE\0\0");
        put_u16(&mut bytes, COFF, 0x8664);
        put_u16(&mut bytes, COFF + 2, 1);
        put_u16(&mut bytes, COFF + 16, OPT_SIZE as u16);
        put_u16(&mut bytes, OPT, 0x020B);
        put_u32(&mut bytes, OPT + 16, 0x1234);
        put_u64(&mut bytes, OPT + 24, 0x0000_0001_4000_0000);
        put_u32(&mut bytes, OPT + 56, 0x2000);
        put_u32(&mut bytes, OPT + 108, 16);
        bytes[SEC..SEC + 8].copy_from_slice(b".rdata\0\0");
        put_u32(&mut bytes, SEC + 8, 0x200);
        put_u32(&mut bytes, SEC + 12, SECTION_RVA);
        put_u32(&mut bytes, SEC + 16, 0x200);
        put_u32(&mut bytes, SEC + 20, RAW_PTR);
        bytes
    }

    fn two_import_pe32_plus() -> Vec<u8> {
        let mut bytes = minimal_pe32_plus();
        let import_rva = SECTION_RVA;
        put_u32(&mut bytes, OPT + 112 + 8, import_rva);
        put_u32(&mut bytes, OPT + 112 + 12, 0x140);

        // Two descriptors followed by the required all-zero terminator.
        put_u32(&mut bytes, RAW_PTR as usize, SECTION_RVA + 0x80);
        put_u32(&mut bytes, RAW_PTR as usize + 12, SECTION_RVA + 0x40);
        put_u32(&mut bytes, RAW_PTR as usize + 16, SECTION_RVA + 0x80);
        put_u32(&mut bytes, RAW_PTR as usize + 20, SECTION_RVA + 0xA0);
        put_u32(&mut bytes, RAW_PTR as usize + 32, SECTION_RVA + 0x50);
        put_u32(&mut bytes, RAW_PTR as usize + 36, SECTION_RVA + 0xA0);

        bytes[RAW_PTR as usize + 0x40..RAW_PTR as usize + 0x4A].copy_from_slice(b"ALPHA.DLL\0");
        bytes[RAW_PTR as usize + 0x50..RAW_PTR as usize + 0x59].copy_from_slice(b"BETA.DLL\0");

        put_u64(
            &mut bytes,
            RAW_PTR as usize + 0x80,
            u64::from(SECTION_RVA + 0x60),
        );
        put_u64(&mut bytes, RAW_PTR as usize + 0x88, 0);
        put_u64(&mut bytes, RAW_PTR as usize + 0xA0, (1u64 << 63) | 7);
        put_u64(&mut bytes, RAW_PTR as usize + 0xA8, 0);

        put_u16(&mut bytes, RAW_PTR as usize + 0x60, 0x1234);
        bytes[RAW_PTR as usize + 0x62..RAW_PTR as usize + 0x6C].copy_from_slice(b"NamedFunc\0");
        bytes
    }

    #[test]
    fn classifies_tracks_accurately() {
        assert_eq!(classify_dll("d3d12_xs.dll"), ShimTrack::TrackCGraphics);
        assert_eq!(classify_dll("xg_xs.dll"), ShimTrack::TrackCGraphics);
        assert_eq!(classify_dll("xmem.dll"), ShimTrack::TrackBMemoryStorage);
        assert_eq!(classify_dll("XCRDAPI.dll"), ShimTrack::TrackBMemoryStorage);
        assert_eq!(
            classify_dll("xgameruntime.dll"),
            ShimTrack::TrackEPlatformAudio
        );
        assert_eq!(classify_dll("AcpHal.dll"), ShimTrack::TrackEPlatformAudio);
        assert_eq!(
            classify_dll("api-ms-win-core-psm-appnotify-l1-1-0.dll"),
            ShimTrack::TrackAWin32Bridge
        );
        assert_eq!(
            classify_dll("ext-ms-win-rtcore-ntuser-window-ansi-l1-1-0.dll"),
            ShimTrack::TrackAWin32Bridge
        );
        assert_eq!(classify_dll("KERNEL32.dll"), ShimTrack::WineBuiltin);
    }

    #[test]
    fn parses_minimal_pe32_plus_without_imports() {
        let summary = probe_pe32_plus(&minimal_pe32_plus()).unwrap();
        assert_eq!(summary.entry_point_rva, 0x1234);
        assert_eq!(summary.image_base, 0x0000_0001_4000_0000);
        assert_eq!(summary.size_of_image, 0x2000);
        assert_eq!(summary.section_count, 1);
        assert!(summary.imports.is_empty());
    }

    #[test]
    fn parses_named_and_ordinal_imports_from_two_dlls() {
        let summary = probe_pe32_plus(&two_import_pe32_plus()).unwrap();
        let expected = BTreeMap::from([
            ("ALPHA.DLL".to_owned(), vec!["NamedFunc".to_owned()]),
            ("BETA.DLL".to_owned(), vec!["ORDINAL:7".to_owned()]),
        ]);
        assert_eq!(summary.imports, expected);
    }

    #[test]
    fn rejects_truncation_at_every_header_boundary() {
        let image = minimal_pe32_plus();
        for len in 0..=0x100 {
            let result = std::panic::catch_unwind(|| probe_pe32_plus(&image[..len]));
            assert!(result.is_ok(), "panic at truncation 0x{len:X}");
            assert!(result.unwrap().is_err(), "accepted truncation 0x{len:X}");
        }
    }

    #[test]
    fn rejects_section_rva_mapping_that_would_wrap_u32() {
        let mut image = minimal_pe32_plus();
        put_u32(&mut image, OPT + 112 + 8, SECTION_RVA + 1);
        put_u32(&mut image, SEC + 20, u32::MAX);
        let result = std::panic::catch_unwind(|| probe_pe32_plus(&image));
        assert!(result.is_ok(), "raw pointer addition panicked");
        assert!(result.unwrap().unwrap().imports.is_empty());
    }

    #[test]
    fn rejects_section_count_larger_than_file() {
        let mut image = minimal_pe32_plus();
        put_u16(&mut image, COFF + 2, u16::MAX);
        assert!(probe_pe32_plus(&image).is_err());
    }

    #[test]
    fn rejects_pe32_magic() {
        let mut image = minimal_pe32_plus();
        put_u16(&mut image, OPT, 0x010B);
        assert_eq!(
            probe_pe32_plus(&image),
            Err(ProbeError::NotPe32Plus("not PE32+ (0x020B)"))
        );
    }

    #[test]
    fn import_directory_outside_all_sections_is_safe() {
        let mut image = minimal_pe32_plus();
        put_u32(&mut image, OPT + 112 + 8, 0x8000);
        let summary = probe_pe32_plus(&image).unwrap();
        assert!(summary.imports.is_empty());
    }

    #[test]
    fn rejects_truncated_import_directory() {
        let mut image = minimal_pe32_plus();
        image.truncate(OPT + 112 + 8 + 2);
        assert!(probe_pe32_plus(&image).is_err());
    }

    #[test]
    fn bounds_unterminated_import_descriptor_walk() {
        let mut image = minimal_pe32_plus();
        put_u32(&mut image, OPT + 112 + 8, SECTION_RVA);
        put_u32(&mut image, SEC + 8, 0x200);
        put_u32(&mut image, SEC + 16, 0x200);
        for off in (RAW_PTR as usize..image.len().saturating_sub(20)).step_by(20) {
            put_u32(&mut image, off, SECTION_RVA + 0x80);
            put_u32(&mut image, off + 12, SECTION_RVA + 0x40);
            put_u32(&mut image, off + 16, SECTION_RVA + 0x80);
        }
        bytes_at(&mut image, RAW_PTR as usize + 0x40, b"NOEND.DLL\0");
        put_u64(&mut image, RAW_PTR as usize + 0x80, 0);
        let result = std::panic::catch_unwind(|| probe_pe32_plus(&image));
        assert!(result.is_ok(), "unterminated descriptor table panicked");
        assert!(result.unwrap().is_ok());
    }

    fn bytes_at(image: &mut [u8], off: usize, value: &[u8]) {
        image[off..off + value.len()].copy_from_slice(value);
    }
}

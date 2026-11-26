//! Synthetic `FSPX` section table carried inside an `ACCC` chunk payload.
//!
//! This is BetterXen's clean-room layout for synthetic blobs and for inputs
//! that adopt the same layout. It is not a description of a console compiler's
//! on-disk format. A successful parse does not mean the bytes are RDNA 2
//! machine code from any particular GPU, and it is not Xbox hardware
//! verification. Section 6 is a byte range only; this module does not
//! disassemble it.
//!
//! A version-1 table holds at most [`FSPX_MAX_SECTIONS`] descriptors. A larger
//! count is rejected before any descriptor is compared with another, so
//! inspection stays allocation-free and the pairwise check stays bounded.

use super::{ShaderStage, read_u32_le};

/// Most sections one synthetic FSPX table may declare.
///
/// Validation re-reads earlier descriptors from the payload and does not
/// allocate. At this cap the pairwise check is at most 2,016 comparisons.
/// `65` or more is an error before that loop starts.
pub const FSPX_MAX_SECTIONS: usize = 64;

/// One bounds-checked section descriptor.
///
/// `offset` is relative to the first byte of the `FSPX` payload (the `FSPX`
/// magic), not relative to the enclosing DXBC container.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FspxSection {
    pub id: u32,
    pub offset: u32,
    pub size: u32,
}

/// Section 1, the shader stage configuration.
///
/// `stage` uses the `bx_shader_stage_t` numbering from
/// `abi/include/bx_shader_bridge.h`: 0 unknown, 1 pixel, 2 vertex, 3 geometry,
/// 4 hull, 5 domain, 6 compute, 7 library, 8 mesh, 9 amplification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FspxStageConfig {
    pub offset: u32,
    pub size: u32,
    pub stage: ShaderStage,
    pub shader_model_major: u8,
    pub shader_model_minor: u8,
}

/// Section 6, the RDNA 2 code byte range inside the `FSPX` payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FspxCodeBounds {
    pub offset: u32,
    pub size: u32,
}

/// A parsed synthetic `FSPX` table.
///
/// Version 0 is the magic-only stub: `section_count` must be 0 and no section
/// bytes are read. Version 1 is the section table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FspxInfo {
    pub version: u32,
    pub flags: u32,
    pub sections: Vec<FspxSection>,
    pub section0: Option<FspxSection>,
    pub stage_config: Option<FspxStageConfig>,
    pub rdna_code: Option<FspxCodeBounds>,
    pub section7: Option<FspxSection>,
}

impl FspxInfo {
    /// Return the section 6 bytes, or `None` when the recorded bounds do not
    /// fit in `fspx_payload`.
    pub fn rdna_code_bytes<'a>(&self, fspx_payload: &'a [u8]) -> Option<&'a [u8]> {
        let code = self.rdna_code.as_ref()?;
        let start = code.offset as usize;
        let end = start.checked_add(code.size as usize)?;
        fspx_payload.get(start..end)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FspxError {
    Truncated,
    BadVersion(u32),
    SectionOutOfBounds { section_id: u32 },
    DuplicateSection { section_id: u32 },
    Overlap { section_id: u32 },
    BadStageConfig,
    TooManySections(u32),
}

const HEADER_LEN: usize = 16;
const DESCRIPTOR_LEN: usize = 12;
const STAGE_CONFIG_LEN: usize = 12;

fn stage_from_abi(raw: u32) -> Option<ShaderStage> {
    Some(match raw {
        0 => ShaderStage::Unknown,
        1 => ShaderStage::Pixel,
        2 => ShaderStage::Vertex,
        3 => ShaderStage::Geometry,
        4 => ShaderStage::Hull,
        5 => ShaderStage::Domain,
        6 => ShaderStage::Compute,
        7 => ShaderStage::Library,
        8 => ShaderStage::Mesh,
        9 => ShaderStage::Amplification,
        _ => return None,
    })
}

fn section_end(offset: u32, size: u32, table_end: usize, payload_len: usize) -> Option<usize> {
    let start = usize::try_from(offset).ok()?;
    let len = usize::try_from(size).ok()?;
    let end = start.checked_add(len)?;
    if start < table_end || end > payload_len {
        return None;
    }
    Some(end)
}

fn ranges_overlap(a_off: u32, a_size: u32, b_off: u32, b_size: u32) -> bool {
    let a0 = u64::from(a_off);
    let a1 = a0 + u64::from(a_size);
    let b0 = u64::from(b_off);
    let b1 = b0 + u64::from(b_size);
    a0 < b1 && b0 < a1
}

fn stage_from_body(body: &[u8]) -> Result<(ShaderStage, u8, u8), FspxError> {
    if body.len() != STAGE_CONFIG_LEN {
        return Err(FspxError::BadStageConfig);
    }
    let stage_raw = read_u32_le(body, 0).ok_or(FspxError::BadStageConfig)?;
    let major = read_u32_le(body, 4).ok_or(FspxError::BadStageConfig)?;
    let minor = read_u32_le(body, 8).ok_or(FspxError::BadStageConfig)?;
    let stage = stage_from_abi(stage_raw).ok_or(FspxError::BadStageConfig)?;
    let shader_model_major = u8::try_from(major).map_err(|_| FspxError::BadStageConfig)?;
    let shader_model_minor = u8::try_from(minor).map_err(|_| FspxError::BadStageConfig)?;
    Ok((stage, shader_model_major, shader_model_minor))
}

fn descriptor_at(payload: &[u8], index: usize) -> Result<(u32, u32, u32), FspxError> {
    let at = HEADER_LEN
        .checked_add(
            index
                .checked_mul(DESCRIPTOR_LEN)
                .ok_or(FspxError::Truncated)?,
        )
        .ok_or(FspxError::Truncated)?;
    let id = read_u32_le(payload, at).ok_or(FspxError::Truncated)?;
    let offset = read_u32_le(payload, at + 4).ok_or(FspxError::Truncated)?;
    let size = read_u32_le(payload, at + 8).ok_or(FspxError::Truncated)?;
    Ok((id, offset, size))
}

struct FspxHeader {
    version: u32,
    flags: u32,
    count: usize,
    table_end: usize,
}

/// `Ok(None)` when the payload does not start with `FSPX`.
fn read_fspx_header(payload: &[u8]) -> Result<Option<FspxHeader>, FspxError> {
    if payload.get(..4) != Some(b"FSPX".as_slice()) {
        return Ok(None);
    }
    if payload.len() < HEADER_LEN {
        return Err(FspxError::Truncated);
    }
    let version = read_u32_le(payload, 4).ok_or(FspxError::Truncated)?;
    let section_count = read_u32_le(payload, 8).ok_or(FspxError::Truncated)?;
    let flags = read_u32_le(payload, 12).ok_or(FspxError::Truncated)?;
    if version == 0 {
        if section_count != 0 {
            return Err(FspxError::BadVersion(0));
        }
        return Ok(Some(FspxHeader {
            version,
            flags,
            count: 0,
            table_end: HEADER_LEN,
        }));
    }
    if version != 1 {
        return Err(FspxError::BadVersion(version));
    }
    let count = usize::try_from(section_count).map_err(|_| FspxError::Truncated)?;
    if count > FSPX_MAX_SECTIONS {
        return Err(FspxError::TooManySections(section_count));
    }
    let table_bytes = count
        .checked_mul(DESCRIPTOR_LEN)
        .ok_or(FspxError::Truncated)?;
    let table_end = HEADER_LEN
        .checked_add(table_bytes)
        .ok_or(FspxError::Truncated)?;
    if table_end > payload.len() {
        return Err(FspxError::Truncated);
    }
    Ok(Some(FspxHeader {
        version,
        flags,
        count,
        table_end,
    }))
}

/// Validate every descriptor by re-reading earlier ones from `payload`.
///
/// The check does not allocate. `on_section` receives the section bytes.
fn walk_sections(
    payload: &[u8],
    header: &FspxHeader,
    mut on_section: impl FnMut(FspxSection, &[u8]) -> Result<(), FspxError>,
) -> Result<(), FspxError> {
    for index in 0..header.count {
        let (id, offset, size) = descriptor_at(payload, index)?;
        let end = section_end(offset, size, header.table_end, payload.len())
            .ok_or(FspxError::SectionOutOfBounds { section_id: id })?;
        for prev in 0..index {
            let (prev_id, prev_off, prev_size) = descriptor_at(payload, prev)?;
            if prev_id == id {
                return Err(FspxError::DuplicateSection { section_id: id });
            }
            if ranges_overlap(prev_off, prev_size, offset, size) {
                return Err(FspxError::Overlap { section_id: id });
            }
        }
        let start = usize::try_from(offset)
            .map_err(|_| FspxError::SectionOutOfBounds { section_id: id })?;
        let body = payload
            .get(start..end)
            .ok_or(FspxError::SectionOutOfBounds { section_id: id })?;
        on_section(FspxSection { id, offset, size }, body)?;
    }
    Ok(())
}

/// Stage word from a version-1 table, without retaining the section list.
///
/// `Ok(None)` means the payload is not `FSPX`. `Ok(Some(scan))` is a
/// magic-only stub or a valid table. `scan.stage` is set only when section 1
/// is present.
pub(crate) struct FspxStageScan {
    pub stage: Option<(ShaderStage, u8, u8)>,
}

pub(crate) fn scan_fspx(payload: &[u8]) -> Result<Option<FspxStageScan>, FspxError> {
    let Some(header) = read_fspx_header(payload)? else {
        return Ok(None);
    };
    let mut stage = None;
    walk_sections(payload, &header, |section, body| {
        if section.id == 1 {
            stage = Some(stage_from_body(body)?);
        }
        Ok(())
    })?;
    Ok(Some(FspxStageScan { stage }))
}

/// Parse an `ACCC` payload.
///
/// `Ok(None)` means the payload does not start with `FSPX`. `Ok(Some(_))` is a
/// magic-only stub or a complete section table. Malformed `FSPX` bytes are
/// `Err` and never panic.
pub(crate) fn parse_fspx(payload: &[u8]) -> Result<Option<FspxInfo>, FspxError> {
    let Some(header) = read_fspx_header(payload)? else {
        return Ok(None);
    };
    let mut sections = Vec::new();
    let mut section0 = None;
    let mut stage_config = None;
    let mut rdna_code = None;
    let mut section7 = None;
    walk_sections(payload, &header, |section, body| {
        match section.id {
            0 => section0 = Some(section),
            1 => {
                let (stage, shader_model_major, shader_model_minor) = stage_from_body(body)?;
                stage_config = Some(FspxStageConfig {
                    offset: section.offset,
                    size: section.size,
                    stage,
                    shader_model_major,
                    shader_model_minor,
                });
            }
            6 => {
                rdna_code = Some(FspxCodeBounds {
                    offset: section.offset,
                    size: section.size,
                });
            }
            7 => section7 = Some(section),
            _ => {}
        }
        sections.push(section);
        Ok(())
    })?;
    Ok(Some(FspxInfo {
        version: header.version,
        flags: header.flags,
        sections,
        section0,
        stage_config,
        rdna_code,
        section7,
    }))
}

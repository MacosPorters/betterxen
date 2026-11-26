//! Track D (M1-D1): Clean-room DXBC / DXIL / ACCC (`FSPX`) shader container classifier.
//!
//! Parses standard DirectX `DXBC` containers to classify whether an incoming shader
//! blob contains standard `DXIL` (fast path to D3DMetal 3.0), legacy `SHDR`/`SHEX`,
//! or console-compiled `ACCC` (`FSPX`) AMD RDNA 2 machine code.
//!
//! # Synthetic FSPX section table
//!
//! An `ACCC` payload that begins with `FSPX` uses the layout documented on
//! [`fspx`]. Version 0 is a magic-only stub. Version 1 is a section table of
//! at most [`FSPX_MAX_SECTIONS`] descriptors. A larger count is
//! [`ParseError::FspxTooManySections`] before any pairwise comparison.
//! Section 0 and section 7 are opaque header ranges, section 1 is the stage
//! configuration (`bx_shader_stage_t` values), and section 6 is the RDNA 2
//! code byte range. The code bytes are not decoded. This layout is synthetic.
//! Parsing it does not establish that a console compiler produced the blob
//! and is not Xbox hardware verification.
//!
//! A `DXIL` chunk is a program only when its payload is a `DxilProgramHeader`
//! from the public DirectXShaderCompiler layout
//! (`include/dxc/DxilContainer/DxilContainer.h`). That is `ProgramVersion`,
//! `SizeInUint32`, then `DxilBitcodeHeader` (`DXIL` magic, version, bitcode
//! offset, bitcode size). `SizeInUint32 * 4` must be at least that 24-byte
//! header and no larger than the chunk. `BitcodeOffset` is relative to the
//! start of `DxilBitcodeHeader`, which begins at program byte 8, and must be
//! at least 16. `BitcodeSize` must be non-zero, and `8 + BitcodeOffset +
//! BitcodeSize` must fit in `SizeInUint32 * 4`. A zero-length bitcode is
//! [`ParseError::BadDxilHeader`], matching DXC's `IsValidDxilProgramHeader`,
//! so preparation does not copy it.
//!
//! The C entry points in [`c_abi`] match `abi/include/bx_shader_bridge.h`.
//! `bx_shader_inspect` fills the C struct without logging. `bx_shader_prepare_for_d3dmetal`
//! copies `container_size` bytes when a `DXIL` chunk is present, including when
//! an accompanying `FSPX` table is malformed, and logs only an ACCC-only
//! `BX_SHADER_ERR_UNSUPPORTED` result. RDNA 2 to DXIL translation is not implemented.

mod c_abi;
mod fspx;

use std::fmt;

pub use c_abi::{
    BX_SHADER_ERR_INVALID_BLOB, BX_SHADER_ERR_OOM, BX_SHADER_ERR_TRANSLATE,
    BX_SHADER_ERR_UNSUPPORTED, BX_SHADER_OK, BxShaderInspection, bx_shader_free_buffer,
    bx_shader_inspect, bx_shader_prepare_for_d3dmetal,
};
pub use fspx::{FSPX_MAX_SECTIONS, FspxCodeBounds, FspxInfo, FspxSection, FspxStageConfig};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShaderKind {
    Unknown,
    Dxil,
    LegacyDxbc,
    AcccOnly,
    DualDxilAndAccc,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShaderStage {
    Unknown,
    Pixel,
    Vertex,
    Geometry,
    Hull,
    Domain,
    Compute,
    Library,
    Mesh,
    Amplification,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChunkInfo {
    pub fourcc: [u8; 4],
    pub offset: u32,
    pub size: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShaderInspection {
    pub kind: ShaderKind,
    pub stage: ShaderStage,
    pub shader_model_major: u8,
    pub shader_model_minor: u8,
    pub container_size: u32,
    pub digest: [u8; 16],
    pub has_root_signature: bool,
    pub dxil_size: u32,
    pub accc_size: u32,
    pub accc_is_fspx: bool,
    /// The first well-formed `FSPX` table in the container, if any.
    /// `accc_size` remains the payload size of the last `ACCC` chunk.
    pub fspx: Option<FspxInfo>,
    pub chunks: Vec<ChunkInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    TooShort,
    BadMagic,
    UnsupportedVersion(u32),
    TruncatedChunkTable,
    ChunkOutOfBounds { index: usize },
    TruncatedContainer,
    FspxTruncated { chunk: usize },
    FspxBadVersion { chunk: usize, version: u32 },
    FspxSectionOutOfBounds { chunk: usize, section_id: u32 },
    FspxDuplicateSection { chunk: usize, section_id: u32 },
    FspxSectionOverlap { chunk: usize, section_id: u32 },
    FspxBadStageConfig { chunk: usize },
    FspxTooManySections { chunk: usize, count: u32 },
    BadDxilHeader { chunk: usize },
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooShort => write!(f, "buffer too short for DXBC header"),
            Self::BadMagic => write!(f, "missing DXBC fourcc magic"),
            Self::UnsupportedVersion(v) => write!(f, "unsupported DXBC version {v}"),
            Self::TruncatedChunkTable => write!(f, "chunk offset table exceeds buffer"),
            Self::ChunkOutOfBounds { index } => write!(f, "chunk {index} out of bounds"),
            Self::TruncatedContainer => write!(f, "DXBC container size is outside the buffer"),
            Self::FspxTruncated { chunk } => {
                write!(f, "FSPX section table truncated in chunk {chunk}")
            }
            Self::FspxBadVersion { chunk, version } => {
                write!(f, "unsupported FSPX version {version} in chunk {chunk}")
            }
            Self::FspxSectionOutOfBounds { chunk, section_id } => {
                write!(
                    f,
                    "FSPX section {section_id} out of bounds in chunk {chunk}"
                )
            }
            Self::FspxDuplicateSection { chunk, section_id } => {
                write!(f, "duplicate FSPX section {section_id} in chunk {chunk}")
            }
            Self::FspxSectionOverlap { chunk, section_id } => {
                write!(f, "overlapping FSPX section {section_id} in chunk {chunk}")
            }
            Self::FspxBadStageConfig { chunk } => {
                write!(f, "malformed FSPX stage configuration in chunk {chunk}")
            }
            Self::FspxTooManySections { chunk, count } => {
                write!(
                    f,
                    "FSPX section count {count} exceeds {max} in chunk {chunk}",
                    max = FSPX_MAX_SECTIONS
                )
            }
            Self::BadDxilHeader { chunk } => {
                write!(f, "DXIL chunk {chunk} is not a DxilProgramHeader")
            }
        }
    }
}

impl std::error::Error for ParseError {}

pub(crate) fn read_u32_le(bytes: &[u8], off: usize) -> Option<u32> {
    let slice = bytes.get(off..off + 4)?;
    Some(u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]))
}

fn decode_stage(program_type: u16) -> ShaderStage {
    match program_type {
        0 => ShaderStage::Pixel,
        1 => ShaderStage::Vertex,
        2 => ShaderStage::Geometry,
        3 => ShaderStage::Hull,
        4 => ShaderStage::Domain,
        5 => ShaderStage::Compute,
        6 => ShaderStage::Library,
        7 => ShaderStage::Mesh,
        8 => ShaderStage::Amplification,
        _ => ShaderStage::Unknown,
    }
}

fn fspx_error(chunk: usize, err: fspx::FspxError) -> ParseError {
    match err {
        fspx::FspxError::Truncated => ParseError::FspxTruncated { chunk },
        fspx::FspxError::BadVersion(version) => ParseError::FspxBadVersion { chunk, version },
        fspx::FspxError::SectionOutOfBounds { section_id } => {
            ParseError::FspxSectionOutOfBounds { chunk, section_id }
        }
        fspx::FspxError::DuplicateSection { section_id } => {
            ParseError::FspxDuplicateSection { chunk, section_id }
        }
        fspx::FspxError::Overlap { section_id } => {
            ParseError::FspxSectionOverlap { chunk, section_id }
        }
        fspx::FspxError::BadStageConfig => ParseError::FspxBadStageConfig { chunk },
        fspx::FspxError::TooManySections(count) => ParseError::FspxTooManySections { chunk, count },
    }
}

/// Keep the first declared FSPX stage and ignore later tables until one exists.
///
/// Both [`summarize_dxbc`] and [`inspect_dxbc`] call this for every accepted
/// table, so a later compute stage fills a gap left by an earlier table.
fn record_fspx_stage(
    slot: &mut Option<(ShaderStage, u8, u8)>,
    stage: Option<(ShaderStage, u8, u8)>,
) {
    if slot.is_none() && stage.is_some() {
        *slot = stage;
    }
}

/// `DxilProgramHeader` check from the public DirectXShaderCompiler container.
///
/// `SizeInUint32 * 4` must cover the 24-byte header and fit in `payload`.
/// `BitcodeOffset` is from the start of `DxilBitcodeHeader` at program byte 8
/// and must be at least 16. `BitcodeSize` must be non-zero, and
/// `8 + BitcodeOffset + BitcodeSize` must fit in the declared program. The
/// bitcode bytes are not decoded.
fn read_dxil_program(payload: &[u8]) -> Result<(ShaderStage, u8, u8), ()> {
    const PROGRAM_PREFIX: u64 = 8;
    const BITCODE_HEADER: u64 = 16;
    const MIN_PROGRAM: u64 = PROGRAM_PREFIX + BITCODE_HEADER;
    if payload.len() < MIN_PROGRAM as usize {
        return Err(());
    }
    let program_version = read_u32_le(payload, 0).ok_or(())?;
    let size_in_u32 = read_u32_le(payload, 4).ok_or(())?;
    let program_bytes = u64::from(size_in_u32).checked_mul(4).ok_or(())?;
    if program_bytes < MIN_PROGRAM || program_bytes > payload.len() as u64 {
        return Err(());
    }
    if payload.get(8..12) != Some(b"DXIL".as_slice()) {
        return Err(());
    }
    let _dxil_version = read_u32_le(payload, 12).ok_or(())?;
    let bitcode_offset = read_u32_le(payload, 16).ok_or(())?;
    let bitcode_size = read_u32_le(payload, 20).ok_or(())?;
    if bitcode_size == 0 || u64::from(bitcode_offset) < BITCODE_HEADER {
        return Err(());
    }
    let from_header = u64::from(bitcode_offset)
        .checked_add(u64::from(bitcode_size))
        .ok_or(())?;
    let end = PROGRAM_PREFIX.checked_add(from_header).ok_or(())?;
    if end > program_bytes {
        return Err(());
    }
    Ok(program_stage(program_version))
}

struct ContainerLimit {
    container_size: u32,
    limit: usize,
    table_end: usize,
    chunk_count: usize,
    digest: [u8; 16],
}

fn container_limit(bytes: &[u8]) -> Result<ContainerLimit, ParseError> {
    if bytes.len() < 32 {
        return Err(ParseError::TooShort);
    }
    if bytes.get(..4) != Some(b"DXBC".as_slice()) {
        return Err(ParseError::BadMagic);
    }
    let mut digest = [0u8; 16];
    let digest_bytes = bytes.get(4..20).ok_or(ParseError::TooShort)?;
    digest.copy_from_slice(digest_bytes);

    let version = read_u32_le(bytes, 20).ok_or(ParseError::TooShort)?;
    if version != 1 {
        return Err(ParseError::UnsupportedVersion(version));
    }
    let container_size = read_u32_le(bytes, 24).ok_or(ParseError::TooShort)?;
    let chunk_count_u32 = read_u32_le(bytes, 28).ok_or(ParseError::TooShort)?;
    let chunk_count =
        usize::try_from(chunk_count_u32).map_err(|_| ParseError::TruncatedChunkTable)?;
    let table_bytes = chunk_count
        .checked_mul(4)
        .ok_or(ParseError::TruncatedChunkTable)?;
    let table_end = 32usize
        .checked_add(table_bytes)
        .ok_or(ParseError::TruncatedChunkTable)?;
    if table_end > bytes.len() {
        return Err(ParseError::TruncatedChunkTable);
    }
    let limit = usize::try_from(container_size).map_err(|_| ParseError::TruncatedContainer)?;
    if limit > bytes.len() || limit < table_end {
        return Err(ParseError::TruncatedContainer);
    }
    Ok(ContainerLimit {
        container_size,
        limit,
        table_end,
        chunk_count,
        digest,
    })
}

fn program_stage(word: u32) -> (ShaderStage, u8, u8) {
    let stage = decode_stage(u16::try_from(word >> 16).unwrap_or(u16::MAX));
    let major = u8::try_from((word >> 4) & 0xF).unwrap_or(0);
    let minor = u8::try_from(word & 0xF).unwrap_or(0);
    (stage, major, minor)
}

fn visit_chunks(
    bytes: &[u8],
    mut visit: impl FnMut(usize, u32, [u8; 4], u32, &[u8]) -> Result<(), ParseError>,
) -> Result<ContainerLimit, ParseError> {
    let header = container_limit(bytes)?;
    for index in 0..header.chunk_count {
        let offset = read_u32_le(bytes, 32 + index * 4).ok_or(ParseError::TruncatedChunkTable)?;
        let off = usize::try_from(offset).map_err(|_| ParseError::ChunkOutOfBounds { index })?;
        if off < header.table_end {
            return Err(ParseError::ChunkOutOfBounds { index });
        }
        let header_end = off
            .checked_add(8)
            .ok_or(ParseError::ChunkOutOfBounds { index })?;
        if header_end > header.limit {
            return Err(ParseError::ChunkOutOfBounds { index });
        }
        let hdr = bytes
            .get(off..header_end)
            .ok_or(ParseError::ChunkOutOfBounds { index })?;
        let fourcc = [hdr[0], hdr[1], hdr[2], hdr[3]];
        let size = u32::from_le_bytes([hdr[4], hdr[5], hdr[6], hdr[7]]);
        let payload_len =
            usize::try_from(size).map_err(|_| ParseError::ChunkOutOfBounds { index })?;
        let payload_end = header_end
            .checked_add(payload_len)
            .ok_or(ParseError::ChunkOutOfBounds { index })?;
        if payload_end > header.limit {
            return Err(ParseError::ChunkOutOfBounds { index });
        }
        let payload = bytes
            .get(header_end..payload_end)
            .ok_or(ParseError::ChunkOutOfBounds { index })?;
        visit(index, offset, fourcc, size, payload)?;
    }
    Ok(header)
}

fn kind_from_flags(has_dxil: bool, has_accc: bool, has_legacy: bool) -> ShaderKind {
    match (has_dxil, has_accc, has_legacy) {
        (true, true, _) => ShaderKind::DualDxilAndAccc,
        (true, false, _) => ShaderKind::Dxil,
        (false, true, _) => ShaderKind::AcccOnly,
        (false, false, true) => ShaderKind::LegacyDxbc,
        (false, false, false) => ShaderKind::Unknown,
    }
}

/// Scalar fields of a container. Used by `bx_shader_inspect`, which does not
/// keep chunk or section lists.
pub(crate) struct DxbcSummary {
    pub kind: ShaderKind,
    pub stage: ShaderStage,
    pub shader_model_major: u8,
    pub shader_model_minor: u8,
    pub container_size: u32,
    pub digest: [u8; 16],
    pub has_root_signature: bool,
    pub dxil_size: u32,
    pub accc_size: u32,
}

/// Classify a container without allocating.
///
/// A malformed `FSPX` table is still [`Err`], so inspection can report it.
pub(crate) fn summarize_dxbc(bytes: &[u8]) -> Result<DxbcSummary, ParseError> {
    let mut has_dxil = false;
    let mut has_accc = false;
    let mut has_legacy = false;
    let mut has_root_signature = false;
    let mut dxil_size = 0u32;
    let mut accc_size = 0u32;
    let mut dxil_stage = None;
    let mut legacy_stage = None;
    let mut fspx_stage = None;
    let header = visit_chunks(bytes, |index, _offset, fourcc, size, payload| {
        match &fourcc {
            b"DXIL" => {
                let stage = read_dxil_program(payload)
                    .map_err(|()| ParseError::BadDxilHeader { chunk: index })?;
                has_dxil = true;
                dxil_size = size;
                dxil_stage = Some(stage);
            }
            b"ACCC" => {
                has_accc = true;
                accc_size = size;
                match fspx::scan_fspx(payload) {
                    Ok(Some(scan)) => record_fspx_stage(&mut fspx_stage, scan.stage),
                    Ok(None) => {}
                    Err(err) => return Err(fspx_error(index, err)),
                }
            }
            b"SHDR" | b"SHEX" => {
                has_legacy = true;
                if let Some(tok) = read_u32_le(payload, 0) {
                    legacy_stage = Some(program_stage(tok));
                }
            }
            b"RTS0" => has_root_signature = true,
            _ => {}
        }
        Ok(())
    })?;
    let (stage, shader_model_major, shader_model_minor) = dxil_stage
        .or(legacy_stage)
        .or(fspx_stage)
        .unwrap_or((ShaderStage::Unknown, 0, 0));
    Ok(DxbcSummary {
        kind: kind_from_flags(has_dxil, has_accc, has_legacy),
        stage,
        shader_model_major,
        shader_model_minor,
        container_size: header.container_size,
        digest: header.digest,
        has_root_signature,
        dxil_size,
        accc_size,
    })
}

/// `Ok(Some(container_size))` when the container is structurally valid and
/// contains a `DXIL` chunk with a `DxilProgramHeader`. A malformed `FSPX`
/// table does not hide that chunk. A `DXIL` fourcc whose payload is not that
/// header is [`ParseError::BadDxilHeader`]. `Ok(None)` means the container is
/// structurally valid and has no `DXIL` chunk.
pub(crate) fn dxil_container_len(bytes: &[u8]) -> Result<Option<u32>, ParseError> {
    let mut has_dxil = false;
    let header = visit_chunks(bytes, |index, _offset, fourcc, _size, payload| {
        if &fourcc == b"DXIL" {
            if read_dxil_program(payload).is_err() {
                return Err(ParseError::BadDxilHeader { chunk: index });
            }
            has_dxil = true;
        }
        Ok(())
    })?;
    Ok(if has_dxil {
        Some(header.container_size)
    } else {
        None
    })
}

/// Inspect a `DXBC` shader container blob and classify its payload chunks.
///
/// Malformed input returns [`ParseError`]. This function does not panic on
/// truncated headers, oversized chunk counts, or `FSPX` sections that fall
/// outside the payload. A malformed `FSPX` table fails the inspection even
/// when a `DXIL` chunk is also present; preparation still passes that `DXIL`
/// container through.
pub fn inspect_dxbc(bytes: &[u8]) -> Result<ShaderInspection, ParseError> {
    let mut chunks = Vec::new();
    let mut has_dxil = false;
    let mut has_accc = false;
    let mut has_legacy = false;
    let mut has_root_signature = false;
    let mut dxil_size = 0u32;
    let mut accc_size = 0u32;
    let mut accc_is_fspx = false;
    let mut dxil_stage = None;
    let mut legacy_stage = None;
    let mut fspx_stage = None;
    let mut fspx = None;

    let header = visit_chunks(bytes, |index, offset, fourcc, size, payload| {
        match &fourcc {
            b"DXIL" => {
                let stage = read_dxil_program(payload)
                    .map_err(|()| ParseError::BadDxilHeader { chunk: index })?;
                has_dxil = true;
                dxil_size = size;
                dxil_stage = Some(stage);
            }
            b"ACCC" => {
                has_accc = true;
                accc_size = size;
                match fspx::parse_fspx(payload) {
                    Ok(Some(info)) => {
                        accc_is_fspx = true;
                        record_fspx_stage(
                            &mut fspx_stage,
                            info.stage_config.map(|cfg| {
                                (cfg.stage, cfg.shader_model_major, cfg.shader_model_minor)
                            }),
                        );
                        if fspx.is_none() {
                            fspx = Some(info);
                        }
                    }
                    Ok(None) => {}
                    Err(err) => return Err(fspx_error(index, err)),
                }
            }
            b"SHDR" | b"SHEX" => {
                has_legacy = true;
                if let Some(tok) = read_u32_le(payload, 0) {
                    legacy_stage = Some(program_stage(tok));
                }
            }
            b"RTS0" => has_root_signature = true,
            _ => {}
        }
        chunks.push(ChunkInfo {
            fourcc,
            offset,
            size,
        });
        Ok(())
    })?;

    let (stage, sm_major, sm_minor) =
        dxil_stage
            .or(legacy_stage)
            .or(fspx_stage)
            .unwrap_or((ShaderStage::Unknown, 0, 0));

    Ok(ShaderInspection {
        kind: kind_from_flags(has_dxil, has_accc, has_legacy),
        stage,
        shader_model_major: sm_major,
        shader_model_minor: sm_minor,
        container_size: header.container_size,
        digest: header.digest,
        has_root_signature,
        dxil_size,
        accc_size,
        accc_is_fspx,
        fspx,
        chunks,
    })
}

#[cfg(test)]
pub(crate) mod testutil {
    pub(crate) fn build_synthetic_dxbc(chunks: &[(&[u8; 4], &[u8])]) -> Vec<u8> {
        let header_size = 32 + chunks.len() * 4;
        let mut out = Vec::new();
        out.extend_from_slice(b"DXBC");
        out.extend_from_slice(&[0xAB; 16]);
        out.extend_from_slice(&1u32.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(&(chunks.len() as u32).to_le_bytes());

        let mut offset = header_size as u32;
        for (_, payload) in chunks {
            out.extend_from_slice(&offset.to_le_bytes());
            offset += 8 + payload.len() as u32;
        }
        for (fourcc, payload) in chunks {
            out.extend_from_slice(*fourcc);
            out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
            out.extend_from_slice(payload);
        }
        let total = out.len() as u32;
        out[24..28].copy_from_slice(&total.to_le_bytes());
        out
    }

    /// 104-byte synthetic FSPX payload.
    ///
    /// Header 16 bytes, four 12-byte descriptors, then section bodies at the
    /// literal offsets 64, 72, 84 and 100.
    pub(crate) fn synthetic_fspx_payload() -> Vec<u8> {
        let mut payload = Vec::new();
        payload.extend_from_slice(b"FSPX");
        payload.extend_from_slice(&1u32.to_le_bytes());
        payload.extend_from_slice(&4u32.to_le_bytes());
        payload.extend_from_slice(&0u32.to_le_bytes());
        for (id, offset, size) in [(0u32, 64u32, 8u32), (1, 72, 12), (6, 84, 16), (7, 100, 4)] {
            payload.extend_from_slice(&id.to_le_bytes());
            payload.extend_from_slice(&offset.to_le_bytes());
            payload.extend_from_slice(&size.to_le_bytes());
        }
        payload.extend_from_slice(&[0xA0, 0x01, 0x00, 0x00, 0xA7, 0x00, 0x00, 0x00]);
        payload.extend_from_slice(&6u32.to_le_bytes());
        payload.extend_from_slice(&6u32.to_le_bytes());
        payload.extend_from_slice(&0u32.to_le_bytes());
        payload.extend_from_slice(b"SYNTH_RDNA2_CODE");
        payload.extend_from_slice(&[0x70, 0x07, 0x00, 0x00]);
        payload
    }

    /// 28-byte `DxilProgramHeader`: version, size 7, `DXIL`, version 1.0,
    /// `BitcodeOffset` 16 from `DxilBitcodeHeader`, and 4 bytes of bitcode
    /// starting at program byte 24.
    pub(crate) fn minimal_dxil_program(program_version: u32) -> [u8; 28] {
        let mut payload = [0u8; 28];
        payload[0..4].copy_from_slice(&program_version.to_le_bytes());
        payload[4..8].copy_from_slice(&7u32.to_le_bytes());
        payload[8..12].copy_from_slice(b"DXIL");
        payload[12..16].copy_from_slice(&0x0100u32.to_le_bytes());
        payload[16..20].copy_from_slice(&16u32.to_le_bytes());
        payload[20..24].copy_from_slice(&4u32.to_le_bytes());
        payload[24..28].copy_from_slice(&[0xD1, 0x11, 0xC0, 0xDE]);
        payload
    }

    pub(crate) fn synthetic_dxil_blob() -> Vec<u8> {
        let prog_ver: u32 = (5u32 << 16) | (6u32 << 4) | 6u32;
        let dxil_payload = minimal_dxil_program(prog_ver);
        build_synthetic_dxbc(&[(b"RTS0", &[1, 2, 3, 4]), (b"DXIL", &dxil_payload)])
    }
}

#[cfg(test)]
mod tests {
    use super::c_abi::{clear_shader_log, shader_log_lines};
    use super::testutil::{
        build_synthetic_dxbc, minimal_dxil_program, synthetic_dxil_blob, synthetic_fspx_payload,
    };
    use super::*;

    fn fspx_header(
        version: u32,
        count: u32,
        descriptors: &[(u32, u32, u32)],
        len: usize,
    ) -> Vec<u8> {
        let mut payload = vec![0u8; len];
        payload[..4].copy_from_slice(b"FSPX");
        payload[4..8].copy_from_slice(&version.to_le_bytes());
        payload[8..12].copy_from_slice(&count.to_le_bytes());
        for (index, (id, offset, size)) in descriptors.iter().enumerate() {
            let at = 16 + index * 12;
            payload[at..at + 4].copy_from_slice(&id.to_le_bytes());
            payload[at + 4..at + 8].copy_from_slice(&offset.to_le_bytes());
            payload[at + 8..at + 12].copy_from_slice(&size.to_le_bytes());
        }
        payload
    }

    #[test]
    fn classifies_synthetic_dxil_cs_6_6() {
        let blob = synthetic_dxil_blob();
        assert_eq!(blob.len(), 88);
        let info = inspect_dxbc(&blob).expect("valid synthetic DXBC");
        assert_eq!(info.kind, ShaderKind::Dxil);
        assert_eq!(info.stage, ShaderStage::Compute);
        assert_eq!((info.shader_model_major, info.shader_model_minor), (6, 6));
        assert!(info.has_root_signature);
        assert!(!info.accc_is_fspx);
        assert!(info.fspx.is_none());
        assert_eq!(info.dxil_size, 28);
        assert_eq!(info.container_size, 88);
        assert_eq!(info.digest, [0xAB; 16]);
    }

    #[test]
    fn classifies_synthetic_accc_fspx_magic_only() {
        let mut accc_payload = Vec::new();
        accc_payload.extend_from_slice(b"FSPX");
        accc_payload.extend_from_slice(&[0u8; 28]);

        let blob = build_synthetic_dxbc(&[(b"ACCC", &accc_payload)]);
        let info = inspect_dxbc(&blob).expect("valid synthetic ACCC");
        assert_eq!(info.kind, ShaderKind::AcccOnly);
        assert!(info.accc_is_fspx);
        assert_eq!(info.accc_size, 32);
        let fspx = info.fspx.expect("version 0 stub");
        assert_eq!(fspx.version, 0);
        assert!(fspx.sections.is_empty());
        assert!(fspx.stage_config.is_none());
        assert!(fspx.rdna_code.is_none());
    }

    #[test]
    fn extracts_synthetic_fspx_sections_stage_and_rdna_bounds() {
        let payload = synthetic_fspx_payload();
        assert_eq!(payload.len(), 104);
        assert_eq!(
            &payload[64..72],
            &[0xA0, 0x01, 0x00, 0x00, 0xA7, 0x00, 0x00, 0x00]
        );
        assert_eq!(&payload[84..100], b"SYNTH_RDNA2_CODE");
        assert_eq!(&payload[100..104], &[0x70, 0x07, 0x00, 0x00]);

        let blob = build_synthetic_dxbc(&[(b"ACCC", &payload)]);
        assert_eq!(blob.len(), 148);
        let info = inspect_dxbc(&blob).expect("section table");
        assert_eq!(info.kind, ShaderKind::AcccOnly);
        assert_eq!(info.stage, ShaderStage::Compute);
        assert_eq!((info.shader_model_major, info.shader_model_minor), (6, 0));
        assert_eq!(info.accc_size, 104);
        assert_eq!(info.container_size, 148);
        assert!(info.accc_is_fspx);

        let fspx = info.fspx.expect("fspx table");
        assert_eq!(fspx.version, 1);
        assert_eq!(
            fspx.sections,
            vec![
                FspxSection {
                    id: 0,
                    offset: 64,
                    size: 8
                },
                FspxSection {
                    id: 1,
                    offset: 72,
                    size: 12
                },
                FspxSection {
                    id: 6,
                    offset: 84,
                    size: 16
                },
                FspxSection {
                    id: 7,
                    offset: 100,
                    size: 4
                },
            ]
        );
        let section0 = fspx.section0.expect("section 0");
        assert_eq!((section0.offset, section0.size), (64, 8));
        let stage = fspx.stage_config.expect("section 1");
        assert_eq!(stage.stage, ShaderStage::Compute);
        assert_eq!((stage.offset, stage.size), (72, 12));
        assert_eq!((stage.shader_model_major, stage.shader_model_minor), (6, 0));
        let code = fspx.rdna_code.expect("section 6");
        assert_eq!((code.offset, code.size), (84, 16));
        assert_eq!(
            fspx.rdna_code_bytes(&payload),
            Some(b"SYNTH_RDNA2_CODE".as_slice())
        );
        let section7 = fspx.section7.expect("section 7");
        assert_eq!((section7.offset, section7.size), (100, 4));
    }

    #[test]
    fn dxil_stage_wins_over_fspx_stage_in_dual_container() {
        let dxil_payload = minimal_dxil_program((5u32 << 16) | (6u32 << 4) | 6u32);
        let mut fspx = synthetic_fspx_payload();
        // Section 1 stage word is at payload offset 72. Overwrite compute (6)
        // with the ABI pixel value 1, and shader model 6.0 with 5.1.
        fspx[72..76].copy_from_slice(&1u32.to_le_bytes());
        fspx[76..80].copy_from_slice(&5u32.to_le_bytes());
        fspx[80..84].copy_from_slice(&1u32.to_le_bytes());

        let blob = build_synthetic_dxbc(&[(b"DXIL", &dxil_payload), (b"ACCC", &fspx)]);
        let info = inspect_dxbc(&blob).expect("dual container");
        assert_eq!(info.kind, ShaderKind::DualDxilAndAccc);
        assert_eq!(info.stage, ShaderStage::Compute);
        assert_eq!((info.shader_model_major, info.shader_model_minor), (6, 6));
        let stage = info.fspx.expect("fspx").stage_config.expect("section 1");
        assert_eq!(stage.stage, ShaderStage::Pixel);
        assert_eq!((stage.shader_model_major, stage.shader_model_minor), (5, 1));
    }

    #[test]
    fn classifies_synthetic_legacy_shdr() {
        let tok: u32 = (1u32 << 16) | (5u32 << 4) | 1u32;
        let payload = tok.to_le_bytes();
        let blob = build_synthetic_dxbc(&[(b"SHDR", &payload)]);
        let info = inspect_dxbc(&blob).expect("legacy container");
        assert_eq!(info.kind, ShaderKind::LegacyDxbc);
        assert_eq!(info.stage, ShaderStage::Vertex);
        assert_eq!((info.shader_model_major, info.shader_model_minor), (5, 1));
        assert!(info.fspx.is_none());
    }

    #[test]
    fn rejects_invalid_magic_and_short_headers() {
        assert_eq!(inspect_dxbc(&[0u8; 64]), Err(ParseError::BadMagic));
        assert_eq!(inspect_dxbc(&[]), Err(ParseError::TooShort));
        assert_eq!(inspect_dxbc(b"DXBC"), Err(ParseError::TooShort));
        assert_eq!(inspect_dxbc(&[0u8; 31]), Err(ParseError::TooShort));

        let mut version = vec![0u8; 32];
        version[..4].copy_from_slice(b"DXBC");
        version[20..24].copy_from_slice(&2u32.to_le_bytes());
        assert_eq!(
            inspect_dxbc(&version),
            Err(ParseError::UnsupportedVersion(2))
        );
    }

    #[test]
    fn rejects_truncated_tables_and_chunk_bounds() {
        let mut huge = vec![0u8; 32];
        huge[..4].copy_from_slice(b"DXBC");
        huge[20..24].copy_from_slice(&1u32.to_le_bytes());
        huge[24..28].copy_from_slice(&32u32.to_le_bytes());
        huge[28..32].copy_from_slice(&u32::MAX.to_le_bytes());
        assert_eq!(inspect_dxbc(&huge), Err(ParseError::TruncatedChunkTable));

        let mut short_container = vec![0u8; 32];
        short_container[..4].copy_from_slice(b"DXBC");
        short_container[20..24].copy_from_slice(&1u32.to_le_bytes());
        short_container[24..28].copy_from_slice(&16u32.to_le_bytes());
        assert_eq!(
            inspect_dxbc(&short_container),
            Err(ParseError::TruncatedContainer)
        );

        let mut declared_too_big = vec![0u8; 32];
        declared_too_big[..4].copy_from_slice(b"DXBC");
        declared_too_big[20..24].copy_from_slice(&1u32.to_le_bytes());
        declared_too_big[24..28].copy_from_slice(&1_000u32.to_le_bytes());
        assert_eq!(
            inspect_dxbc(&declared_too_big),
            Err(ParseError::TruncatedContainer)
        );

        let mut wild_offset = vec![0u8; 36];
        wild_offset[..4].copy_from_slice(b"DXBC");
        wild_offset[20..24].copy_from_slice(&1u32.to_le_bytes());
        wild_offset[24..28].copy_from_slice(&36u32.to_le_bytes());
        wild_offset[28..32].copy_from_slice(&1u32.to_le_bytes());
        wild_offset[32..36].copy_from_slice(&0xFFFF_FFF0u32.to_le_bytes());
        assert_eq!(
            inspect_dxbc(&wild_offset),
            Err(ParseError::ChunkOutOfBounds { index: 0 })
        );

        let mut oversized = vec![0u8; 80];
        oversized[..4].copy_from_slice(b"DXBC");
        oversized[20..24].copy_from_slice(&1u32.to_le_bytes());
        oversized[24..28].copy_from_slice(&40u32.to_le_bytes());
        oversized[28..32].copy_from_slice(&1u32.to_le_bytes());
        oversized[32..36].copy_from_slice(&36u32.to_le_bytes());
        oversized[36..40].copy_from_slice(b"DXIL");
        oversized[40..44].copy_from_slice(&20u32.to_le_bytes());
        assert_eq!(
            inspect_dxbc(&oversized),
            Err(ParseError::ChunkOutOfBounds { index: 0 })
        );
    }

    #[test]
    fn rejects_malformed_fspx_sections() {
        let cases = [
            (
                fspx_header(1, 3, &[], 16),
                ParseError::FspxTruncated { chunk: 0 },
            ),
            (
                fspx_header(99, 0, &[], 16),
                ParseError::FspxBadVersion {
                    chunk: 0,
                    version: 99,
                },
            ),
            (
                fspx_header(0, 1, &[], 16),
                ParseError::FspxBadVersion {
                    chunk: 0,
                    version: 0,
                },
            ),
            (
                {
                    let mut payload = fspx_header(1, 1, &[(6, 64, u32::MAX)], 28);
                    payload.truncate(28);
                    payload
                },
                ParseError::FspxSectionOutOfBounds {
                    chunk: 0,
                    section_id: 6,
                },
            ),
            (
                fspx_header(1, 1, &[(1, 0, 4)], 28),
                ParseError::FspxSectionOutOfBounds {
                    chunk: 0,
                    section_id: 1,
                },
            ),
            (
                fspx_header(1, 2, &[(6, 40, 4), (6, 48, 4)], 52),
                ParseError::FspxDuplicateSection {
                    chunk: 0,
                    section_id: 6,
                },
            ),
            (
                fspx_header(1, 2, &[(0, 64, 16), (6, 72, 8)], 80),
                ParseError::FspxSectionOverlap {
                    chunk: 0,
                    section_id: 6,
                },
            ),
            (
                fspx_header(1, 1, &[(1, 28, 4)], 32),
                ParseError::FspxBadStageConfig { chunk: 0 },
            ),
        ];
        for (payload, expected) in cases {
            let blob = build_synthetic_dxbc(&[(b"ACCC", &payload)]);
            assert_eq!(inspect_dxbc(&blob), Err(expected));
        }

        let mut bad_stage = fspx_header(1, 1, &[(1, 28, 12)], 40);
        bad_stage[28..32].copy_from_slice(&99u32.to_le_bytes());
        let blob = build_synthetic_dxbc(&[(b"ACCC", &bad_stage)]);
        assert_eq!(
            inspect_dxbc(&blob),
            Err(ParseError::FspxBadStageConfig { chunk: 0 })
        );

        let mut big_major = fspx_header(1, 1, &[(1, 28, 12)], 40);
        big_major[28..32].copy_from_slice(&6u32.to_le_bytes());
        big_major[32..36].copy_from_slice(&256u32.to_le_bytes());
        let blob = build_synthetic_dxbc(&[(b"ACCC", &big_major)]);
        assert_eq!(
            inspect_dxbc(&blob),
            Err(ParseError::FspxBadStageConfig { chunk: 0 })
        );

        let magic_only = b"FSPX".to_vec();
        let blob = build_synthetic_dxbc(&[(b"ACCC", &magic_only)]);
        assert_eq!(
            inspect_dxbc(&blob),
            Err(ParseError::FspxTruncated { chunk: 0 })
        );
    }

    #[test]
    fn truncated_prefixes_do_not_panic() {
        let blobs = [
            synthetic_dxil_blob(),
            build_synthetic_dxbc(&[(b"ACCC", &synthetic_fspx_payload())]),
            vec![0xFF; 64],
        ];
        for blob in blobs {
            for len in 0..=blob.len() {
                let _ = inspect_dxbc(&blob[..len]);
            }
        }
    }

    #[test]
    fn c_abi_inspect_and_passthrough_round_trip() {
        clear_shader_log();
        let blob = synthetic_dxil_blob();
        let mut info = BxShaderInspection {
            kind: 99,
            stage: 99,
            shader_model_major: 99,
            shader_model_minor: 99,
            container_size: 99,
            dxil_chunk_size: 99,
            accc_chunk_size: 99,
            has_root_signature: 99,
            digest_hash: [0x11; 16],
        };
        let status = unsafe { bx_shader_inspect(blob.as_ptr(), blob.len(), &mut info) };
        assert_eq!(status, BX_SHADER_OK);
        assert_eq!(info.kind, 1);
        assert_eq!(info.stage, 6);
        assert_eq!(info.shader_model_major, 6);
        assert_eq!(info.shader_model_minor, 6);
        assert_eq!(info.container_size, 88);
        assert_eq!(info.dxil_chunk_size, 28);
        assert_eq!(info.accc_chunk_size, 0);
        assert_eq!(info.has_root_signature, 1);
        assert_eq!(info.digest_hash, [0xAB; 16]);

        let mut out_ptr: *mut u8 = ptr_sentinel();
        let mut out_len = 99usize;
        let status = unsafe {
            bx_shader_prepare_for_d3dmetal(blob.as_ptr(), blob.len(), &mut out_ptr, &mut out_len)
        };
        assert_eq!(status, BX_SHADER_OK);
        assert_eq!(out_len, 88);
        assert!(!out_ptr.is_null());
        let copied = unsafe { std::slice::from_raw_parts(out_ptr, out_len) };
        assert_eq!(copied, blob.as_slice());
        unsafe { bx_shader_free_buffer(out_ptr, out_len) };
        assert!(shader_log_lines().is_empty());
    }

    #[test]
    fn c_abi_accc_only_is_unsupported_and_logged() {
        clear_shader_log();
        let payload = synthetic_fspx_payload();
        let blob = build_synthetic_dxbc(&[(b"ACCC", &payload)]);
        assert_eq!(blob.len(), 148);

        let mut info = BxShaderInspection::default();
        let status = unsafe { bx_shader_inspect(blob.as_ptr(), blob.len(), &mut info) };
        assert_eq!(status, BX_SHADER_OK);
        assert_eq!(info.kind, 3);
        assert_eq!(info.stage, 6);
        assert_eq!(info.shader_model_major, 6);
        assert_eq!(info.shader_model_minor, 0);
        assert_eq!(info.accc_chunk_size, 104);
        assert_eq!(info.dxil_chunk_size, 0);
        assert_eq!(info.container_size, 148);
        assert_eq!(info.digest_hash, [0xAB; 16]);
        assert!(shader_log_lines().is_empty(), "inspect does not log");

        let mut out_ptr = ptr_sentinel();
        let mut out_len = 99usize;
        let status = unsafe {
            bx_shader_prepare_for_d3dmetal(blob.as_ptr(), blob.len(), &mut out_ptr, &mut out_len)
        };
        assert_eq!(status, BX_SHADER_ERR_UNSUPPORTED);
        assert!(out_ptr.is_null());
        assert_eq!(out_len, 0);

        let lines = shader_log_lines();
        assert_eq!(lines.len(), 1);
        let prepare = lines
            .iter()
            .find(|line| line.contains("event=prepare"))
            .expect("prepare log");
        for field in [
            "status=unsupported",
            "detail=no_dxil_passthrough",
            "kind=accc_only(3)",
            "stage=compute(6)",
            "sm_major=6",
            "sm_minor=0",
            "container_size=148",
            "dxil_chunk_size=0",
            "accc_chunk_size=104",
            "has_root_signature=0",
            "digest=abababababababababababababababab",
            "fspx=1",
            "fspx_version=1",
            "sections=0@64+8,1@72+12,6@84+16,7@100+4",
            "rdna_off=84",
            "rdna_size=16",
        ] {
            assert!(prepare.contains(field), "missing {field} in {prepare}");
        }
    }

    #[test]
    fn c_abi_legacy_is_unsupported_and_invalid_inputs_allocate_nothing() {
        clear_shader_log();
        let tok: u32 = (1u32 << 16) | (5u32 << 4) | 1u32;
        let blob = build_synthetic_dxbc(&[(b"SHDR", &tok.to_le_bytes())]);
        let mut out_ptr = ptr_sentinel();
        let mut out_len = 7usize;
        let status = unsafe {
            bx_shader_prepare_for_d3dmetal(blob.as_ptr(), blob.len(), &mut out_ptr, &mut out_len)
        };
        assert_eq!(status, BX_SHADER_ERR_UNSUPPORTED);
        assert!(out_ptr.is_null());
        assert_eq!(out_len, 0);
        assert!(shader_log_lines().is_empty());

        clear_shader_log();
        let mut info = BxShaderInspection {
            kind: 4,
            ..BxShaderInspection::default()
        };
        let status = unsafe { bx_shader_inspect(b"nope".as_ptr(), 4, &mut info) };
        assert_eq!(status, BX_SHADER_ERR_INVALID_BLOB);
        assert_eq!(info, BxShaderInspection::default());

        let mut out_ptr = ptr_sentinel();
        let mut out_len = 7usize;
        let status = unsafe {
            bx_shader_prepare_for_d3dmetal(b"nope".as_ptr(), 4, &mut out_ptr, &mut out_len)
        };
        assert_eq!(status, BX_SHADER_ERR_INVALID_BLOB);
        assert!(out_ptr.is_null());
        assert_eq!(out_len, 0);

        assert_eq!(
            unsafe { bx_shader_inspect(std::ptr::null(), 0, std::ptr::null_mut()) },
            BX_SHADER_ERR_INVALID_BLOB
        );
        assert_eq!(
            unsafe {
                bx_shader_prepare_for_d3dmetal(
                    std::ptr::null(),
                    0,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                )
            },
            BX_SHADER_ERR_INVALID_BLOB
        );
        unsafe { bx_shader_free_buffer(std::ptr::null_mut(), 0) };
        unsafe { bx_shader_free_buffer(std::ptr::null_mut(), 32) };
    }

    #[test]
    fn prepare_copies_container_size_not_trailing_bytes() {
        clear_shader_log();
        let mut blob = synthetic_dxil_blob();
        let container_size = blob.len();
        assert_eq!(container_size, 88);
        blob.extend_from_slice(&[0xEE, 0xEE, 0xEE, 0xEE]);

        let mut out_ptr = ptr_sentinel();
        let mut out_len = 0usize;
        let status = unsafe {
            bx_shader_prepare_for_d3dmetal(blob.as_ptr(), blob.len(), &mut out_ptr, &mut out_len)
        };
        assert_eq!(status, BX_SHADER_OK);
        assert_eq!(out_len, container_size);
        let copied = unsafe { std::slice::from_raw_parts(out_ptr, out_len) };
        assert_eq!(copied, &blob[..container_size]);
        unsafe { bx_shader_free_buffer(out_ptr, out_len) };
        assert!(shader_log_lines().is_empty());
    }

    #[test]
    fn prepare_passes_dxil_through_when_fspx_is_malformed() {
        clear_shader_log();
        let dxil_payload = minimal_dxil_program((5u32 << 16) | (6u32 << 4) | 6u32);
        let bad_fspx = fspx_header(99, 0, &[], 16);
        let mut blob = build_synthetic_dxbc(&[(b"DXIL", &dxil_payload), (b"ACCC", &bad_fspx)]);
        let container_size = blob.len();
        blob.extend_from_slice(&[0xEE; 4]);

        assert_eq!(
            inspect_dxbc(&blob),
            Err(ParseError::FspxBadVersion {
                chunk: 1,
                version: 99,
            })
        );

        let mut info = BxShaderInspection {
            kind: 7,
            ..BxShaderInspection::default()
        };
        let status = unsafe { bx_shader_inspect(blob.as_ptr(), blob.len(), &mut info) };
        assert_eq!(status, BX_SHADER_ERR_INVALID_BLOB);
        assert_eq!(info, BxShaderInspection::default());
        assert!(shader_log_lines().is_empty());

        let mut out_ptr = ptr_sentinel();
        let mut out_len = 1usize;
        let status = unsafe {
            bx_shader_prepare_for_d3dmetal(blob.as_ptr(), blob.len(), &mut out_ptr, &mut out_len)
        };
        assert_eq!(status, BX_SHADER_OK);
        assert_eq!(out_len, container_size);
        let copied = unsafe { std::slice::from_raw_parts(out_ptr, out_len) };
        assert_eq!(copied, &blob[..container_size]);
        unsafe { bx_shader_free_buffer(out_ptr, out_len) };
        assert!(shader_log_lines().is_empty());
    }

    #[test]
    fn fspx_section_count_is_bounded() {
        let at_limit = fspx_table_with_sections(FSPX_MAX_SECTIONS as u32);
        let blob = build_synthetic_dxbc(&[(b"ACCC", &at_limit)]);
        let info = inspect_dxbc(&blob).expect("64 sections");
        let fspx = info.fspx.expect("table");
        assert_eq!(fspx.sections.len(), FSPX_MAX_SECTIONS);

        let mut c_info = BxShaderInspection::default();
        let status = unsafe { bx_shader_inspect(blob.as_ptr(), blob.len(), &mut c_info) };
        assert_eq!(status, BX_SHADER_OK);
        assert_eq!(c_info.kind, 3);

        let over = fspx_table_with_sections(FSPX_MAX_SECTIONS as u32 + 1);
        let blob = build_synthetic_dxbc(&[(b"ACCC", &over)]);
        assert_eq!(
            inspect_dxbc(&blob),
            Err(ParseError::FspxTooManySections {
                chunk: 0,
                count: FSPX_MAX_SECTIONS as u32 + 1,
            })
        );
        let status = unsafe { bx_shader_inspect(blob.as_ptr(), blob.len(), &mut c_info) };
        assert_eq!(status, BX_SHADER_ERR_INVALID_BLOB);
    }

    #[test]
    fn c_abi_empty_dxil_chunk_returns_invalid() {
        clear_shader_log();
        let blob = build_synthetic_dxbc(&[(b"DXIL", &[])]);
        assert_eq!(
            inspect_dxbc(&blob),
            Err(ParseError::BadDxilHeader { chunk: 0 })
        );

        let mut out_ptr = ptr_sentinel();
        let mut out_len = 8usize;
        let status = unsafe {
            bx_shader_prepare_for_d3dmetal(blob.as_ptr(), blob.len(), &mut out_ptr, &mut out_len)
        };
        assert_eq!(status, BX_SHADER_ERR_INVALID_BLOB);
        assert!(out_ptr.is_null());
        assert_eq!(out_len, 0);
        assert!(shader_log_lines().is_empty());
    }

    #[test]
    fn c_abi_zero_bitcode_dxil_returns_invalid() {
        clear_shader_log();
        let mut payload = minimal_dxil_program((5u32 << 16) | (6u32 << 4) | 6u32);
        payload[20..24].copy_from_slice(&0u32.to_le_bytes());
        let blob = build_synthetic_dxbc(&[(b"DXIL", &payload)]);
        assert_eq!(
            inspect_dxbc(&blob),
            Err(ParseError::BadDxilHeader { chunk: 0 })
        );

        let mut out_ptr = ptr_sentinel();
        let mut out_len = 8usize;
        let status = unsafe {
            bx_shader_prepare_for_d3dmetal(blob.as_ptr(), blob.len(), &mut out_ptr, &mut out_len)
        };
        assert_eq!(status, BX_SHADER_ERR_INVALID_BLOB);
        assert!(out_ptr.is_null());
        assert_eq!(out_len, 0);
        assert!(shader_log_lines().is_empty());
    }

    #[test]
    fn dxil_bitcode_offset_is_relative_to_bitcode_header() {
        let prog = (5u32 << 16) | (6u32 << 4) | 6u32;
        let accepted = minimal_dxil_program(prog);
        assert_eq!(&accepted[16..20], &16u32.to_le_bytes());
        assert_eq!(&accepted[20..24], &4u32.to_le_bytes());
        let blob = build_synthetic_dxbc(&[(b"DXIL", &accepted)]);
        let info = inspect_dxbc(&blob).expect("DXC-shaped offset 16");
        assert_eq!(info.kind, ShaderKind::Dxil);
        assert_eq!(info.stage, ShaderStage::Compute);

        let mut rejected = accepted;
        rejected[16..20].copy_from_slice(&15u32.to_le_bytes());
        let blob = build_synthetic_dxbc(&[(b"DXIL", &rejected)]);
        assert_eq!(
            inspect_dxbc(&blob),
            Err(ParseError::BadDxilHeader { chunk: 0 })
        );
    }

    #[test]
    fn later_fspx_stage_is_used_when_earlier_table_has_none() {
        clear_shader_log();
        let first = fspx_header(1, 0, &[], 16);
        let second = fspx_compute_stage();
        let blob = build_synthetic_dxbc(&[(b"ACCC", &first), (b"ACCC", &second)]);

        let info = inspect_dxbc(&blob).expect("two ACCC chunks");
        assert_eq!(info.stage, ShaderStage::Compute);
        assert_eq!((info.shader_model_major, info.shader_model_minor), (6, 0));
        assert!(info.fspx.expect("first table").stage_config.is_none());

        let mut c_info = BxShaderInspection::default();
        let status = unsafe { bx_shader_inspect(blob.as_ptr(), blob.len(), &mut c_info) };
        assert_eq!(status, BX_SHADER_OK);
        assert_eq!(c_info.stage, 6);
        assert_eq!(c_info.shader_model_major, 6);
        assert_eq!(c_info.shader_model_minor, 0);

        let mut out_ptr = ptr_sentinel();
        let mut out_len = 3usize;
        let status = unsafe {
            bx_shader_prepare_for_d3dmetal(blob.as_ptr(), blob.len(), &mut out_ptr, &mut out_len)
        };
        assert_eq!(status, BX_SHADER_ERR_UNSUPPORTED);
        assert!(out_ptr.is_null());
        assert_eq!(out_len, 0);
        let log = shader_log_lines();
        assert_eq!(log.len(), 1);
        assert!(log[0].contains("stage=compute(6)"));
        assert!(log[0].contains("sm_major=6"));
        assert!(log[0].contains("sm_minor=0"));
    }

    fn fspx_table_with_sections(count: u32) -> Vec<u8> {
        let count_usize = count as usize;
        let table_end = 16 + count_usize * 12;
        let mut payload = vec![0u8; table_end];
        payload[..4].copy_from_slice(b"FSPX");
        payload[4..8].copy_from_slice(&1u32.to_le_bytes());
        payload[8..12].copy_from_slice(&count.to_le_bytes());
        let end = table_end as u32;
        for index in 0..count_usize {
            let at = 16 + index * 12;
            let id = 100 + index as u32;
            payload[at..at + 4].copy_from_slice(&id.to_le_bytes());
            payload[at + 4..at + 8].copy_from_slice(&end.to_le_bytes());
            payload[at + 8..at + 12].copy_from_slice(&0u32.to_le_bytes());
        }
        payload
    }

    fn fspx_compute_stage() -> Vec<u8> {
        let mut payload = vec![0u8; 40];
        payload[..4].copy_from_slice(b"FSPX");
        payload[4..8].copy_from_slice(&1u32.to_le_bytes());
        payload[8..12].copy_from_slice(&1u32.to_le_bytes());
        payload[16..20].copy_from_slice(&1u32.to_le_bytes());
        payload[20..24].copy_from_slice(&28u32.to_le_bytes());
        payload[24..28].copy_from_slice(&12u32.to_le_bytes());
        payload[28..32].copy_from_slice(&6u32.to_le_bytes());
        payload[32..36].copy_from_slice(&6u32.to_le_bytes());
        payload
    }

    fn ptr_sentinel() -> *mut u8 {
        std::ptr::dangling_mut()
    }
}

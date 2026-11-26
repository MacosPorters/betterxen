//! C ABI entry points from `abi/include/bx_shader_bridge.h`.
//!
//! `bx_shader_inspect` classifies a blob and fills `bx_shader_inspection_t`
//! without logging or retaining chunk lists. `bx_shader_prepare_for_d3dmetal`
//! copies `container_size` bytes when the container has a `DXIL` program
//! header, even if an `FSPX` table in the same container is malformed. An
//! empty `DXIL` chunk is `BX_SHADER_ERR_INVALID_BLOB`. It returns
//! `BX_SHADER_ERR_UNSUPPORTED` for ACCC-only input and writes that inspection
//! record to stderr. Other statuses are not logged. RDNA 2 to DXIL translation
//! is not implemented. `bx_shader_free_buffer` releases a buffer returned by
//! prepare.
//!
//! The shim logger `bx_log_emit` (declared in `abi/include/bx_log.h`) is
//! provided by `bx_runtime_veh.dll`. This crate does not link that DLL. A
//! failed stderr write is ignored so it cannot abort the host.

use std::alloc::{Layout, alloc, dealloc};
#[cfg(test)]
use std::cell::RefCell;
use std::io::Write;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::ptr;

use super::{
    DxbcSummary, ShaderInspection, ShaderKind, ShaderStage, dxil_container_len, inspect_dxbc,
    summarize_dxbc,
};

pub const BX_SHADER_OK: i32 = 0;
pub const BX_SHADER_ERR_INVALID_BLOB: i32 = 1;
pub const BX_SHADER_ERR_UNSUPPORTED: i32 = 2;
pub const BX_SHADER_ERR_TRANSLATE: i32 = 3;
pub const BX_SHADER_ERR_OOM: i32 = 4;

const _: () = {
    assert!(BX_SHADER_OK == 0);
    assert!(BX_SHADER_ERR_INVALID_BLOB == 1);
    assert!(BX_SHADER_ERR_UNSUPPORTED == 2);
    assert!(BX_SHADER_ERR_TRANSLATE == 3);
    assert!(BX_SHADER_ERR_OOM == 4);
};

/// Rust layout of `bx_shader_inspection_t`.
///
/// The frozen header uses C enums, which are 4-byte `int` values on the
/// mingw-w64 and Apple targets this ABI is written for, then six `uint32_t`
/// fields and a 16-byte digest. The struct is 48 bytes with 4-byte alignment.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct BxShaderInspection {
    pub kind: i32,
    pub stage: i32,
    pub shader_model_major: u32,
    pub shader_model_minor: u32,
    pub container_size: u32,
    pub dxil_chunk_size: u32,
    pub accc_chunk_size: u32,
    pub has_root_signature: u32,
    pub digest_hash: [u8; 16],
}

const _: () = {
    assert!(std::mem::size_of::<BxShaderInspection>() == 48);
    assert!(std::mem::align_of::<BxShaderInspection>() == 4);
    assert!(std::mem::offset_of!(BxShaderInspection, kind) == 0);
    assert!(std::mem::offset_of!(BxShaderInspection, stage) == 4);
    assert!(std::mem::offset_of!(BxShaderInspection, shader_model_major) == 8);
    assert!(std::mem::offset_of!(BxShaderInspection, shader_model_minor) == 12);
    assert!(std::mem::offset_of!(BxShaderInspection, container_size) == 16);
    assert!(std::mem::offset_of!(BxShaderInspection, dxil_chunk_size) == 20);
    assert!(std::mem::offset_of!(BxShaderInspection, accc_chunk_size) == 24);
    assert!(std::mem::offset_of!(BxShaderInspection, has_root_signature) == 28);
    assert!(std::mem::offset_of!(BxShaderInspection, digest_hash) == 32);
};

#[cfg(test)]
thread_local! {
    static SHADER_LOG: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

#[cfg(test)]
pub(crate) fn clear_shader_log() {
    SHADER_LOG.with(|log| log.borrow_mut().clear());
}

#[cfg(test)]
pub(crate) fn shader_log_lines() -> Vec<String> {
    SHADER_LOG.with(|log| log.borrow().clone())
}

fn emit(line: String) {
    let _ = writeln!(std::io::stderr().lock(), "{line}");
    #[cfg(test)]
    SHADER_LOG.with(|log| log.borrow_mut().push(line));
}

fn kind_name(kind: ShaderKind) -> &'static str {
    match kind {
        ShaderKind::Unknown => "unknown",
        ShaderKind::Dxil => "dxil",
        ShaderKind::LegacyDxbc => "legacy_dxbc",
        ShaderKind::AcccOnly => "accc_only",
        ShaderKind::DualDxilAndAccc => "dual",
    }
}

fn stage_name(stage: ShaderStage) -> &'static str {
    match stage {
        ShaderStage::Unknown => "unknown",
        ShaderStage::Pixel => "pixel",
        ShaderStage::Vertex => "vertex",
        ShaderStage::Geometry => "geometry",
        ShaderStage::Hull => "hull",
        ShaderStage::Domain => "domain",
        ShaderStage::Compute => "compute",
        ShaderStage::Library => "library",
        ShaderStage::Mesh => "mesh",
        ShaderStage::Amplification => "amplification",
    }
}

fn abi_kind(kind: ShaderKind) -> i32 {
    match kind {
        ShaderKind::Unknown => 0,
        ShaderKind::Dxil => 1,
        ShaderKind::LegacyDxbc => 2,
        ShaderKind::AcccOnly => 3,
        ShaderKind::DualDxilAndAccc => 4,
    }
}

fn abi_stage(stage: ShaderStage) -> i32 {
    match stage {
        ShaderStage::Unknown => 0,
        ShaderStage::Pixel => 1,
        ShaderStage::Vertex => 2,
        ShaderStage::Geometry => 3,
        ShaderStage::Hull => 4,
        ShaderStage::Domain => 5,
        ShaderStage::Compute => 6,
        ShaderStage::Library => 7,
        ShaderStage::Mesh => 8,
        ShaderStage::Amplification => 9,
    }
}

fn hex_digest(bytes: &[u8; 16]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(32);
    for byte in bytes {
        out.push(HEX[usize::from(byte >> 4)] as char);
        out.push(HEX[usize::from(byte & 0x0f)] as char);
    }
    out
}

fn format_inspection(event: &str, status: &str, detail: &str, info: &ShaderInspection) -> String {
    let (fspx, version, sections, rdna_off, rdna_size) = if let Some(fspx) = &info.fspx {
        let sections = if fspx.sections.is_empty() {
            "none".to_string()
        } else {
            fspx.sections
                .iter()
                .map(|section| format!("{}@{}+{}", section.id, section.offset, section.size))
                .collect::<Vec<_>>()
                .join(",")
        };
        let (rdna_off, rdna_size) = match fspx.rdna_code {
            Some(code) => (code.offset.to_string(), code.size.to_string()),
            None => ("none".to_string(), "none".to_string()),
        };
        ("1", fspx.version.to_string(), sections, rdna_off, rdna_size)
    } else {
        (
            "0",
            "none".to_string(),
            "none".to_string(),
            "none".to_string(),
            "none".to_string(),
        )
    };
    format!(
        "bx-shader event={event} status={status} detail={detail} kind={}({}) stage={}({}) sm_major={} sm_minor={} container_size={} dxil_chunk_size={} accc_chunk_size={} has_root_signature={} digest={} fspx={fspx} fspx_version={version} sections={sections} rdna_off={rdna_off} rdna_size={rdna_size}",
        kind_name(info.kind),
        abi_kind(info.kind),
        stage_name(info.stage),
        abi_stage(info.stage),
        info.shader_model_major,
        info.shader_model_minor,
        info.container_size,
        info.dxil_size,
        info.accc_size,
        u32::from(info.has_root_signature),
        hex_digest(&info.digest),
    )
}

fn log_accc_unsupported(info: &ShaderInspection) {
    emit(format_inspection(
        "prepare",
        "unsupported",
        "no_dxil_passthrough",
        info,
    ));
}

fn to_c_inspection(info: &DxbcSummary) -> BxShaderInspection {
    BxShaderInspection {
        kind: abi_kind(info.kind),
        stage: abi_stage(info.stage),
        shader_model_major: u32::from(info.shader_model_major),
        shader_model_minor: u32::from(info.shader_model_minor),
        container_size: info.container_size,
        dxil_chunk_size: info.dxil_size,
        accc_chunk_size: info.accc_size,
        has_root_signature: u32::from(info.has_root_signature),
        digest_hash: info.digest,
    }
}

fn input_bytes<'a>(bytes: *const u8, len: usize) -> Option<&'a [u8]> {
    if len == 0 {
        return Some(&[]);
    }
    if bytes.is_null() {
        return None;
    }
    // SAFETY: The caller of the unsafe entry point guarantees `bytes` is
    // readable for `len` bytes when it is non-null. `len` is non-zero here.
    Some(unsafe { std::slice::from_raw_parts(bytes, len) })
}

fn alloc_copy(src: &[u8]) -> Result<*mut u8, ()> {
    if src.is_empty() {
        return Err(());
    }
    let layout = Layout::from_size_align(src.len(), 1).map_err(|_| ())?;
    // SAFETY: `layout` has a non-zero size and alignment 1.
    let ptr = unsafe { alloc(layout) };
    if ptr.is_null() {
        return Err(());
    }
    // SAFETY: `alloc` returned a writable block of `src.len()` bytes, and
    // `src` is a valid readable region of that length. The regions do not
    // overlap.
    unsafe {
        ptr::copy_nonoverlapping(src.as_ptr(), ptr, src.len());
    }
    Ok(ptr)
}

fn guard(body: impl FnOnce() -> i32 + std::panic::UnwindSafe) -> i32 {
    match catch_unwind(body) {
        Ok(status) => status,
        Err(_) => BX_SHADER_ERR_INVALID_BLOB,
    }
}

fn inspect_status(bytes: *const u8, len: usize, out_info: *mut BxShaderInspection) -> i32 {
    if out_info.is_null() {
        return BX_SHADER_ERR_INVALID_BLOB;
    }
    let Some(bytes) = input_bytes(bytes, len) else {
        // SAFETY: `out_info` was checked non-null and the caller guarantees it
        // is writable for one `BxShaderInspection`.
        unsafe { out_info.write(BxShaderInspection::default()) };
        return BX_SHADER_ERR_INVALID_BLOB;
    };
    match summarize_dxbc(bytes) {
        Ok(info) => {
            let c_info = to_c_inspection(&info);
            // SAFETY: `out_info` is non-null and writable for one inspection.
            unsafe { out_info.write(c_info) };
            BX_SHADER_OK
        }
        Err(_) => {
            // SAFETY: `out_info` is non-null and writable for one inspection.
            unsafe { out_info.write(BxShaderInspection::default()) };
            BX_SHADER_ERR_INVALID_BLOB
        }
    }
}

fn prepare_status(
    in_bytes: *const u8,
    in_len: usize,
    out_bytes: *mut *mut u8,
    out_len: *mut usize,
) -> i32 {
    if out_bytes.is_null() || out_len.is_null() {
        return BX_SHADER_ERR_INVALID_BLOB;
    }
    // SAFETY: both pointers were checked non-null and the caller guarantees
    // they are writable.
    unsafe {
        *out_bytes = ptr::null_mut();
        *out_len = 0;
    }
    let Some(bytes) = input_bytes(in_bytes, in_len) else {
        return BX_SHADER_ERR_INVALID_BLOB;
    };
    let container_size = match dxil_container_len(bytes) {
        Ok(Some(size)) => size,
        Ok(None) => {
            return match inspect_dxbc(bytes) {
                Ok(info) if info.kind == ShaderKind::AcccOnly => {
                    log_accc_unsupported(&info);
                    BX_SHADER_ERR_UNSUPPORTED
                }
                Ok(_) => BX_SHADER_ERR_UNSUPPORTED,
                Err(_) => BX_SHADER_ERR_INVALID_BLOB,
            };
        }
        Err(_) => return BX_SHADER_ERR_INVALID_BLOB,
    };
    let Some(len) = usize::try_from(container_size).ok() else {
        return BX_SHADER_ERR_INVALID_BLOB;
    };
    if len == 0 || len > bytes.len() {
        return BX_SHADER_ERR_INVALID_BLOB;
    }
    let ptr = match alloc_copy(&bytes[..len]) {
        Ok(ptr) => ptr,
        Err(()) => return BX_SHADER_ERR_OOM,
    };
    // SAFETY: `out_bytes` and `out_len` are non-null and writable. `len` is
    // the container size, which is not larger than `bytes`.
    unsafe {
        *out_bytes = ptr;
        *out_len = len;
    }
    BX_SHADER_OK
}

/// Inspect `bytes` without allocating.
///
/// # Safety
///
/// When `bytes` is non-null it must be readable for `len` bytes. A null
/// `bytes` pointer is not dereferenced. `out_info` must be non-null and
/// writable; a null `out_info` returns [`BX_SHADER_ERR_INVALID_BLOB`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bx_shader_inspect(
    bytes: *const u8,
    len: usize,
    out_info: *mut BxShaderInspection,
) -> i32 {
    let status = guard(AssertUnwindSafe(|| inspect_status(bytes, len, out_info)));
    if status == BX_SHADER_ERR_INVALID_BLOB && !out_info.is_null() {
        // SAFETY: a non-null `out_info` is writable for one inspection. This
        // also covers a panic after a partial write.
        unsafe { out_info.write(BxShaderInspection::default()) };
    }
    status
}

/// Copy a DXIL container, or report that an ACCC-only container cannot be
/// prepared for D3DMetal.
///
/// On [`BX_SHADER_OK`], `*out_bytes` is a copy of the first `container_size`
/// bytes and `*out_len` is that size. Bytes past `container_size` are not
/// copied. A `DXIL` chunk is copied even when an `FSPX` table in the same
/// container is malformed. The caller frees the pointer with
/// [`bx_shader_free_buffer`]. On every other status, `*out_bytes` is null and
/// `*out_len` is 0 when the output pointers are non-null.
///
/// # Safety
///
/// When `in_bytes` is non-null it must be readable for `in_len` bytes.
/// `out_bytes` and `out_len` must both be non-null and writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bx_shader_prepare_for_d3dmetal(
    in_bytes: *const u8,
    in_len: usize,
    out_bytes: *mut *mut u8,
    out_len: *mut usize,
) -> i32 {
    let status = guard(AssertUnwindSafe(|| {
        prepare_status(in_bytes, in_len, out_bytes, out_len)
    }));
    if status != BX_SHADER_OK && !out_bytes.is_null() && !out_len.is_null() {
        // SAFETY: the caller guarantees both pointers are writable. A panic
        // after publication drops the copy instead of returning it.
        unsafe {
            let ptr = *out_bytes;
            let len = *out_len;
            *out_bytes = ptr::null_mut();
            *out_len = 0;
            if !ptr.is_null() && len > 0 {
                free_buffer(ptr, len);
            }
        }
    }
    status
}

fn free_buffer(ptr: *mut u8, len: usize) {
    if ptr.is_null() || len == 0 {
        return;
    }
    let Ok(layout) = Layout::from_size_align(len, 1) else {
        return;
    };
    // SAFETY: The caller guarantees `ptr` was allocated by `alloc_copy` with
    // this layout (size `len`, alignment 1) and has not been freed.
    unsafe { dealloc(ptr, layout) };
}

/// Free a buffer allocated by [`bx_shader_prepare_for_d3dmetal`].
///
/// A null `ptr` or a `len` of 0 is a no-op. `len` must be the length returned
/// in `out_len` for that pointer. The allocation uses alignment 1.
///
/// # Safety
///
/// When `ptr` is non-null and `len` is non-zero, `ptr` must have been returned
/// by [`bx_shader_prepare_for_d3dmetal`] and `len` must be the matching length.
/// The pointer must not be freed twice.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bx_shader_free_buffer(ptr: *mut u8, len: usize) {
    let _ = catch_unwind(AssertUnwindSafe(|| free_buffer(ptr, len)));
}

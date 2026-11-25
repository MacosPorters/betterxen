//! Xbox Series shaders to host shaders.
//!
//! An Xbox Series shader is a standard DXBC container ([`dxbc`]). Its `DXIL`
//! part ([`dxil`]) may carry Xbox additions around the LLVM bitcode: an
//! `XDAT` table before it indexing blobs after it, among them `xbop`
//! (compiler options) and the zlib-compressed GFX10 code. The bitcode may use
//! console-only intrinsics, so the GFX10 code is translated in general.

#[rustfmt::skip] // BetterXen: vendored from upstream, kept diffable (see ../UPSTREAM.md)
pub mod bvh;
#[rustfmt::skip] // BetterXen: vendored from upstream, kept diffable (see ../UPSTREAM.md)
pub mod dxbc;
#[rustfmt::skip] // BetterXen: vendored from upstream, kept diffable (see ../UPSTREAM.md)
pub mod dxil;
#[rustfmt::skip] // BetterXen: vendored from upstream, kept diffable (see ../UPSTREAM.md)
pub mod emit;
#[rustfmt::skip] // BetterXen: vendored from upstream, kept diffable (see ../UPSTREAM.md)
pub mod gfx10;
#[rustfmt::skip] // BetterXen: vendored from upstream, kept diffable (see ../UPSTREAM.md)
pub mod io;
#[rustfmt::skip] // BetterXen: vendored from upstream, kept diffable (see ../UPSTREAM.md)
pub mod ir;
#[rustfmt::skip] // BetterXen: vendored from upstream, kept diffable (see ../UPSTREAM.md)
pub mod native;
#[rustfmt::skip] // BetterXen: vendored from upstream, kept diffable (see ../UPSTREAM.md)
pub mod resource;
#[rustfmt::skip] // BetterXen: vendored from upstream, kept diffable (see ../UPSTREAM.md)
pub mod spill;
#[rustfmt::skip] // BetterXen: vendored from upstream, kept diffable (see ../UPSTREAM.md)
pub mod structure;

pub use dxbc::{Container, FourCC, Part};
pub use dxil::{DxilProgram, ShaderKind, XboxExtras};
pub use native::{
    HwStage, NativeProgram, ResourceType, RootParam, RootType, UsedResource, native_programs,
};

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ShaderError {
    #[error("not a DXBC container")]
    NotDxbc,
    #[error("malformed shader: {0}")]
    Malformed(&'static str),
    #[error("GFX10 code at {offset:#x}: {reason}")]
    Isa { offset: usize, reason: &'static str },
    /// Valid code the translator cannot handle yet: an opcode name or a construct.
    #[error("GFX10 code at {offset:#x}: {what} is not supported")]
    Unsupported { offset: usize, what: &'static str },
}

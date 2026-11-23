//! Typed operands and instruction forms for the bounded gfx1030 compute and graphics subsets.

use std::fmt;

mod rdna2;
pub use rdna2::{
    BUFFER_LOAD_DWORD, BUFFER_STORE_DWORD, EXP, IMAGE_SAMPLE_LZ, S_BRANCH, S_CBRANCH_EXECZ,
    S_CBRANCH_SCC0, S_CBRANCH_VCCZ, S_CMP_EQ_U32, S_ENDPGM, S_MOV_B32, S_WAITCNT, S_WAITCNT_VSCNT,
    V_ADD_F32, V_ADD_NC_U32, V_AND_B32, V_ASHRREV_I32, V_INTERP_P1_F32, V_INTERP_P2_F32,
    V_LSHLREV_B32, V_LSHRREV_B32, V_MOV_B32, V_MUL_F32, V_OR_B32, V_XOR_B32,
};

/// Selects the instruction encoding independently of the container and execution ABI.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Architecture {
    /// LLVM's gfx1030 target for AMD RDNA 2.
    Gfx1030,
}

/// A generated first-word opcode pattern and its base length, before literal extensions.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Opcode {
    mask: u32,
    bits: u32,
    bytes: u8,
}

impl Opcode {
    /// Checks the format and opcode bits. The decoder must still validate operands.
    pub fn matches(self, word: u32) -> bool {
        word & self.mask == self.bits
    }

    /// Returns the base instruction size in bytes.
    pub fn byte_len(self) -> u8 {
        self.bytes
    }
}

/// A 32-bit source for an unmodified vector move.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VectorSource {
    /// VGPR index.
    Register(u8),
    /// Inline integer or literal word, represented by its bits.
    Immediate(u32),
}

impl fmt::Display for VectorSource {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Register(register) => write!(formatter, "v{register}"),
            Self::Immediate(value) => write!(formatter, "0x{value:x}"),
        }
    }
}

/// A scalar 32-bit source.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScalarSource {
    /// A normal SGPR index.
    Register(u8),
    /// Inline integer or literal word, represented by its bits.
    Immediate(u32),
}

impl fmt::Display for ScalarSource {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::Register(register) => write!(formatter, "s{register}"),
            Self::Immediate(value) => match value as i32 {
                integer @ -16..=64 => write!(formatter, "{integer}"),
                _ => write!(formatter, "0x{value:x}"),
            },
        }
    }
}

/// Output targeted by a supported uncompressed four-component export.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExportTarget {
    /// Vertex clip position.
    Position0,
    /// Vertex parameter slot zero.
    Parameter0,
    /// Pixel color attachment zero.
    RenderTarget0,
}

/// Phase of a pixel parameter interpolation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InterpolationPhase {
    /// Initialize the destination with the base and I-coordinate terms.
    First,
    /// Add the J-coordinate term to the destination.
    Second,
}

/// Instruction forms accepted by the gfx1030 decoder.
///
/// Vector operations use VGPR indices and scalar operations use SGPR indices.
/// The buffer `resource` is the first SGPR of its four-word descriptor.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(missing_docs)]
pub enum Instruction {
    /// Indexed 32-bit buffer read into a VGPR.
    BufferLoadDword { data: u8, index: u8, resource: u8 },
    /// Indexed 32-bit buffer write from a VGPR.
    BufferStoreDword { data: u8, index: u8, resource: u8 },
    /// `s_waitcnt vmcnt(0)`.
    WaitLoad,
    /// `s_waitcnt_vscnt null, 0`.
    WaitStore,
    /// VOP1 unmodified 32-bit vector move.
    MoveB32 { dst: u8, src: VectorSource },
    /// VOP3 unsigned 32-bit addition with no carry or input/output modifiers.
    AddNoCarryU32 { dst: u8, src0: u8, src1: u8 },
    /// SOP1 32-bit move to a normal SGPR or an EXEC/VCC half.
    ScalarMoveB32 { dst: u8, src: ScalarSource },
    /// SOPC 32-bit equality comparison writing SCC.
    ScalarCompareEqU32 {
        src0: ScalarSource,
        src1: ScalarSource,
    },
    /// SOPP jump to a stream byte offset when SCC is clear.
    BranchSccZero { target: usize },
    /// SOPP jump when the Wave32 execution mask is empty.
    BranchExecZero { target: usize },
    /// SOPP jump when the Wave32 vector condition mask is empty.
    BranchVccZero { target: usize },
    /// SOPP unconditional jump to a stream byte offset.
    Branch { target: usize },
    /// VOP2 logical left shift with the count in src0.
    VectorLshlrevB32 { dst: u8, src0: u8, src1: u8 },
    /// VOP2 logical right shift with the count in src0.
    VectorLshrrevB32 { dst: u8, src0: u8, src1: u8 },
    /// VOP2 arithmetic right shift with the count in src0.
    VectorAshrrevI32 { dst: u8, src0: u8, src1: u8 },
    /// VOP2 bitwise AND.
    VectorAndB32 { dst: u8, src0: u8, src1: u8 },
    /// VOP2 bitwise OR.
    VectorOrB32 { dst: u8, src0: u8, src1: u8 },
    /// VOP2 bitwise XOR.
    VectorXorB32 { dst: u8, src0: u8, src1: u8 },
    /// VOP2 single-precision addition. Execution requires an explicit FP mode.
    VectorAddF32 { dst: u8, src0: u8, src1: u8 },
    /// VOP2 single-precision multiplication. Execution requires an explicit FP mode.
    VectorMulF32 { dst: u8, src0: u8, src1: u8 },
    /// Full, uncompressed four-component graphics export.
    Export {
        target: ExportTarget,
        sources: [u8; 4],
    },
    /// Parameter interpolation with a source barycentric coordinate and an attribute component.
    Interpolate {
        phase: InterpolationPhase,
        dst: u8,
        coordinate: u8,
        attribute: u8,
        component: u8,
    },
    /// MIMG image_sample_lz 2D RGBA8 texture sample with level of detail zero.
    ImageSampleLz {
        /// Four contiguous destination VGPRs [dst..dst+3] receiving RGBA.
        dst: u8,
        /// Two contiguous coordinate VGPRs [coord..coord+1] containing (u, v).
        coord: u8,
        /// First SGPR of the eight-word resource descriptor s[resource..resource+7].
        resource: u8,
        /// First SGPR of the four-word sampler descriptor s[sampler..sampler+3].
        sampler: u8,
    },
    /// Terminates the wave and waits for outstanding memory operations.
    EndProgram,
}

impl Instruction {
    /// Returns the mnemonic for the exact encoding family represented.
    pub fn mnemonic(self) -> &'static str {
        match self {
            Self::BufferLoadDword { .. } => "buffer_load_dword",
            Self::BufferStoreDword { .. } => "buffer_store_dword",
            Self::WaitLoad => "s_waitcnt",
            Self::WaitStore => "s_waitcnt_vscnt",
            Self::MoveB32 { .. } => "v_mov_b32_e32",
            Self::AddNoCarryU32 { .. } => "v_add_nc_u32_e64",
            Self::ScalarMoveB32 { .. } => "s_mov_b32",
            Self::ScalarCompareEqU32 { .. } => "s_cmp_eq_u32",
            Self::BranchSccZero { .. } => "s_cbranch_scc0",
            Self::BranchExecZero { .. } => "s_cbranch_execz",
            Self::BranchVccZero { .. } => "s_cbranch_vccz",
            Self::Branch { .. } => "s_branch",
            Self::VectorLshlrevB32 { .. } => "v_lshlrev_b32_e32",
            Self::VectorLshrrevB32 { .. } => "v_lshrrev_b32_e32",
            Self::VectorAshrrevI32 { .. } => "v_ashrrev_i32_e32",
            Self::VectorAndB32 { .. } => "v_and_b32_e32",
            Self::VectorOrB32 { .. } => "v_or_b32_e32",
            Self::VectorXorB32 { .. } => "v_xor_b32_e32",
            Self::VectorAddF32 { .. } => "v_add_f32_e32",
            Self::VectorMulF32 { .. } => "v_mul_f32_e32",
            Self::Export { .. } => "exp",
            Self::Interpolate {
                phase: InterpolationPhase::First,
                ..
            } => "v_interp_p1_f32_e32",
            Self::Interpolate {
                phase: InterpolationPhase::Second,
                ..
            } => "v_interp_p2_f32_e32",
            Self::ImageSampleLz { .. } => "image_sample_lz",
            Self::EndProgram => "s_endpgm",
        }
    }
}

impl fmt::Display for Instruction {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.mnemonic())?;
        match *self {
            Self::BufferLoadDword {
                data,
                index,
                resource,
            }
            | Self::BufferStoreDword {
                data,
                index,
                resource,
            } => {
                let last = u16::from(resource) + 3;
                write!(
                    formatter,
                    " v{data}, v{index}, s[{resource}:{last}], 0 idxen"
                )
            }
            Self::WaitLoad => formatter.write_str(" vmcnt(0)"),
            Self::WaitStore => formatter.write_str(" null, 0"),
            Self::MoveB32 { dst, src } => write!(formatter, " v{dst}, {src}"),
            Self::AddNoCarryU32 { dst, src0, src1 }
            | Self::VectorLshlrevB32 { dst, src0, src1 }
            | Self::VectorLshrrevB32 { dst, src0, src1 }
            | Self::VectorAshrrevI32 { dst, src0, src1 }
            | Self::VectorAndB32 { dst, src0, src1 }
            | Self::VectorOrB32 { dst, src0, src1 }
            | Self::VectorXorB32 { dst, src0, src1 }
            | Self::VectorAddF32 { dst, src0, src1 }
            | Self::VectorMulF32 { dst, src0, src1 } => {
                write!(formatter, " v{dst}, v{src0}, v{src1}")
            }
            Self::ScalarMoveB32 { dst, src } => match dst {
                106 => write!(formatter, " vcc_lo, {src}"),
                107 => write!(formatter, " vcc_hi, {src}"),
                126 => write!(formatter, " exec_lo, {src}"),
                127 => write!(formatter, " exec_hi, {src}"),
                _ => write!(formatter, " s{dst}, {src}"),
            },
            Self::ScalarCompareEqU32 { src0, src1 } => write!(formatter, " {src0}, {src1}"),
            Self::Branch { target }
            | Self::BranchSccZero { target }
            | Self::BranchExecZero { target }
            | Self::BranchVccZero { target } => {
                write!(formatter, " -> 0x{target:04x}")
            }
            Self::Export {
                target,
                sources: [a, b, c, d],
            } => {
                let (name, flags) = match target {
                    ExportTarget::Position0 => ("pos0", " done"),
                    ExportTarget::Parameter0 => ("param0", ""),
                    ExportTarget::RenderTarget0 => ("mrt0", " done vm"),
                };
                write!(formatter, " {name}, v{a}, v{b}, v{c}, v{d}{flags}")
            }
            Self::Interpolate {
                dst,
                coordinate,
                attribute,
                component,
                ..
            } => {
                let channel = ["x", "y", "z", "w"]
                    .get(usize::from(component))
                    .ok_or(fmt::Error)?;
                write!(
                    formatter,
                    " v{dst}, v{coordinate}, attr{attribute}.{channel}"
                )
            }
            Self::ImageSampleLz {
                dst,
                coord,
                resource,
                sampler,
            } => {
                let last_dst = dst + 3;
                let last_coord = coord + 1;
                let last_res = resource + 7;
                let last_samp = sampler + 3;
                write!(
                    formatter,
                    " v[{dst}:{last_dst}], v[{coord}:{last_coord}], s[{resource}:{last_res}], s[{sampler}:{last_samp}] dmask:0xf dim:SQ_RSRC_IMG_2D"
                )
            }
            Self::EndProgram => Ok(()),
        }
    }
}

// SPDX-License-Identifier: GPL-3.0-or-later

//! Verified compilation for Xenora's synthetic compute and graphics ABIs.

use std::fmt::{self, Write};

mod graphics;
pub use graphics::{PIXEL_ENTRY_POINT, VERTEX_ENTRY_POINT, compile_graphics};

pub use xenora_decoder::Architecture;
use xenora_decoder::{DecodeError, DecodedInstruction, Instruction, ScalarSource, VectorSource};

/// Entry point of all kernels produced by this compiler.
pub const ENTRY_POINT: &str = "xenora_compute";
const MAX_CODE_BYTES: usize = 1024;

#[derive(Debug)]
pub struct CompiledShader {
    pub disassembly: String,
    pub ir: String,
    pub msl: String,
}

#[derive(Debug)]
pub enum CompileError {
    InvalidLength {
        bytes: usize,
    },
    Decode(DecodeError),
    GraphicsStage {
        stage: &'static str,
        error: Box<CompileError>,
    },
    GraphicsLink {
        reason: &'static str,
    },
    Verify {
        offset: usize,
        reason: &'static str,
    },
}

impl fmt::Display for CompileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidLength { bytes } => write!(
                formatter,
                "fixture length must be a nonzero multiple of four up to {MAX_CODE_BYTES} bytes, got {bytes}"
            ),
            Self::Decode(error) => error.fmt(formatter),
            Self::GraphicsStage { stage, error } => write!(formatter, "{stage} shader: {error}"),
            Self::GraphicsLink { reason } => write!(formatter, "graphics linking: {reason}"),
            Self::Verify { offset, reason } => {
                write!(
                    formatter,
                    "shader verification at byte 0x{offset:x}: {reason}"
                )
            }
        }
    }
}

impl std::error::Error for CompileError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Decode(error) => Some(error),
            Self::GraphicsStage { error, .. } => Some(error),
            _ => None,
        }
    }
}

#[derive(Clone, Copy)]
struct Value(usize);

impl fmt::Display for Value {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "t{}", self.0)
    }
}

#[derive(Clone, Copy)]
enum Op {
    Load,
    Constant(u32),
    Binary(Binary, Value, Value),
    MergeLanes {
        mask: u32,
        active: Value,
        inactive: Value,
    },
    Store(Value),
    ScalarBranch {
        target: usize,
        taken: bool,
    },
}

#[derive(Clone, Copy)]
enum Binary {
    Add,
    ShiftLeft,
    ShiftRight,
    ArithmeticShiftRight,
    And,
    Or,
    Xor,
    FloatAdd,
    FloatMul,
}

impl Binary {
    fn operator(self) -> (&'static str, &'static str) {
        match self {
            Self::Add => ("add.u32", "+"),
            Self::ShiftLeft => ("lshl.b32", "<<"),
            Self::ShiftRight => ("lshr.b32", ">>"),
            Self::ArithmeticShiftRight => ("ashr.i32", ">>"),
            Self::And => ("and.b32", "&"),
            Self::Or => ("or.b32", "|"),
            Self::Xor => ("xor.b32", "^"),
            Self::FloatAdd => ("add.f32", "+"),
            Self::FloatMul => ("mul.f32", "*"),
        }
    }
}

// Each bit is either known zero, known one or unconstrained.
#[derive(Clone, Copy)]
struct KnownBits {
    zero: u32,
    one: u32,
    values: Option<[u32; 2]>,
}

impl KnownBits {
    const UNKNOWN: Self = Self {
        zero: 0,
        one: 0,
        values: None,
    };

    fn constant(value: u32) -> Self {
        Self {
            zero: !value,
            one: value,
            values: Some([value; 2]),
        }
    }

    fn and(self, other: Self) -> Self {
        Self {
            zero: self.zero | other.zero,
            one: self.one & other.one,
            values: None,
        }
    }

    fn or(self, other: Self) -> Self {
        Self {
            zero: self.zero & other.zero,
            one: self.one | other.one,
            values: None,
        }
    }

    fn xor(self, other: Self) -> Self {
        Self {
            zero: (self.zero & other.zero) | (self.one & other.one),
            one: (self.zero & other.one) | (self.one & other.zero),
            values: None,
        }
    }

    fn positive_normal_candidates(self) -> Option<[f32; 2]> {
        let values = match self.values {
            Some(values) => values,
            None => {
                let unknown = !(self.zero | self.one);
                if unknown.count_ones() > 1 {
                    return None;
                }
                [self.one, self.one | unknown]
            }
        };
        let candidates = values.map(f32::from_bits);
        candidates
            .iter()
            .all(|n| n.is_normal() && n.is_sign_positive())
            .then_some(candidates)
    }
}

fn exact_float_result(operator: Binary, left: KnownBits, right: KnownBits) -> Option<KnownBits> {
    let left = left.positive_normal_candidates()?;
    let right = right.positive_normal_candidates()?;
    let mut zero = u32::MAX;
    let mut one = u32::MAX;
    let mut results = Vec::with_capacity(4);
    for a in left.map(f64::from) {
        for b in right.map(f64::from) {
            let exact = match operator {
                Binary::FloatAdd => a + b,
                Binary::FloatMul => a * b,
                _ => return None,
            };
            let value = exact as f32;
            if !value.is_normal() || f64::from(value) != exact {
                return None;
            }
            // Binary64 can round away the smaller operand of a widely separated
            // binary32 sum. Require that subtraction recovers the smaller input.
            if matches!(operator, Binary::FloatAdd) && exact - a.max(b) != a.min(b) {
                return None;
            }
            let bits = value.to_bits();
            zero &= !bits;
            one &= bits;
            if !results.contains(&bits) {
                results.push(bits);
            }
        }
    }
    let values = match results.as_slice() {
        [only] => Some([*only; 2]),
        [first, second] => Some([*first, *second]),
        _ => None,
    };
    Some(KnownBits { zero, one, values })
}

/// Compiles explicitly tagged raw machine code to a synthetic Metal compute kernel.
///
/// Bind equal-length disjoint input and output word buffers and dispatch one
/// invocation per word in a one-dimensional grid.
pub fn compile(architecture: Architecture, bytes: &[u8]) -> Result<CompiledShader, CompileError> {
    let decoded = decode_fixture(architecture, bytes)?;
    let operations = lift(&decoded, bytes.len())?;

    let disassembly = decoded.iter().map(|line| format!("{line}\n")).collect();
    let mut ir = String::new();
    let mut msl = format!(
        "#include <metal_stdlib>\nusing namespace metal;\nkernel void {ENTRY_POINT}(\n    device const uint* input [[buffer(0)]],\n    device uint* output [[buffer(1)]],\n    uint index [[thread_position_in_grid]]) {{\n"
    );
    if operations.iter().any(|(_, operation)| {
        matches!(
            operation,
            Op::Binary(Binary::FloatAdd | Binary::FloatMul, _, _)
        )
    }) {
        msl.push_str("    #pragma clang fp contract(off)\n    #pragma clang fp reassociate(off)\n");
    }
    for (index, (offset, operation)) in operations.into_iter().enumerate() {
        let dest = Value(index);
        let (step, statement) = match operation {
            Op::Load => (
                format!("{dest} = load.u32 input[index]"),
                format!("uint {dest} = input[index];"),
            ),
            Op::Constant(value) => (
                format!("{dest} = const.u32 0x{value:08x}"),
                format!("uint {dest} = 0x{value:08x}u;"),
            ),
            Op::Binary(operator, left, right) => {
                let (mnemonic, symbol) = operator.operator();
                let expression = match operator {
                    Binary::FloatAdd | Binary::FloatMul => format!(
                        "as_type<uint>(as_type<float>({left}) {symbol} as_type<float>({right}))"
                    ),
                    Binary::ArithmeticShiftRight => {
                        format!("as_type<uint>(as_type<int>({left}) {symbol} ({right} & 31u))")
                    }
                    Binary::ShiftLeft | Binary::ShiftRight => {
                        format!("{left} {symbol} ({right} & 31u)")
                    }
                    _ => format!("{left} {symbol} {right}"),
                };
                (
                    format!("{dest} = {mnemonic} {left}, {right}"),
                    format!("uint {dest} = {expression};"),
                )
            }
            Op::ScalarBranch { target, taken } => {
                let _ = writeln!(
                    ir,
                    "{offset:04x}: branch.uniform 0x{target:04x} taken={taken}"
                );
                continue;
            }
            Op::MergeLanes {
                mask,
                active,
                inactive,
            } => (
                format!("{dest} = merge.lanes mask=0x{mask:08x}, {active}, {inactive}"),
                format!(
                    "uint {dest} = ((0x{mask:08x}u >> (index & 31u)) & 1u) != 0u ? {active} : {inactive};"
                ),
            ),
            Op::Store(value) => (
                format!("store.u32 output[index], {value}"),
                format!("output[index] = {value};"),
            ),
        };
        // Writing to a String is infallible.
        let _ = writeln!(ir, "{offset:04x}: {step}");
        let _ = writeln!(msl, "    {statement}");
    }
    msl.push_str("}\n");
    Ok(CompiledShader {
        disassembly,
        ir,
        msl,
    })
}

fn decode_fixture(
    architecture: Architecture,
    bytes: &[u8],
) -> Result<Vec<DecodedInstruction>, CompileError> {
    if bytes.is_empty() || bytes.len() > MAX_CODE_BYTES || !bytes.len().is_multiple_of(4) {
        return Err(CompileError::InvalidLength { bytes: bytes.len() });
    }
    xenora_decoder::decode(architecture, bytes).map_err(CompileError::Decode)
}

fn lift(decoded: &[DecodedInstruction], code_len: usize) -> Result<Vec<(usize, Op)>, CompileError> {
    let mut operations = Vec::new();
    let mut registers = [None; 256];
    let mut facts = Vec::<KnownBits>::new();
    let mut pending_load = None;
    let mut stored = false;
    let mut store_pending = false;
    let mut scalars = [None; 128];
    let mut exec = u32::MAX; // The fixture starts with full Wave32 EXEC.
    scalars[126] = Some(exec);
    let mut scc = None;
    let mut skip_before = 0;

    // Every decoded instruction must satisfy the fixture layout and binding
    // contract, even when a known branch skips it.
    for (index, instruction) in decoded.iter().enumerate() {
        let offset = instruction.offset;
        let reason = match instruction.instruction {
            Instruction::Branch { target }
            | Instruction::BranchSccZero { target }
            | Instruction::BranchExecZero { target }
            | Instruction::BranchVccZero { target }
                if target <= offset
                    || decoded
                        .binary_search_by_key(&target, |instruction| instruction.offset)
                        .is_err() =>
            {
                "branch target must be a forward instruction boundary"
            }
            Instruction::BufferLoadDword {
                index, resource, ..
            } if (index, resource) != (0, 0) => "unsupported resource or buffer index",
            Instruction::BufferStoreDword {
                index, resource, ..
            } if (index, resource) != (0, 4) => "unsupported resource or buffer index",
            Instruction::Export { .. }
            | Instruction::Interpolate { .. }
            | Instruction::ImageSampleLz { .. } => {
                "graphics instruction is unsupported in a compute fixture"
            }
            Instruction::EndProgram if index + 1 != decoded.len() => {
                return Err(CompileError::Verify {
                    offset: decoded[index + 1].offset,
                    reason: "instruction follows program termination",
                });
            }
            _ => continue,
        };
        return Err(CompileError::Verify { offset, reason });
    }

    for instruction in decoded {
        let offset = instruction.offset;
        let error = |reason| CompileError::Verify { offset, reason };
        let read = |register: u8| {
            registers[usize::from(register)].ok_or_else(|| {
                error("source register is uninitialized, pending a load wait, or reserved for indexing")
            })
        };
        if offset < skip_before {
            continue;
        }
        let read_scalar = |source| match source {
            ScalarSource::Immediate(value) => Ok(value),
            ScalarSource::Register(register) => scalars
                .get(usize::from(register))
                .copied()
                .flatten()
                .ok_or_else(|| error("scalar source has no known value")),
        };
        // Scalar instructions and waits still execute when all lanes are inactive.
        if exec == 0 {
            match instruction.instruction {
                Instruction::MoveB32 { .. }
                | Instruction::AddNoCarryU32 { .. }
                | Instruction::VectorLshlrevB32 { .. }
                | Instruction::VectorLshrrevB32 { .. }
                | Instruction::VectorAshrrevI32 { .. }
                | Instruction::VectorAndB32 { .. }
                | Instruction::VectorOrB32 { .. }
                | Instruction::VectorXorB32 { .. }
                | Instruction::VectorAddF32 { .. }
                | Instruction::VectorMulF32 { .. } => continue,
                _ => {}
            }
        }
        if exec != u32::MAX
            && matches!(
                instruction.instruction,
                Instruction::BufferLoadDword { .. } | Instruction::BufferStoreDword { .. }
            )
        {
            return Err(error("vector memory requires full Wave32 EXEC"));
        }
        let next = Value(operations.len());
        let (dest, operation) = match instruction.instruction {
            Instruction::BufferLoadDword { data, .. } => {
                let slot = usize::from(data);
                if data == 0 || pending_load.is_some() {
                    return Err(error(
                        "load writes the index register or another load is outstanding",
                    ));
                }
                registers[slot] = None;
                pending_load = Some((slot, next));
                (None, Op::Load)
            }
            Instruction::BufferStoreDword { data, .. } => {
                if stored {
                    return Err(error("only one output store is supported"));
                }
                stored = true;
                store_pending = true;
                (None, Op::Store(read(data)?))
            }
            Instruction::WaitLoad => {
                if let Some((register, value)) = pending_load.take() {
                    registers[register] = Some(value);
                }
                continue;
            }
            Instruction::WaitStore => {
                store_pending = false;
                continue;
            }
            Instruction::MoveB32 {
                dst,
                src: VectorSource::Register(src),
            } => {
                let value = read(src)?;
                let slot = usize::from(dst);
                if dst == 0 || pending_load.is_some_and(|(register, _)| register == slot) {
                    return Err(error(
                        "move writes the index register or replaces a pending load",
                    ));
                }
                registers[slot] = Some(merge_lanes(
                    &mut operations,
                    &mut facts,
                    offset,
                    exec,
                    value,
                    registers[slot],
                )?);
                continue;
            }
            Instruction::MoveB32 {
                dst,
                src: VectorSource::Immediate(value),
            } => (Some(dst), Op::Constant(value)),
            Instruction::AddNoCarryU32 { dst, src0, src1 } => {
                (Some(dst), Op::Binary(Binary::Add, read(src0)?, read(src1)?))
            }
            Instruction::VectorLshlrevB32 { dst, src0, src1 } => (
                Some(dst),
                Op::Binary(Binary::ShiftLeft, read(src1)?, read(src0)?),
            ),
            Instruction::VectorAshrrevI32 { dst, src0, src1 } => (
                Some(dst),
                Op::Binary(Binary::ArithmeticShiftRight, read(src1)?, read(src0)?),
            ),
            Instruction::VectorLshrrevB32 { dst, src0, src1 } => (
                Some(dst),
                Op::Binary(Binary::ShiftRight, read(src1)?, read(src0)?),
            ),
            Instruction::VectorAndB32 { dst, src0, src1 } => {
                (Some(dst), Op::Binary(Binary::And, read(src0)?, read(src1)?))
            }
            Instruction::VectorOrB32 { dst, src0, src1 } => {
                (Some(dst), Op::Binary(Binary::Or, read(src0)?, read(src1)?))
            }
            Instruction::VectorXorB32 { dst, src0, src1 } => {
                (Some(dst), Op::Binary(Binary::Xor, read(src0)?, read(src1)?))
            }
            Instruction::ScalarMoveB32 { dst, src } => {
                let value = read_scalar(src)?;
                if dst < 8 {
                    return Err(error("scalar move would modify a resource descriptor"));
                }
                if dst == 126 {
                    exec = value;
                }
                let slot = scalars
                    .get_mut(usize::from(dst))
                    .ok_or_else(|| error("scalar destination exceeds the fixture register file"))?;
                *slot = Some(value);
                continue;
            }
            Instruction::ScalarCompareEqU32 { src0, src1 } => {
                scc = Some(read_scalar(src0)? == read_scalar(src1)?);
                continue;
            }
            Instruction::BranchSccZero { target }
            | Instruction::BranchExecZero { target }
            | Instruction::BranchVccZero { target } => {
                let taken = match instruction.instruction {
                    Instruction::BranchSccZero { .. } => {
                        !scc.ok_or_else(|| error("conditional branch reads uninitialized SCC"))?
                    }
                    Instruction::BranchVccZero { .. } => {
                        scalars[106]
                            .ok_or_else(|| error("conditional branch reads uninitialized VCC_LO"))?
                            == 0
                    }
                    _ => exec == 0,
                };
                if taken {
                    skip_before = target;
                }
                (None, Op::ScalarBranch { target, taken })
            }
            Instruction::Branch { target } => {
                skip_before = target;
                (
                    None,
                    Op::ScalarBranch {
                        target,
                        taken: true,
                    },
                )
            }
            Instruction::VectorAddF32 { dst, src0, src1 } => (
                Some(dst),
                Op::Binary(Binary::FloatAdd, read(src0)?, read(src1)?),
            ),
            Instruction::VectorMulF32 { dst, src0, src1 } => (
                Some(dst),
                Op::Binary(Binary::FloatMul, read(src0)?, read(src1)?),
            ),
            Instruction::Export { .. }
            | Instruction::Interpolate { .. }
            | Instruction::ImageSampleLz { .. } => {
                return Err(error(
                    "graphics instruction is unsupported in a compute fixture",
                ));
            }
            Instruction::EndProgram => {
                if !stored || store_pending || pending_load.is_some() {
                    return Err(error(
                        "termination requires one completed store and no pending loads",
                    ));
                }
                return Ok(operations);
            }
        };
        let value_bits = match operation {
            Op::Constant(bits) => KnownBits::constant(bits),
            Op::Binary(Binary::And, left, right) => facts[left.0].and(facts[right.0]),
            Op::Binary(Binary::Or, left, right) => facts[left.0].or(facts[right.0]),
            Op::Binary(Binary::Xor, left, right) => facts[left.0].xor(facts[right.0]),
            Op::Binary(operator @ (Binary::FloatAdd | Binary::FloatMul), left, right) => {
                exact_float_result(operator, facts[left.0], facts[right.0])
                    .ok_or_else(|| error("FP32 operation is not proven exact for positive normal operands and results"))?
            }
            _ => KnownBits::UNKNOWN,
        };
        facts.push(value_bits);
        operations.push((offset, operation));
        if let Some(dest) = dest {
            let slot = usize::from(dest);
            if dest == 0 || pending_load.is_some_and(|(register, _)| register == slot) {
                return Err(error(
                    "instruction writes the index register or replaces a pending load",
                ));
            }
            registers[slot] = Some(merge_lanes(
                &mut operations,
                &mut facts,
                offset,
                exec,
                next,
                registers[slot],
            )?);
        }
    }
    Err(CompileError::Verify {
        offset: code_len,
        reason: "missing program termination",
    })
}

// Keeping a value for inactive lanes requires a previously initialized destination.
fn merge_lanes(
    operations: &mut Vec<(usize, Op)>,
    facts: &mut Vec<KnownBits>,
    offset: usize,
    mask: u32,
    active: Value,
    inactive: Option<Value>,
) -> Result<Value, CompileError> {
    if mask == u32::MAX {
        return Ok(active);
    }
    let inactive = inactive.ok_or(CompileError::Verify {
        offset,
        reason: "partial EXEC requires an initialized vector destination",
    })?;
    let next = Value(operations.len());
    operations.push((
        offset,
        Op::MergeLanes {
            mask,
            active,
            inactive,
        },
    ));
    facts.push(KnownBits {
        zero: facts[active.0].zero & facts[inactive.0].zero,
        one: facts[active.0].one & facts[inactive.0].one,
        values: None,
    });
    Ok(next)
}

#[cfg(test)]
mod tests {
    use super::{Binary, KnownBits, exact_float_result};

    #[test]
    fn propagates_known_bits_through_integer_operators() {
        let left = KnownBits::constant(0b1100);
        let right = KnownBits::constant(0b1010);
        let and = left.and(right);
        assert_eq!((and.zero, and.one), (!0b1000, 0b1000));
        let xor = left.xor(right);
        assert_eq!((xor.zero, xor.one), (!0b0110, 0b0110));
    }

    #[test]
    fn rejects_unbounded_or_invalid_float_operands() {
        let one = KnownBits::constant(1.0f32.to_bits());
        let varying = KnownBits::UNKNOWN
            .and(KnownBits::constant(0x0040_0000))
            .xor(one);
        assert_eq!(varying.positive_normal_candidates(), Some([1.0, 1.5]));
        assert_eq!(one.and(one).positive_normal_candidates(), Some([1.0; 2]));
        assert!(KnownBits::UNKNOWN.positive_normal_candidates().is_none());
        for invalid in [0x8000_0000, 0xbf80_0000, 1, 0x7f80_0000] {
            assert!(
                KnownBits::constant(invalid)
                    .positive_normal_candidates()
                    .is_none()
            );
        }
    }

    #[test]
    fn keeps_only_bits_shared_by_every_exact_result() {
        let operand = KnownBits::UNKNOWN
            .and(KnownBits::constant(0x0040_0000))
            .xor(KnownBits::constant(1.0f32.to_bits()));
        let sum = exact_float_result(Binary::FloatAdd, operand, operand).unwrap();
        // The sums 2, 2.5 and 3 differ only in the two highest fraction bits.
        assert_eq!(sum.zero, 0xbf9f_ffff);
        assert_eq!(sum.one, 0x4000_0000);
        assert!(sum.values.is_none());
    }
}

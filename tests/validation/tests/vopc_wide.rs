// SPDX-License-Identifier: GPL-3.0-or-later

//! Independent per-lane expected values for the 28 Wave32 VOPC 16-bit and
//! 64-bit integer compares (#215 B1): `v_cmp_{lt,eq,le,gt,ne,ge}_{i16,u16}`
//! (opcodes 137-142, 169-174) and `v_cmp_{f,lt,eq,le,gt,ne,ge,t}_{i64,u64}`
//! (160-167, 224-231), `_e32` only, under the contract in
//! `docs/shaders/vector-compare.md` (sections 2-7).
//!
//! The expectations are written below from the public AMD "RDNA 2" Instruction
//! Set Architecture Reference Guide only: 12.9 (Tables 59-61), 3.9, 3.6.3,
//! 6.2 (Table 20 and 6.2.1), 6.2.5, 6.8 and 9.1. Nothing here comes from the
//! ISA crate, the semantic registry, the decoder or the shader compiler. Every
//! case runs on the reference interpreter `xenora_ref` and on the second
//! interpreter `independent_ref`. The ignored native Metal test needs the
//! translator's 16- and 64-bit forms; until then a CPU test checks that the
//! production compiler rejects all 28.
//!
//! Readings of the guide (not hardware evidence):
//! - Table 59: offsets F 0, LT 1, EQ 2, LE 3, GT 4, LG/NE 5, GE 6, TRU 7 with
//!   `D.u = (S0 op S1)`, `D.u = 0`, `D.u = 1`; Table 61 lists the 16-bit groups
//!   without F and T (136, 143, 168 and 175 are class compares or unlisted).
//! - 16-bit: 6.2 says the operand data size is the one in the instruction
//!   name; 6.2.5 says VOPC selects a half only through SDWA, and 6.8 that SDWA
//!   is what references the upper half. So the e32 form reads bits 15:0 of the
//!   32-bit VGPR, SGPR or VCC_LO and ignores bits 31:16. This is an inference:
//!   the guide never states the non-SDWA half in so many words. `_I16` treats
//!   bits 15:0 as two's complement, `_U16` as unsigned.
//! - 64-bit: 3.6.3 "the LSBs are in SGPR[n], and the MSBs are in SGPR[n+1]"
//!   and the pair must be even-aligned; for VGPRs 3.6.4 gives no alignment
//!   rule and 9.1 says for 64-bit addresses "ADDR has the LSBs, and ADDR+1 has
//!   the MSBs", read here as the general VGPR pair order. `_I64` is two's
//!   complement over all 64 bits, `_U64` unsigned.
//! - Table 20: selectors 128-192 are the integers 0..64 and 193-208 are -1..-16.
//!   Read as values at the operand width, -1 is 0xffff at 16 bits and
//!   0xffff_ffff_ffff_ffff at 64 bits (sign extended, also for `_U64`). The
//!   guide lists the values, not their bit patterns, so the negative patterns
//!   (above all for `_U64`) are a reading, not a quotation. 6.2.1's
//!   zero-extension rule for unsigned 64-bit operands is headed "Literal
//!   Expansion to 64 bits" and covers literals only.
//! - Section 3.9: `VCC[n] = EXEC[n] & (test passed for thread[n])`, "VCC is
//!   fully written"; in wave32 only VCC_LO contributes to VCCZ. Lanes past the
//!   end of a partial final wave do not exist and give 0 (contract section 3).
//! - Odd SGPR bases are illegal for 64-bit data (3.6.3 "Even-aligned SGPRs are
//!   required"), so they are rejected. Literals are rejected by the contract
//!   subset: for 16-bit forms the guide does not say which literal bits are
//!   used; for 64-bit forms 6.2.1 defines an expansion, which the subset leaves
//!   out. VCC as a 64-bit source and v255 as a pair base (v[255:256] does not
//!   exist) are also outside the subset.
//! - Under EXEC_LO = 0 the result is 0 (3.9); that no source is read then is
//!   the contract's rule (section 5), not a guide statement.

#[path = "operand_domains/harness.rs"]
#[allow(dead_code)]
mod harness;
#[path = "../independent_ref.rs"]
mod independent_ref;

use std::collections::BTreeMap;

use harness::{Case, words};
use xenora_shader::{Architecture, CompileError, compile};

/// One of the 28 forms: ENC_VOPC opcode (Table 61) and Table 59 offset.
#[derive(Clone, Copy, Debug)]
struct Form {
    op: u32,
    name: &'static str,
    bits: u32,
    signed: bool,
    offset: u32,
}

const RELATIONS: [&str; 8] = ["f", "lt", "eq", "le", "gt", "ne", "ge", "t"];

/// Table 61: 137 + (offset - 1) for I16, 169 + (offset - 1) for U16 (LT..GE),
/// 160 + offset for I64 and 224 + offset for U64 (F..T).
fn forms() -> Vec<Form> {
    let mut forms = Vec::new();
    for (base, bits, signed, offsets) in [
        (136, 16, true, 1..=6),
        (168, 16, false, 1..=6),
        (160, 64, true, 0..=7),
        (224, 64, false, 0..=7),
    ] {
        for offset in offsets {
            let kind = if signed { 'i' } else { 'u' };
            let name = format!("v_cmp_{}_{kind}{bits}", RELATIONS[offset as usize]);
            forms.push(Form {
                op: base + offset,
                name: Box::leak(name.into_boxed_str()),
                bits,
                signed,
                offset,
            });
        }
    }
    forms
}

/// The operand's integer value. `raw` holds the register bits: for 16-bit
/// forms the 32-bit register (only bits 15:0 count), for 64-bit forms
/// `lo | hi << 32`.
fn value(form: Form, raw: u64) -> i128 {
    if form.bits == 16 {
        // 16-bit: S.i16 / S.u16 = bits 15:0.
        let half = (raw % 0x1_0000) as i128;
        if form.signed && half >= 0x8000 {
            half - 0x1_0000
        } else {
            half
        }
    } else {
        // 64-bit: {SGPR/VGPR[n+1] (MSBs), [n] (LSBs)}.
        let (lo, hi) = (raw % (1 << 32), raw / (1 << 32));
        let hi = if form.signed && hi >= 0x8000_0000 {
            hi as i128 - (1 << 32)
        } else {
            hi as i128
        };
        hi * (1 << 32) + lo as i128
    }
}

/// Table 59 / Table 61: D[threadId] for S0 and S1 of one lane.
fn test(form: Form, s0: u64, s1: u64) -> bool {
    let (a, b) = (value(form, s0), value(form, s1));
    match form.offset {
        0 => false,  // F: D = 0
        1 => a < b,  // LT
        2 => a == b, // EQ
        3 => a <= b, // LE
        4 => a > b,  // GT
        5 => a != b, // NE (<>)
        6 => a >= b, // GE
        _ => true,   // T: D = 1
    }
}

/// Section 3.9 and contract section 3: VCC_LO = OR over the lanes n that
/// exist (`present`) of (EXEC[n] & test(n)) << n.
fn vcc(form: Form, exec: u32, present: u32, operands: impl Fn(usize) -> (u64, u64)) -> u32 {
    (0..32)
        .filter(|&n| (present & exec) >> n & 1 == 1)
        .filter(|&n| {
            let (s0, s1) = operands(n);
            test(form, s0, s1)
        })
        .fold(0, |mask, n| mask | 1 << n)
}

/// Lane inputs (v1). Bits 15:0 cover 0, 1, 0x7fff, 0x8000, 0xffff and their
/// neighbours under varied bits 31:16; the full dwords cover 0, 1,
/// 0x7fff_ffff, 0x8000_0000 and 0xffff_ffff for the 64-bit low dword.
const LANES: [u32; 32] = [
    0,
    1,
    0x7fff_ffff,
    0x8000_0000,
    0xffff_ffff,
    0xffff_fffe,
    0x8000_0001,
    0x1234_5678,
    0xdead_0000,
    0xbeef_0001,
    0x0001_7fff,
    0xffff_8000,
    0x8000_ffff,
    0x5a5a_7ffe,
    0xa5a5_8001,
    0x0000_7fff,
    0x0000_8000,
    0x0000_ffff,
    0x7fff_0000,
    0xffff_0001,
    0x1357_9bdf,
    0x0f0f_f0f0,
    0xc0de_0005,
    0x0042_0042,
    0x0010_fff0,
    0x8000_7fff,
    0x7fff_8000,
    0x0003_0003,
    0xffff_7fff,
    0x0000_0005,
    0x4000_4000,
    0xfedc_ba98,
];
/// Uniform 16-bit operands: boundary low halves with garbage high halves.
const B16: [u32; 8] = [
    0,
    0xffff_0001,
    0x0000_7fff,
    0x1234_8000,
    0xabcd_ffff,
    0x8000_0000,
    0x0001_0005,
    0xdead_7ffe,
];
/// Uniform 64-bit operands.
const B64: [u64; 8] = [
    0,
    1,
    0x7fff_ffff_ffff_ffff,
    0x8000_0000_0000_0000,
    u64::MAX,
    0x0000_0001_0000_0000,
    0x7fff_ffff_8000_0000,
    0xffff_ffff_0000_0000,
];
/// High dwords joined to the lane input to make 64-bit lane operands.
const HIGHS: [u32; 5] = [0, 1, 0x7fff_ffff, 0x8000_0000, 0xffff_ffff];
const SGPR16: [u32; 3] = [0xffff_0000, 0x0000_8000, 0x1234_ffff];
const SGPR64: [u64; 3] = [0x8000_0000_0000_0000, 0x8000_0000_ffff_ffff, u64::MAX];
const EXEC_MASKS: [u32; 7] = [
    0,
    0x5555_5555,
    0xaaaa_aaaa,
    0x8000_0001,
    0x0000_ffff,
    0x7fff_fffe,
    u32::MAX,
];
const STALE_VCC: u32 = 0xa5a5_c33c;
const OLD: u32 = 0x0bad_f00d;

const F16: usize = 12;
const F64: usize = 16;
const COUNTS: [(&str, usize); 22] = [
    ("vopc16 domain", F16 * 8 * 2),
    ("vopc16 aliasing", F16 * 2),
    ("vopc16 inline integer", F16 * 81),
    ("vopc16 sgpr source", F16 * 2 * 3),
    ("vopc16 vcc_lo source", F16 * 4),
    ("vopc16 max register", F16 * 3),
    ("vopc16 exec", F16 * 7),
    ("vopc16 empty exec reads nothing", F16 * 3),
    ("vopc16 partial wave", F16 * 3),
    ("vopc16 vccz branch", F16 * 4),
    ("vopc16 exec unchanged", F16 * 2),
    ("vopc64 domain", F64 * 5 * 8 * 2),
    ("vopc64 lane-varying high dword", F64 * 8 * 2),
    ("vopc64 aliasing", F64 * 4),
    ("vopc64 inline integer", F64 * 81 * 2),
    ("vopc64 sgpr pair source", F64 * 2 * 3),
    ("vopc64 max register", F64 * 3),
    ("vopc64 exec", F64 * 7),
    ("vopc64 empty exec reads nothing", F64 * 3),
    ("vopc64 partial wave", F64 * 3),
    ("vopc64 vccz branch", F64 * 4),
    ("vopc64 exec unchanged", F64 * 2),
];
/// 16-bit per form: 4 uninitialized sources, 2 pending loads, a literal with
/// and without its dword, 9 float inline constants, 9 special selectors, a
/// float constant under empty EXEC, SDWA and 3 DPP forms, `_e64`, `v_cmpx`
/// and a lane-derived vccz branch over a scalar instruction.
const REJECTIONS16: usize = 4 + 2 + 2 + 9 + 9 + 1 + 4 + 1 + 1 + 1;
/// 64-bit per form: 7 uninitialized pair halves, 2 pending loads on a high
/// half, vcc (known and under empty EXEC), a literal with and without its
/// dword, 2 odd SGPR bases and one under empty EXEC, v255 as either base and
/// under empty EXEC, v255 as VSRC1 after an SGPR pair (also under empty
/// EXEC) and after an inline integer, then as for 16-bit: 9 float, 9 special,
/// a float under empty EXEC, SDWA/DPP 4, `_e64`, `v_cmpx`, vccz over a scalar
/// instruction.
const REJECTIONS64: usize = 7 + 2 + 2 + 2 + 3 + 3 + 3 + 9 + 9 + 1 + 4 + 1 + 1 + 1;

const LITERAL: u32 = 255;
const VCC_LO: u32 = 106;
const EXEC_LO: u32 = 126;

/// ENCODING 0b0111110 in bits 25-31, OP 17-24, VSRC1 9-16, SRC0 0-8 (guide
/// 13.3.3); SRC0 256 + n is VGPR n.
fn vopc(op: u32, src0: u32, vsrc1: u32) -> u32 {
    0b011_1110 << 25 | op << 17 | vsrc1 << 9 | src0
}

const fn vgpr(n: u32) -> u32 {
    256 + n
}

/// `v_mov_b32 v<reg>, literal`.
fn vmov(reg: u32, value: u32) -> [u32; 2] {
    [0x7e00_02ff | reg << 17, value]
}

/// `v_mov_b32 v<dst>, v<src>`.
fn vcopy(dst: u32, src: u32) -> u32 {
    0x7e00_0200 | dst << 17 | vgpr(src)
}

/// `s_mov_b32 s<reg>, literal`.
fn smov(reg: u32, value: u32) -> [u32; 2] {
    [0xbe80_03ff | reg << 16, value]
}

/// v[reg:reg+1] = `value` (low dword in v[reg]).
fn vmov64(reg: u32, value: u64) -> Vec<u32> {
    let mut code = vmov(reg, value as u32).to_vec();
    code.extend(vmov(reg + 1, (value >> 32) as u32));
    code
}

/// s[reg:reg+1] = `value` (low dword in s[reg]).
fn smov64(reg: u32, value: u64) -> Vec<u32> {
    let mut code = smov(reg, value as u32).to_vec();
    code.extend(smov(reg + 1, (value >> 32) as u32));
    code
}

/// v[4:5] = (v1, `high`): the lane input as the low dword.
fn lane_pair(high: u32) -> Vec<u32> {
    let mut code = vec![vcopy(4, 1)];
    code.extend(vmov(5, high));
    code
}

const fn join(lo: u32, hi: u32) -> u64 {
    lo as u64 | (hi as u64) << 32
}

const FULL_EXEC: u32 = 0xbefe_03c1;
const READ_VCC: u32 = 0x7e04_026a;
/// `v_cmp_lt_u32 vcc_lo, v1, v3`.
const LT_U32_V1_V3: u32 = 0x7d82_0701;
const VCCZ_OVER_ONE: u32 = 0xbf86_0001;
/// `v_xor_b32 v2, v1, v2`.
const XOR_V2_V1: u32 = 29 << 25 | 2 << 17 | 2 << 9 | (256 + 1);
/// `s_mov_b32 s8, 0` and `s_mov_b32 s8, 1`.
const SMOV_S8_0: u32 = 0xbe88_0380;
const SMOV_S8_1: u32 = 0xbe88_0381;
/// `v_mov_b32 v2, s8`.
const V2_FROM_S8: u32 = 0x7e04_0208;

const PREFIX: [u32; 3] = [0xe030_2000, 0x8000_0100, 0xbf8c_3f70];
const STORE_V2: [u32; 4] = [0xe070_2000, 0x8001_0200, 0xbbfd_0000, 0xbf81_0000];

/// Load v1 = lane input, `setup`, EXEC_LO = `exec` (unless `None`), the compare
/// words, EXEC_LO = -1 again, v2 = vcc_lo, `tail`, then store v2. Returns the
/// program and the compare's byte offset.
fn program(setup: &[u32], exec: Option<u32>, compare: &[u32], tail: &[u32]) -> (Vec<u8>, usize) {
    let mut code = PREFIX.to_vec();
    code.extend(setup);
    if let Some(exec) = exec {
        code.extend(smov(EXEC_LO, exec));
    }
    let offset = 4 * code.len();
    code.extend(compare);
    if exec.is_some() {
        code.push(FULL_EXEC);
    }
    code.push(READ_VCC);
    code.extend(tail);
    code.extend(STORE_V2);
    (words(&code), offset)
}

fn code(setup: &[u32], exec: Option<u32>, compare: &[u32]) -> Vec<u8> {
    program(setup, exec, compare, &[]).0
}

fn present(input: &[u32], n: usize) -> u32 {
    let in_wave = (input.len() - n / 32 * 32).min(32);
    if in_wave == 32 {
        u32::MAX
    } else {
        (1 << in_wave) - 1
    }
}

fn case(
    form: Form,
    family: &'static str,
    label: String,
    code: Vec<u8>,
    input: Vec<u32>,
    lane: impl Fn(usize, u32, u32) -> u32,
) -> Case {
    let expected = input
        .iter()
        .enumerate()
        .map(|(n, &x)| lane(n, x, present(&input, n)))
        .collect();
    Case {
        form: form.name,
        family,
        label: format!("{} {label}", form.name),
        code,
        input,
        expected,
    }
}

/// Table 20: the integer of selectors 128-208.
fn inline_integer(selector: u32) -> i64 {
    match selector {
        128..=192 => i64::from(selector) - 128,
        193..=208 => 192 - i64::from(selector),
        _ => unreachable!("not an integer inline constant"),
    }
}

/// The inline integer as register bits at the form's width: two's complement
/// over 64 bits (16-bit forms then use bits 15:0).
fn inline_bits(selector: u32) -> u64 {
    inline_integer(selector) as u64
}

fn full(form: Form, operands: impl Fn(usize) -> (u64, u64)) -> impl Fn(usize, u32, u32) -> u32 {
    move |_, _, present| vcc(form, u32::MAX, present, &operands)
}

fn v1(n: usize) -> u64 {
    u64::from(LANES[n % 32])
}

fn cases16(form: Form, cases: &mut Vec<Case>) {
    let op = form.op;
    let mut push = |family, label: String, code, lane: Box<dyn Fn(usize, u32, u32) -> u32>| {
        cases.push(case(form, family, label, code, LANES.to_vec(), lane));
    };
    for b in B16 {
        let setup = vmov(3, b);
        let f = full(form, move |n| (v1(n), u64::from(b)));
        push(
            "vopc16 domain",
            format!("v1, {b:#x}"),
            code(&setup, None, &[vopc(op, vgpr(1), 3)]),
            Box::new(f),
        );
        let f = full(form, move |n| (u64::from(b), v1(n)));
        push(
            "vopc16 domain",
            format!("{b:#x}, v1"),
            code(&setup, None, &[vopc(op, vgpr(3), 1)]),
            Box::new(f),
        );
    }
    let b = 0x7654_8000;
    let f = full(form, |n| (v1(n), v1(n)));
    push(
        "vopc16 aliasing",
        "v1, v1".into(),
        code(&vmov(3, b), None, &[vopc(op, vgpr(1), 1)]),
        Box::new(f),
    );
    let f = full(form, move |_| (u64::from(b), u64::from(b)));
    push(
        "vopc16 aliasing",
        "v3, v3".into(),
        code(&vmov(3, b), None, &[vopc(op, vgpr(3), 3)]),
        Box::new(f),
    );
    for selector in 128..=208 {
        let value = inline_bits(selector);
        let f = full(form, move |n| (value, v1(n)));
        let label = format!("inline {selector} ({}), v1", inline_integer(selector));
        push(
            "vopc16 inline integer",
            label,
            code(&[], None, &[vopc(op, selector, 1)]),
            Box::new(f),
        );
    }
    for reg in [8, 105] {
        for value in SGPR16 {
            let f = full(form, move |n| (u64::from(value), v1(n)));
            let label = format!("s{reg} = {value:#x}, v1");
            push(
                "vopc16 sgpr source",
                label,
                code(&smov(reg, value), None, &[vopc(op, reg, 1)]),
                Box::new(f),
            );
        }
    }
    for value in SGPR16 {
        let f = full(form, move |n| (u64::from(value), v1(n)));
        let label = format!("vcc_lo = {value:#x}, v1");
        let compare = [vopc(op, VCC_LO, 1)];
        push(
            "vopc16 vcc_lo source",
            label,
            code(&smov(VCC_LO, value), None, &compare),
            Box::new(f),
        );
    }
    // Lane-derived VCC_LO from v_cmp_lt_u32 v1, v3 (v3 = 0x80000000).
    let mut setup = vmov(3, 0x8000_0000).to_vec();
    setup.push(LT_U32_V1_V3);
    let old = (0..32)
        .filter(|&n| v1(n) < 0x8000_0000)
        .fold(0u64, |mask, n| mask | 1 << n);
    let f = full(form, move |n| (old, v1(n)));
    let label = "vcc_lo lane-derived, v1".into();
    push(
        "vopc16 vcc_lo source",
        label,
        code(&setup, None, &[vopc(op, VCC_LO, 1)]),
        Box::new(f),
    );
    let b = 0x0bad_8000;
    for (src0, vsrc1, label) in [
        (vgpr(255), 1, "v255, v1"),
        (vgpr(1), 255, "v1, v255"),
        (vgpr(255), 255, "v255, v255"),
    ] {
        let f = full(form, move |n| {
            let pick = |is_v255: bool| if is_v255 { u64::from(b) } else { v1(n) };
            (pick(src0 == vgpr(255)), pick(vsrc1 == 255))
        });
        push(
            "vopc16 max register",
            label.into(),
            code(&vmov(255, b), None, &[vopc(op, src0, vsrc1)]),
            Box::new(f),
        );
    }
    let b = 0xf00f_0005;
    let mut setup = vmov(3, b).to_vec();
    setup.extend(smov(VCC_LO, STALE_VCC));
    for exec in EXEC_MASKS {
        let label = format!("exec {exec:#x}, stale vcc_lo, v1, {b:#x}");
        let f = move |_, _, p| vcc(form, exec, p, |n| (v1(n), u64::from(b)));
        push(
            "vopc16 exec",
            label,
            code(&setup, Some(exec), &[vopc(op, vgpr(1), 3)]),
            Box::new(f),
        );
    }
    for (src0, vsrc1, label) in [
        (20, 1, "s20 (unknown), v1"),
        (VCC_LO, 1, "vcc_lo (uninitialized), v1"),
        (vgpr(9), 9, "v9, v9 (uninitialized)"),
    ] {
        let f = |_, _, _| 0;
        push(
            "vopc16 empty exec reads nothing",
            format!("exec 0, {label}"),
            code(&[], Some(0), &[vopc(op, src0, vsrc1)]),
            Box::new(f),
        );
    }
    common_tail(form, cases, &vmov(3, 5), vgpr(1), 3, 5, "vopc16");
}

fn cases64(form: Form, cases: &mut Vec<Case>) {
    let op = form.op;
    let mut push = |family, label: String, code, lane: Box<dyn Fn(usize, u32, u32) -> u32>| {
        cases.push(case(form, family, label, code, LANES.to_vec(), lane));
    };
    // v[4:5] = (v1, high), v[6:7] = b.
    for high in HIGHS {
        for b in B64 {
            let mut setup = lane_pair(high);
            setup.extend(vmov64(6, b));
            let pair = move |n: usize| join(LANES[n % 32], high);
            let f = full(form, move |n| (pair(n), b));
            let label = format!("v[4:5] = (v1, {high:#x}), {b:#x}");
            push(
                "vopc64 domain",
                label,
                code(&setup, None, &[vopc(op, vgpr(4), 6)]),
                Box::new(f),
            );
            let f = full(form, move |n| (b, pair(n)));
            let label = format!("{b:#x}, v[4:5] = (v1, {high:#x})");
            push(
                "vopc64 domain",
                label,
                code(&setup, None, &[vopc(op, vgpr(6), 4)]),
                Box::new(f),
            );
        }
    }
    // v[4:5] = (v1, v1): the high dword differs per lane.
    for b in B64 {
        let mut setup = vec![vcopy(4, 1), vcopy(5, 1)];
        setup.extend(vmov64(6, b));
        let pair = |n: usize| join(LANES[n % 32], LANES[n % 32]);
        let f = full(form, move |n| (pair(n), b));
        let family = "vopc64 lane-varying high dword";
        push(
            family,
            format!("(v1, v1), {b:#x}"),
            code(&setup, None, &[vopc(op, vgpr(4), 6)]),
            Box::new(f),
        );
        let f = full(form, move |n| (b, pair(n)));
        push(
            family,
            format!("{b:#x}, (v1, v1)"),
            code(&setup, None, &[vopc(op, vgpr(6), 4)]),
            Box::new(f),
        );
    }
    // Aliasing: the same pair twice, and the overlapping pairs v[4:5], v[5:6].
    // v[5:6] has the same high dword as v[4:5], so the low dwords decide.
    let (high, c) = (0x8000_0000, 0x8000_0000);
    let mut setup = lane_pair(high);
    setup.extend(vmov(6, c));
    setup.extend(vmov(7, 0x8000_0000));
    let lo4 = move |n: usize| join(LANES[n % 32], high);
    let lo5 = join(high, c);
    let lo6 = join(c, 0x8000_0000);
    for (src0, vsrc1, label, operands) in [
        (
            4,
            4,
            "v[4:5], v[4:5]",
            Box::new(move |n| (lo4(n), lo4(n))) as Box<dyn Fn(usize) -> (u64, u64)>,
        ),
        (6, 6, "v[6:7], v[6:7]", Box::new(move |_| (lo6, lo6))),
        (4, 5, "v[4:5], v[5:6]", Box::new(move |n| (lo4(n), lo5))),
        (5, 4, "v[5:6], v[4:5]", Box::new(move |n| (lo5, lo4(n)))),
    ] {
        let f = full(form, operands);
        push(
            "vopc64 aliasing",
            label.into(),
            code(&setup, None, &[vopc(op, vgpr(src0), vsrc1)]),
            Box::new(f),
        );
    }
    // Inline integers against v[4:5] = (v1, 0) and (v1, 0xffffffff).
    for high in [0, 0xffff_ffff] {
        for selector in 128..=208 {
            let value = inline_bits(selector);
            let pair = move |n: usize| join(LANES[n % 32], high);
            let f = full(form, move |n| (value, pair(n)));
            let label = format!(
                "inline {selector} ({}), (v1, {high:#x})",
                inline_integer(selector)
            );
            let compare = [vopc(op, selector, 4)];
            push(
                "vopc64 inline integer",
                label,
                code(&lane_pair(high), None, &compare),
                Box::new(f),
            );
        }
    }
    // SGPR pairs s[8:9] and s[104:105] against v[4:5] = (v1, 0x80000000).
    for reg in [8, 104] {
        for value in SGPR64 {
            let mut setup = lane_pair(0x8000_0000);
            setup.extend(smov64(reg, value));
            let f = full(form, move |n| (value, join(LANES[n % 32], 0x8000_0000)));
            let label = format!("s[{reg}:{}] = {value:#x}, (v1, 0x80000000)", reg + 1);
            push(
                "vopc64 sgpr pair source",
                label,
                code(&setup, None, &[vopc(op, reg, 4)]),
                Box::new(f),
            );
        }
    }
    // v[254:255], the highest pair; its high dword equals the lane pair's,
    // so v254 decides, and reading v255 as anything but 1 changes the result.
    let b = join(0x8000_0000, 1);
    let mut setup = lane_pair(1);
    setup.extend(vmov64(254, b));
    for (src0, vsrc1, label) in [
        (254, 4, "v[254:255], (v1, 1)"),
        (4, 254, "(v1, 1), v[254:255]"),
        (254, 254, "v[254:255], v[254:255]"),
    ] {
        let f = full(form, move |n| {
            let pick = |top: bool| if top { b } else { join(LANES[n % 32], 1) };
            (pick(src0 == 254), pick(vsrc1 == 254))
        });
        push(
            "vopc64 max register",
            label.into(),
            code(&setup, None, &[vopc(op, vgpr(src0), vsrc1)]),
            Box::new(f),
        );
    }
    let b = 0x0000_0000_0f0f_0f0f;
    let mut setup = lane_pair(0);
    setup.extend(vmov64(6, b));
    setup.extend(smov(VCC_LO, STALE_VCC));
    for exec in EXEC_MASKS {
        let label = format!("exec {exec:#x}, stale vcc_lo, (v1, 0), {b:#x}");
        let f = move |_, _, p| vcc(form, exec, p, |n| (v1(n), b));
        push(
            "vopc64 exec",
            label,
            code(&setup, Some(exec), &[vopc(op, vgpr(4), 6)]),
            Box::new(f),
        );
    }
    for (src0, vsrc1, label) in [
        (20, 4, "s[20:21] (unknown), v[4:5] (unknown)"),
        (vgpr(9), 9, "v[9:10], v[9:10] (uninitialized)"),
        (vgpr(1), 1, "v[1:2], v[1:2] (v2 uninitialized)"),
    ] {
        let f = |_, _, _| 0;
        push(
            "vopc64 empty exec reads nothing",
            format!("exec 0, {label}"),
            code(&[], Some(0), &[vopc(op, src0, vsrc1)]),
            Box::new(f),
        );
    }
    // b = 5 in v[6:7]; the lane operand is (v1, 0).
    let mut setup = lane_pair(0);
    setup.extend(vmov64(6, 5));
    common_tail(form, cases, &setup, vgpr(4), 6, 5, "vopc64");
}

/// Partial waves, vccz branches and EXEC preservation: SRC0 `src0` holds the
/// lane input (with a zero high dword for 64-bit forms), VSRC1 `vsrc1` holds `b`.
fn common_tail(
    form: Form,
    cases: &mut Vec<Case>,
    setup: &[u32],
    src0: u32,
    vsrc1: u32,
    b: u64,
    width: &str,
) {
    let op = form.op;
    let family =
        |name: &str| -> &'static str { Box::leak(format!("{width} {name}").into_boxed_str()) };
    // The second wave holds different values from the first one's lanes 0-12.
    let long: Vec<u32> = LANES
        .iter()
        .chain(LANES.iter().rev().take(13))
        .copied()
        .collect();
    let short = LANES[..7].to_vec();
    for (exec, input, label) in [
        (u32::MAX, short, "7 lanes"),
        (u32::MAX, long.clone(), "45 lanes"),
        (0x5555_5555, long, "45 lanes, exec 0x55555555"),
    ] {
        let exec_word = (exec != u32::MAX).then_some(exec);
        let copy = input.clone();
        let code = code(setup, exec_word, &[vopc(op, src0, vsrc1)]);
        cases.push(case(
            form,
            family("partial wave"),
            label.into(),
            code,
            input,
            move |n, _, p| vcc(form, exec, p, |k| (u64::from(copy[n / 32 * 32 + k]), b)),
        ));
    }
    for exec in [u32::MAX, 0x0000_8000, 0] {
        let (code, _) = program(
            setup,
            Some(exec),
            &[vopc(op, src0, vsrc1)],
            &[VCCZ_OVER_ONE, XOR_V2_V1],
        );
        let label = format!("exec {exec:#x}, then s_cbranch_vccz");
        cases.push(case(
            form,
            family("vccz branch"),
            label,
            code,
            LANES.to_vec(),
            move |_, x, p| {
                let mask = vcc(form, exec, p, |n| (v1(n), b));
                if mask == 0 { 0 } else { mask ^ x }
            },
        ));
    }
    let (code, _) = program(
        setup,
        Some(0),
        &[vopc(op, src0, vsrc1)],
        &[SMOV_S8_0, VCCZ_OVER_ONE, SMOV_S8_1, V2_FROM_S8],
    );
    // Taken: s8 stays 0 and v2 = s8 = 0; falling through would give 1.
    let label = "exec 0, then s_cbranch_vccz over s_mov, v2 = s8".into();
    cases.push(case(
        form,
        family("vccz branch"),
        label,
        code,
        LANES.to_vec(),
        |_, _, _| 0,
    ));
    for exec in [0x5555_5555, 0x8000_0001] {
        let mut code = PREFIX.to_vec();
        code.extend(setup);
        code.extend(vmov(9, OLD));
        code.push(29 << 25 | 2 << 17 | 1 << 9 | vgpr(9)); // v_xor_b32 v2, v9, v1
        code.extend(smov(EXEC_LO, exec));
        code.extend([vopc(op, src0, vsrc1), READ_VCC, FULL_EXEC]);
        code.extend(STORE_V2);
        let label = format!("exec {exec:#x}, v2 = vcc_lo before EXEC is restored");
        cases.push(case(
            form,
            family("exec unchanged"),
            label,
            words(&code),
            LANES.to_vec(),
            move |n, x, p| {
                if exec >> (n % 32) & 1 == 1 {
                    vcc(form, exec, p, |k| (v1(k), b))
                } else {
                    x ^ OLD
                }
            },
        ));
    }
}

fn cases() -> Vec<Case> {
    let mut cases = Vec::new();
    for form in forms() {
        if form.bits == 16 {
            cases16(form, &mut cases);
        } else {
            cases64(form, &mut cases);
        }
    }
    cases
}

/// Programs both interpreters must refuse, with the refusing byte offset.
fn rejections() -> Vec<(String, Vec<u8>, usize)> {
    let mut all = Vec::new();
    for form in forms() {
        let op = form.op;
        let wide = form.bits == 64;
        let mut reject = |label: String, (code, offset): (Vec<u8>, usize)| {
            all.push((format!("{} {label}", form.name), code, offset));
        };
        // v3 (16-bit) or v[4:5] = (v1, 7) (64-bit) is the initialized VSRC1.
        let (setup, other) = if wide {
            (lane_pair(7), 4)
        } else {
            (vmov(3, 7).to_vec(), 3)
        };
        let with = |extra: &[u32]| {
            let mut code = setup.clone();
            code.extend(extra);
            code
        };
        if wide {
            // Pair halves not initialized under nonzero EXEC.
            for (extra, src0, vsrc1, label) in [
                (vec![], 20, other, "s[20:21] unknown"),
                (smov(8, 1).to_vec(), 8, other, "s9 unknown in s[8:9]"),
                (smov(9, 1).to_vec(), 8, other, "s8 unknown in s[8:9]"),
                (vec![], vgpr(1), other, "v2 uninitialized in SRC0 v[1:2]"),
                (vec![], vgpr(4), 1, "v2 uninitialized in VSRC1 v[1:2]"),
                (
                    vmov(8, 1).to_vec(),
                    vgpr(8),
                    other,
                    "v9 uninitialized in v[8:9]",
                ),
                (vec![], vgpr(9), other, "v[9:10] uninitialized"),
            ] {
                reject(
                    label.into(),
                    program(&with(&extra), None, &[vopc(op, src0, vsrc1)], &[]),
                );
            }
        } else {
            for (extra, src0, vsrc1, label) in [
                (vec![], 20, other, "s20 unknown"),
                (vec![], VCC_LO, other, "vcc_lo uninitialized"),
                (vec![], vgpr(9), other, "v9 uninitialized as SRC0"),
                (vec![], vgpr(1), 9, "v9 uninitialized as VSRC1"),
            ] {
                reject(
                    label.into(),
                    program(&with(&extra), None, &[vopc(op, src0, vsrc1)], &[]),
                );
            }
        }
        // A load outstanding on v3 (16-bit) or on v5, the high half of
        // v[4:5] (64-bit), in each slot.
        let (target, partner) = if wide { (5, 6) } else { (3, 1) };
        let base = if wide { 4 } else { 3 };
        for slot in ["SRC0", "VSRC1"] {
            let mut code = PREFIX.to_vec();
            code.extend(&setup);
            if wide {
                code.extend(vmov64(6, 9));
            }
            code.extend([0xe030_2000, 0x8000_0000 | target << 8]);
            let offset = 4 * code.len();
            code.push(if slot == "SRC0" {
                vopc(op, vgpr(base), partner)
            } else {
                vopc(op, vgpr(partner), base)
            });
            code.extend([READ_VCC, 0xbf8c_3f70]);
            code.extend(STORE_V2);
            reject(
                format!("v{target} load outstanding in {slot}"),
                (words(&code), offset),
            );
        }
        if wide {
            reject(
                "vcc as SRC0".into(),
                program(
                    &with(&smov(VCC_LO, 1)),
                    None,
                    &[vopc(op, VCC_LO, other)],
                    &[],
                ),
            );
            reject(
                "vcc as SRC0 under empty exec".into(),
                program(&setup, Some(0), &[vopc(op, VCC_LO, other)], &[]),
            );
        }
        // A literal, with its dword and without one.
        reject(
            "literal".into(),
            program(&setup, None, &[vopc(op, LITERAL, other), 5], &[]),
        );
        let mut code = PREFIX.to_vec();
        code.extend(&setup);
        let offset = 4 * code.len();
        code.push(vopc(op, LITERAL, other));
        reject("missing literal".into(), (words(&code), offset));
        if wide {
            for reg in [9, 105] {
                let mut extra = smov(reg, 1).to_vec();
                if reg < 105 {
                    extra.extend(smov(reg + 1, 1));
                }
                reject(
                    format!("odd SGPR base s{reg}"),
                    program(&with(&extra), None, &[vopc(op, reg, other)], &[]),
                );
            }
            reject(
                "odd SGPR base s9 under empty exec".into(),
                program(&setup, Some(0), &[vopc(op, 9, other)], &[]),
            );
            let top = with(&vmov(255, 1));
            reject(
                "v255 as SRC0 base".into(),
                program(&top, None, &[vopc(op, vgpr(255), other)], &[]),
            );
            reject(
                "v255 as VSRC1 base".into(),
                program(&top, None, &[vopc(op, vgpr(other), 255)], &[]),
            );
            reject(
                "v255 as a base under empty exec".into(),
                program(&top, Some(0), &[vopc(op, vgpr(255), other)], &[]),
            );
            let pair = with(&smov64(8, 1));
            reject(
                "s[8:9], v255 as VSRC1 base".into(),
                program(&pair, None, &[vopc(op, 8, 255)], &[]),
            );
            reject(
                "s[8:9], v255 as VSRC1 base under empty exec".into(),
                program(&pair, Some(0), &[vopc(op, 8, 255)], &[]),
            );
            reject(
                "inline 1, v255 as VSRC1 base".into(),
                program(&setup, None, &[vopc(op, 129, 255)], &[]),
            );
        }
        for src0 in (240..=248).chain([107, 124, 125, 126, 127, 251, 252, 253, 254]) {
            reject(
                format!("src0 {src0}"),
                program(&setup, None, &[vopc(op, src0, other)], &[]),
            );
        }
        reject(
            "src0 240 under empty exec".into(),
            program(&setup, Some(0), &[vopc(op, 240, other)], &[]),
        );
        for (src0, extension) in [
            (249, 0x0606_0601),
            (250, 0xff00_e401),
            (233, 0xfac6_8801),
            (234, 0xfac6_8801),
        ] {
            let compare = [vopc(op, src0, other), extension];
            reject(
                format!("src0 {src0} extension"),
                program(&setup, None, &compare, &[]),
            );
        }
        // VOP3 `_e64`: v_cmp vcc_lo, v1 (or v[4:5]), VSRC1.
        let src0 = if wide { vgpr(4) } else { vgpr(1) };
        let vop3 = [0xd400_0000 | op << 16 | VCC_LO, src0 | vgpr(other) << 9];
        reject("_e64".into(), program(&setup, None, &vop3, &[]));
        // The matching v_cmpx (Table 61: opcode + 16).
        let label = format!("v_cmpx opcode {}", op + 16);
        reject(
            label,
            program(&setup, None, &[vopc(op + 16, src0, other)], &[]),
        );
        // Lane-derived result, then a vccz branch over `s_mov_b32 s8, 0`.
        let (code, at) = program(
            &setup,
            None,
            &[vopc(op, src0, other)],
            &[VCCZ_OVER_ONE, SMOV_S8_0],
        );
        reject("lane-derived vccz over s_mov".into(), (code, at + 12));
    }
    all
}

fn assert_rejected_at(label: &str, code: &[u8], offset: usize, input: &[u32]) {
    match xenora_ref::execute(code, input) {
        Ok(_) => panic!("reference accepted {label}"),
        Err(error) => assert_eq!(
            error.offset, offset,
            "reference rejected {label} elsewhere: {error:?}"
        ),
    }
    match independent_ref::execute(code, input) {
        Ok(_) => panic!("independent accepted {label}"),
        Err(error) => assert!(
            error.starts_with(&format!("{offset:#x}:")),
            "independent rejected {label} elsewhere: {error}"
        ),
    }
}

fn tally(cases: &[Case], key: fn(&Case) -> &'static str) -> BTreeMap<&'static str, usize> {
    let mut counts = BTreeMap::new();
    for case in cases {
        *counts.entry(key(case)).or_default() += 1;
    }
    counts
}

fn failures(
    cases: &[Case],
    execute: impl Fn(&[u8], &[u32]) -> Result<Vec<u32>, String>,
) -> Vec<String> {
    let mut failed: BTreeMap<&str, (usize, String)> = BTreeMap::new();
    for case in cases {
        let output = execute(&case.code, &case.input);
        if output.as_deref().ok() != Some(case.expected.as_slice()) {
            let first = format!("{}: {output:x?}, expected {:x?}", case.label, case.expected);
            failed.entry(case.family).or_insert((0, first)).0 += 1;
        }
    }
    failed
        .into_iter()
        .map(|(family, (n, first))| format!("  {family}: {n} failed, first: {first}"))
        .collect()
}

fn checked_cases() -> Vec<Case> {
    let cases = cases();
    assert_eq!(tally(&cases, |case| case.family), BTreeMap::from(COUNTS));
    assert_eq!(tally(&cases, |case| case.form).len(), 28);
    cases
}

fn form(name: &str) -> Form {
    forms().into_iter().find(|form| form.name == name).unwrap()
}

#[test]
fn expectations_follow_the_guide_examples() {
    // Worked by hand from Tables 59-61, 3.6.3 and Table 20.
    for (name, s0, s1, expected) in [
        // 16-bit: bits 31:16 are ignored; 0x8000 is -32768 signed, 32768 unsigned.
        ("v_cmp_eq_i16", 0xdead_0001, 0x0000_0001, true),
        ("v_cmp_eq_u16", 0xffff_0000, 0x0000_0000, true),
        ("v_cmp_lt_i16", 0x0000_8000, 0x0000_7fff, true),
        ("v_cmp_lt_u16", 0x0000_8000, 0x0000_7fff, false),
        ("v_cmp_gt_i16", 0x0001_ffff, 0x0000_0000, false), // -1 > 0
        ("v_cmp_ge_u16", 0x0000_ffff, 0x7fff_0000, true),
        ("v_cmp_ne_u16", 0x1234_5678, 0x8765_5678, false),
        ("v_cmp_le_i16", 0x0000_ffff, 0xffff_ffff, true),
        // 64-bit: {hi, lo}; equal low dword with different high dword.
        (
            "v_cmp_eq_u64",
            0x0000_0001_0000_0005,
            0x0000_0000_0000_0005,
            false,
        ),
        (
            "v_cmp_lt_i64",
            0x8000_0000_0000_0000,
            0x7fff_ffff_ffff_ffff,
            true,
        ),
        (
            "v_cmp_lt_u64",
            0x8000_0000_0000_0000,
            0x7fff_ffff_ffff_ffff,
            false,
        ),
        ("v_cmp_gt_i64", u64::MAX, 0, false), // -1 > 0
        ("v_cmp_gt_u64", u64::MAX, 0, true),
        (
            "v_cmp_le_u64",
            0x0000_0001_0000_0000,
            0x0000_0000_ffff_ffff,
            false,
        ),
        (
            "v_cmp_ge_i64",
            0xffff_ffff_0000_0000,
            0xffff_fffe_ffff_ffff,
            true,
        ),
        ("v_cmp_f_i64", 0, 0, false),
        ("v_cmp_t_u64", 0, 1, true),
    ] {
        assert_eq!(test(form(name), s0, s1), expected, "{name} {s0:#x} {s1:#x}");
    }
    // Table 20 at each width: selector 193 (-1) is 0xffff at 16 bits and
    // u64::MAX at 64 bits; 192 is 64.
    assert!(test(form("v_cmp_eq_u16"), inline_bits(193), 0xffff));
    assert!(test(form("v_cmp_eq_u64"), inline_bits(193), u64::MAX));
    assert!(test(
        form("v_cmp_eq_i64"),
        inline_bits(208),
        (-16_i64) as u64
    ));
    assert!(test(form("v_cmp_eq_u64"), inline_bits(192), 64));
    // Section 3.9: inactive and absent lanes give 0.
    let lt = form("v_cmp_lt_u16");
    assert_eq!(vcc(lt, 0x0000_00f0, 0x1ff, |_| (0, 1)), 0xf0);
    assert_eq!(vcc(lt, u32::MAX, 0x7f, |_| (0, 1)), 0x7f);
    assert_eq!(vcc(lt, 0, u32::MAX, |_| (0, 1)), 0);
}

#[test]
fn vopc_wide_match_the_guide_on_the_reference() {
    let cases = checked_cases();
    let failed = failures(&cases, |code, input| {
        xenora_ref::execute(code, input).map_err(|error| format!("{error:?}"))
    });
    assert!(
        failed.is_empty(),
        "reference disagrees with the guide:\n{}",
        failed.join("\n")
    );
    eprintln!("vopc_wide: {} reference cases", cases.len());
}

#[test]
fn vopc_wide_match_the_guide_on_the_independent_interpreter() {
    let cases = checked_cases();
    let failed = failures(&cases, independent_ref::execute);
    assert!(
        failed.is_empty(),
        "independent interpreter disagrees with the guide:\n{}",
        failed.join("\n")
    );
    eprintln!("vopc_wide: {} independent-interpreter cases", cases.len());
}

#[test]
fn vopc_wide_outside_the_contract_are_rejected() {
    let rejections = rejections();
    assert_eq!(rejections.len(), F16 * REJECTIONS16 + F64 * REJECTIONS64);
    for (label, code, offset) in &rejections {
        assert_rejected_at(label, code, *offset, &LANES);
    }
    eprintln!("vopc_wide: {} rejections", rejections.len());
}

#[test]
fn production_compiler_still_rejects_the_wide_compares() {
    // docs/shaders/vector-compare.md 2a: the oracle runs ahead of the
    // translator, which rejects all 28 forms at the compare until it
    // implements them. This test fails once it does; then the Metal test
    // below takes over.
    for form in forms() {
        let (setup, other) = if form.bits == 64 {
            (lane_pair(7), 4)
        } else {
            (vmov(3, 7).to_vec(), 3)
        };
        let (code, offset) = program(&setup, None, &[vopc(form.op, vgpr(1), other)], &[]);
        let at = match compile(Architecture::Gfx1030, &code) {
            Err(CompileError::Decode(error)) => error.offset,
            Err(CompileError::Verify { offset, .. }) => offset,
            other => panic!("compiler accepted {}: {other:?}", form.name),
        };
        assert_eq!(at, offset, "compiler rejected {} elsewhere", form.name);
    }
}

#[test]
fn pair_halves_are_checked_without_active_lanes() {
    // Contract section 5: with EXEC_LO != 0 every source register is checked
    // on every existing lane, even when no existing lane is active. Here the
    // wave has 7 lanes and EXEC_LO = 0xffff_ff80 enables only absent lanes;
    // the high half v2 of v[1:2] is uninitialized in each slot.
    let input = &LANES[..7];
    let mut n = 0;
    for form in forms().into_iter().filter(|form| form.bits == 64) {
        for (src0, vsrc1, slot) in [(vgpr(1), 4, "SRC0"), (vgpr(4), 1, "VSRC1")] {
            let compare = [vopc(form.op, src0, vsrc1)];
            let (code, offset) = program(&lane_pair(7), Some(0xffff_ff80), &compare, &[]);
            let label = format!("{} v2 uninitialized in {slot}, no active lane", form.name);
            assert_rejected_at(&label, &code, offset, input);
            n += 1;
        }
    }
    assert_eq!(n, F64 * 2);
}

#[test]
fn neighbouring_compares_stay_unsupported() {
    // Table 61 neighbours of the 28 forms: 136 V_CMP_CLASS_F32, 143
    // V_CMP_CLASS_F16, 168 V_CMP_CLASS_F64, 175 (unlisted), the v_cmpx 16-bit
    // and 64-bit groups (153, 158, 176, 183, 185, 190, 240, 247) and 232
    // V_CMP_U_F16.
    let mut setup = lane_pair(7);
    setup.extend(vmov(3, 7));
    for op in [
        136, 143, 168, 175, 153, 158, 176, 183, 185, 190, 232, 240, 247,
    ] {
        let (code, offset) = program(&setup, None, &[vopc(op, vgpr(4), 3)], &[]);
        assert_rejected_at(&format!("opcode {op}"), &code, offset, &LANES);
    }
}

#[test]
#[ignore = "requires a native Metal device with API and shader validation, and the translator's 16- and 64-bit forms (skipped by scripts/test-gpu.sh until then)"]
fn vopc_wide_match_the_guide_on_native_metal() {
    assert_eq!(std::env::var("MTL_DEBUG_LAYER").as_deref(), Ok("1"));
    assert_eq!(std::env::var("MTL_SHADER_VALIDATION").as_deref(), Ok("1"));
    for (label, code, offset) in rejections() {
        let at = match compile(Architecture::Gfx1030, &code) {
            Err(CompileError::Decode(error)) => error.offset,
            Err(CompileError::Verify { offset, .. }) => offset,
            other => panic!("compiler did not reject {label}: {other:?}"),
        };
        assert_eq!(at, offset, "compiler rejected {label} elsewhere");
    }
    let cases = checked_cases();
    let (failures, programs) = harness::check_metal(&cases);
    failures.assert_none("native Metal disagrees with the guide");
    eprintln!(
        "vopc_wide: {} Metal cases in {programs} programs",
        cases.len()
    );
}

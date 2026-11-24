// SPDX-License-Identifier: GPL-3.0-or-later

//! Independent expected values for the eight SOP2 forms added in #215 stage C:
//! `s_andn2_b32`, `s_orn2_b32`, `s_nand_b32`, `s_nor_b32`, `s_xnor_b32`,
//! `s_lshl_b32`, `s_lshr_b32` and `s_ashr_i32`.
//!
//! The expectations are written below from the public AMD "RDNA 2" Instruction
//! Set Architecture Reference Guide only: section 12.1 "SOP2 Instructions" for
//! D and SCC of each form, section 5.3 for the SCC rule of bit/logical
//! operations, and the scalar operand table (section 3) for inline integer
//! constants. Nothing here comes from the ISA crate, the semantic registry, the
//! decoder or the shader compiler. Every case runs on the reference interpreter
//! `xenora_ref` and on the second interpreter `independent_ref`; the ignored
//! test also runs the translated MSL on native Metal.

#[path = "operand_domains/harness.rs"]
#[allow(dead_code)]
mod harness;
#[path = "../independent_ref.rs"]
mod independent_ref;

use std::collections::BTreeMap;

use harness::{Case, words};

/// The eight stage C forms with their ENC_SOP2 opcodes (guide 12.1).
#[derive(Clone, Copy, Debug)]
enum Form {
    Andn2 = 20,
    Orn2 = 22,
    Nand = 24,
    Nor = 26,
    Xnor = 28,
    Lshl = 30,
    Lshr = 32,
    Ashr = 34,
}

const FORMS: [(Form, &str); 8] = [
    (Form::Andn2, "s_andn2_b32"),
    (Form::Orn2, "s_orn2_b32"),
    (Form::Nand, "s_nand_b32"),
    (Form::Nor, "s_nor_b32"),
    (Form::Xnor, "s_xnor_b32"),
    (Form::Lshl, "s_lshl_b32"),
    (Form::Lshr, "s_lshr_b32"),
    (Form::Ashr, "s_ashr_i32"),
];

/// The guide's D and SCC for `form` with S0 = `s0` and S1 = `s1`. None of
/// these forms reads SCC, so the value of SCC before the instruction is not an
/// input.
fn expect(form: Form, s0: u32, s1: u32) -> (u32, bool) {
    let d = match form {
        // 20 S_ANDN2_B32: D = S0 & ~S1; SCC = (D != 0).
        Form::Andn2 => s0 & !s1,
        // 22 S_ORN2_B32: D = S0 | ~S1; SCC = (D != 0).
        Form::Orn2 => s0 | !s1,
        // 24 S_NAND_B32: D = ~(S0 & S1); SCC = (D != 0).
        Form::Nand => !(s0 & s1),
        // 26 S_NOR_B32: D = ~(S0 | S1); SCC = (D != 0).
        Form::Nor => !(s0 | s1),
        // 28 S_XNOR_B32: D = ~(S0 ^ S1); SCC = (D != 0).
        Form::Xnor => !(s0 ^ s1),
        // 30 S_LSHL_B32: D.u = S0.u << S1.u[4:0]; SCC = (D.u != 0).
        Form::Lshl => s0 << (s1 & 0x1f),
        // 32 S_LSHR_B32: D.u = S0.u >> S1.u[4:0]; SCC = (D.u != 0).
        Form::Lshr => s0 >> (s1 & 0x1f),
        // 34 S_ASHR_I32: D.i = signext(S0.i) >> S1.u[4:0]; SCC = (D.i != 0).
        // S1 is read as unsigned and only its low five bits count, so its
        // sign never matters; the shift replicates S0's sign bit.
        Form::Ashr => ((s0 as i32) >> (s1 & 0x1f)) as u32,
    };
    // Section 5.3: bit/logical operations set SCC = 1 iff the result is not zero.
    (d, d != 0)
}

/// Guide section 3 scalar operand table: 128 is 0, 129-192 are 1 to 64 and
/// 193-208 are -1 to -16, as 32-bit words.
fn inline_integer(selector: u32) -> u32 {
    match selector {
        128..=192 => selector - 128,
        193..=208 => (-((selector - 192) as i32)) as u32,
        _ => unreachable!("not an integer inline constant"),
    }
}

const LITERAL: u32 = 255;
/// Boundary operands and alternating bit patterns.
const VALUES: [u32; 8] = [
    0,
    1,
    0x7fff_ffff,
    0x8000_0000,
    0xffff_ffff,
    0xaaaa_aaaa,
    0x5555_5555,
    0x1234_5678,
];
/// Shift counts around the five-bit field, and one whose low bits are 1.
const SHIFTS: [u32; 7] = [0, 1, 31, 32, 33, 63, 0xffff_ffe1];
const INPUT: [u32; 2] = [0, u32::MAX];

const COUNTS: [(&str, usize); 5] = [
    ("stage c domain", 8 * 8 * 8 * 2),
    ("stage c shift count", 3 * 8 * 7 * 2),
    ("stage c aliasing", 8 * 2 * 5 * 2),
    ("stage c inline integer", 8 * 81 * 2 * 2 * 2),
    ("stage c literal", 8 * 6 * 2 * 2),
];

/// ENCODING 0b10 in bits 30-31, OP 23-29, SDST 16-22, SSRC1 8-15, SSRC0 0-7.
fn sop2(form: Form, dst: u32, src0: u32, src1: u32) -> u32 {
    0x8000_0000 | (form as u32) << 23 | dst << 16 | src1 << 8 | src0
}

/// The program around `alu`: load v1, `s_mov_b32` each `init` pair, set SCC to
/// `scc` with `s_cmp_eq_u32`, run `alu`, then v2 = s<result>, XORed with v1
/// when SCC is set afterwards, and store v2. With the lane input [0, !0] the
/// output is [D, D] when SCC is clear and [D, !D] when it is set.
fn program(init: &[(u32, u32)], scc: bool, alu: &[u32], result: u32) -> Vec<u8> {
    let mut code = vec![0xe030_2000, 0x8000_0100, 0xbf8c_3f70];
    for &(reg, value) in init {
        code.extend([0xbe80_03ff | reg << 16, value]);
    }
    code.push(if scc { 0xbf06_8080 } else { 0xbf06_8180 });
    code.extend(alu);
    code.extend([0x7e04_0200 | result, 0xbf84_0001, 0x3a04_0501]);
    code.extend([0xe070_2000, 0x8001_0200, 0xbbfd_0000, 0xbf81_0000]);
    words(&code)
}

fn case(
    name: &'static str,
    family: &'static str,
    label: String,
    code: Vec<u8>,
    out: (u32, bool),
) -> Case {
    let (d, scc) = out;
    Case {
        form: name,
        family,
        label: format!("{name} {label}"),
        code,
        input: INPUT.to_vec(),
        expected: vec![d, if scc { !d } else { d }],
    }
}

fn cases() -> Vec<Case> {
    let mut cases = Vec::new();
    for (form, name) in FORMS {
        let shift = matches!(form, Form::Lshl | Form::Lshr | Form::Ashr);
        for scc in [false, true] {
            let alu = [sop2(form, 10, 8, 9)];
            let pairs = VALUES
                .iter()
                .flat_map(|&a| VALUES.map(|b| (a, b, "stage c domain")));
            let counts = VALUES
                .iter()
                .flat_map(|&a| SHIFTS.map(|b| (a, b, "stage c shift count")));
            let counts: Vec<_> = if shift { counts.collect() } else { Vec::new() };
            for (a, b, family) in pairs.chain(counts) {
                let code = program(&[(8, a), (9, b)], scc, &alu, 10);
                let label = format!("{a:#x}, {b:#x}, scc before {scc}");
                cases.push(case(name, family, label, code, expect(form, a, b)));
            }
            // dst/src aliasing with s8 = a, s9 = b.
            for (a, b) in [(0xffff_fff0, 0x21), (0x8000_0001, 0x8000_0001)] {
                for (dst, src0, src1) in [(8, 8, 9), (9, 8, 9), (8, 8, 8), (9, 9, 9), (8, 9, 8)] {
                    let pick = |reg| if reg == 8 { a } else { b };
                    let code = program(&[(8, a), (9, b)], scc, &[sop2(form, dst, src0, src1)], dst);
                    let label =
                        format!("s{dst} = s{src0}, s{src1} ({a:#x}, {b:#x}), scc before {scc}");
                    let out = expect(form, pick(src0), pick(src1));
                    cases.push(case(name, "stage c aliasing", label, code, out));
                }
            }
            // Every integer inline constant (-16..64) in each source slot. The
            // register operand s8 takes a word and its complement, so a wrong
            // inline value changes D for at least one of them (as a shift
            // count it is 0x40 and 0x21, i.e. counts 0 and 1).
            for selector in 128..=208 {
                let value = inline_integer(selector);
                for (slot, other) in [0, 1].into_iter().flat_map(|slot| {
                    let others = if slot == 0 && shift {
                        [0x40, 0x21]
                    } else {
                        [0x8000_0f0f, 0x7fff_f0f0]
                    };
                    others.map(|other| (slot, other))
                }) {
                    let (alu, out) = if slot == 0 {
                        (sop2(form, 10, selector, 8), expect(form, value, other))
                    } else {
                        (sop2(form, 10, 8, selector), expect(form, other, value))
                    };
                    let code = program(&[(8, other)], scc, &[alu], 10);
                    let label =
                        format!("inline {value:#x} in src{slot}, s8 {other:#x}, scc before {scc}");
                    cases.push(case(name, "stage c inline integer", label, code, out));
                }
            }
            // A 32-bit literal dword after the SOP2 word in each source slot.
            for value in [0, 1, 0x7fff_ffff, 0x8000_0000, u32::MAX, 0xffff_ffe1] {
                for slot in 0..2 {
                    let other = 0xc000_0003;
                    let (alu, out) = if slot == 0 {
                        (
                            [sop2(form, 10, LITERAL, 8), value],
                            expect(form, value, other),
                        )
                    } else {
                        (
                            [sop2(form, 10, 8, LITERAL), value],
                            expect(form, other, value),
                        )
                    };
                    let code = program(&[(8, other)], scc, &alu, 10);
                    let label = format!("literal {value:#x} in src{slot}, scc before {scc}");
                    cases.push(case(name, "stage c literal", label, code, out));
                }
            }
        }
    }
    cases
}

fn tally(cases: &[Case], key: fn(&Case) -> &'static str) -> BTreeMap<&'static str, usize> {
    let mut counts = BTreeMap::new();
    for case in cases {
        *counts.entry(key(case)).or_default() += 1;
    }
    counts
}

/// Runs `execute` on every case and returns one line per failing family: the
/// number of failures and the first one.
fn failures(
    cases: &[Case],
    execute: impl Fn(&[u8], &[u32]) -> Result<Vec<u32>, String>,
) -> Vec<String> {
    let mut failed: BTreeMap<&str, (usize, String)> = BTreeMap::new();
    for case in cases {
        let output = execute(&case.code, &case.input);
        if output.as_deref().ok() != Some(case.expected.as_slice()) {
            let first = format!("{}: {output:?}, expected {:?}", case.label, case.expected);
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
    cases
}

#[test]
fn stage_c_forms_match_the_guide_on_the_reference() {
    let cases = checked_cases();
    let failed = failures(&cases, |code, input| {
        xenora_ref::execute(code, input).map_err(|error| format!("{error:?}"))
    });
    assert!(
        failed.is_empty(),
        "reference disagrees with the guide:\n{}",
        failed.join("\n")
    );
    eprintln!("sop2_stage_c: {} reference cases", cases.len());
    for (form, n) in tally(&cases, |case| case.form) {
        eprintln!("  form {form}: {n}");
    }
}

#[test]
fn stage_c_forms_match_the_guide_on_the_independent_interpreter() {
    let cases = checked_cases();
    let failed = failures(&cases, independent_ref::execute);
    assert!(
        failed.is_empty(),
        "independent interpreter disagrees with the guide:\n{}",
        failed.join("\n")
    );
    eprintln!(
        "sop2_stage_c: {} independent-interpreter cases",
        cases.len()
    );
}

#[test]
fn sop2_64_bit_shifts_stay_unsupported() {
    // Aligned, initialized pairs distinguish rejection of each shift opcode
    // from rejection of an operand.
    for op in [31, 33, 35] {
        let alu = [0x8000_0000 | op << 23 | 10 << 16 | 12 << 8 | 8];
        let init = [(8, 1), (9, 2), (12, 3), (13, 4)];
        let code = program(&init, false, &alu, 10);
        assert!(
            xenora_ref::execute(&code, &INPUT).is_err(),
            "reference accepted opcode {op}"
        );
        assert!(
            independent_ref::execute(&code, &INPUT).is_err(),
            "independent accepted opcode {op}"
        );
    }
}

#[test]
#[ignore = "requires a native Metal device with API and shader validation"]
fn stage_c_forms_match_the_guide_on_native_metal() {
    assert_eq!(std::env::var("MTL_DEBUG_LAYER").as_deref(), Ok("1"));
    assert_eq!(std::env::var("MTL_SHADER_VALIDATION").as_deref(), Ok("1"));
    let cases = checked_cases();
    let (failures, programs) = harness::check_metal(&cases);
    failures.assert_none("native Metal disagrees with the guide");
    eprintln!(
        "sop2_stage_c: {} Metal cases in {programs} programs",
        cases.len()
    );
}

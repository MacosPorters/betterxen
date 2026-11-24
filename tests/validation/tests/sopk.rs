// SPDX-License-Identifier: GPL-3.0-or-later

//! Independent expected values for the sixteen SOPK integer forms of #215:
//! `s_movk_i32`, `s_cmovk_i32`, `s_cmpk_{eq,lg,gt,ge,lt,le}_{i32,u32}`,
//! `s_addk_i32` and `s_mulk_i32`.
//!
//! The expectations are written below from the public AMD "RDNA 2" Instruction
//! Set Architecture Reference Guide only: section 12.2 "SOPK Instructions" for
//! each form's operation (and that no literal dword may follow), section
//! 13.1.2 for the SOPK word and its SDST range, and Tables 11-14 of sections
//! 5.4-5.7 for which forms set SCC. Nothing here comes from the ISA crate, the
//! semantic registry, the decoder or the shader compiler. Every case runs on
//! the reference interpreter `xenora_ref` and on the second interpreter
//! `independent_ref`; the ignored test also runs the translated MSL on native
//! Metal.
//!
//! Supported subset: SDST s0-s105 (s0-s7 are readable but, as resource
//! SGPRs, never written). Refused: SDST 106-127 (VCC, TTMP, M0, NULL, EXEC)
//! and every other SOPK opcode (`s_version`, `s_getreg`, `s_setreg*`,
//! `s_call_b64`, `s_waitcnt_*` other than the fixed `s_waitcnt_vscnt null, 0`
//! store fence, and the subvector loops).

#[path = "operand_domains/harness.rs"]
#[allow(dead_code)]
mod harness;
#[path = "../independent_ref.rs"]
mod independent_ref;

use std::collections::BTreeMap;

use harness::{Case, words};
use xenora_shader::{Architecture, compile};

/// The sixteen forms with their ENC_SOPK opcodes (guide 13.1.2, Table 66).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Form {
    MovK = 0,
    CmovK = 2,
    CmpEqI32 = 3,
    CmpLgI32 = 4,
    CmpGtI32 = 5,
    CmpGeI32 = 6,
    CmpLtI32 = 7,
    CmpLeI32 = 8,
    CmpEqU32 = 9,
    CmpLgU32 = 10,
    CmpGtU32 = 11,
    CmpGeU32 = 12,
    CmpLtU32 = 13,
    CmpLeU32 = 14,
    AddK = 15,
    MulK = 16,
}

const FORMS: [(Form, &str); 16] = [
    (Form::MovK, "s_movk_i32"),
    (Form::CmovK, "s_cmovk_i32"),
    (Form::CmpEqI32, "s_cmpk_eq_i32"),
    (Form::CmpLgI32, "s_cmpk_lg_i32"),
    (Form::CmpGtI32, "s_cmpk_gt_i32"),
    (Form::CmpGeI32, "s_cmpk_ge_i32"),
    (Form::CmpLtI32, "s_cmpk_lt_i32"),
    (Form::CmpLeI32, "s_cmpk_le_i32"),
    (Form::CmpEqU32, "s_cmpk_eq_u32"),
    (Form::CmpLgU32, "s_cmpk_lg_u32"),
    (Form::CmpGtU32, "s_cmpk_gt_u32"),
    (Form::CmpGeU32, "s_cmpk_ge_u32"),
    (Form::CmpLtU32, "s_cmpk_lt_u32"),
    (Form::CmpLeU32, "s_cmpk_le_u32"),
    (Form::AddK, "s_addk_i32"),
    (Form::MulK, "s_mulk_i32"),
];

fn is_compare(form: Form) -> bool {
    (Form::CmpEqI32 as u32..=Form::CmpLeU32 as u32).contains(&(form as u32))
}

/// signext(SIMM16[15:0]) as a 32-bit word.
fn sext(simm16: u16) -> u32 {
    i32::from(simm16 as i16) as u32
}

/// The guide's (D after, SCC after) for `form` with D = `d` before, SCC =
/// `scc` before and the immediate `k`.
fn expect(form: Form, d: u32, scc: bool, k: u16) -> (u32, bool) {
    let (di, ki, ku) = (d as i32, sext(k) as i32, u32::from(k));
    match form {
        // 0 S_MOVK_I32: D.i32 = signext(SIMM16[15:0]); Table 14: no SCC.
        Form::MovK => (sext(k), scc),
        // 2 S_CMOVK_I32: if (SCC) D.i32 = signext(SIMM16[15:0]); Table 12: no SCC.
        Form::CmovK => (if scc { sext(k) } else { d }, scc),
        // 3-8: SCC = (S0.i32 <cond> signext(SIMM16[15:0])), S0 = D.
        Form::CmpEqI32 => (d, di == ki),
        Form::CmpLgI32 => (d, di != ki),
        Form::CmpGtI32 => (d, di > ki),
        Form::CmpGeI32 => (d, di >= ki),
        Form::CmpLtI32 => (d, di < ki),
        Form::CmpLeI32 => (d, di <= ki),
        // 9-14: SCC = (S0.u32 <cond> SIMM16[15:0]), zero-extended.
        Form::CmpEqU32 => (d, d == ku),
        Form::CmpLgU32 => (d, d != ku),
        Form::CmpGtU32 => (d, d > ku),
        Form::CmpGeU32 => (d, d >= ku),
        Form::CmpLtU32 => (d, d < ku),
        Form::CmpLeU32 => (d, d <= ku),
        // 15 S_ADDK_I32: D.i32 = D.i32 + signext(SIMM16), SCC = signed overflow.
        Form::AddK => match di.checked_add(ki) {
            Some(sum) => (sum as u32, false),
            None => (di.wrapping_add(ki) as u32, true),
        },
        // 16 S_MULK_I32: D.i32 = D.i32 * signext(SIMM16) (low 32 bits); Table 11: no SCC.
        Form::MulK => ((i64::from(di) * i64::from(ki)) as u32, scc),
    }
}

/// SIMM16 values: zero, one, both sides of bit 15, all ones, an all-ones low
/// byte (selector 255 as a register field would mean a literal) and two
/// mixed patterns.
const IMMEDIATES: [u16; 10] = [
    0, 1, 0x7fff, 0x8000, 0x8001, 0xfffe, 0xffff, 0x00ff, 0x1234, 0xedcb,
];
/// Signed and unsigned boundaries, and the words the sign- and zero-extended
/// immediates produce.
const VALUES: [u32; 15] = [
    0,
    1,
    0x7fff_fffe,
    0x7fff_ffff,
    0x8000_0000,
    0x8000_0001,
    0xffff_fffe,
    0xffff_ffff,
    0x7fff,
    0x8000,
    0xffff,
    0x1_0000,
    0xffff_8000,
    0xffff_7fff,
    0x8001,
];
/// The value s10 holds throughout.
const MARKER: u32 = 0x0f0f_1234;
const INPUT: [u32; 2] = [0, u32::MAX];

const COUNTS: [(&str, usize); 9] = [
    ("sopk movk", 10 * 2),
    ("sopk cmovk", 10 * 2 * 2),
    ("sopk cmpk", 12 * 15 * 10 * 2),
    ("sopk addk", 15 * 10),
    ("sopk mulk", 15 * 10 * 2),
    ("sopk no literal", 16 * 2),
    ("sopk s105", 16),
    ("sopk cmovk unwritten sdst", 10),
    ("sopk no other write", 16),
];
/// Per form: SDST 106-127; per writing form: SDST s0-s7; per reading form: an
/// uninitialized SDST.
const REJECTIONS: usize = 16 * 22 + 4 * 8 + 14;

/// ENCODING 0b1011 in bits 28-31, OP 23-27, SDST 16-22, SIMM16 0-15 (guide
/// 13.1.2).
fn sopk(op: u32, sdst: u32, simm16: u16) -> u32 {
    0xb000_0000 | op << 23 | sdst << 16 | u32::from(simm16)
}

/// The program around `alu`: load v1, `s_mov_b32` each `init` pair and
/// s10 = MARKER, set SCC to `scc` with `s_and_b32 s12, x, x` (x = 1 or 0),
/// run `alu`, then v2 = s<result>, `s_cbranch_scc0` over
/// `v_xor_b32 v2, v1, v2`, and store v2. With the lane input [0, !0] the
/// output is [R, R] when SCC is clear and [R, !R] when it is set.
fn program(init: &[(u32, u32)], scc: bool, alu: &[u32], result: u32) -> Vec<u8> {
    let mut code = vec![0xe030_2000, 0x8000_0100, 0xbf8c_3f70];
    for &(reg, value) in init.iter().chain(&[(10, MARKER)]) {
        code.extend([0xbe80_03ff | reg << 16, value]);
    }
    let x = if scc { 129 } else { 128 };
    code.push(0x8000_0000 | 14 << 23 | 12 << 16 | x << 8 | x);
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
    let (r, scc) = out;
    Case {
        form: name,
        family,
        label: format!("{name} {label}"),
        code,
        input: INPUT.to_vec(),
        expected: vec![r, if scc { !r } else { r }],
    }
}

/// One form on s8 = `d` with SCC = `scc` before; the result is s8.
fn single(form: Form, name: &'static str, family: &'static str, d: u32, scc: bool, k: u16) -> Case {
    let code = program(&[(8, d)], scc, &[sopk(form as u32, 8, k)], 8);
    let label = format!("s8 {d:#x}, simm16 {k:#06x}, scc before {scc}");
    case(name, family, label, code, expect(form, d, scc, k))
}

fn cases() -> Vec<Case> {
    let mut cases = Vec::new();
    for (form, name) in FORMS {
        let op = form as u32;
        for k in IMMEDIATES {
            match form {
                Form::MovK => {
                    for scc in [false, true] {
                        cases.push(single(form, name, "sopk movk", 0x5a5a_5a5a, scc, k));
                    }
                }
                Form::CmovK => {
                    for d in [0x5a5a_5a5a, sext(k)] {
                        for scc in [false, true] {
                            cases.push(single(form, name, "sopk cmovk", d, scc, k));
                        }
                    }
                }
                // SCC before is the opposite of the result, so a missing
                // write shows.
                Form::AddK => {
                    for d in VALUES {
                        let scc = !expect(form, d, false, k).1;
                        cases.push(single(form, name, "sopk addk", d, scc, k));
                    }
                }
                Form::MulK => {
                    for d in VALUES {
                        for scc in [false, true] {
                            cases.push(single(form, name, "sopk mulk", d, scc, k));
                        }
                    }
                }
                // Both SCC values before, so neither a missing write nor a
                // flip of the old SCC passes.
                _ => {
                    for d in VALUES {
                        for scc in [false, true] {
                            cases.push(single(form, name, "sopk cmpk", d, scc, k));
                        }
                    }
                }
            }
        }
        // An immediate whose low byte is 0xff (or both bytes) followed by
        // `s_movk_i32 s9, 0x42`: had the word taken a literal, the move would
        // be consumed and s9 never written.
        for k in [0x00ff, 0xffff] {
            let (d, scc) = (0x8000_0000, true);
            let code = program(&[(8, d)], scc, &[sopk(op, 8, k), sopk(0, 9, 0x42)], 9);
            let label = format!("simm16 {k:#06x}, then s_movk_i32 s9");
            let after = expect(form, d, scc, k).1;
            cases.push(case(name, "sopk no literal", label, code, (0x42, after)));
        }
        // s10 (the marker) and s9 keep their values: the form writes only
        // SDST s8. The result is s10.
        let code = program(
            &[(8, 0x8000_0000), (9, 0x9999)],
            true,
            &[sopk(op, 8, 0x8000)],
            10,
        );
        let after = expect(form, 0x8000_0000, true, 0x8000).1;
        let label = "s10 after the form on s8".to_string();
        cases.push(case(
            name,
            "sopk no other write",
            label,
            code,
            (MARKER, after),
        ));
        if form == Form::CmovK {
            // A taken move initializes a never-written SDST: s20 is not read.
            for k in IMMEDIATES {
                let code = program(&[], true, &[sopk(op, 20, k)], 20);
                let label = format!("s20 unwritten, simm16 {k:#06x}");
                let family = "sopk cmovk unwritten sdst";
                cases.push(case(name, family, label, code, (sext(k), true)));
            }
        }
        // The top supported SGPR, s105, as SDST; the result is s105.
        let (d, k) = (0x7fff_fff0, 0x0020);
        let scc = if is_compare(form) || form == Form::AddK {
            !expect(form, d, false, k).1
        } else {
            true
        };
        let code = program(&[(105, d)], scc, &[sopk(op, 105, k)], 105);
        let label = format!("s105 {d:#x}, simm16 {k:#06x}");
        cases.push(case(
            name,
            "sopk s105",
            label,
            code,
            expect(form, d, scc, k),
        ));
    }
    cases
}

/// The SOPK word's byte offset in `program` with `init` SGPR moves: three
/// header words, one 8-byte move per init pair and the marker, and the AND.
fn alu_offset(init: usize) -> usize {
    12 + 8 * (init + 1) + 4
}

/// Both interpreters must refuse `code` at the SOPK word, not later.
fn assert_rejected_at_alu(label: &str, code: &[u8], init: usize) {
    let offset = alu_offset(init);
    match xenora_ref::execute(code, &INPUT) {
        Ok(_) => panic!("reference accepted {label}"),
        Err(error) => assert_eq!(error.offset, offset, "reference rejected {label} elsewhere"),
    }
    match independent_ref::execute(code, &INPUT) {
        Ok(_) => panic!("independent accepted {label}"),
        Err(error) => assert!(
            error.starts_with(&format!("{offset:#x}:")),
            "independent rejected {label} elsewhere: {error}"
        ),
    }
}

/// Programs both interpreters must refuse at the SOPK word, with the number
/// of init pairs before it.
fn rejections() -> Vec<(String, Vec<u8>, usize)> {
    let mut all = Vec::new();
    for (form, name) in FORMS {
        let op = form as u32;
        // VCC_LO/HI, TTMP0-15, M0, NULL, EXEC_LO/HI: outside the subset.
        for sdst in 106..=127 {
            let code = program(&[(8, 1)], true, &[sopk(op, sdst, 1)], 8);
            all.push((format!("{name} sdst {sdst}"), code, 1));
        }
        if is_compare(form) {
            // s20 is never written: the compare reads SDST.
            let code = program(&[(8, 1)], true, &[sopk(op, 20, 1)], 8);
            all.push((format!("{name} uninitialized s20"), code, 1));
        } else {
            for sdst in 0..8 {
                let code = program(&[(8, 1)], true, &[sopk(op, sdst, 1)], 8);
                all.push((format!("{name} write to resource s{sdst}"), code, 1));
            }
            if matches!(form, Form::AddK | Form::MulK) {
                let code = program(&[(8, 1)], true, &[sopk(op, 20, 1)], 8);
                all.push((format!("{name} uninitialized s20"), code, 1));
            }
        }
    }
    all
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

fn reference(code: &[u8], input: &[u32]) -> Result<Vec<u32>, String> {
    xenora_ref::execute(code, input).map_err(|error| format!("{error:?}"))
}

#[test]
fn sopk_forms_match_the_guide_on_the_reference() {
    let cases = checked_cases();
    let failed = failures(&cases, reference);
    assert!(
        failed.is_empty(),
        "reference disagrees with the guide:\n{}",
        failed.join("\n")
    );
    eprintln!("sopk: {} reference cases", cases.len());
    for (form, n) in tally(&cases, |case| case.form) {
        eprintln!("  form {form}: {n}");
    }
}

#[test]
fn sopk_forms_match_the_guide_on_the_independent_interpreter() {
    let cases = checked_cases();
    let failed = failures(&cases, independent_ref::execute);
    assert!(
        failed.is_empty(),
        "independent interpreter disagrees with the guide:\n{}",
        failed.join("\n")
    );
    eprintln!("sopk: {} independent-interpreter cases", cases.len());
}

#[test]
fn unsupported_sopk_operands_are_rejected() {
    let rejections = rejections();
    assert_eq!(rejections.len(), REJECTIONS);
    for (label, code, init) in &rejections {
        assert_rejected_at_alu(label, code, *init);
    }
    eprintln!("sopk: {} rejections", rejections.len());
}

#[test]
fn other_sopk_opcodes_stay_unsupported() {
    // Table 66: 1 s_version, 18-19 getreg and setreg, 21 setreg_imm32 (its
    // literal here would be the next word, refused all the same), 22 call,
    // 23-26 waitcnt and 27-28 subvector loops; 17 and 20 are unlisted. 29-31
    // are the SOP1, SOPC and SOPP encodings. With SDST s8, opcode 23 is not
    // the `null, 0` store fence either.
    for op in [1].into_iter().chain(17..=28) {
        let code = program(&[(8, 1)], true, &[sopk(op, 8, 0)], 8);
        assert_rejected_at_alu(&format!("opcode {op}"), &code, 1);
    }
    // `s_waitcnt_vscnt null, 1`: only the zero count is the supported fence.
    let code = program(&[(8, 1)], true, &[sopk(23, 125, 1)], 8);
    assert_rejected_at_alu("s_waitcnt_vscnt null, 1", &code, 1);
}

#[test]
fn cmovk_without_scc_is_rejected() {
    // The header and the s10 marker move, then `s_cmovk_i32 s8, 1` with SCC
    // never written: both interpreters must refuse it at byte 20.
    let code = words(&[
        0xe030_2000,
        0x8000_0100,
        0xbf8c_3f70,
        0xbe80_03ff | 10 << 16,
        MARKER,
        sopk(Form::CmovK as u32, 8, 1),
        0x7e04_0208,
        0xe070_2000,
        0x8001_0200,
        0xbbfd_0000,
        0xbf81_0000,
    ]);
    match xenora_ref::execute(&code, &INPUT) {
        Ok(_) => panic!("reference accepted s_cmovk_i32 without SCC"),
        Err(error) => assert_eq!(error.offset, 20, "reference rejected it elsewhere"),
    }
    match independent_ref::execute(&code, &INPUT) {
        Ok(_) => panic!("independent accepted s_cmovk_i32 without SCC"),
        Err(error) => assert!(error.starts_with("0x14:"), "independent: {error}"),
    }
}

#[test]
#[ignore = "requires a native Metal device with API and shader validation"]
fn sopk_forms_match_the_guide_on_native_metal() {
    assert_eq!(std::env::var("MTL_DEBUG_LAYER").as_deref(), Ok("1"));
    assert_eq!(std::env::var("MTL_SHADER_VALIDATION").as_deref(), Ok("1"));
    for (label, code, _) in rejections() {
        assert!(
            compile(Architecture::Gfx1030, &code).is_err(),
            "compiler accepted {label}"
        );
    }
    let cases = checked_cases();
    let (failures, programs) = harness::check_metal(&cases);
    failures.assert_none("native Metal disagrees with the guide");
    eprintln!("sopk: {} Metal cases in {programs} programs", cases.len());
}

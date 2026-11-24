// SPDX-License-Identifier: GPL-3.0-or-later

//! Guide-derived truth tables for SOP2 pair logic (AMD RDNA 2 ISA guide 12.1).
//! Encodings use sections 5.2 and 13.1.1. The pinned XML marks literal sources
//! U64, so unsigned expansion in section 6.2.1 gives a zero upper dword.

#[path = "operand_domains/harness.rs"]
#[allow(dead_code)]
mod harness;
#[path = "../independent_ref.rs"]
mod independent_ref;

use harness::{Case, Rejection, words};
use xenora_shader::{Architecture, compile};

// Each mask encodes the answers for input bits (00, 01, 10, 11), low bit first.
const FORMS: [(u32, &str, u64); 8] = [
    (15, "s_and_b64", 0b1000),
    (17, "s_or_b64", 0b1110),
    (19, "s_xor_b64", 0b0110),
    (21, "s_andn2_b64", 0b0100),
    (23, "s_orn2_b64", 0b1101),
    (25, "s_nand_b64", 0b0111),
    (27, "s_nor_b64", 0b0001),
    (29, "s_xnor_b64", 0b1001),
];
const VALUES: [u64; 6] = [0, 1, 1 << 32, 1 << 63, 0x1234_5678_a5a5_a5a5, u64::MAX];
const PREFIX: [u32; 3] = [0xe030_2000, 0x8000_0100, 0xbf8c_3f70];

fn sop2(op: u32, dst: u32, left: u32, right: u32) -> u32 {
    0x8000_0000 | op << 23 | dst << 16 | right << 8 | left
}

fn program(a: u64, b: u64, alu: &[u32], result: u32) -> Vec<u8> {
    let mut code = PREFIX.to_vec();
    for (base, value) in [(8u32, a), (104, b)] {
        code.extend([
            0xbe80_03ff | base << 16,
            value as u32,
            0xbe80_03ff | (base + 1) << 16,
            (value >> 32) as u32,
        ]);
    }
    code.extend(alu);
    // On input [0, !0], SCC controls whether the second lane is complemented.
    code.extend([0x7e04_0200 | result, 0xbf84_0001, 0x3a04_0501]);
    code.extend([0xe070_2000, 0x8001_0200, 0xbbfd_0000, 0xbf81_0000]);
    words(&code)
}

fn cases() -> Vec<Case> {
    let mut cases = Vec::new();
    for (op, form, mask) in FORMS {
        let mut add = |dst, src0, src1, literal, a, b, x: u64, y: u64| {
            let value = (0..64).rev().fold(0, |result, i| {
                let index = ((x >> i) % 2) * 2 + (y >> i) % 2;
                result * 2 + (mask >> index) % 2
            });
            let mut alu = vec![sop2(op, dst, src0, src1)];
            alu.extend(literal);
            for half in 0..2 {
                let word = (value >> (half * 32)) as u32;
                cases.push(Case {
                    form,
                    family: "SOP2 pair logic",
                    label: format!(
                        "{form} {dst}, {src0}, {src1}, {x:#018x}, {y:#018x}, half {half}"
                    ),
                    code: program(a, b, &alu, dst + half),
                    input: vec![0, u32::MAX],
                    expected: vec![word, if value != 0 { !word } else { word }],
                });
            }
        };
        for a in VALUES {
            for b in VALUES {
                for dst in [8, 10, 104] {
                    add(dst, 8, 104, None, a, b, a, b);
                }
            }
        }
        add(8, 8, 8, None, VALUES[4], 0, VALUES[4], VALUES[4]);
        for (selector, immediate, literal) in (128..=208)
            .map(|selector| {
                let value = if selector <= 192 {
                    selector - 128
                } else {
                    192 - selector
                };
                (selector as u32, value as u64, None)
            })
            .chain(
                [0x8000_0000, 0xffff_ffff, 0x1234_5678]
                    .map(|word| (255, u64::from(word), Some(word))),
            )
        {
            let a = VALUES[4];
            add(10, selector, 8, literal, a, 0, immediate, a);
            add(10, 8, selector, literal, a, 0, a, immediate);
        }
    }
    cases
}

#[test]
fn sop2_b64_oracle_matches_cpu_and_translation() {
    let cases = cases();
    assert_eq!(cases.len(), 4432);
    for case in &cases {
        let reference = xenora_ref::execute(&case.code, &case.input).unwrap();
        let independent = independent_ref::execute(&case.code, &case.input).unwrap();
        assert_eq!(reference, case.expected, "reference: {}", case.label);
        assert_eq!(
            independent, case.expected,
            "second interpreter: {}",
            case.label
        );
        let compiled = compile(Architecture::Gfx1030, &case.code).unwrap();
        let constant = format!("const.u32 0x{:08x}", case.expected[0]);
        let branch = format!("taken={}", case.expected[0] == case.expected[1]);
        assert!(
            compiled.ir.contains(&constant),
            "{}: {constant}\n{}",
            case.label,
            compiled.ir
        );
        assert!(
            compiled.ir.contains(&branch),
            "{}: {branch}\n{}",
            case.label,
            compiled.ir
        );
    }
    eprintln!(
        "{} SOP2 pair cases on both interpreters and translated IR",
        cases.len()
    );
}

#[test]
fn sop2_b64_rejects_operands_outside_the_pair_contract() {
    let mut rejections = Vec::new();
    for (op, form, _) in FORMS {
        let mut reject = |dst, a, b, truncated: bool| {
            let mut code = PREFIX.to_vec();
            code.extend([0xbe88_0380, sop2(op, dst, a, b)]);
            if !truncated {
                code.extend([0xe070_2000, 0x8001_0100, 0xbbfd_0000, 0xbf81_0000]);
            }
            rejections.push(Rejection {
                family: form,
                truncated,
                label: format!("{form} {dst}, {a}, {b}"),
                code: words(&code),
                offset: 16,
            });
        };
        for dst in (1..=105).step_by(2).chain(106..=127).chain([0, 2, 4, 6]) {
            reject(dst, 128, 128, false);
        }
        for src in (1..=105).step_by(2).chain(106..=127).chain(209..=254) {
            reject(10, src, 128, false);
            reject(10, 128, src, false);
        }
        reject(10, 8, 128, false); // s8 initialized, s9 missing.
        reject(10, 128, 8, false);
        reject(10, 255, 255, false);
        reject(10, 255, 128, true);
        reject(10, 128, 255, true);
    }
    assert_eq!(rejections.len(), 2608);
    harness::check_rejected(&rejections).assert_none("SOP2 pair rejections");
    for rejection in &rejections {
        let error = independent_ref::execute(&rejection.code, &[0, 1]).unwrap_err();
        if rejection.truncated {
            assert_eq!(error, "0x14: truncated instruction", "{}", rejection.label);
        } else {
            assert!(error.starts_with("0x10:"), "{}: {error}", rejection.label);
        }
    }
    eprintln!("{} SOP2 pair rejections", rejections.len());
}

#[test]
#[ignore = "requires a native Metal device with API and shader validation"]
fn sop2_b64_oracle_matches_native_metal() {
    assert_eq!(std::env::var("MTL_DEBUG_LAYER").as_deref(), Ok("1"));
    assert_eq!(std::env::var("MTL_SHADER_VALIDATION").as_deref(), Ok("1"));
    let cases = cases();
    let (failures, programs) = harness::check_metal(&cases);
    failures.assert_none("native Metal disagrees with the SOP2 pair oracle");
    eprintln!(
        "{} SOP2 pair cases in {programs} native Metal programs",
        cases.len()
    );
}

// SPDX-License-Identifier: GPL-3.0-or-later

//! Wave32 VOP2 select and carry/borrow oracle from AMD RDNA 2 ISA guide
//! sections 3.3, 3.9, 12.7 and 13.3.1.

#[path = "../independent_ref.rs"]
mod independent_ref;

use xenora_shader::{Architecture, CompileError, compile};

const FORMS: [u32; 4] = [1, 40, 41, 42];
const EXEC_PATTERNS: [u32; 3] = [u32::MAX, 0xaaaa_aaab, 0];
const INITIAL_DST: u32 = 0x1234_5678;
const SCALAR_VCC: u32 = 0x8000_0005;

fn words(code: &[u32]) -> Vec<u8> {
    code.iter().flat_map(|word| word.to_le_bytes()).collect()
}

fn vop2(op: u32, dst: u32, src0: u32, src1: u32) -> u32 {
    op << 25 | dst << 17 | src1 << 9 | src0
}

fn program(body: &[u32], output: u32) -> Vec<u8> {
    let mut code = vec![0xe030_2000, 0x8000_0100, 0xbf8c_3f70];
    code.extend(body);
    code.extend([
        0xe070_2000,
        0x8001_0000 | output << 8,
        0xbbfd_0000,
        0xbf81_0000,
    ]);
    words(&code)
}

fn input() -> Vec<u32> {
    [0, 1, 2, u32::MAX, u32::MAX - 1, 0x8000_0000, 0, 31]
        .into_iter()
        .cycle()
        .take(67)
        .collect()
}

/// Guide 12.7 gives the unbounded result and the 32-bit destination.
/// For SUBREV the prose and D equation reverse the operands. The printed
/// borrow formula repeats SUB's order, inconsistent with that subtraction.
fn expected(op: u32, a: u32, b: u32, bit: u32) -> (u32, bool) {
    let (a, b, bit) = (i128::from(a), i128::from(b), i128::from(bit));
    let exact = match op {
        1 => return (if bit == 0 { a } else { b } as u32, false),
        40 => a + b + bit,
        41 => a - b - bit,
        42 => b - a - bit,
        _ => unreachable!(),
    };
    let modulus = 1_i128 << 32;
    (
        exact.rem_euclid(modulus) as u32,
        !(0..modulus).contains(&exact),
    )
}

fn cases() -> Vec<(Vec<u8>, Vec<u32>)> {
    let input = input();
    let mut cases = Vec::new();
    for op in FORMS {
        for exec in EXEC_PATTERNS {
            for (selector, extension, src0) in [
                (129, None, 1),
                (193, None, u32::MAX),
                (8, None, 0x8000_0000),
                (255, Some(u32::MAX - 1), u32::MAX - 1),
                (259, None, INITIAL_DST),
                (106, None, 0),
            ] {
                for from_compare in [false, true] {
                    let mut body = vec![0x7e06_02ff, INITIAL_DST, 0xbe88_03ff, 0x8000_0000];
                    if from_compare {
                        body.push(0x7d84_0280); // v_cmp_eq_u32_e32 vcc_lo, 0, v1
                    } else {
                        body.extend([0xbeea_03ff, SCALAR_VCC]); // s_mov_b32 vcc_lo, literal
                    }
                    body.extend([0xbefe_03ff, exec]);
                    body.push(vop2(op, 3, selector, 1));
                    body.extend(extension);
                    body.push(0xbefe_03c1); // restore full EXEC for the store

                    let mut values = Vec::new();
                    let mut masks = Vec::new();
                    for wave in input.chunks(32) {
                        let incoming = if from_compare {
                            wave.iter()
                                .enumerate()
                                .fold(0, |mask, (lane, &x)| mask | (u32::from(x == 0) << lane))
                        } else {
                            SCALAR_VCC
                        };
                        let mut mask = if op == 1 { incoming } else { 0 };
                        for (lane, &x) in wave.iter().enumerate() {
                            if exec & (1 << lane) == 0 {
                                values.push(INITIAL_DST);
                            } else {
                                let a = if selector == 106 { incoming } else { src0 };
                                let (value, flag) = expected(op, a, x, (incoming >> lane) & 1);
                                values.push(value);
                                mask |= u32::from(flag) << lane;
                            }
                        }
                        masks.extend(std::iter::repeat_n(mask, wave.len()));
                    }
                    cases.push((program(&body, 3), values));
                    body.push(0x7e08_026a); // v_mov_b32_e32 v4, vcc_lo
                    cases.push((program(&body, 4), masks));
                }
            }
        }
    }
    cases
}

#[test]
fn vop2_vcc_matches_guide_on_both_interpreters() {
    let input = input();
    for (index, (code, expected)) in cases().into_iter().enumerate() {
        compile(Architecture::Gfx1030, &code).unwrap();
        assert_eq!(
            xenora_ref::execute(&code, &input).unwrap(),
            expected,
            "reference case {index}"
        );
        assert_eq!(
            independent_ref::execute(&code, &input).unwrap(),
            expected,
            "independent case {index}"
        );
    }
}

#[test]
fn vop2_vcc_rejects_unsupported_operands_and_uninitialized_state() {
    let mut cases = Vec::new();
    for op in FORMS {
        cases.push((program(&[vop2(op, 2, 129, 1)], 2), 12));
        for selector in [107, 124, 126, 128 + 112, 249, 250] {
            cases.push((program(&[vop2(op, 2, selector, 1), 0], 2), 12));
        }
        let fixture = words(&[vop2(op, 2, 255, 1)]);
        assert!(matches!(
            compile(Architecture::Gfx1030, &fixture),
            Err(CompileError::Decode(_))
        ));
        assert!(xenora_ref::execute(&fixture, &[1]).is_err());
        assert!(independent_ref::execute(&fixture, &[1]).is_err());
        cases.push((
            program(&[0xbeea_03c1, 0xbefe_0381, vop2(op, 2, 129, 1)], 2),
            20,
        ));
    }
    // LLVM-assembled e64 forms with an explicit scalar condition operand.
    for first in [0xd501_0002, 0xd528_0902, 0xd529_0902, 0xd52a_0902] {
        cases.push((program(&[first, 0x0022_0303], 2), 12));
    }
    for (code, offset) in cases {
        let result = compile(Architecture::Gfx1030, &code);
        let got = match result {
            Err(CompileError::Decode(error)) => error.offset,
            Err(CompileError::Verify { offset, .. }) => offset,
            other => panic!("expected rejection at {offset:#x}: {other:?}"),
        };
        assert_eq!(got, offset);
        assert_eq!(
            xenora_ref::execute(&code, &[1, 2]).unwrap_err().offset,
            offset
        );
        assert!(
            independent_ref::execute(&code, &[1, 2])
                .unwrap_err()
                .starts_with(&format!("{offset:#x}:"))
        );
    }
}

#[test]
#[ignore = "requires a native Metal device with API and shader validation"]
fn vop2_vcc_matches_guide_on_native_metal() {
    assert_eq!(std::env::var("MTL_DEBUG_LAYER").as_deref(), Ok("1"));
    assert_eq!(std::env::var("MTL_SHADER_VALIDATION").as_deref(), Ok("1"));
    let metal = xenora_metal::NativeCompute::new().unwrap();
    let input = input();
    let fixture = include_bytes!("../../../fixtures/rdna2/vector-vcc.bin");
    let fixture_expected = input
        .chunks(32)
        .flat_map(|wave| {
            let mask = wave
                .iter()
                .enumerate()
                .fold(0, |mask, (lane, &x)| mask | (u32::from(x == 0) << lane));
            std::iter::repeat_n(mask, wave.len())
        })
        .collect();
    for (index, (code, expected)) in cases()
        .into_iter()
        .chain([(fixture.to_vec(), fixture_expected)])
        .enumerate()
    {
        let shader = compile(Architecture::Gfx1030, &code).unwrap();
        assert_eq!(
            metal
                .run_words(&shader.msl, xenora_shader::ENTRY_POINT, &input)
                .unwrap(),
            expected,
            "Metal case {index}",
        );
    }
}

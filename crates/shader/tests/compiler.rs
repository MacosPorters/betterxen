// SPDX-License-Identifier: GPL-3.0-or-later

use xenora_shader::{Architecture, CompileError, CompileOptions, compile, compile_with};

const FIXTURE: &[u8] = include_bytes!("../../../fixtures/rdna2/add.bin");

#[test]
fn compiles_original_fixture() {
    let result = compile(Architecture::Gfx1030, FIXTURE).unwrap();
    assert_eq!(result.ir, include_str!("../../../fixtures/golden/add.ir"));
    assert_eq!(
        result.msl,
        include_str!("../../../fixtures/golden/add.metal")
    );
}

#[test]
fn vop3_compare_to_vcc_lo_drives_a_runtime_vccz_branch() {
    let code: Vec<_> = [
        0x7e02_0281u32, // v_mov_b32_e32 v1, 1
        0x7e04_0282,    // v_mov_b32_e32 v2, 2
        0xd4c2_006a,    // v_cmp_eq_u32_e64 vcc_lo, v1, v2
        0x0202_0501,
        0xbf86_0001, // s_cbranch_vccz 1
        0x7e02_0302, // v_mov_b32_e32 v1, v2
    ]
    .into_iter()
    .flat_map(u32::to_le_bytes)
    .chain(FIXTURE[28..].iter().copied())
    .collect();
    let shader = compile(Architecture::Gfx1030, &code).unwrap();
    assert!(
        shader
            .disassembly
            .contains("v_cmp_eq_u32_e64 vcc_lo, v1, v2")
    );
    assert!(shader.ir.contains("0008: t2 = wave.mask"));
    assert!(shader.ir.contains("0010: branch.vccz t2 -> 0x0018"));
    assert!(shader.ir.contains("join.vccz t2"));
    assert!(shader.msl.contains("lane_index >= grid"));
}

#[test]
fn vop3_compare_to_s4_rejects_later_scalar_reads() {
    for (setup, offset) in [
        (&[0x7e02_0281u32, 0x7e04_0282][..], 16), // Initialize both VGPRs.
        (&[0xbefe_0380][..], 12),                 // Empty EXEC. The VGPRs need not be initialized.
    ] {
        let code: Vec<_> = setup
            .iter()
            .copied()
            .chain([
                0xd4c2_0004, // v_cmp_eq_u32_e64 s4, v1, v2
                0x0202_0501,
                0xbf06_8004, // s_cmp_eq_u32 s4, 0
                0xbefe_03c1, // s_mov_b32 exec_lo, -1
                0x7e02_0281, // v_mov_b32_e32 v1, 1
            ])
            .flat_map(u32::to_le_bytes)
            .chain(FIXTURE[28..].iter().copied())
            .collect();
        let error = compile(Architecture::Gfx1030, &code).unwrap_err();
        assert!(
            matches!(
                error,
                CompileError::Verify {
                    offset: actual,
                    reason: "scalar source reads a runtime-only SGPR compare mask",
                } if actual == offset
            ),
            "{error}"
        );
    }
}

#[test]
fn vop3_compare_mask_cannot_be_read_as_a_buffer_descriptor() {
    let code: Vec<_> = [
        0x7e02_0281u32, // v_mov_b32_e32 v1, 1
        0x7e04_0282,    // v_mov_b32_e32 v2, 2
        0xd4c2_0004,    // v_cmp_eq_u32_e64 s4, v1, v2
        0x0202_0501,
    ]
    .into_iter()
    .flat_map(u32::to_le_bytes)
    .chain(FIXTURE[28..].iter().copied())
    .collect();
    assert!(matches!(
        compile(Architecture::Gfx1030, &code),
        Err(CompileError::Verify {
            offset: 16,
            reason: "resource descriptor reads a runtime-only SGPR compare mask",
        })
    ));
}

#[test]
fn vop3_compare_to_normal_sgpr_preserves_vcc_and_can_be_overwritten() {
    let code: Vec<_> = [
        0x7e02_0281u32, // v_mov_b32_e32 v1, 1
        0x7e04_0282,    // v_mov_b32_e32 v2, 2
        0xbeea_0381,    // s_mov_b32 vcc_lo, 1
        0xd4c2_0008,    // v_cmp_eq_u32_e64 s8, v1, v2
        0x0202_0501,
        0xbf86_0001, // s_cbranch_vccz 1
        0xbe88_0381, // s_mov_b32 s8, 1
        0xbf06_8008, // s_cmp_eq_u32 s8, 0
    ]
    .into_iter()
    .flat_map(u32::to_le_bytes)
    .chain(FIXTURE[28..].iter().copied())
    .collect();
    let shader = compile(Architecture::Gfx1030, &code).unwrap();
    assert!(
        shader
            .ir
            .contains("0014: branch.uniform 0x001c taken=false")
    );
}

#[test]
fn load_wait_is_required_before_using_loaded_data() {
    let mut code = FIXTURE.to_vec();
    code.drain(8..12);
    assert!(matches!(
        compile(Architecture::Gfx1030, &code),
        Err(CompileError::Verify { offset: 16, .. })
    ));
}

#[test]
fn store_wait_is_required_before_termination() {
    let mut code = FIXTURE.to_vec();
    code.drain(36..40);
    assert!(matches!(
        compile(Architecture::Gfx1030, &code),
        Err(CompileError::Verify { offset: 36, .. })
    ));
}

#[test]
fn rejects_unaligned_length_before_decoding() {
    assert!(matches!(
        compile(Architecture::Gfx1030, &FIXTURE[..FIXTURE.len() - 1]),
        Err(CompileError::InvalidLength { .. })
    ));
}

const BRANCH: &[u8] = include_bytes!("../../../fixtures/rdna2/bitwise-branch.bin");
const FLOAT: &[u8] = include_bytes!("../../../fixtures/rdna2/float-exact.bin");

#[test]
fn uniform_branch_selects_each_known_path() {
    let taken = compile(Architecture::Gfx1030, BRANCH).unwrap();
    assert_eq!(
        taken.ir,
        include_str!("../../../fixtures/golden/bitwise-branch.ir")
    );
    assert_eq!(
        taken.msl,
        include_str!("../../../fixtures/golden/bitwise-branch.metal")
    );

    let mut equal = BRANCH.to_vec();
    equal[17] = 0x87; // Compare s8 with 7 instead of 6.
    let fallthrough = compile(Architecture::Gfx1030, &equal).unwrap();
    assert!(fallthrough.ir.contains("branch.uniform 0x0020 taken=false"));
    assert!(!fallthrough.ir.contains("and.b32"));
    assert!(fallthrough.ir.contains("const.u32 0x00000000"));
}

#[test]
fn rejects_branch_into_a_literal_word() {
    let mut invalid = BRANCH.to_vec();
    invalid[20] = 3; // Branch to byte 36, inside the literal move at byte 32.
    assert!(matches!(
        compile(Architecture::Gfx1030, &invalid),
        Err(CompileError::Decode(error)) if error.offset == 20
    ));
}

#[test]
fn compiles_only_proven_exact_float_operations() {
    let result = compile(Architecture::Gfx1030, FLOAT).unwrap();
    assert_eq!(
        result.ir,
        include_str!("../../../fixtures/golden/float-exact.ir")
    );
    assert_eq!(
        result.msl,
        include_str!("../../../fixtures/golden/float-exact.metal")
    );

    let mut invalid = FLOAT.to_vec();
    invalid[16..20].copy_from_slice(&0x007f_ffffu32.to_le_bytes());
    assert!(matches!(
        compile(Architecture::Gfx1030, &invalid),
        Err(CompileError::Verify { offset: 40, .. })
    ));
}

#[test]
fn enforces_the_fixture_size_limit_before_decoding() {
    assert!(matches!(
        compile(Architecture::Gfx1030, &[]),
        Err(CompileError::InvalidLength { bytes: 0 })
    ));
    let mut code = FIXTURE.to_vec();
    // Prepend redundant s_waitcnt vmcnt(0) instructions to reach 1024 bytes.
    code.splice(
        ..0,
        [0x70, 0x3f, 0x8c, 0xbf].repeat((1024 - FIXTURE.len()) / 4),
    );
    assert!(compile(Architecture::Gfx1030, &code).is_ok());
    code.extend_from_slice(&[0; 4]);
    assert!(matches!(
        compile(Architecture::Gfx1030, &code),
        Err(CompileError::InvalidLength { bytes: 1028 })
    ));
}

#[test]
fn preserves_the_decoder_diagnostic_and_error_source() {
    let error = compile(Architecture::Gfx1030, &[0xff; 4]).unwrap_err();
    let expected = "gfx1030 decode at byte 0x0: unsupported opcode in 0xffffffff";
    assert_eq!(error.to_string(), expected);
    assert_eq!(
        std::error::Error::source(&error).unwrap().to_string(),
        expected
    );
}

#[test]
fn rejects_register_moves_into_index_and_outstanding_load_destinations() {
    for (offset, instruction) in [
        (12, [0x01, 0x03, 0x00, 0x7e]), // v_mov_b32_e32 v0, v1
        (24, [0x02, 0x03, 0x02, 0x7e]), // v_mov_b32_e32 v1, v2
    ] {
        let mut code = FIXTURE.to_vec();
        if offset == 24 {
            // Move the load wait after the register move. v2 is initialized.
            code.drain(8..12);
            code.splice(16..16, instruction);
        } else {
            code.splice(offset..offset, instruction);
        }
        let expected_offset = if offset == 24 { 16 } else { offset };
        assert!(matches!(
            compile(Architecture::Gfx1030, &code),
            Err(CompileError::Verify {
                offset,
                reason: "move writes the index register or replaces a pending load",
            }) if offset == expected_offset
        ));
    }
}

#[test]
fn refuses_to_terminate_with_an_outstanding_load() {
    let mut code = FIXTURE.to_vec();
    // Add a second input load after the completed store. An output exists,
    // but the new load still needs its own wait.
    code.splice(40..40, FIXTURE[..8].iter().copied());
    assert!(matches!(
        compile(Architecture::Gfx1030, &code),
        Err(CompileError::Verify {
            offset: 48,
            reason: "termination requires one completed store and no pending loads",
        })
    ));
}

#[test]
fn protects_reserved_scalar_resource_registers() {
    let mut code = FIXTURE.to_vec();
    code.splice(..0, 0xbe87_0380u32.to_le_bytes()); // s_mov_b32 s7, 0
    assert!(matches!(
        compile(Architecture::Gfx1030, &code),
        Err(CompileError::Verify {
            offset: 0,
            reason: "scalar move would modify a resource descriptor",
        })
    ));
    code[2] = 0x88; // s8 is outside both resource descriptors.
    assert!(compile(Architecture::Gfx1030, &code).is_ok());
}

#[test]
fn proves_a_float_product_using_multiplication() {
    let mut code: Vec<_> = [
        0x7e02_02ffu32, // v_mov_b32_e32 v1, 0x4e800000 (2^30)
        0x4e80_0000,
        0x7e04_02ff, // v_mov_b32_e32 v2, 0x40000000 (2.0)
        0x4000_0000,
        0x1006_0501, // v_mul_f32_e32 v3, v1, v2
    ]
    .into_iter()
    .flat_map(u32::to_le_bytes)
    .collect();
    code.extend_from_slice(&FIXTURE[28..]);
    code[25] = 3; // Store v3.
    // The product 2^31 is exact, while 2^30 + 2 is not representable.
    let shader = compile(Architecture::Gfx1030, &code).unwrap();
    assert!(shader.ir.contains("mul.f32 t0, t1"));
    assert!(
        shader
            .msl
            .contains("as_type<float>(t0) * as_type<float>(t1)")
    );
}

#[test]
fn rejects_load_into_index_or_before_waiting_on_previous_load() {
    let mut code = FIXTURE.to_vec();
    code[5] = 0; // Load into reserved v0.
    assert!(matches!(
        compile(Architecture::Gfx1030, &code),
        Err(CompileError::Verify { offset: 0, .. })
    ));
    let mut code = FIXTURE.to_vec();
    code.splice(8..8, FIXTURE[..8].iter().copied());
    assert!(matches!(
        compile(Architecture::Gfx1030, &code),
        Err(CompileError::Verify { offset: 8, .. })
    ));
}

#[test]
fn checks_fixture_layout_and_bindings_even_in_skipped_code() {
    for (prefix, expected_offset, expected_reason) in [
        (
            &[0xbf82_0001u32, 0xbf81_0000][..], // Skip an early s_endpgm.
            8,
            "instruction follows program termination",
        ),
        (
            &[0xbf82_0002, 0xe030_2000, 0x8001_0100], // Skip load from s[4:7].
            4,
            "unsupported resource or buffer index",
        ),
        (
            &[0xbf82_0002, 0xe070_2000, 0x8000_0100], // Skip store to s[0:3].
            4,
            "unsupported resource or buffer index",
        ),
    ] {
        let code: Vec<_> = prefix
            .iter()
            .copied()
            .flat_map(u32::to_le_bytes)
            .chain(FIXTURE.iter().copied())
            .collect();
        assert!(matches!(
            compile(Architecture::Gfx1030, &code),
            Err(CompileError::Verify { offset, reason })
                if offset == expected_offset && reason == expected_reason
        ));
    }

    // A structurally valid instruction on the skipped path need not read an
    // initialized register. Here v_mov_b32_e32 v1, v255 never runs.
    let code: Vec<_> = [0xbf82_0001u32, 0x7e02_03ff]
        .into_iter()
        .flat_map(u32::to_le_bytes)
        .chain(FIXTURE.iter().copied())
        .collect();
    let compiled = compile(Architecture::Gfx1030, &code).unwrap();
    assert!(compiled.ir.contains("branch.uniform 0x0008 taken=true"));
    assert!(compiled.msl.contains("uint t1 = input[index];"));
    assert!(compiled.msl.contains("output[index] = t3;"));
}

#[test]
fn scalar_alu_results_reach_the_kernel_as_constants() {
    let code = include_bytes!("../../../fixtures/rdna2/scalar-alu.bin");
    let compiled = compile(Architecture::Gfx1030, code).unwrap();
    assert_eq!(
        compiled.ir,
        include_str!("../../../fixtures/golden/scalar-alu.ir")
    );
    assert_eq!(
        compiled.msl,
        include_str!("../../../fixtures/golden/scalar-alu.metal")
    );
}

#[test]
fn scalar_logic_results_reach_the_kernel_as_constants() {
    let code = include_bytes!("../../../fixtures/rdna2/scalar-logic.bin");
    let compiled = compile(Architecture::Gfx1030, code).unwrap();
    assert_eq!(
        compiled.ir,
        include_str!("../../../fixtures/golden/scalar-logic.ir")
    );
    assert_eq!(
        compiled.msl,
        include_str!("../../../fixtures/golden/scalar-logic.metal")
    );
}

#[test]
fn scalar_compare_results_resolve_every_branch() {
    let code = include_bytes!("../../../fixtures/rdna2/scalar-compare.bin");
    let compiled = compile(Architecture::Gfx1030, code).unwrap();
    assert_eq!(
        compiled.ir,
        include_str!("../../../fixtures/golden/scalar-compare.ir")
    );
    assert_eq!(
        compiled.msl,
        include_str!("../../../fixtures/golden/scalar-compare.metal")
    );
}

#[test]
fn scalar_immediate_results_resolve_every_branch() {
    let code = include_bytes!("../../../fixtures/rdna2/scalar-immediate.bin");
    let compiled = compile(Architecture::Gfx1030, code).unwrap();
    assert_eq!(
        compiled.ir,
        include_str!("../../../fixtures/golden/scalar-immediate.ir")
    );
    assert_eq!(
        compiled.msl,
        include_str!("../../../fixtures/golden/scalar-immediate.metal")
    );
}

#[test]
fn scalar_unary_results_reach_the_kernel_as_constants() {
    let code = include_bytes!("../../../fixtures/rdna2/scalar-unary.bin");
    let compiled = compile(Architecture::Gfx1030, code).unwrap();
    assert_eq!(
        compiled.ir,
        include_str!("../../../fixtures/golden/scalar-unary.ir")
    );
    assert_eq!(
        compiled.msl,
        include_str!("../../../fixtures/golden/scalar-unary.metal")
    );
}

#[test]
fn scalar_arith_results_reach_the_kernel_as_constants() {
    let code = include_bytes!("../../../fixtures/rdna2/scalar-arith.bin");
    let compiled = compile(Architecture::Gfx1030, code).unwrap();
    assert_eq!(
        compiled.ir,
        include_str!("../../../fixtures/golden/scalar-arith.ir")
    );
    assert_eq!(
        compiled.msl,
        include_str!("../../../fixtures/golden/scalar-arith.metal")
    );
}

#[test]
fn vector_integer_forms_lower_to_per_lane_templates() {
    let code = include_bytes!("../../../fixtures/rdna2/vector-integer.bin");
    let compiled = compile(Architecture::Gfx1030, code).unwrap();
    assert_eq!(
        compiled.ir,
        include_str!("../../../fixtures/golden/vector-integer.ir")
    );
    assert_eq!(
        compiled.msl,
        include_str!("../../../fixtures/golden/vector-integer.metal")
    );
}

#[test]
fn vop3_integer_forms_lower_to_per_lane_templates() {
    let code = include_bytes!("../../../fixtures/rdna2/vop3-integer.bin");
    let compiled = compile(Architecture::Gfx1030, code).unwrap();
    assert_eq!(
        compiled.ir,
        include_str!("../../../fixtures/golden/vop3-integer.ir")
    );
    assert_eq!(
        compiled.msl,
        include_str!("../../../fixtures/golden/vop3-integer.metal")
    );
}

#[test]
fn vector_f_and_t_compares_lower_to_operand_free_wave_masks() {
    let code = include_bytes!("../../../fixtures/rdna2/vector-compare-ft.bin");
    let result = compile(Architecture::Gfx1030, code).unwrap();
    assert_eq!(
        result.ir,
        include_str!("../../../fixtures/golden/vector-compare-ft.ir")
    );
    assert_eq!(
        result.msl,
        include_str!("../../../fixtures/golden/vector-compare-ft.metal")
    );
}

#[test]
fn vector_compares_lower_to_per_thread_wave_masks_and_selects() {
    let code = include_bytes!("../../../fixtures/rdna2/vector-compare.bin");
    let result = compile(Architecture::Gfx1030, code).unwrap();
    assert_eq!(
        result.ir,
        include_str!("../../../fixtures/golden/vector-compare.ir")
    );
    assert_eq!(
        result.msl,
        include_str!("../../../fixtures/golden/vector-compare.metal")
    );
}

#[test]
fn vop3_compare_forms_lower_to_wave_masks() {
    let code = include_bytes!("../../../fixtures/rdna2/vop3-compare.bin");
    let compiled = compile(Architecture::Gfx1030, code).unwrap();
    assert_eq!(
        compiled.ir,
        include_str!("../../../fixtures/golden/vop3-compare.ir")
    );
    assert_eq!(
        compiled.msl,
        include_str!("../../../fixtures/golden/vop3-compare.metal")
    );
}

#[test]
fn vector_vcc_fixture_matches_ir_and_msl_goldens() {
    let code = include_bytes!("../../../fixtures/rdna2/vector-vcc.bin");
    let result = compile(Architecture::Gfx1030, code).unwrap();
    assert_eq!(
        result.ir,
        include_str!("../../../fixtures/golden/vector-vcc.ir")
    );
    assert_eq!(
        result.msl,
        include_str!("../../../fixtures/golden/vector-vcc.metal")
    );
}

#[test]
fn vop1_integer_fixture_lowers_unary_and_masked_swap() {
    let code = include_bytes!("../../../fixtures/rdna2/vector-unary.bin");
    let compiled = compile(Architecture::Gfx1030, code).unwrap();
    assert_eq!(
        compiled.ir,
        include_str!("../../../fixtures/golden/vector-unary.ir")
    );
    assert_eq!(
        compiled.msl,
        include_str!("../../../fixtures/golden/vector-unary.metal")
    );
}

// --- Opt-in constant folding: hand-computed known-answer fixtures -------------
// Instruction words are built by hand to match the existing RDNA2 fixture shape
// (load, wait, mov literals, op, store). Expected constants are derived from the
// RDNA2 guide / MSL templates in lib.rs, then checked against fold-on IR. Fold-off
// (and default compile) must leave the op mnemonic in the IR.

fn words(words: &[u32]) -> Vec<u8> {
    words.iter().flat_map(|word| word.to_le_bytes()).collect()
}

fn mov_lit(dst: u32) -> u32 {
    0x7e00_0000 | (dst << 17) | (1 << 9) | 255
}

fn vop2(op: u32, dst: u32, src0_vgpr: u32, src1_vgpr: u32) -> u32 {
    (op << 25) | (dst << 17) | (src1_vgpr << 9) | (0x100 + src0_vgpr)
}

fn vop1(op: u32, dst: u32, src_vgpr: u32) -> u32 {
    0x7e00_0000 | (dst << 17) | (op << 9) | (0x100 + src_vgpr)
}

/// load v1; mov v2,A; mov v3,B; VOP2 dst=v2 (or v4); store the result VGPR.
fn binary_const_fixture(op: u32, left: u32, right: u32, dst_is_v4: bool) -> Vec<u8> {
    let (dst, store_hi) = if dst_is_v4 {
        (4u32, 0x8001_0400u32)
    } else {
        (2, 0x8001_0200)
    };
    // For shift-rev forms the ISA encodes count in SRC0 and value in SRC1; the
    // caller passes (count, value) already swapped into (src0, src1) via left/right
    // naming below when needed. Non-rev forms use SRC0=v2, SRC1=v3.
    let op_word = if dst_is_v4 {
        // Shift-rev: v_l*rev dst, v3(count), v2(value) → SRC0=v3, SRC1=v2.
        vop2(op, dst, 3, 2)
    } else {
        vop2(op, dst, 2, 3)
    };
    words(&[
        0xe030_2000,
        0x8000_0100, // buffer_load_dword v1
        0xbf8c_3f70, // s_waitcnt vmcnt(0)
        mov_lit(2),
        left, // v_mov_b32 v2, left
        mov_lit(3),
        right, // v_mov_b32 v3, right
        op_word,
        0xe070_2000,
        store_hi,    // buffer_store_dword result
        0xbbfd_0000, // s_waitcnt_vscnt
        0xbf81_0000, // s_endpgm
    ])
}

fn unary_const_fixture(op: u32, source: u32) -> Vec<u8> {
    words(&[
        0xe030_2000,
        0x8000_0100,
        0xbf8c_3f70,
        mov_lit(2),
        source,         // v_mov_b32 v2, source
        vop1(op, 3, 2), // v_* v3, v2
        0xe070_2000,
        0x8001_0300, // buffer_store_dword v3
        0xbbfd_0000,
        0xbf81_0000,
    ])
}

/// load; mov v2,A; mov v3,B; and v2,v2,v3; xor v1,v1,v2; store.
/// Hand-computed: 0x12345678 & 0xff00ff00 == 0x12005600.
fn constant_and_fixture() -> Vec<u8> {
    const A: u32 = 0x1234_5678;
    const B: u32 = 0xff00_ff00;
    words(&[
        0xe030_2000,
        0x8000_0100,
        0xbf8c_3f70,
        mov_lit(2),
        A,
        mov_lit(3),
        B,
        0x3604_0702, // v_and_b32 v2, v2, v3 (OP=27)
        0x3a02_0501, // v_xor_b32 v1, v1, v2
        0xe070_2000,
        0x8001_0100,
        0xbbfd_0000,
        0xbf81_0000,
    ])
}

fn compile_fold_off(code: &[u8]) -> xenora_shader::CompiledShader {
    compile_with(
        Architecture::Gfx1030,
        code,
        CompileOptions {
            fold_constants: false,
        },
    )
    .unwrap()
}

fn compile_fold_on(code: &[u8]) -> xenora_shader::CompiledShader {
    compile_with(
        Architecture::Gfx1030,
        code,
        CompileOptions {
            fold_constants: true,
        },
    )
    .unwrap()
}

fn assert_folds_to(code: &[u8], mnemonic: &str, expected: u32) {
    let plain = compile_fold_off(code);
    let folded = compile_fold_on(code);
    let default = compile(Architecture::Gfx1030, code).unwrap();
    assert_eq!(
        default.ir, plain.ir,
        "default compile must match fold_constants: false"
    );
    assert!(
        plain.ir.contains(mnemonic),
        "fold-off IR must keep {mnemonic}: {}",
        plain.ir
    );
    assert!(
        !folded.ir.contains(mnemonic),
        "fold-on IR must drop {mnemonic}: {}",
        folded.ir
    );
    let const_ir = format!("const.u32 0x{expected:08x}");
    assert!(
        folded.ir.contains(&const_ir),
        "fold-on IR must contain {const_ir}: {}",
        folded.ir
    );
    let const_msl = format!("0x{expected:08x}u");
    assert!(
        folded.msl.contains(&const_msl),
        "fold-on MSL must contain {const_msl}: {}",
        folded.msl
    );
}

#[test]
fn default_compile_matches_fold_constants_off() {
    let code = constant_and_fixture();
    let baseline = compile(Architecture::Gfx1030, &code).unwrap();
    let explicit = compile_fold_off(&code);
    assert_eq!(baseline.ir, explicit.ir);
    assert_eq!(baseline.msl, explicit.msl);
    assert!(baseline.ir.contains("and.b32"));
}

#[test]
fn folds_constant_and_b32_to_known_answer() {
    // 0x12345678 & 0xff00ff00 = 0x12005600 (bitwise and of each bit).
    assert_folds_to(&constant_and_fixture(), "and.b32", 0x1200_5600);
}

#[test]
fn folding_is_noop_on_add_fixture() {
    let folded = compile_fold_on(FIXTURE);
    let plain = compile(Architecture::Gfx1030, FIXTURE).unwrap();
    // add.bin has no pure constant-constant vector op, so folding is a no-op.
    assert_eq!(folded.ir, plain.ir);
    assert_eq!(folded.msl, plain.msl);
    assert_eq!(plain.ir, include_str!("../../../fixtures/golden/add.ir"));
}

#[test]
fn folds_lshl_by_32_as_shift_by_0() {
    // Guide / MSL: count masked with S0[4:0] → 32 & 31 == 0, so 0xf0 << 0 == 0xf0.
    // v_lshlrev_b32 OP=26; fixture stores into v4 (rev encoding).
    let code = binary_const_fixture(26, 0x0000_00f0, 32, true);
    assert_folds_to(&code, "lshl.b32", 0x0000_00f0);
}

#[test]
fn folds_lshl_by_33_as_shift_by_1() {
    // 33 & 31 == 1, so 0xf0 << 1 == 0x1e0.
    let code = binary_const_fixture(26, 0x0000_00f0, 33, true);
    assert_folds_to(&code, "lshl.b32", 0x0000_01e0);
}

#[test]
fn folds_lshr_by_32_as_shift_by_0() {
    // 32 & 31 == 0, so 0xf0 >> 0 == 0xf0. v_lshrrev_b32 OP=22.
    let code = binary_const_fixture(22, 0x0000_00f0, 32, true);
    assert_folds_to(&code, "lshr.b32", 0x0000_00f0);
}

#[test]
fn folds_lshr_by_33_as_shift_by_1() {
    // 33 & 31 == 1, so 0xf0 >> 1 == 0x78.
    let code = binary_const_fixture(22, 0x0000_00f0, 33, true);
    assert_folds_to(&code, "lshr.b32", 0x0000_0078);
}

#[test]
fn folds_ashr_of_negative_value() {
    // (-16 as i32) >> 4 == -1 → 0xffffffff. v_ashrrev_i32 OP=24.
    let code = binary_const_fixture(24, 0xffff_fff0, 4, true);
    assert_folds_to(&code, "ashr.i32", 0xffff_ffff);
}

#[test]
fn folds_min_i32_and_min_u32_differ_on_sign_bit() {
    // min_i32(0x80000000, 1) = min(-2^31, 1) = 0x80000000.
    // min_u32(0x80000000, 1) = min(2^31, 1) = 1.
    // They must differ. OP MinI32=17, MinU32=19.
    assert_folds_to(
        &binary_const_fixture(17, 0x8000_0000, 1, false),
        "min.i32",
        0x8000_0000,
    );
    assert_folds_to(
        &binary_const_fixture(19, 0x8000_0000, 1, false),
        "min.u32",
        1,
    );
}

#[test]
fn folds_mul_hi_i32_i24_with_negative_input() {
    // i24(0x00ffffff) = ((0x00ffffff << 8) >> 8) = -1; i24(2) = 2;
    // (-1 * 2) as i64 >> 32 = -1 → 0xffffffff. v_mul_hi_i32_i24 OP=10.
    let code = binary_const_fixture(10, 0x00ff_ffff, 2, false);
    assert_folds_to(&code, "mulhi24.i32", 0xffff_ffff);
}

#[test]
fn folds_ffbh_i32_on_zero_all_ones_and_one() {
    // RDNA2 / MSL: return 0xffffffff when every bit matches the sign bit (0 or -1);
    // else clz(x ^ (0 - (x >> 31))). For 1: clz(1) = 31. v_ffbh_i32 OP=59.
    assert_folds_to(&unary_const_fixture(59, 0), "ffbh.i32", 0xffff_ffff);
    assert_folds_to(
        &unary_const_fixture(59, 0xffff_ffff),
        "ffbh.i32",
        0xffff_ffff,
    );
    assert_folds_to(&unary_const_fixture(59, 1), "ffbh.i32", 31);
}

#[test]
fn folds_exact_positive_normal_float_add() {
    // 1.0 + 2.0 = 3.0 exactly (positive normals). Bits: 0x3f800000 + 0x40000000
    // → 0x40400000. VOP2 add_f32 OP=3.
    let code = binary_const_fixture(3, 0x3f80_0000, 0x4000_0000, false);
    assert_folds_to(&code, "add.f32", 0x4040_0000);
}

#[test]
fn float_add_from_load_stays_unfolded_with_fold_on() {
    // float-exact.bin feeds add.f32 from a load-derived value (not a pure
    // constant), so the op must remain even with fold_constants. Constant
    // denormal / NaN / negative / inexact float pairs are rejected at verify
    // (see rejects_inexact_float_add_of_constants_at_verify) and never reach
    // the folder as stay-unfolded ops.
    let plain = compile_fold_off(FLOAT);
    let folded = compile_fold_on(FLOAT);
    assert!(
        plain.ir.contains("add.f32"),
        "fold-off must keep add.f32: {}",
        plain.ir
    );
    assert!(
        folded.ir.contains("add.f32"),
        "fold-on must not rewrite load-derived add.f32: {}",
        folded.ir
    );
    assert!(
        folded.ir.contains("mul.f32"),
        "fold-on must not rewrite load-derived mul.f32: {}",
        folded.ir
    );
}

#[test]
fn rejects_inexact_float_add_of_constants_at_verify() {
    // Both operands are positive normals, but 1.0 + 2^-24 is not representable
    // in f32 (ulp of 1.0 is 2^-23), so exact_float_result refuses. Lift/verify
    // rejects before fold_constant_ops runs; the op cannot "stay unfolded".
    // Probed: fold_off and fold_on both return Verify at the add_f32 word.
    let code = binary_const_fixture(3, 0x3f80_0000, 0x3380_0000, false);
    let off = compile_with(
        Architecture::Gfx1030,
        &code,
        CompileOptions {
            fold_constants: false,
        },
    );
    let on = compile_with(
        Architecture::Gfx1030,
        &code,
        CompileOptions {
            fold_constants: true,
        },
    );
    assert!(
        matches!(off, Err(CompileError::Verify { .. })),
        "fold-off must reject inexact constant add.f32 at verify: {off:?}"
    );
    assert!(
        matches!(on, Err(CompileError::Verify { .. })),
        "fold-on must reject inexact constant add.f32 at verify: {on:?}"
    );
    // Folder unchanged: no stay-unfolded path exists for this pair.
}

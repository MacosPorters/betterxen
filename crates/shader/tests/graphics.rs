// SPDX-License-Identifier: GPL-3.0-or-later

use xenora_shader::{Architecture::Gfx1030, CompileError, compile, compile_graphics};

const CONSTANT_VS: &[u8] = include_bytes!("../../../fixtures/rdna2/x9-constant-vs.bin");
const CONSTANT_PS: &[u8] = include_bytes!("../../../fixtures/rdna2/x9-constant-ps.bin");
const VARYING_VS: &[u8] = include_bytes!("../../../fixtures/rdna2/x9-varying-vs.bin");
const VARYING_PS: &[u8] = include_bytes!("../../../fixtures/rdna2/x9-varying-ps.bin");
const TRANSFORM_VS: &[u8] = include_bytes!("../../../fixtures/rdna2/x9-transform-vs.bin");
const END: &[u8] = &[0x00, 0x00, 0x81, 0xbf];

#[test]
fn compiles_image_sample_lz_in_pixel_shader_and_emits_msl_texture_sampling() {
    // VS: simple passthrough that exports pos0 from v[0:3] and param0 from v[4:7]
    let vs_bytes = include_bytes!("../../../fixtures/rdna2/x9-varying-vs.bin");

    // PS that samples a texture at (u, v) and exports color to mrt0:
    // v_interp_p1_f32_e32 v2, v0, attr0.x
    // v_interp_p2_f32_e32 v2, v1, attr0.x
    // v_interp_p1_f32_e32 v3, v0, attr0.y
    // v_interp_p2_f32_e32 v3, v1, attr0.y
    // image_sample_lz v[4:7], v[2:3], s[0:7], s[8:11] dmask:0xf dim:SQ_RSRC_IMG_2D
    // exp mrt0, v4, v5, v6, v7 done vm
    // s_endpgm
    let mut ps_bytes = Vec::new();
    // attr0.x interp p1 & p2
    ps_bytes.extend_from_slice(&[0x00, 0x00, 0x08, 0xc8]); // v_interp_p1_f32_e32 v2, v0, attr0.x
    ps_bytes.extend_from_slice(&[0x01, 0x00, 0x09, 0xc8]); // v_interp_p2_f32_e32 v2, v1, attr0.x
    // attr0.y interp p1 & p2
    ps_bytes.extend_from_slice(&[0x00, 0x01, 0x0c, 0xc8]); // v_interp_p1_f32_e32 v3, v0, attr0.y
    ps_bytes.extend_from_slice(&[0x01, 0x01, 0x0d, 0xc8]); // v_interp_p2_f32_e32 v3, v1, attr0.y
    // image_sample_lz v[4:7], v[2:3], s[0:7], s[8:11] dmask:0xf dim:SQ_RSRC_IMG_2D
    ps_bytes.extend_from_slice(&[0x08, 0x0f, 0x9c, 0xf0, 0x02, 0x04, 0x40, 0x00]);
    // exp mrt0, v4, v5, v6, v7 done vm
    ps_bytes.extend_from_slice(&[0x0f, 0x18, 0x00, 0xf8, 0x04, 0x05, 0x06, 0x07]);
    // s_endpgm
    ps_bytes.extend_from_slice(&[0x00, 0x00, 0x81, 0xbf]);

    let compiled = compile_graphics(Gfx1030, vs_bytes, &ps_bytes)
        .expect("compile graphics with texture sampling");

    assert!(compiled.ir.contains("sample_lz.2d.f32 tex0, samp0"));
    assert!(
        compiled
            .msl
            .contains("texture2d<float, access::sample> tex0 [[texture(0)]]")
    );
    assert!(compiled.msl.contains("sampler samp0 [[sampler(0)]]"));
    assert!(compiled.msl.contains("tex0.sample(samp0, float2("));
}

#[test]
fn links_constant_and_varying_graphics_fixtures() {
    let constant = compile_graphics(Gfx1030, CONSTANT_VS, CONSTANT_PS).unwrap();
    assert!(
        constant
            .disassembly
            .contains("exp mrt0, v8, v9, v9, v8 done vm")
    );
    assert!(constant.ir.contains("export.f32x4 mrt0, t0, t1, t1, t0"));
    assert!(constant.msl.contains("fragment PixelOutput xenora_pixel()"));

    let varying = compile_graphics(Gfx1030, VARYING_VS, VARYING_PS).unwrap();
    assert!(varying.ir.contains("export.f32x4 param0, t4, t5, t6, t7"));
    assert!(varying.ir.contains("export.f32x4 mrt0, t0, t1, t2, t3"));
    assert!(
        varying
            .msl
            .contains("float4 parameter [[user(locn0), center_perspective]]")
    );
    for component in 0..4 {
        assert!(varying.ir.contains(&format!(
            "interpolate.center_perspective.f32 attr0[{component}]"
        )));
    }
}

#[test]
fn preserves_transform_operands_and_keeps_vertex_arithmetic_unfused() {
    let shader = compile_graphics(Gfx1030, TRANSFORM_VS, CONSTANT_PS).unwrap();
    assert!(shader.ir.contains("0010: t10 = mul.f32 t0, t8"));
    assert!(shader.ir.contains("0014: t11 = mul.f32 t1, t8"));
    assert!(shader.ir.contains("0018: t12 = add.f32 t10, t9"));
    assert!(
        shader
            .ir
            .contains("001c: export.f32x4 pos0, t12, t11, t2, t3")
    );
    assert!(
        shader
            .msl
            .contains("    #pragma clang fp contract(off)\n    #pragma clang fp reassociate(off)")
    );
}

#[test]
fn refuses_unlinked_or_missing_exports() {
    for (vertex, pixel, expected) in [
        (VARYING_VS, CONSTANT_PS, "PARAM0"),
        (CONSTANT_VS, VARYING_PS, "PARAM0"),
        (END, CONSTANT_PS, "POS0"),
        (CONSTANT_VS, END, "MRT0"),
    ] {
        let error = compile_graphics(Gfx1030, vertex, pixel).unwrap_err();
        assert!(matches!(error, CompileError::GraphicsLink { .. }));
        assert!(error.to_string().contains(expected), "{error}");
    }
}

#[test]
fn refuses_invalid_export_sources_stage_and_placement() {
    let mut uninitialized = CONSTANT_VS.to_vec();
    uninitialized[4] = 8;
    let mut duplicate = CONSTANT_VS[..8].to_vec();
    duplicate.extend_from_slice(CONSTANT_VS);
    let mut trailing = CONSTANT_VS.to_vec();
    trailing.extend_from_slice(END);
    let mut misplaced_move = CONSTANT_VS[..8].to_vec();
    misplaced_move.extend_from_slice(&CONSTANT_PS[..4]);
    misplaced_move.extend_from_slice(END);
    for (vertex, expected_offset, message) in [
        (
            &uninitialized[..],
            0,
            "source VGPR is uninitialized or reserved",
        ),
        (&duplicate[..], 8, "export target is written more than once"),
        (&trailing[..], 12, "instruction follows program termination"),
        (&misplaced_move[..], 8, "instruction is unsupported"),
        (&CONSTANT_VS[..8], 8, "missing program termination"),
        (CONSTANT_PS, 8, "export target is invalid for this stage"),
    ] {
        let error = compile_graphics(Gfx1030, vertex, CONSTANT_PS).unwrap_err();
        assert_stage_error(&error, "vertex", expected_offset, message);
    }
}

#[test]
fn refuses_unsupported_interpolation_and_graphics_resources() {
    for (index, replacement, expected_offset, expected) in [
        (0, 1, 0, "instruction is unsupported"),
        (1, 4, 0, "instruction is unsupported"),
        (4, 2, 4, "matching second interpolation phase"),
        (5, 1, 4, "matching second interpolation phase"),
        (6, 0x25, 4, "matching second interpolation phase"),
    ] {
        let mut pixel = VARYING_PS.to_vec();
        pixel[index] = replacement;
        let error = compile_graphics(Gfx1030, VARYING_VS, &pixel).unwrap_err();
        assert_stage_error(&error, "pixel", expected_offset, expected);
    }
    let resource_instruction = &include_bytes!("../../../fixtures/rdna2/add.bin")[..8];
    let error = compile_graphics(Gfx1030, resource_instruction, CONSTANT_PS).unwrap_err();
    assert_stage_error(&error, "vertex", 0, "instruction is unsupported");
    let error = compile(Gfx1030, CONSTANT_VS).unwrap_err();
    assert!(matches!(error, CompileError::Verify { offset: 0, .. }));
}

fn assert_stage_error(error: &CompileError, stage: &str, offset: usize, message: &str) {
    let CompileError::GraphicsStage {
        stage: actual_stage,
        error: cause,
    } = error
    else {
        panic!("expected stage-specific error, found {error}");
    };
    assert_eq!(*actual_stage, stage);
    assert!(
        matches!(cause.as_ref(), CompileError::Verify { offset: actual, reason } if *actual == offset && reason.contains(message)),
        "{error}"
    );
}

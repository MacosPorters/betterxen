// SPDX-License-Identifier: GPL-3.0-or-later

use xenora_shader::{Architecture, PIXEL_ENTRY_POINT, VERTEX_ENTRY_POINT, compile_graphics};

use super::context;

const ENTRIES: [&str; 2] = [VERTEX_ENTRY_POINT, PIXEL_ENTRY_POINT];
const POSITIONS: [[f32; 4]; 3] = [
    [-1.0, -1.0, 0.5, 1.0],
    [3.0, -1.0, 0.5, 1.0],
    [-1.0, 3.0, 0.5, 1.0],
];
const CLEAR: [u8; 4] = [19, 43, 93, 251];

#[test]
fn translated_constant_pixel_shader() {
    let compiled = compile_graphics(
        Architecture::Gfx1030,
        include_bytes!("../../../../fixtures/rdna2/x9-constant-vs.bin"),
        include_bytes!("../../../../fixtures/rdna2/x9-constant-ps.bin"),
    )
    .expect("compile constant graphics fixture");
    let gpu = context();
    // This triangle falls entirely past the right clipping plane.
    let offscreen = POSITIONS.map(|[_, y, z, w]| [2.0, y, z, w]);
    for (positions, pixel) in [(&POSITIONS, [255, 0, 0, 255]), (&offscreen, CLEAR)] {
        let actual = gpu
            .render_rgba8(
                &compiled.msl,
                ENTRIES,
                positions,
                &[[0.0; 4]; 3],
                [4, 3],
                CLEAR,
            )
            .expect("draw translated constant pixel shader");
        assert_eq!(actual, pixel.repeat(12));
    }
}

#[test]
fn translated_varying_pixel_shader() {
    let compiled = compile_graphics(
        Architecture::Gfx1030,
        include_bytes!("../../../../fixtures/rdna2/x9-varying-vs.bin"),
        include_bytes!("../../../../fixtures/rdna2/x9-varying-ps.bin"),
    )
    .expect("compile varying graphics fixture");
    // Double the second vertex's homogeneous coordinates to keep the
    // projected triangle fixed but vary its perspective interpolation weight.
    let mut positions = POSITIONS;
    positions[1] = positions[1].map(|component| 2.0 * component);
    let colors = [
        [0.0, 0.0, 0.0, 1.0],
        [128.0 / 255.0, 0.0, 64.0 / 255.0, 1.0],
        [0.0, 128.0 / 255.0, 64.0 / 255.0, 1.0],
    ];
    let gpu = context();
    let actual = gpu
        .render_rgba8(&compiled.msl, ENTRIES, &positions, &colors, [4, 4], CLEAR)
        .expect("draw translated varying pixel shader");

    // The triangle legs span eight pixels from the bottom-left viewport
    // corner. At pixel centers the second and third vertex's affine
    // weights are (2x+1)/16 and (7-2y)/16. Dividing each by its clip W
    // and normalizing gives the perspective weights below. The odd
    // denominator makes nearest-byte conversion unambiguous.
    let expected: Vec<u8> = (0..4)
        .flat_map(|y| {
            (0..4).flat_map(move |x| {
                let u = 2 * x + 1;
                let v = 2 * (7 - 2 * y);
                let sum = 32 - u;
                let byte = |value| ((value + sum / 2) / sum) as u8;
                [byte(128 * u), byte(128 * v), byte(64 * (u + v)), 255]
            })
        })
        .collect();
    assert_eq!(actual, expected);

    let swapped_colors = colors.map(|[r, g, b, a]| [g, r, b, a]);
    let swapped = gpu
        .render_rgba8(
            &compiled.msl,
            ENTRIES,
            &positions,
            &swapped_colors,
            [4, 4],
            CLEAR,
        )
        .expect("draw the same translated shader with red and green vertex data swapped");
    let mut expected_swapped = expected.clone();
    for pixel in expected_swapped.as_chunks_mut::<4>().0 {
        pixel.swap(0, 1);
    }
    assert_ne!(expected_swapped, expected);
    assert_eq!(swapped, expected_swapped);
    println!("checked 16 perspective-interpolated RGBA8 pixels per color input");
}

#[test]
fn translated_render_rejects_out_of_policy_inputs() {
    let gpu = context();
    for extent in [[0, 1], [64, 65], [usize::MAX, 2]] {
        let error = gpu
            .render_rgba8("unused", ENTRIES, &POSITIONS, &[[0.0; 4]; 3], extent, CLEAR)
            .expect_err("invalid dimensions must fail before MSL compilation");
        assert_eq!(error.operation, "render bounds");
    }
    let error = gpu
        .render_rgba8(
            "unused",
            ENTRIES,
            &POSITIONS,
            &[[f32::NAN; 4]; 3],
            [1, 1],
            CLEAR,
        )
        .expect_err("non-finite colors must fail before MSL compilation");
    assert_eq!(error.operation, "render input");
}

#[test]
fn transformed_vertex_shader_draws_three_interior_pixels() {
    let constant_ps = include_bytes!("../../../../fixtures/rdna2/x9-constant-ps.bin");
    let transformed = compile_graphics(
        Architecture::Gfx1030,
        include_bytes!("../../../../fixtures/rdna2/x9-transform-vs.bin"),
        constant_ps,
    )
    .expect("compile position transform");
    let passthrough = compile_graphics(
        Architecture::Gfx1030,
        include_bytes!("../../../../fixtures/rdna2/x9-constant-vs.bin"),
        constant_ps,
    )
    .expect("compile pass-through control");
    let positions = [
        [-1.5, -1.0, 0.5, 1.0],
        [0.5, -1.0, 0.5, 1.0],
        [-1.5, 2.0, 0.5, 1.0],
    ];

    // Scaling x and y by half and biasing x by one quarter places the
    // vertices at (1, 3), (3, 3), and (1, 0) in this 4x4 viewport.
    // A sample center is inside exactly when 1 < x < 3 and
    // 3*(x-1)/2 < y < 3. No edge crosses a sample center.
    let mut expected = [CLEAR; 16];
    for index in [5, 9, 10] {
        expected[index] = [255, 0, 0, 255];
    }
    let gpu = context();
    let colors = [[0.0; 4]; 3];
    let transformed_pixels = gpu
        .render_rgba8(
            &transformed.msl,
            ENTRIES,
            &positions,
            &colors,
            [4, 4],
            CLEAR,
        )
        .expect("draw translated transform");
    assert_eq!(transformed_pixels, expected.as_flattened());

    let passthrough_pixels = gpu
        .render_rgba8(
            &passthrough.msl,
            ENTRIES,
            &positions,
            &colors,
            [4, 4],
            CLEAR,
        )
        .expect("draw unchanged positions through pass-through control");
    // Without the transform, the diagonal extends from (-1, -2) to (3, 4)
    // and the viewport clips both the left and upper edges of the triangle.
    let mut expected_passthrough = [CLEAR; 16];
    for index in [0, 4, 8, 9, 12, 13, 14] {
        expected_passthrough[index] = [255, 0, 0, 255];
    }
    assert_eq!(passthrough_pixels, expected_passthrough.as_flattened());
    println!("checked 3 covered pixels and 13 clear pixels from translated vertex arithmetic");
}

#[test]
fn translated_image_sample_lz_pixel_shader_matches_cpu_reference() {
    let gpu = context();

    // 1. Prepare 2x2 RGBA8 texture with four distinct colors:
    // (0, 0): Red   [255,   0,   0, 255]
    // (1, 0): Green [  0, 255,   0, 255]
    // (0, 1): Blue  [  0,   0, 255, 255]
    // (1, 1): White [255, 255, 255, 255]
    let texels_level0: [u8; 16] = [
        255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255,
    ];
    let tex_desc = xenora_metal::texture_pipeline::Rgba8TextureDesc::new(2, 2, 1)
        .expect("valid 2x2 texture desc");
    let mip0 = xenora_metal::texture_pipeline::Rgba8MipLevel::new_packed(0, 2, 2, &texels_level0);
    let texture =
        xenora_metal::texture_pipeline::MetalRgba8Texture::upload(gpu.device(), tex_desc, &[mip0])
            .expect("upload RGBA8 2x2 texture to Metal");

    // 2. Create native Metal sampler (nearest, clamp-to-edge)
    let sampler_desc = xenora_texture::SamplerDesc::nearest_clamp();
    let sampler = xenora_metal::sampler::MetalSampler::new(gpu.device(), sampler_desc)
        .expect("create native Metal sampler");

    // 3. VS: x9-varying-vs.bin exports position from buffer(0) and UV from buffer(1) via param0
    let vs_bytes = include_bytes!("../../../../fixtures/rdna2/x9-varying-vs.bin");

    // 4. PS: genuine gfx1030 machine-code instructions:
    // v_interp_p1_f32_e32 v2, v0, attr0.x
    // v_interp_p2_f32_e32 v2, v1, attr0.x
    // v_interp_p1_f32_e32 v3, v0, attr0.y
    // v_interp_p2_f32_e32 v3, v1, attr0.y
    // image_sample_lz v[4:7], v[2:3], s[0:7], s[8:11] dmask:0xf dim:SQ_RSRC_IMG_2D
    // exp mrt0, v4, v5, v6, v7 done vm
    // s_endpgm
    let mut ps_bytes = Vec::new();
    ps_bytes.extend_from_slice(&[0x00, 0x00, 0x08, 0xc8]); // v_interp_p1_f32_e32 v2, v0, attr0.x
    ps_bytes.extend_from_slice(&[0x01, 0x00, 0x09, 0xc8]); // v_interp_p2_f32_e32 v2, v1, attr0.x
    ps_bytes.extend_from_slice(&[0x00, 0x01, 0x0c, 0xc8]); // v_interp_p1_f32_e32 v3, v0, attr0.y
    ps_bytes.extend_from_slice(&[0x01, 0x01, 0x0d, 0xc8]); // v_interp_p2_f32_e32 v3, v1, attr0.y
    ps_bytes.extend_from_slice(&[0x08, 0x0f, 0x9c, 0xf0, 0x02, 0x04, 0x40, 0x00]); // image_sample_lz
    ps_bytes.extend_from_slice(&[0x0f, 0x18, 0x00, 0xf8, 0x04, 0x05, 0x06, 0x07]); // exp mrt0
    ps_bytes.extend_from_slice(&[0x00, 0x00, 0x81, 0xbf]); // s_endpgm

    // 5. Compile through Xenora shader compiler pipeline:
    // gfx1030 binary -> xenora-decoder -> xenora-shader IR -> MSL
    let compiled = compile_graphics(Architecture::Gfx1030, vs_bytes, &ps_bytes)
        .expect("compile translated image_sample_lz graphics shader");

    assert!(compiled.ir.contains("sample_lz.2d.f32 tex0, samp0"));
    assert!(
        compiled
            .msl
            .contains("texture2d<float, access::sample> tex0 [[texture(0)]]")
    );
    assert!(compiled.msl.contains("sampler samp0 [[sampler(0)]]"));

    // 6. Set up a fullscreen triangle covering a 2x2 render viewport:
    // In Metal framebuffer layout, row 0 is at the top (NDC y = +1.0) and row 1 is at the bottom (NDC y = -1.0).
    // Mapping UV coordinates so top row samples V in [0.0..0.5] and bottom row samples V in [0.5..1.0]:
    // Vertex 0 (NDC [-1, -1], bottom-left)  -> UV (0.0, 1.0)
    // Vertex 1 (NDC [ 3, -1], bottom-right) -> UV (2.0, 1.0)
    // Vertex 2 (NDC [-1,  3], top-left)     -> UV (0.0, -1.0)
    // In the 2x2 render target:
    // Pixel (x=0, y=0, top-left row 0): center UV = (0.25, 0.25) -> Texel (0, 0) Red   [255,   0,   0, 255]
    // Pixel (x=1, y=0, top-right row 0): center UV = (0.75, 0.25) -> Texel (1, 0) Green [  0, 255,   0, 255]
    // Pixel (x=0, y=1, bottom-left row 1): center UV = (0.25, 0.75) -> Texel (0, 1) Blue  [  0,   0, 255, 255]
    // Pixel (x=1, y=1, bottom-right row 1): center UV = (0.75, 0.75) -> Texel (1, 1) White [255, 255, 255, 255]
    let uv_coords = [
        [0.0_f32, 1.0, 0.0, 1.0],
        [2.0, 1.0, 0.0, 1.0],
        [0.0, -1.0, 0.0, 1.0],
    ];

    let actual_pixels = gpu
        .render_rgba8_sampled(
            &compiled.msl,
            ENTRIES,
            &POSITIONS,
            &uv_coords,
            [2, 2],
            CLEAR,
            Some((&texture, &sampler)),
        )
        .expect("draw translated texture sampling pipeline on Apple GPU");

    // 7. Verify with independent CPU texture sampling reference
    let ref_desc = xenora_ref::texture_sampling::Rgba8TextureDesc::new_packed(2, 2, 1)
        .expect("valid CPU ref texture desc");
    let test_points = [
        [0.25_f32, 0.25], // pixel (0, 0)
        [0.75, 0.25],     // pixel (1, 0)
        [0.25, 0.75],     // pixel (0, 1)
        [0.75, 0.75],     // pixel (1, 1)
    ];

    let mut expected_pixels = Vec::new();
    for [u, v] in test_points {
        let cpu_sample =
            xenora_ref::texture_sampling::sample_rgba8_2d(&texels_level0, &ref_desc, 0, u, v)
                .expect("CPU reference sampling");
        expected_pixels.extend_from_slice(&cpu_sample);
    }

    assert_eq!(
        actual_pixels, expected_pixels,
        "GPU translated sampling output must bitwise match independent CPU sampling oracle"
    );

    println!(
        "device={} verified translated RDNA2 image_sample_lz against independent CPU reference on Apple Silicon GPU",
        gpu.device_name()
    );
}

// SPDX-License-Identifier: GPL-3.0-or-later

use xenora_shader::{Architecture, PIXEL_ENTRY_POINT, VERTEX_ENTRY_POINT, compile_graphics};
use xenora_texture::{Bc1Upload, Bc1UploadDesc, SamplerDesc, decode_bc1};

use super::{context, native};

/// The translated `image_sample_lz` pixel stage samples a native BC1
/// texture bound by `render_rgba8_sampled_bc1`. Each of the 8×8 texture's
/// four blocks selects only its two endpoints, which expand exactly to 0 or
/// 255, so the image is known without a decoder: selector 1 (the second
/// endpoint) where `x == 0 || y == 3` within a block, else selector 0.
#[test]
fn translated_image_sample_lz_reads_bound_bc1_texture() {
    let blocks: [[u8; 8]; 4] = [
        [0x00, 0xF8, 0x1F, 0x00, 0x01, 0x01, 0x01, 0x55], // red, blue
        [0xE0, 0x07, 0x00, 0x00, 0x01, 0x01, 0x01, 0x55], // green, black
        [0xFF, 0xFF, 0x1F, 0xF8, 0x01, 0x01, 0x01, 0x55], // white, magenta
        [0xE0, 0xFF, 0xFF, 0x07, 0x01, 0x01, 0x01, 0x55], // yellow, cyan
    ];
    let endpoints: [[[u8; 4]; 2]; 4] = [
        [[255, 0, 0, 255], [0, 0, 255, 255]],
        [[0, 255, 0, 255], [0, 0, 0, 255]],
        [[255, 255, 255, 255], [255, 0, 255, 255]],
        [[255, 255, 0, 255], [0, 255, 255, 255]],
    ];
    let mut expected = Vec::new();
    for y in 0..8 {
        for x in 0..8 {
            let selector = usize::from(x % 4 == 0 || y % 4 == 3);
            expected.extend(endpoints[(y / 4) * 2 + x / 4][selector]);
        }
    }
    for (index, block) in blocks.into_iter().enumerate() {
        let (bx, by) = (index % 2 * 4, index / 2 * 4);
        for (i, texel) in decode_bc1(block).iter().enumerate() {
            let at = ((by + i / 4) * 8 + bx + i % 4) * 4;
            assert_eq!(texel, &expected[at..at + 4], "decoder, block {index}");
        }
    }

    let ps: Vec<u8> = [
        &[0x00, 0x00, 0x08, 0xc8][..], // v_interp_p1_f32 v2, v0, attr0.x
        &[0x01, 0x00, 0x09, 0xc8],     // v_interp_p2_f32 v2, v1, attr0.x
        &[0x00, 0x01, 0x0c, 0xc8],     // v_interp_p1_f32 v3, v0, attr0.y
        &[0x01, 0x01, 0x0d, 0xc8],     // v_interp_p2_f32 v3, v1, attr0.y
        &[0x08, 0x0f, 0x9c, 0xf0, 0x02, 0x04, 0x40, 0x00], // image_sample_lz
        &[0x0f, 0x18, 0x00, 0xf8, 0x04, 0x05, 0x06, 0x07], // exp mrt0
        &[0x00, 0x00, 0x81, 0xbf],     // s_endpgm
    ]
    .concat();
    let compiled = compile_graphics(
        Architecture::Gfx1030,
        include_bytes!("../../../../fixtures/rdna2/x9-varying-vs.bin"),
        &ps,
    )
    .expect("compile translated image_sample_lz shader");
    let gpu = context();
    let upload = Bc1Upload::new(
        blocks.as_flattened(),
        Bc1UploadDesc {
            width: 8,
            height: 8,
            mips: 1,
            layers: 1,
            format: 1,
            usage: 1,
            row_pitch: 16,
            slice_pitch: 32,
        },
    )
    .expect("valid BC1 upload");
    let sampler = native(xenora_metal::sampler::MetalSampler::new(
        gpu.device(),
        SamplerDesc::nearest_clamp(),
    ));
    // Pixel (x, y) of the 8×8 target samples uv ((x + 0.5) / 8, (y + 0.5) / 8).
    let positions = [
        [-1.0, -1.0, 0.5, 1.0],
        [3.0, -1.0, 0.5, 1.0],
        [-1.0, 3.0, 0.5, 1.0],
    ];
    let uvs = [
        [0.0, 1.0, 0.0, 1.0],
        [2.0, 1.0, 0.0, 1.0],
        [0.0, -1.0, 0.0, 1.0],
    ];
    let pixels = native(gpu.render_rgba8_sampled_bc1(
        &compiled.msl,
        [VERTEX_ENTRY_POINT, PIXEL_ENTRY_POINT],
        &positions,
        &uvs,
        [8, 8],
        [19, 43, 93, 251],
        &upload,
        &sampler,
    ));
    assert_eq!(pixels, expected);
}

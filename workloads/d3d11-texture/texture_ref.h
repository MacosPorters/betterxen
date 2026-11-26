// SPDX-License-Identifier: GPL-3.0-or-later
//
// CPU-side textures and point-sample expectations for the D3D11 workload.
// This header does not include a Direct3D header.

#ifndef TEXTURE_REF_H
#define TEXTURE_REF_H

#include <stddef.h>
#include <stdint.h>

#define TEX_W 8
#define TEX_H 8
#define TEX_PIXELS (TEX_W * TEX_H)
#define TEX_BYTES (TEX_PIXELS * 4)

enum {
    PATTERN_LATTICE = 0,
    PATTERN_DIAGONAL = 1,
    PATTERN_COUNT = 2
};

// One texel of U on an 8-wide texture. Exact in float32.
#define TEXTURE_TEXEL_U (1.0f / (float)TEX_W)

const char *texture_pattern_name(int pattern);

// Write TEX_BYTES of tightly packed RGBA8. Patterns are synthetic.
void texture_fill(int pattern, uint8_t *rgba);

// Expected render-target bytes for a point sample of `texture`.
// u_bias and v_bias are added to the pixel-center UV, matching sample.hlsl.
// The workload passes zeros; the host self-test also checks a one-texel bias.
void texture_expect_point(const uint8_t *texture, float u_bias, float v_bias, uint8_t *expected);

// FNV-1a 64 over the bytes in order.
uint64_t texture_digest(const uint8_t *bytes, size_t count);

#endif

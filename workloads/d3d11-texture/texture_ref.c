// SPDX-License-Identifier: GPL-3.0-or-later
//
// CPU reference for the D3D11 texture workload. It never reads a GPU result.
//
// sample.hlsl covers the viewport with a triangle whose interpolated uv at
// pixel (x, y) is ((x + 0.5) / width, (y + 0.5) / height). Point addressing
// then takes the integer part of uv * size, which is floor for these
// coordinates, and clamp pulls an out-of-range texel back to the edge
// (D3D 11.3 functional spec 7.18.7 and 7.18.9). The centers are half a texel
// from either boundary. An 8-bit UNORM point sample written with blending
// off round-trips that texel (spec 19.1.2), so the expected bytes are the
// addressed texel bytes.

#include "texture_ref.h"

#include <string.h>

const char *texture_pattern_name(int pattern)
{
    if (pattern == PATTERN_LATTICE)
        return "lattice";
    if (pattern == PATTERN_DIAGONAL)
        return "diagonal";
    return "unknown";
}

void texture_fill(int pattern, uint8_t *rgba)
{
    for (int y = 0; y < TEX_H; y++) {
        for (int x = 0; x < TEX_W; x++) {
            uint8_t *pixel = rgba + (y * TEX_W + x) * 4;
            if (pattern == PATTERN_LATTICE) {
                // R follows x, G follows y, B is a checker. Each texel differs.
                pixel[0] = (uint8_t)(x * 32 + 4);
                pixel[1] = (uint8_t)(y * 32 + 4);
                pixel[2] = ((x ^ y) & 1) ? 210 : 40;
                pixel[3] = 255;
            } else {
                // Different from lattice in B for every texel: B here is
                // 255, 223, 191, 159, 127, 95, 63, 31.
                pixel[0] = (uint8_t)((x + y) * 16 + 1);
                pixel[1] = (uint8_t)(x * 16 + y);
                pixel[2] = (uint8_t)(255 - x * 32);
                pixel[3] = 255;
            }
        }
    }
}

static int clamp_texel(int texel, int size)
{
    if (texel < 0)
        return 0;
    if (texel >= size)
        return size - 1;
    return texel;
}

// floor(scaled) for the small coordinates this workload uses, then clamp.
static int point_address(float coord, float bias, int size)
{
    float scaled = (coord + bias) * (float)size;
    int texel = (int)scaled;
    if ((float)texel > scaled)
        texel -= 1;
    return clamp_texel(texel, size);
}

void texture_expect_point(const uint8_t *texture, float u_bias, float v_bias, uint8_t *expected)
{
    for (int y = 0; y < TEX_H; y++) {
        for (int x = 0; x < TEX_W; x++) {
            float u = ((float)x + 0.5f) / (float)TEX_W;
            float v = ((float)y + 0.5f) / (float)TEX_H;
            int tx = point_address(u, u_bias, TEX_W);
            int ty = point_address(v, v_bias, TEX_H);
            const uint8_t *src = texture + (ty * TEX_W + tx) * 4;
            uint8_t *dst = expected + (y * TEX_W + x) * 4;
            memcpy(dst, src, 4);
        }
    }
}

uint64_t texture_digest(const uint8_t *bytes, size_t count)
{
    uint64_t hash = 14695981039346656037ull;
    for (size_t i = 0; i < count; i++) {
        hash ^= bytes[i];
        hash *= 1099511628211ull;
    }
    return hash;
}

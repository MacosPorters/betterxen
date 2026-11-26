// SPDX-License-Identifier: GPL-3.0-or-later
//
// Host check of the CPU reference. It does not create a Direct3D device.

#include "texture_ref.h"

#include <stdio.h>
#include <string.h>

static int failures;

static void check(int condition, const char *what)
{
    if (!condition) {
        fprintf(stderr, "texture-ref: FAIL %s\n", what);
        failures++;
    }
}

static int pixels_equal(const uint8_t *image, int x, int y, const uint8_t *pixel)
{
    return memcmp(image + (y * TEX_W + x) * 4, pixel, 4) == 0;
}

int main(void)
{
    uint8_t lattice[TEX_BYTES];
    uint8_t diagonal[TEX_BYTES];
    uint8_t expected[TEX_BYTES];
    const uint8_t letter_a = 'a';

    // Published FNV-1a 64 offset basis, and the published hash of "a".
    check(texture_digest(&letter_a, 0) == 0xcbf29ce484222325ull, "empty digest");
    check(texture_digest(&letter_a, 1) == 0xaf63dc4c8601ec8cull, "digest of a");

    texture_fill(PATTERN_LATTICE, lattice);
    texture_fill(PATTERN_DIAGONAL, diagonal);

    // lattice: R = x*32+4, G = y*32+4, B = 210 when (x^y) is odd else 40.
    const uint8_t lattice_00[4] = {4, 4, 40, 255};
    const uint8_t lattice_10[4] = {36, 4, 210, 255};
    const uint8_t lattice_77[4] = {228, 228, 40, 255};
    // diagonal: R = (x+y)*16+1, G = x*16+y, B = 255-x*32.
    const uint8_t diagonal_00[4] = {1, 0, 255, 255};
    const uint8_t diagonal_12[4] = {49, 18, 223, 255};
    const uint8_t diagonal_30[4] = {49, 48, 159, 255};
    const uint8_t diagonal_77[4] = {225, 119, 31, 255};
    const uint8_t magenta[4] = {255, 0, 255, 255};

    check(pixels_equal(lattice, 0, 0, lattice_00), "lattice (0,0)");
    check(pixels_equal(lattice, 1, 0, lattice_10), "lattice (1,0)");
    check(pixels_equal(lattice, 7, 7, lattice_77), "lattice (7,7)");
    check(pixels_equal(diagonal, 0, 0, diagonal_00), "diagonal (0,0)");
    check(pixels_equal(diagonal, 1, 2, diagonal_12), "diagonal (1,2)");
    check(pixels_equal(diagonal, 3, 0, diagonal_30), "diagonal (3,0)");
    check(pixels_equal(diagonal, 7, 7, diagonal_77), "diagonal (7,7)");

    for (int pattern = 0; pattern < PATTERN_COUNT; pattern++) {
        const uint8_t *image = pattern == PATTERN_LATTICE ? lattice : diagonal;
        int unique = 1;
        int not_magenta = 1;
        int differs = 1;
        for (int i = 0; i < TEX_PIXELS; i++) {
            for (int j = 0; j < i; j++) {
                if (memcmp(image + i * 4, image + j * 4, 4) == 0)
                    unique = 0;
            }
            if (memcmp(image + i * 4, magenta, 4) == 0)
                not_magenta = 0;
            if (memcmp(lattice + i * 4, diagonal + i * 4, 4) == 0)
                differs = 0;
        }
        check(unique, "texels are unique");
        check(not_magenta, "clear color is not a texel");
        check(differs, "patterns differ at every pixel");

        texture_expect_point(image, 0.0f, 0.0f, expected);
        check(memcmp(expected, image, TEX_BYTES) == 0, "zero bias reads the same texel");

        texture_expect_point(image, TEXTURE_TEXEL_U, 0.0f, expected);
        int shifted = 1;
        for (int y = 0; y < TEX_H; y++) {
            for (int x = 0; x < TEX_W; x++) {
                int src_x = x + 1;
                if (src_x >= TEX_W)
                    src_x = TEX_W - 1;
                if (memcmp(expected + (y * TEX_W + x) * 4, image + (y * TEX_W + src_x) * 4, 4) != 0)
                    shifted = 0;
            }
        }
        check(shifted, "one-texel U bias selects the next column, then clamps");
    }

    texture_expect_point(lattice, TEXTURE_TEXEL_U, 0.0f, expected);
    check(pixels_equal(expected, 0, 0, lattice_10), "biased lattice (0,0) is texel (1,0)");

    if (failures) {
        fprintf(stderr, "texture-ref: FAIL count=%d\n", failures);
        return 1;
    }
    printf("texture-ref: PASS\n");
    return 0;
}

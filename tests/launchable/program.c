// SPDX-License-Identifier: GPL-3.0-or-later
//
// Source-build fixture for issue #313. The frame buffer is guest-local
// until the graphics bridge contract is agreed.

enum { width = 4, height = 3, channels = 4, frame_count = 60 };

static volatile unsigned char frame_pixels[height][width][channels];
static volatile unsigned int frame_number;
#if defined(LAUNCH_EXHAUST)
static volatile unsigned int keep_spinning = 1;
#endif

__declspec(dllimport) void ExitProcess(unsigned int status);
#if defined(LAUNCH_MISSING_IMPORT)
__declspec(dllimport) void LaunchMissingFixtureImport(void);
#endif

void launch_entry(void) {
#if defined(LAUNCH_MISSING_IMPORT)
    LaunchMissingFixtureImport();
#endif
    for (unsigned int frame = 0; frame < frame_count; ++frame) {
        frame_number = frame;
        for (unsigned int y = 0; y < height; ++y) {
            for (unsigned int x = 0; x < width; ++x) {
                const unsigned int textured = frame >= 30;
                frame_pixels[y][x][0] = textured ? 64 * x : 255;
                frame_pixels[y][x][1] = textured ? 96 * y : 128;
                frame_pixels[y][x][2] = textured ? 255 - 64 * x : 0;
                frame_pixels[y][x][3] = 255;
            }
        }
#if defined(LAUNCH_EXHAUST)
        while (keep_spinning) {
        }
#endif
    }

#if defined(LAUNCH_WRONG_EXPECTATION)
    const unsigned int expected = 254;
#else
    const unsigned int expected = 255;
#endif
    if (frame_number != 59 || frame_pixels[0][0][2] != expected) {
        ExitProcess(1);
        return;
    }
    ExitProcess(0x4C4E4348u);
}

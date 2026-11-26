// SPDX-License-Identifier: GPL-3.0-or-later
// Original BetterXen workload (issue #364): a Windows console program that checks itself.
// It computes CRC-32 over "123456789" (the standard check value is 0xCBF43926), runs a
// threaded counter, and prints the architecture it was compiled for. Exit code 0 means every
// check passed; non-zero names the failing check. It touches no game data and no graphics.
#include <stdint.h>
#include <stdio.h>
#include <windows.h>

static uint32_t crc32(const unsigned char *p, size_t n) {
    uint32_t c = 0xFFFFFFFFu;
    for (size_t i = 0; i < n; i++) {
        c ^= p[i];
        for (int k = 0; k < 8; k++) c = (c >> 1) ^ (0xEDB88320u & (0u - (c & 1u)));
    }
    return ~c;
}

static volatile LONG counter;
static DWORD WINAPI bump(LPVOID arg) {
    (void)arg;
    for (int i = 0; i < 100000; i++) InterlockedIncrement(&counter);
    return 0;
}

int main(void) {
#if defined(__x86_64__)
    const char *arch = "x86_64";
#elif defined(__aarch64__)
    const char *arch = "aarch64";
#else
    const char *arch = "other";
#endif
    printf("selfcheck arch=%s pointer_bits=%zu\n", arch, sizeof(void *) * 8);
    uint32_t c = crc32((const unsigned char *)"123456789", 9);
    if (c != 0xCBF43926u) { printf("FAIL crc32 got %08x\n", (unsigned)c); return 1; }
    HANDLE t[4];
    for (int i = 0; i < 4; i++) t[i] = CreateThread(NULL, 0, bump, NULL, 0, NULL);
    for (int i = 0; i < 4; i++) {
        if (!t[i]) { printf("FAIL CreateThread\n"); return 2; }
    }
    WaitForMultipleObjects(4, t, TRUE, 30000);
    if (counter != 400000) { printf("FAIL counter=%ld\n", (long)counter); return 3; }
    printf("PASS\n");
    return 0;
}

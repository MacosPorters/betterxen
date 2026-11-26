// SPDX-License-Identifier: GPL-3.0-or-later
//
// #453: original synthetic kernel for SM 6.6 64-bit atomics on a typed UAV
// (RWTexture2D<uint64_t> over a DXGI_FORMAT_R32G32_UINT texture, 8 x 3 texels).
// 256 threads, 32 per texel: row 0 takes a 64-bit add whose low halves carry
// into the high half, row 1 a 64-bit max decided by the high half. Row 2 gets a
// plain store (no atomic), so "atomics had no effect" is told apart from "the
// kernel never ran".
// Expected values are computed independently on the CPU by the fixture.

RWTexture2D<uint64_t> Img : register(u0);

[numthreads(64, 1, 1)]
void main(uint3 id : SV_DispatchThreadID)
{
    uint px = id.x & 7u;
    uint64_t add = (uint64_t(0x10000000u) << 32) | uint64_t(0xF0000000u + id.x);
    InterlockedAdd(Img[uint2(px, 0)], add);
    uint64_t m = (uint64_t(id.x) << 40) | uint64_t(0xFFFFFFFFu - id.x);
    InterlockedMax(Img[uint2(px, 1)], m);
    Img[uint2(px, 2)] = (uint64_t(0xC0DEu) << 32) | uint64_t(px);
}

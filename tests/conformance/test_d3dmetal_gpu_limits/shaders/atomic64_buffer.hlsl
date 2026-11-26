// SPDX-License-Identifier: GPL-3.0-or-later
//
// #453: control for atomic64_image.hlsl. The same operations on a raw buffer
// (RWByteAddressBuffer::InterlockedAdd64 / InterlockedMax64), so a failure of
// the typed form can be told apart from 64-bit atomics failing everywhere.
// Layout: 8 add slots, 8 max slots, then 8 plain-store marker slots, 8 bytes each.

RWByteAddressBuffer Buf : register(u0);

[numthreads(64, 1, 1)]
void main(uint3 id : SV_DispatchThreadID)
{
    uint px = id.x & 7u;
    uint64_t add = (uint64_t(0x10000000u) << 32) | uint64_t(0xF0000000u + id.x);
    Buf.InterlockedAdd64(px * 8u, add);
    uint64_t m = (uint64_t(id.x) << 40) | uint64_t(0xFFFFFFFFu - id.x);
    Buf.InterlockedMax64(64u + px * 8u, m);
    Buf.Store2(128u + px * 8u, uint2(px, 0xC0DEu));
}

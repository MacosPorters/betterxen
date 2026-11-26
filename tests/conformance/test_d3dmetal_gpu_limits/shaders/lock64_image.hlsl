// SPDX-License-Identifier: GPL-3.0-or-later
//
// #453: original synthetic kernel. Emulates the two 64-bit atomics of
// atomic64_image.hlsl with a per-texel spin lock over 32-bit words, with a
// bounded number of tries (root constant `cap`).
//
//   Data (R32_UINT, 16 x 2): texel (2p, row) = low word, (2p + 1, row) = high word
//   Lock (R32_UINT,  8 x 2): 0 = free, 1 = held
//   Stats (raw buffer): [0] updates dropped after `cap` tries, [4] total tries
//
// The critical section sits inside the retry loop, so a lane that wins the lock
// finishes it in the same iteration. Lanes that lose keep looping with it and
// none waits for another lane to leave a loop. A lane that runs out of tries
// counts a drop instead of spinning on. The fixture checks values only when no
// update was dropped.

globallycoherent RWTexture2D<uint> Data : register(u0);
globallycoherent RWTexture2D<uint> Lock : register(u1);
RWByteAddressBuffer Stats : register(u2);

cbuffer Params : register(b0)
{
    uint cap;
};

uint64_t load64(uint p, uint row)
{
    return (uint64_t(Data[uint2(2u * p + 1u, row)]) << 32) | uint64_t(Data[uint2(2u * p, row)]);
}

void store64(uint p, uint row, uint64_t v)
{
    Data[uint2(2u * p, row)] = uint(v);
    Data[uint2(2u * p + 1u, row)] = uint(v >> 32);
}

void locked_update(uint p, uint row, uint64_t operand)
{
    bool done = false;
    uint tries = 0;
    [allow_uav_condition]
    for (uint t = 0; t < cap && !done; ++t) {
        tries++;
        uint prev;
        InterlockedCompareExchange(Lock[uint2(p, row)], 0u, 1u, prev);
        if (prev == 0u) {
            // Acquire: order the data reads after taking the lock, so they see the previous
            // holder's writes (globallycoherent gives visibility, not ordering).
            DeviceMemoryBarrier();
            uint64_t old = load64(p, row);
            uint64_t now = row == 0u ? old + operand : max(old, operand);
            store64(p, row, now);
            // Release: the data writes become visible before the lock is freed.
            DeviceMemoryBarrier();
            uint ignored;
            InterlockedExchange(Lock[uint2(p, row)], 0u, ignored);
            done = true;
        }
    }
    uint ignored2;
    if (!done) {
        Stats.InterlockedAdd(0, 1u, ignored2);
    }
    Stats.InterlockedAdd(4, tries, ignored2);
}

[numthreads(64, 1, 1)]
void main(uint3 id : SV_DispatchThreadID)
{
    uint px = id.x & 7u;
    locked_update(px, 0u, (uint64_t(0x10000000u) << 32) | uint64_t(0xF0000000u + id.x));
    locked_update(px, 1u, (uint64_t(id.x) << 40) | uint64_t(0xFFFFFFFFu - id.x));
}

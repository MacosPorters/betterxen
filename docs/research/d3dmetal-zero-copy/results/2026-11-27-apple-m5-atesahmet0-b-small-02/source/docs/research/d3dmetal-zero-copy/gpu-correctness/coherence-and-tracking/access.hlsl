// SPDX-License-Identifier: GPL-3.0-or-later
ByteAddressBuffer source : register(t0);
RWByteAddressBuffer target : register(u0);

cbuffer Params : register(b0) {
    uint outputs;
    uint strideWords;
    uint readsPerOutput;
    uint seed;
};

[numthreads(64, 1, 1)]
void main(uint index : SV_DispatchThreadID) {
    if (index >= outputs)
        return;
    uint result = seed + index * 37u;
    // At stride one the wave reads consecutive words for each iteration.
    for (uint n = 0; n < readsPerOutput; ++n)
        result += source.Load(4u * strideWords * (index + n * outputs));
    target.Store(4u * index, result ^ 0xa5c35a3cu);
}

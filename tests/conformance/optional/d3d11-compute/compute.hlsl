// SPDX-License-Identifier: GPL-3.0-or-later
StructuredBuffer<uint> inputWords : register(t0);
RWStructuredBuffer<uint> outputWords : register(u0);

cbuffer Parameters : register(b0)
{
    uint count;
};

[numthreads(64, 1, 1)]
void main(uint3 id : SV_DispatchThreadID)
{
    if (id.x < count)
        outputWords[id.x] = inputWords[id.x] + (id.x & 15u) + 1u;
}

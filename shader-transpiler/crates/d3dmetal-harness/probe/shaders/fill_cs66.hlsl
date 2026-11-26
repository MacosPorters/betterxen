// SPDX-License-Identifier: GPL-3.0-or-later
//
// BetterXen M1-D2 synthetic compute kernel (original source).
//
// Every thread writes one 32-bit word derived from its dispatch index. The
// arm64 harness computes the same function independently in Rust
// (`expected_word` in src/lib.rs); the probe never computes expected values.
//
// Compile (DXC v1.9.2609, see ../README.md for provenance):
//   dxc -T cs_6_6 -E main -Vn g_fill_cs66 -Fh fill_cs66.h fill_cs66.hlsl

RWStructuredBuffer<uint> Out : register(u0);

[numthreads(64, 1, 1)]
void main(uint3 id : SV_DispatchThreadID)
{
    uint v = id.x * 0x9E3779B1u;
    v ^= v >> 15;
    v += 0x01234567u;
    Out[id.x] = v;
}

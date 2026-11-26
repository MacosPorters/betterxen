// SPDX-License-Identifier: GPL-3.0-or-later
//
// #453: original synthetic kernel. Each thread of one 64-thread group records the
// wave width it ran with, its lane index, and the dispatch index of its wave's first
// active lane (which identifies the wave). Built twice (see compile.sh):
//   g_wave_lanes_cs    no WaveSize attribute (control: must run)
//   g_wave64_lanes_cs  BX_WAVE64 defined, [WaveSize(64)] (the case under test)
// The fixture computes the expected layout itself from the reported wave width.

RWStructuredBuffer<uint> Out : register(u0);

#ifdef BX_WAVE64
[WaveSize(64)]
#endif
[numthreads(64, 1, 1)]
void main(uint3 id : SV_DispatchThreadID)
{
    Out[2 * id.x] = (WaveGetLaneCount() << 16) | WaveGetLaneIndex();
    Out[2 * id.x + 1] = WaveReadLaneFirst(id.x);
}

// SPDX-License-Identifier: GPL-3.0-or-later
// Expected (by hand): each launched thread t stores (laneCount << 8) | t, where laneCount is the
// device's wave width (32 per the capability record), so 0x2000, 0x2001, 0x2002, 0x2003.
RWByteAddressBuffer o : register(u0);
[numthreads(4, 1, 1)]
void main(uint t : SV_GroupIndex) { o.Store(t * 4, (WaveGetLaneCount() << 8) | WaveGetLaneIndex()); }

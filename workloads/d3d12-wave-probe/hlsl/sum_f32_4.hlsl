// SPDX-License-Identifier: GPL-3.0-or-later
// Expected (by hand): 4 threads each add 1.25f, every launched lane reads 5.0 = 0x40a00000.
RWByteAddressBuffer o : register(u0);
[numthreads(4, 1, 1)]
void main(uint t : SV_GroupIndex) { o.Store(t * 4, asuint(WaveActiveSum(1.25f))); }

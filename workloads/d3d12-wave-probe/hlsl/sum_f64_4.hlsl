// SPDX-License-Identifier: GPL-3.0-or-later
// Expected (by hand): 4 threads each add the double 1.25; the sum is 5.0, whose bit pattern is
// 0x4014000000000000, so each lane stores low word 0x00000000 and high word 0x40140000.
// The device reports DoublePrecisionFloatShaderOps = 0; this probe shows what actually happens.
RWByteAddressBuffer o : register(u0);
[numthreads(4, 1, 1)]
void main(uint t : SV_GroupIndex) {
    double s = WaveActiveSum(1.25);
    uint lo, hi;
    asuint(s, lo, hi);
    o.Store2(t * 8, uint2(lo, hi));
}

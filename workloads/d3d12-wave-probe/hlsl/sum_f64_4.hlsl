// SPDX-License-Identifier: GPL-3.0-or-later
// A wave sum over a double that is only known at run time (see add_f64_4.hlsl).
// Expected (by hand): 4 threads each add 1.25, sum 5.0 = 0x4014000000000000, so each stores low
// word 0x00000000 and high word 0x40140000.
// The device reports DoublePrecisionFloatShaderOps = 0; this probe shows what actually happens.
RWByteAddressBuffer o : register(u0);
[numthreads(4, 1, 1)]
void main(uint t : SV_GroupIndex) {
    uint z = o.Load(t * 8) ^ 0xDEADBEEF;
    double s = WaveActiveSum(asdouble(z, z ^ 0x3FF40000));
    uint lo, hi;
    asuint(s, lo, hi);
    o.Store2(t * 8, uint2(lo, hi));
}

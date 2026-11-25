// SPDX-License-Identifier: GPL-3.0-or-later
// Control for add_f64_4: the same run-time load, add and store in single precision. If this writes
// and the double version does not, the load/store path is not the cause.
// Expected (by hand): word 0 = 2.0f (0x40000000), word 1 = 0 for each of the 4 threads.
RWByteAddressBuffer o : register(u0);
[numthreads(4, 1, 1)]
void main(uint t : SV_GroupIndex) {
    uint z = o.Load(t * 8) ^ 0xDEADBEEF;
    float s = asfloat(z ^ 0x3FA00000) + asfloat(z ^ 0x3F400000);  // 1.25 + 0.75
    o.Store2(t * 8, uint2(asuint(s), z));
}

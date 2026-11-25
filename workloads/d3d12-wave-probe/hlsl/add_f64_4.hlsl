// SPDX-License-Identifier: GPL-3.0-or-later
// No wave operation: a plain double addition 1.25 + 0.75 = 2.0 (0x4000000000000000), to separate
// "doubles unsupported" from "wave operations on doubles unsupported".
// Expected (by hand): low word 0x00000000, high word 0x40000000.
RWByteAddressBuffer o : register(u0);
[numthreads(4, 1, 1)]
void main(uint t : SV_GroupIndex) {
    double a = 1.25, b = 0.75 + (double)(t - t);
    double s = a + b;
    uint lo, hi;
    asuint(s, lo, hi);
    o.Store2(t * 8, uint2(lo, hi));
}

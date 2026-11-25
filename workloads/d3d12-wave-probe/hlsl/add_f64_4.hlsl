// SPDX-License-Identifier: GPL-3.0-or-later
// No wave operation: a double addition 1.25 + 0.75 = 2.0 (0x4000000000000000) whose operands are
// only known at run time, so the compiler cannot fold it. The host fills the buffer with
// 0xDEADBEEF; `z` is therefore 0 when the shader runs, but not at compile time. The compiled DXIL
// must contain `fadd ... double` (check with `dxc -dumpbin`). Separates "doubles unsupported" from
// "wave operations on doubles unsupported".
// Expected (by hand): low word 0x00000000, high word 0x40000000 for each of the 4 threads.
RWByteAddressBuffer o : register(u0);
[numthreads(4, 1, 1)]
void main(uint t : SV_GroupIndex) {
    uint z = o.Load(t * 8) ^ 0xDEADBEEF;
    double a = asdouble(z, z ^ 0x3FF40000);  // 1.25
    double b = asdouble(z, z ^ 0x3FE80000);  // 0.75
    double s = a + b;
    uint lo, hi;
    asuint(s, lo, hi);
    o.Store2(t * 8, uint2(lo, hi));
}

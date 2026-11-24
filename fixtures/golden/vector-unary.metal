#include <metal_stdlib>
using namespace metal;
kernel void xenora_compute(
    device const uint* input [[buffer(0)]],
    device uint* output [[buffer(1)]],
    uint index [[thread_position_in_grid]]) {
    uint t0 = input[index];
    uint t1 = ~t0;
    uint t2 = ((0xaaaaaaaau >> (index & 31u)) & 1u) != 0u ? t1 : t0;
    uint t3 = ((0xaaaaaaaau >> (index & 31u)) & 1u) != 0u ? t0 : t1;
    uint t4 = reverse_bits(t2);
    uint t5 = t3 == 0u ? 0xffffffffu : clz(t3);
    uint t6 = t2 == 0u ? 0xffffffffu : ctz(t2);
    uint t7 = (t3 == 0u || t3 == 0xffffffffu) ? 0xffffffffu : clz(t3 ^ (0u - (t3 >> 31u)));
    uint t8 = t4 ^ t5;
    uint t9 = t6 ^ t8;
    uint t10 = t7 ^ t9;
    output[index] = t10;
}

#include <metal_stdlib>
using namespace metal;
kernel void xenora_compute(
    device const uint* input [[buffer(0)]],
    device uint* output [[buffer(1)]],
    uint index [[thread_position_in_grid]]) {
    uint t0 = input[index];
    uint t17 = 0x00007d75u;
    uint t18 = 0xc000ffffu;
    uint t19 = t18 ^ t17;
    uint t20 = 0x7fffffffu;
    uint t21 = t20 ^ t19;
    uint t22 = 0xffffff00u;
    uint t23 = t22 ^ t21;
    uint t24 = t0 ^ t23;
    output[index] = t24;
}

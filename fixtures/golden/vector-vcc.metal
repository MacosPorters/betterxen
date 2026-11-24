#include <metal_stdlib>
using namespace metal;
kernel void xenora_compute(
    device const uint* input [[buffer(0)]],
    device uint* output [[buffer(1)]],
    uint index [[thread_position_in_grid]],
    uint grid [[threads_per_grid]]) {
    uint t0 = input[index];
    uint t1 = 0x00000000u;
    uint t2 = 0u;
    for (uint lane = 0u; lane < 32u; ++lane) {
        uint lane_index = (index & ~31u) + lane;
        if (lane_index >= grid) { break; }
        uint l0 = input[lane_index];
        uint pass = (t1 == l0) ? 1u : 0u;
        t2 |= ((0xffffffffu >> lane) & pass) << lane;
    }
    uint t3 = 0xffffffffu;
    uint t4 = ((t2 >> (index & 31u)) & 1u) != 0u ? t0 : t3;
    uint t5 = 0x00000001u;
    uint t6 = uint((ulong(t5) + ulong(t4) + ((t2 >> (index & 31u)) & 1u)) >> 32u);
    uint t7 = 0u;
    for (uint lane = 0u; lane < 32u; ++lane) {
        uint lane_index = (index & ~31u) + lane;
        if (lane_index >= grid) { break; }
        uint l0 = input[lane_index];
        uint l4 = ((t2 >> lane) & 1u) != 0u ? l0 : t3;
        uint l6 = uint((ulong(t5) + ulong(l4) + ((t2 >> lane) & 1u)) >> 32u);
        uint pass = l6;
        t7 |= ((0xffffffffu >> lane) & pass) << lane;
    }
    uint t8 = t5 + t4 + ((t2 >> (index & 31u)) & 1u);
    uint t9 = 0x00000000u;
    uint t10 = uint(ulong(t8) + ((t7 >> (index & 31u)) & 1u) > ulong(t9));
    uint t11 = 0u;
    for (uint lane = 0u; lane < 32u; ++lane) {
        uint lane_index = (index & ~31u) + lane;
        if (lane_index >= grid) { break; }
        uint l0 = input[lane_index];
        uint l4 = ((t2 >> lane) & 1u) != 0u ? l0 : t3;
        uint l8 = t5 + l4 + ((t2 >> lane) & 1u);
        uint l10 = uint(ulong(l8) + ((t7 >> lane) & 1u) > ulong(t9));
        uint pass = l10;
        t11 |= ((0xffffffffu >> lane) & pass) << lane;
    }
    uint t12 = t9 - t8 - ((t7 >> (index & 31u)) & 1u);
    uint t13 = 0xfffffffeu;
    uint t14 = uint(ulong(t13) + ((t11 >> (index & 31u)) & 1u) > ulong(t12));
    uint t15 = 0u;
    for (uint lane = 0u; lane < 32u; ++lane) {
        uint lane_index = (index & ~31u) + lane;
        if (lane_index >= grid) { break; }
        uint l0 = input[lane_index];
        uint l4 = ((t2 >> lane) & 1u) != 0u ? l0 : t3;
        uint l8 = t5 + l4 + ((t2 >> lane) & 1u);
        uint l12 = t9 - l8 - ((t7 >> lane) & 1u);
        uint l14 = uint(ulong(t13) + ((t11 >> lane) & 1u) > ulong(l12));
        uint pass = l14;
        t15 |= ((0xffffffffu >> lane) & pass) << lane;
    }
    uint t16 = t12 - t13 - ((t11 >> (index & 31u)) & 1u);
    output[index] = t15;
}

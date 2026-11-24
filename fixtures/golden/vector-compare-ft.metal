#include <metal_stdlib>
using namespace metal;
kernel void xenora_compute(
    device const uint* input [[buffer(0)]],
    device uint* output [[buffer(1)]],
    uint index [[thread_position_in_grid]],
    uint grid [[threads_per_grid]]) {
    uint t0 = input[index];
    uint t1 = 0x00000000u;
    uint t2 = 0x00000001u;
    uint t3 = 0u;
    for (uint lane = 0u; lane < 32u; ++lane) {
        uint lane_index = (index & ~31u) + lane;
        if (lane_index >= grid) { break; }
        uint pass = (false) ? 1u : 0u;
        t3 |= ((0xffffffffu >> lane) & pass) << lane;
    }
    uint t4 = t1 ^ t3;
    uint t5 = 0x12345678u;
    uint t6 = 0u;
    for (uint lane = 0u; lane < 32u; ++lane) {
        uint lane_index = (index & ~31u) + lane;
        if (lane_index >= grid) { break; }
        uint pass = (true) ? 1u : 0u;
        t6 |= ((0xffffffffu >> lane) & pass) << lane;
    }
    uint t7 = t4 << (t2 & 31u);
    uint t8 = t7 ^ t6;
    uint t9 = 0xfffffff9u;
    uint t10 = 0u;
    for (uint lane = 0u; lane < 32u; ++lane) {
        uint lane_index = (index & ~31u) + lane;
        if (lane_index >= grid) { break; }
        uint pass = (true) ? 1u : 0u;
        t10 |= ((0x0f0f0f0fu >> lane) & pass) << lane;
    }
    uint t11 = t8 << (t2 & 31u);
    uint t12 = t11 ^ t10;
    uint t13 = 0u;
    for (uint lane = 0u; lane < 32u; ++lane) {
        uint lane_index = (index & ~31u) + lane;
        if (lane_index >= grid) { break; }
        uint pass = (true) ? 1u : 0u;
        t13 |= ((0xffffffffu >> lane) & pass) << lane;
    }
    uint t14 = 0xdeadbeefu;
    uint t15 = 0u;
    for (uint lane = 0u; lane < 32u; ++lane) {
        uint lane_index = (index & ~31u) + lane;
        if (lane_index >= grid) { break; }
        uint pass = (false) ? 1u : 0u;
        t15 |= ((0x3c3c3c3cu >> lane) & pass) << lane;
    }
    uint t16 = t12 << (t2 & 31u);
    uint t17 = t16 ^ t15;
    uint t18 = 0u;
    for (uint lane = 0u; lane < 32u; ++lane) {
        uint lane_index = (index & ~31u) + lane;
        if (lane_index >= grid) { break; }
        uint pass = (true) ? 1u : 0u;
        t18 |= ((0xffffffffu >> lane) & pass) << lane;
    }
    uint t20 = t17 ^ t0;
    uint t21 = t18 != 0u ? t20 : t17;
    uint t22 = 0x00000040u;
    uint t23 = 0u;
    for (uint lane = 0u; lane < 32u; ++lane) {
        uint lane_index = (index & ~31u) + lane;
        if (lane_index >= grid) { break; }
        uint pass = (false) ? 1u : 0u;
        t23 |= ((0xffffffffu >> lane) & pass) << lane;
    }
    uint t25 = 0x00000000u;
    uint t26 = t23 != 0u ? t25 : t21;
    output[index] = t26;
}

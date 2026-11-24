// SPDX-License-Identifier: GPL-3.0-or-later
// Original Wave32 example. The compare derives VCC from the input, then each
// carry/borrow feeds the next operation. Before reverse subtract, v3 is
// 0xfffffffe for a zero input and 0xffffffff otherwise, with borrow-in 1.
// Subtracting 0xfffffffe and the borrow-in sets the final VCC bit for zero inputs.
.text
buffer_load_dword v1, v0, s[0:3], 0 idxen
s_waitcnt vmcnt(0)
v_cmp_eq_u32_e32 vcc_lo, 0, v1
v_cndmask_b32_e32 v2, -1, v1, vcc_lo
v_add_co_ci_u32_e32 v2, vcc_lo, 1, v2, vcc_lo
v_sub_co_ci_u32_e32 v3, vcc_lo, 0, v2, vcc_lo
v_subrev_co_ci_u32_e32 v3, vcc_lo, -2, v3, vcc_lo
v_mov_b32_e32 v4, vcc_lo
buffer_store_dword v4, v0, s[4:7], 0 idxen
s_waitcnt_vscnt null, 0
s_endpgm

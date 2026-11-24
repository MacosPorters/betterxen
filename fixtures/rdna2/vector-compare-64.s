// SPDX-License-Identifier: GPL-3.0-or-later
// Encoding inventory for Wave32 64-bit VOPC compares (guide Table 61).
// Operand and execution cases are in tests/validation/tests/vopc_wide.rs.
.text
buffer_load_dword v2, v0, s[0:3], 0 idxen
s_waitcnt vmcnt(0)
v_mov_b32_e32 v3, 0
v_mov_b32_e32 v4, 7
v_mov_b32_e32 v5, 0
v_cmp_f_i64_e32 vcc_lo, v[2:3], v[4:5]
v_cmp_lt_i64_e32 vcc_lo, v[2:3], v[4:5]
v_cmp_eq_i64_e32 vcc_lo, v[2:3], v[4:5]
v_cmp_le_i64_e32 vcc_lo, v[2:3], v[4:5]
v_cmp_gt_i64_e32 vcc_lo, v[2:3], v[4:5]
v_cmp_ne_i64_e32 vcc_lo, v[2:3], v[4:5]
v_cmp_ge_i64_e32 vcc_lo, v[2:3], v[4:5]
v_cmp_t_i64_e32 vcc_lo, v[2:3], v[4:5]
v_cmp_f_u64_e32 vcc_lo, v[2:3], v[4:5]
v_cmp_lt_u64_e32 vcc_lo, v[2:3], v[4:5]
v_cmp_eq_u64_e32 vcc_lo, v[2:3], v[4:5]
v_cmp_le_u64_e32 vcc_lo, v[2:3], v[4:5]
v_cmp_gt_u64_e32 vcc_lo, v[2:3], v[4:5]
v_cmp_ne_u64_e32 vcc_lo, v[2:3], v[4:5]
v_cmp_ge_u64_e32 vcc_lo, v[2:3], v[4:5]
v_cmp_t_u64_e32 vcc_lo, v[2:3], v[4:5]
v_mov_b32_e32 v2, vcc_lo
buffer_store_dword v2, v0, s[4:7], 0 idxen
s_waitcnt_vscnt null, 0
s_endpgm

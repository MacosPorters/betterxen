// SPDX-License-Identifier: GPL-3.0-or-later
// Original Wave32 VOPC vectors for the F and T compares: both signednesses
// with VGPR, SGPR, inline, literal and vcc_lo sources; F clearing a stale
// VCC; T and F under partial EXEC; and runtime s_cbranch_vccz on a T result
// (never taken) and an F result (always taken). Each wave folds its masks
// into v2 as v2 = (v2 << 1) ^ mask. Expected results come from exact
// definitions in tests/validation/known_answers.rs, not from this file.
.text
buffer_load_dword v1, v0, s[0:3], 0 idxen
s_waitcnt vmcnt(0)
s_mov_b32 s8, 0x12345678
v_mov_b32_e32 v2, 0                   // accumulator
v_mov_b32_e32 v5, 1                   // shift count
s_mov_b32 vcc_lo, -1                  // stale nonzero mask
v_cmp_f_i32_e32 vcc_lo, v1, v1        // VCC_LO = 0
v_mov_b32_e32 v4, vcc_lo
v_xor_b32_e32 v2, v2, v4
v_cmp_t_i32_e32 vcc_lo, s8, v1        // every lane of the wave
v_mov_b32_e32 v4, vcc_lo
v_lshlrev_b32_e32 v2, v5, v2
v_xor_b32_e32 v2, v2, v4
s_mov_b32 exec_lo, 0x0f0f0f0f
v_cmp_t_u32_e32 vcc_lo, -7, v1        // only the EXEC lanes
s_mov_b32 exec_lo, -1
v_mov_b32_e32 v4, vcc_lo
v_lshlrev_b32_e32 v2, v5, v2
v_xor_b32_e32 v2, v2, v4
v_cmp_t_i32_e32 vcc_lo, v1, v1        // nonzero again before the F
s_mov_b32 exec_lo, 0x3c3c3c3c
v_cmp_f_u32_e32 vcc_lo, 0xdeadbeef, v1 // 0 even in active lanes
s_mov_b32 exec_lo, -1
v_mov_b32_e32 v4, vcc_lo
v_lshlrev_b32_e32 v2, v5, v2
v_xor_b32_e32 v2, v2, v4
v_cmp_t_u32_e32 vcc_lo, vcc_lo, v1    // reads the zero mask; T anyway
s_cbranch_vccz .Lskip                 // runtime branch, never taken
v_xor_b32_e32 v2, v2, v1
.Lskip:
v_cmp_f_i32_e32 vcc_lo, 64, v1
s_cbranch_vccz .Ldone                 // runtime branch, always taken
v_mov_b32_e32 v2, 0
.Ldone:
buffer_store_dword v2, v0, s[4:7], 0 idxen
s_waitcnt_vscnt null, 0
s_endpgm

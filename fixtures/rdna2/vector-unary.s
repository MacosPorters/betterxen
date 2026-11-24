// SPDX-License-Identifier: GPL-3.0-or-later
// Wave32 VOP1 integer forms (AMD RDNA 2 ISA guide 12.8).
// Exchange the input and its complement in odd lanes, then use both values
// and all three bit scans to form the output.
.text
buffer_load_dword v1, v0, s[0:3], 0 idxen
s_waitcnt vmcnt(0)
v_not_b32_e32 v2, v1
s_mov_b32 exec_lo, 0xaaaaaaaa
v_swap_b32 v1, v2
s_mov_b32 exec_lo, -1
v_bfrev_b32_e32 v3, v1
v_ffbh_u32_e32 v4, v2
v_ffbl_b32_e32 v5, v1
v_ffbh_i32_e32 v6, v2
v_xor_b32_e32 v7, v3, v4
v_xor_b32_e32 v7, v5, v7
v_xor_b32_e32 v7, v6, v7
buffer_store_dword v7, v0, s[4:7], 0 idxen
s_waitcnt_vscnt null, 0
s_endpgm

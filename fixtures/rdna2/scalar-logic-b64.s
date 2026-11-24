// SPDX-License-Identifier: GPL-3.0-or-later
// Fold both SGPR halves of each logical result into a lane-wise XOR key.
// The first AND has a zero low dword and nonzero high dword, so SCC is set.
.text
buffer_load_dword v1, v0, s[0:3], 0 idxen
s_waitcnt vmcnt(0)
s_mov_b32 s8, 0x55555555
s_mov_b32 s9, 0x80000001
s_mov_b32 s10, 0xaaaaaaaa
s_mov_b32 s11, 0x80000000
s_mov_b32 s20, 0
s_and_b64 s[12:13], s[8:9], s[10:11]
s_cbranch_scc0 .Lskip
s_mov_b32 s20, 0x12345678
.Lskip:
s_xor_b32 s20, s20, s13
s_or_b64 s[12:13], s[8:9], 0x80000000
s_xor_b32 s20, s20, s12
s_xor_b32 s20, s20, s13
s_xor_b64 s[12:13], -1, s[10:11]
s_xor_b32 s20, s20, s12
s_xor_b32 s20, s20, s13
s_andn2_b64 s[12:13], s[8:9], s[10:11]
s_xor_b32 s20, s20, s12
s_xor_b32 s20, s20, s13
s_orn2_b64 s[12:13], s[10:11], s[8:9]
s_xor_b32 s20, s20, s12
s_xor_b32 s20, s20, s13
s_nand_b64 s[12:13], s[8:9], s[10:11]
s_xor_b32 s20, s20, s12
s_xor_b32 s20, s20, s13
s_nor_b64 s[12:13], s[8:9], s[10:11]
s_xor_b32 s20, s20, s12
s_xor_b32 s20, s20, s13
s_xnor_b64 s[12:13], s[8:9], s[10:11]
s_xor_b32 s20, s20, s12
s_xor_b32 s20, s20, s13
v_mov_b32_e32 v2, s20
v_xor_b32_e32 v2, v1, v2
buffer_store_dword v2, v0, s[4:7], 0 idxen
s_waitcnt_vscnt null, 0
s_endpgm

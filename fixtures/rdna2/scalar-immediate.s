// SPDX-License-Identifier: GPL-3.0-or-later
// Original SOPK vectors: the sixteen 16-bit-immediate integer forms. s8 =
// 0xffff8000 (s_movk_i32 sign-extends 0x8000), so the twelve s_cmpk compares
// split signed (sign-extended SIMM16) from unsigned (zero-extended); an
// s_cbranch_scc0 skips setting that compare's bit in s20. s_addk_i32 sets SCC
// on signed overflow, s_mulk_i32 leaves SCC alone, and s_cmovk_i32 moves only
// while SCC is set. Bits worked out by hand below.
// Expected: input ^ 0x40008275 (s20 ^ s9 ^ s10 ^ s11).
.text
buffer_load_dword v1, v0, s[0:3], 0 idxen
s_waitcnt vmcnt(0)
s_movk_i32 s8, 0x8000           // s8 = 0xffff8000
s_mov_b32 s20, 0
s_cmpk_eq_i32 s8, 0x8000  // SCC = 1 (-32768 == -32768)
s_cbranch_scc0 .Lskip0
s_or_b32 s20, s20, 0x1
.Lskip0:
s_cmpk_eq_u32 s8, 0x8000  // 0xffff8000 == 0x00008000: SCC = 0
s_cbranch_scc0 .Lskip1
s_or_b32 s20, s20, 0x2
.Lskip1:
s_cmpk_lg_i32 s8, 0x7fff  // -32768 != 32767: SCC = 1
s_cbranch_scc0 .Lskip2
s_or_b32 s20, s20, 0x4
.Lskip2:
s_cmpk_gt_i32 s8, 0xffff  // -32768 > -1 (0xffff sign-extends): SCC = 0
s_cbranch_scc0 .Lskip3
s_or_b32 s20, s20, 0x8
.Lskip3:
s_cmpk_gt_u32 s8, 0xffff  // 0xffff8000 > 0x0000ffff (zero-extends): SCC = 1
s_cbranch_scc0 .Lskip4
s_or_b32 s20, s20, 0x10
.Lskip4:
s_cmpk_ge_i32 s8, 0x8000  // -32768 >= -32768: SCC = 1
s_cbranch_scc0 .Lskip5
s_or_b32 s20, s20, 0x20
.Lskip5:
s_cmpk_lt_i32 s8, 0x1  // -32768 < 1: SCC = 1
s_cbranch_scc0 .Lskip6
s_or_b32 s20, s20, 0x40
.Lskip6:
s_cmpk_lt_u32 s8, 0x1  // 0xffff8000 < 1: SCC = 0
s_cbranch_scc0 .Lskip7
s_or_b32 s20, s20, 0x80
.Lskip7:
s_cmpk_le_i32 s8, 0x8000  // -32768 <= -32768: SCC = 1
s_cbranch_scc0 .Lskip8
s_or_b32 s20, s20, 0x100
.Lskip8:
s_cmpk_le_u32 s8, 0x8000  // 0xffff8000 <= 0x8000: SCC = 0
s_cbranch_scc0 .Lskip9
s_or_b32 s20, s20, 0x200
.Lskip9:
s_cmpk_ge_u32 s8, 0x8000  // 0xffff8000 >= 0x8000: SCC = 1
s_cbranch_scc0 .Lskip10
s_or_b32 s20, s20, 0x400
.Lskip10:
s_cmpk_lg_u32 s8, 0x8000  // 0xffff8000 != 0x8000: SCC = 1
s_cbranch_scc0 .Lskip11
s_or_b32 s20, s20, 0x800
.Lskip11:
s_movk_i32 s9, 0x7fff           // s9 = 32767
s_mov_b32 s10, 0x7fffffff
s_addk_i32 s10, 1               // 0x7fffffff + 1 overflows: s10 = 0x80000000, SCC = 1
s_cbranch_scc0 .Lskip12
s_or_b32 s20, s20, 0x1000
.Lskip12:
s_mulk_i32 s9, 0x8000           // 32767 * -32768: s9 = 0xc0008000, SCC stays 1
s_cbranch_scc0 .Lskip13
s_or_b32 s20, s20, 0x2000
.Lskip13:
s_addk_i32 s10, 0xffff          // 0x80000000 + -1 overflows: s10 = 0x7fffffff, SCC = 1
s_cbranch_scc0 .Lskip14
s_or_b32 s20, s20, 0x4000
.Lskip14:
s_addk_i32 s9, 0x7fff           // 0xc0008000 + 32767 = 0xc000ffff, no overflow: SCC = 0
s_cbranch_scc0 .Lskip15
s_or_b32 s20, s20, 0x8000
.Lskip15:
s_mov_b32 s11, 0x1111
s_cmovk_i32 s11, 0xff00         // SCC = 0: s11 stays 0x1111
s_cmpk_eq_u32 s11, 0x1111       // SCC = 1
s_cmovk_i32 s11, 0xff00         // SCC = 1: s11 = 0xffffff00
v_mov_b32_e32 v2, s20           // s20 = 0x00007d75
v_mov_b32_e32 v3, s9          // s9 = 0xc000ffff
v_xor_b32_e32 v2, v3, v2
v_mov_b32_e32 v3, s10          // s10 = 0x7fffffff
v_xor_b32_e32 v2, v3, v2
v_mov_b32_e32 v3, s11          // s11 = 0xffffff00
v_xor_b32_e32 v2, v3, v2
v_xor_b32_e32 v2, v1, v2
buffer_store_dword v2, v0, s[4:7], 0 idxen
s_waitcnt_vscnt null, 0
s_endpgm

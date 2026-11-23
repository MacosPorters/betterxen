// SPDX-License-Identifier: GPL-3.0-or-later

//! Bounded ARM64 machine instruction emitter for DBT blocks.

/// Standard ARM64 Register indices (0..=31, where 31 is SP or ZR depending on instruction).
pub const X0: u8 = 0;
pub const X1: u8 = 1;
pub const X2: u8 = 2;
pub const X3: u8 = 3;
pub const X4: u8 = 4;
pub const X5: u8 = 5;
pub const X6: u8 = 6;
pub const X7: u8 = 7;
pub const X8: u8 = 8;
pub const X9: u8 = 9;
pub const X10: u8 = 10;
pub const X11: u8 = 11;
pub const X12: u8 = 12;
pub const X13: u8 = 13;
pub const X14: u8 = 14;
pub const X15: u8 = 15;
pub const X16: u8 = 16;
pub const X17: u8 = 17;
pub const X18: u8 = 18;
pub const X19: u8 = 19;
pub const X20: u8 = 20;
pub const X21: u8 = 21;
pub const X22: u8 = 22;
pub const X23: u8 = 23;
pub const X24: u8 = 24;
pub const X25: u8 = 25;
pub const X26: u8 = 26;
pub const X27: u8 = 27;
pub const X28: u8 = 28;
pub const X29: u8 = 29; // Frame Pointer (FP)
pub const X30: u8 = 30; // Link Register (LR)
pub const SP: u8 = 31; // Stack Pointer
pub const XZR: u8 = 31; // Zero Register

/// ARM64 Condition codes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Condition {
    Eq = 0,  // Equal (Z == 1)
    Ne = 1,  // Not equal (Z == 0)
    Cs = 2,  // Carry set / unsigned higher or same (C == 1)
    Cc = 3,  // Carry clear / unsigned lower (C == 0)
    Mi = 4,  // Minus / negative (N == 1)
    Pl = 5,  // Plus / positive or zero (N == 0)
    Vs = 6,  // Overflow (V == 1)
    Vc = 7,  // No overflow (V == 0)
    Hi = 8,  // Unsigned higher (C == 1 && Z == 0)
    Ls = 9,  // Unsigned lower or same (C == 0 || Z == 1)
    Ge = 10, // Signed greater than or equal (N == V)
    Lt = 11, // Signed less than (N != V)
    Gt = 12, // Signed greater than (Z == 0 && N == V)
    Le = 13, // Signed less than or equal (Z == 1 || N != V)
    Al = 14, // Always
}

/// Helper for assembling 32-bit ARM64 instructions.
#[derive(Default)]
pub struct Arm64Assembler {
    code: Vec<u32>,
}

impl Arm64Assembler {
    pub fn new() -> Self {
        Self { code: Vec::new() }
    }

    pub fn instructions(&self) -> &[u32] {
        &self.code
    }

    pub fn current_offset(&self) -> usize {
        self.code.len()
    }

    pub fn emit(&mut self, insn: u32) {
        self.code.push(insn);
    }

    /// RET [X30]
    pub fn ret(&mut self) {
        self.emit(0xd65f03c0);
    }

    /// NOP
    pub fn nop(&mut self) {
        self.emit(0xd503201f);
    }

    /// BR Xn (Branch to register)
    pub fn br(&mut self, xn: u8) {
        self.emit(0xd61f0000 | ((xn as u32 & 0x1f) << 5));
    }

    /// BLR Xn (Branch with link to register)
    pub fn blr(&mut self, xn: u8) {
        self.emit(0xd63f0000 | ((xn as u32 & 0x1f) << 5));
    }

    /// B imm26 (Unconditional branch)
    pub fn b(&mut self, offset_insns: i32) {
        let imm26 = (offset_insns as u32) & 0x03ff_ffff;
        self.emit(0x1400_0000 | imm26);
    }

    /// B.cond imm19 (Conditional branch)
    pub fn b_cond(&mut self, cond: Condition, offset_insns: i32) {
        let imm19 = ((offset_insns as u32) & 0x0007_ffff) << 5;
        self.emit(0x5400_0000 | imm19 | (cond as u32 & 0xf));
    }

    /// Patch previously emitted branch instruction at `index`.
    pub fn patch_b_cond(&mut self, index: usize, cond: Condition, offset_insns: i32) {
        let imm19 = ((offset_insns as u32) & 0x0007_ffff) << 5;
        self.code[index] = 0x5400_0000 | imm19 | (cond as u32 & 0xf);
    }

    /// ADD (extended/shifted register, 64-bit): ADD Xd, Xn, Xm
    pub fn add_reg_64(&mut self, xd: u8, xn: u8, xm: u8) {
        self.emit(
            0x8b000000
                | ((xm as u32 & 0x1f) << 16)
                | ((xn as u32 & 0x1f) << 5)
                | (xd as u32 & 0x1f),
        );
    }

    /// ADD (extended/shifted register, 32-bit): ADD Wd, Wn, Wm
    pub fn add_reg_32(&mut self, wd: u8, wn: u8, wm: u8) {
        self.emit(
            0x0b000000
                | ((wm as u32 & 0x1f) << 16)
                | ((wn as u32 & 0x1f) << 5)
                | (wd as u32 & 0x1f),
        );
    }

    /// ADDS (64-bit, flags updating): ADDS Xd, Xn, Xm
    pub fn adds_reg_64(&mut self, xd: u8, xn: u8, xm: u8) {
        self.emit(
            0xab000000
                | ((xm as u32 & 0x1f) << 16)
                | ((xn as u32 & 0x1f) << 5)
                | (xd as u32 & 0x1f),
        );
    }

    /// ADDS (32-bit, flags updating): ADDS Wd, Wn, Wm
    pub fn adds_reg_32(&mut self, wd: u8, wn: u8, wm: u8) {
        self.emit(
            0x2b000000
                | ((wm as u32 & 0x1f) << 16)
                | ((wn as u32 & 0x1f) << 5)
                | (wd as u32 & 0x1f),
        );
    }

    /// ADD (immediate, 64-bit): ADD Xd, Xn, #imm12
    pub fn add_imm_64(&mut self, xd: u8, xn: u8, imm12: u16) {
        self.emit(
            0x91000000
                | (((imm12 & 0x0fff) as u32) << 10)
                | ((xn as u32 & 0x1f) << 5)
                | (xd as u32 & 0x1f),
        );
    }

    /// SUB (immediate, 64-bit): SUB Xd, Xn, #imm12
    pub fn sub_imm_64(&mut self, xd: u8, xn: u8, imm12: u16) {
        self.emit(
            0xd1000000
                | (((imm12 & 0x0fff) as u32) << 10)
                | ((xn as u32 & 0x1f) << 5)
                | (xd as u32 & 0x1f),
        );
    }

    /// SUB (register, 64-bit): SUB Xd, Xn, Xm
    pub fn sub_reg_64(&mut self, xd: u8, xn: u8, xm: u8) {
        self.emit(
            0xcb000000
                | ((xm as u32 & 0x1f) << 16)
                | ((xn as u32 & 0x1f) << 5)
                | (xd as u32 & 0x1f),
        );
    }

    /// SUB (register, 32-bit): SUB Wd, Wn, Wm
    pub fn sub_reg_32(&mut self, wd: u8, wn: u8, wm: u8) {
        self.emit(
            0x4b000000
                | ((wm as u32 & 0x1f) << 16)
                | ((wn as u32 & 0x1f) << 5)
                | (wd as u32 & 0x1f),
        );
    }

    /// SUBS (64-bit, flags updating): SUBS Xd, Xn, Xm
    pub fn subs_reg_64(&mut self, xd: u8, xn: u8, xm: u8) {
        self.emit(
            0xeb000000
                | ((xm as u32 & 0x1f) << 16)
                | ((xn as u32 & 0x1f) << 5)
                | (xd as u32 & 0x1f),
        );
    }

    /// SUBS (32-bit, flags updating): SUBS Wd, Wn, Wm
    pub fn subs_reg_32(&mut self, wd: u8, wn: u8, wm: u8) {
        self.emit(
            0x6b000000
                | ((wm as u32 & 0x1f) << 16)
                | ((wn as u32 & 0x1f) << 5)
                | (wd as u32 & 0x1f),
        );
    }

    /// CMP (register, 64-bit): SUBS XZR, Xn, Xm
    pub fn cmp_reg_64(&mut self, xn: u8, xm: u8) {
        self.subs_reg_64(XZR, xn, xm);
    }

    /// CMP (register, 32-bit): SUBS WZR, Wn, Wm
    pub fn cmp_reg_32(&mut self, wn: u8, wm: u8) {
        self.subs_reg_32(XZR, wn, wm);
    }

    /// AND (register, 64-bit): AND Xd, Xn, Xm
    pub fn and_reg_64(&mut self, xd: u8, xn: u8, xm: u8) {
        self.emit(
            0x8a000000
                | ((xm as u32 & 0x1f) << 16)
                | ((xn as u32 & 0x1f) << 5)
                | (xd as u32 & 0x1f),
        );
    }

    /// ANDS (register, 64-bit, flags updating / TST): ANDS Xd, Xn, Xm
    pub fn ands_reg_64(&mut self, xd: u8, xn: u8, xm: u8) {
        self.emit(
            0xea000000
                | ((xm as u32 & 0x1f) << 16)
                | ((xn as u32 & 0x1f) << 5)
                | (xd as u32 & 0x1f),
        );
    }

    /// ORR (register, 64-bit): ORR Xd, Xn, Xm
    pub fn orr_reg_64(&mut self, xd: u8, xn: u8, xm: u8) {
        self.emit(
            0xaa000000
                | ((xm as u32 & 0x1f) << 16)
                | ((xn as u32 & 0x1f) << 5)
                | (xd as u32 & 0x1f),
        );
    }

    /// EOR (register, 64-bit, XOR): EOR Xd, Xn, Xm
    pub fn eor_reg_64(&mut self, xd: u8, xn: u8, xm: u8) {
        self.emit(
            0xca000000
                | ((xm as u32 & 0x1f) << 16)
                | ((xn as u32 & 0x1f) << 5)
                | (xd as u32 & 0x1f),
        );
    }

    /// MOV (register, 64-bit): ORR Xd, XZR, Xm
    pub fn mov_reg_64(&mut self, xd: u8, xm: u8) {
        self.orr_reg_64(xd, XZR, xm);
    }

    /// MOV (register, 32-bit zero-extended): ORR Wd, WZR, Wm
    pub fn mov_reg_32(&mut self, wd: u8, wm: u8) {
        self.emit(0x2a000000 | ((wm as u32 & 0x1f) << 16) | (31 << 5) | (wd as u32 & 0x1f));
    }

    /// MOVZ (64-bit with shift): MOVZ Xd, #imm16, LSL #shift
    pub fn movz_64(&mut self, xd: u8, imm16: u16, shift_idx: u8) {
        let hw = (shift_idx & 3) as u32;
        self.emit(0xd2800000 | (hw << 21) | ((imm16 as u32) << 5) | (xd as u32 & 0x1f));
    }

    /// MOVK (64-bit with shift): MOVK Xd, #imm16, LSL #shift
    pub fn movk_64(&mut self, xd: u8, imm16: u16, shift_idx: u8) {
        let hw = (shift_idx & 3) as u32;
        self.emit(0xf2800000 | (hw << 21) | ((imm16 as u32) << 5) | (xd as u32 & 0x1f));
    }

    /// Load a full 64-bit constant into register Xd.
    pub fn load_imm_64(&mut self, xd: u8, val: u64) {
        let w0 = (val & 0xffff) as u16;
        let w1 = ((val >> 16) & 0xffff) as u16;
        let w2 = ((val >> 32) & 0xffff) as u16;
        let w3 = ((val >> 48) & 0xffff) as u16;

        self.movz_64(xd, w0, 0);
        if w1 != 0 || (w2 == 0 && w3 == 0) {
            self.movk_64(xd, w1, 1);
        }
        if w2 != 0 || w3 != 0 {
            self.movk_64(xd, w2, 2);
        }
        if w3 != 0 {
            self.movk_64(xd, w3, 3);
        }
    }

    /// LDR (64-bit, unsigned offset): LDR Xt, [Xn, #imm]
    pub fn ldr_uoff_64(&mut self, xt: u8, xn: u8, offset_bytes: u32) {
        let pimm = (offset_bytes / 8) & 0x0fff;
        self.emit(0xf9400000 | (pimm << 10) | ((xn as u32 & 0x1f) << 5) | (xt as u32 & 0x1f));
    }

    /// STR (64-bit, unsigned offset): STR Xt, [Xn, #imm]
    pub fn str_uoff_64(&mut self, xt: u8, xn: u8, offset_bytes: u32) {
        let pimm = (offset_bytes / 8) & 0x0fff;
        self.emit(0xf9000000 | (pimm << 10) | ((xn as u32 & 0x1f) << 5) | (xt as u32 & 0x1f));
    }

    /// LDR (32-bit, unsigned offset): LDR Wt, [Xn, #imm]
    pub fn ldr_uoff_32(&mut self, wt: u8, xn: u8, offset_bytes: u32) {
        let pimm = (offset_bytes / 4) & 0x0fff;
        self.emit(0xb9400000 | (pimm << 10) | ((xn as u32 & 0x1f) << 5) | (wt as u32 & 0x1f));
    }

    /// STR (32-bit, unsigned offset): STR Wt, [Xn, #imm]
    pub fn str_uoff_32(&mut self, wt: u8, xn: u8, offset_bytes: u32) {
        let pimm = (offset_bytes / 4) & 0x0fff;
        self.emit(0xb9000000 | (pimm << 10) | ((xn as u32 & 0x1f) << 5) | (wt as u32 & 0x1f));
    }

    /// LDR (64-bit register offset): LDR Xt, [Xn, Xm]
    pub fn ldr_reg_64(&mut self, xt: u8, xn: u8, xm: u8) {
        self.emit(
            0xf8606800
                | ((xm as u32 & 0x1f) << 16)
                | ((xn as u32 & 0x1f) << 5)
                | (xt as u32 & 0x1f),
        );
    }

    /// STR (64-bit register offset): STR Xt, [Xn, Xm]
    pub fn str_reg_64(&mut self, xt: u8, xn: u8, xm: u8) {
        self.emit(
            0xf8206800
                | ((xm as u32 & 0x1f) << 16)
                | ((xn as u32 & 0x1f) << 5)
                | (xt as u32 & 0x1f),
        );
    }

    /// STP (64-bit pre-index): STP Xt1, Xt2, [Xn, #imm7]!
    pub fn stp_pre_64(&mut self, xt1: u8, xt2: u8, xn: u8, offset_bytes: i32) {
        let imm7 = ((offset_bytes / 8) as u32) & 0x7f;
        self.emit(
            0xa9a00000
                | 0x001e0000
                | (imm7 << 15)
                | ((xt2 as u32 & 0x1f) << 10)
                | ((xn as u32 & 0x1f) << 5)
                | (xt1 as u32 & 0x1f),
        );
    }

    /// LDP (64-bit post-index): LDP Xt1, Xt2, [Xn], #imm7
    pub fn ldp_post_64(&mut self, xt1: u8, xt2: u8, xn: u8, offset_bytes: i32) {
        let imm7 = ((offset_bytes / 8) as u32) & 0x7f;
        self.emit(
            0xa8c00000
                | (imm7 << 15)
                | ((xt2 as u32 & 0x1f) << 10)
                | ((xn as u32 & 0x1f) << 5)
                | (xt1 as u32 & 0x1f),
        );
    }

    /// MRS Xt, NZCV (Read flags into register)
    pub fn mrs_nzcv(&mut self, xt: u8) {
        self.emit(0xd53b4200 | (xt as u32 & 0x1f));
    }

    /// MSR NZCV, Xt (Write flags from register)
    pub fn msr_nzcv(&mut self, xt: u8) {
        self.emit(0xd51b4200 | (xt as u32 & 0x1f));
    }
}

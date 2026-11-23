// SPDX-License-Identifier: GPL-3.0-or-later

//! Minimal x86-64 guest CPU register state and bounded instruction interpreter.
//!
//! Executes real x86-64 opcodes in software against isolated `GuestMemory`.
//! Never executes guest x86-64 code natively on host ARM64.

use crate::memory::{GuestMemory, MemoryError};
use std::fmt;

/// 64-bit General Purpose Register identifiers in x86-64.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Register {
    Rax = 0,
    Rcx = 1,
    Rdx = 2,
    Rbx = 3,
    Rsp = 4,
    Rbp = 5,
    Rsi = 6,
    Rdi = 7,
    R8 = 8,
    R9 = 9,
    R10 = 10,
    R11 = 11,
    R12 = 12,
    R13 = 13,
    R14 = 14,
    R15 = 15,
}

impl Register {
    pub fn from_index(index: u8) -> Self {
        match index & 0xF {
            0 => Self::Rax,
            1 => Self::Rcx,
            2 => Self::Rdx,
            3 => Self::Rbx,
            4 => Self::Rsp,
            5 => Self::Rbp,
            6 => Self::Rsi,
            7 => Self::Rdi,
            8 => Self::R8,
            9 => Self::R9,
            10 => Self::R10,
            11 => Self::R11,
            12 => Self::R12,
            13 => Self::R13,
            14 => Self::R14,
            15 => Self::R15,
            _ => unreachable!(),
        }
    }
}

/// RFLAGS status bits tracked during guest execution.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RFlags {
    pub cf: bool, // Carry flag
    pub zf: bool, // Zero flag
    pub sf: bool, // Sign flag
    pub of: bool, // Overflow flag
}

/// x86-64 CPU state.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CpuState {
    pub gpr: [u64; 16],
    pub rip: u64,
    pub flags: RFlags,
}

impl CpuState {
    pub fn new(entry_point: u64, stack_pointer: u64) -> Self {
        let mut gpr = [0u64; 16];
        gpr[Register::Rsp as usize] = stack_pointer;
        Self {
            gpr,
            rip: entry_point,
            flags: RFlags::default(),
        }
    }

    pub fn get(&self, reg: Register) -> u64 {
        self.gpr[reg as usize]
    }

    pub fn set(&mut self, reg: Register, val: u64) {
        self.gpr[reg as usize] = val;
    }
}

/// Termination reason for guest execution.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExitReason {
    Halt,
    Syscall(u64),
    StepLimitExceeded,
    MemoryFault(MemoryError),
    IllegalInstruction(u8),
    StackUnderflow,
}

impl fmt::Display for ExitReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Halt => write!(f, "guest executed HLT"),
            Self::Syscall(nr) => write!(f, "guest executed SYSCALL ({nr})"),
            Self::StepLimitExceeded => write!(f, "execution step budget exhausted"),
            Self::MemoryFault(e) => write!(f, "memory fault: {e}"),
            Self::IllegalInstruction(op) => write!(f, "illegal or unsupported opcode: 0x{op:02x}"),
            Self::StackUnderflow => write!(f, "stack underflow"),
        }
    }
}

impl std::error::Error for ExitReason {}

/// Result returned after running guest execution.
#[derive(Debug)]
pub struct ExecutionResult {
    pub instructions_executed: usize,
    pub final_state: CpuState,
    pub exit_reason: ExitReason,
}

/// Bounded software interpreter for x86-64 instructions.
pub struct Interpreter<'a> {
    pub cpu: CpuState,
    pub memory: &'a mut GuestMemory,
    pub max_instructions: usize,
    pub instructions_executed: usize,
}

impl<'a> Interpreter<'a> {
    pub fn new(cpu: CpuState, memory: &'a mut GuestMemory, max_instructions: usize) -> Self {
        Self {
            cpu,
            memory,
            max_instructions,
            instructions_executed: 0,
        }
    }

    /// Run interpreter until halt, error, or instruction budget limit.
    pub fn run(&mut self) -> ExecutionResult {
        while self.instructions_executed < self.max_instructions {
            match self.step() {
                Ok(None) => {
                    self.instructions_executed += 1;
                }
                Ok(Some(reason)) => {
                    self.instructions_executed += 1;
                    return ExecutionResult {
                        instructions_executed: self.instructions_executed,
                        final_state: self.cpu.clone(),
                        exit_reason: reason,
                    };
                }
                Err(reason) => {
                    return ExecutionResult {
                        instructions_executed: self.instructions_executed,
                        final_state: self.cpu.clone(),
                        exit_reason: reason,
                    };
                }
            }
        }

        ExecutionResult {
            instructions_executed: self.instructions_executed,
            final_state: self.cpu.clone(),
            exit_reason: ExitReason::StepLimitExceeded,
        }
    }

    fn fetch_u8(&mut self) -> Result<u8, ExitReason> {
        let b = self
            .memory
            .fetch_instruction_byte(self.cpu.rip)
            .map_err(ExitReason::MemoryFault)?;
        self.cpu.rip += 1;
        Ok(b)
    }

    fn fetch_u32(&mut self) -> Result<u32, ExitReason> {
        let mut bytes = [0u8; 4];
        for b in &mut bytes {
            *b = self.fetch_u8()?;
        }
        Ok(u32::from_le_bytes(bytes))
    }

    fn fetch_u64(&mut self) -> Result<u64, ExitReason> {
        let mut bytes = [0u8; 8];
        for b in &mut bytes {
            *b = self.fetch_u8()?;
        }
        Ok(u64::from_le_bytes(bytes))
    }

    fn fetch_i8(&mut self) -> Result<i8, ExitReason> {
        Ok(self.fetch_u8()? as i8)
    }

    fn fetch_i32(&mut self) -> Result<i32, ExitReason> {
        Ok(self.fetch_u32()? as i32)
    }

    /// Single-step execution. Returns Ok(Some(ExitReason)) on clean exit (HLT/SYSCALL).
    pub fn step(&mut self) -> Result<Option<ExitReason>, ExitReason> {
        let mut rex_w = false;
        let mut rex_r = false;
        let mut rex_b = false;

        let mut byte = self.fetch_u8()?;

        // Handle REX prefix (0x40 - 0x4F)
        if (byte & 0xF0) == 0x40 {
            rex_w = (byte & 0x08) != 0;
            rex_r = (byte & 0x04) != 0;
            rex_b = (byte & 0x01) != 0;
            byte = self.fetch_u8()?;
        }

        match byte {
            // HLT: Halt execution
            0xF4 => Ok(Some(ExitReason::Halt)),

            // SYSCALL: 0x0F 0x05
            0x0F => {
                let second = self.fetch_u8()?;
                if second == 0x05 {
                    let syscall_num = self.cpu.get(Register::Rax);
                    Ok(Some(ExitReason::Syscall(syscall_num)))
                } else {
                    Err(ExitReason::IllegalInstruction(second))
                }
            }

            // MOV r32, imm32 (0xB8 + reg) or MOV r64, imm64 with REX.W
            0xB8..=0xBF => {
                let reg_idx = (byte - 0xB8) + if rex_b { 8 } else { 0 };
                let reg = Register::from_index(reg_idx);
                if rex_w {
                    let imm = self.fetch_u64()?;
                    self.cpu.set(reg, imm);
                } else {
                    let imm = self.fetch_u32()? as u64; // Zero-extended in x86-64
                    self.cpu.set(reg, imm);
                }
                Ok(None)
            }

            // XOR r/m, reg (0x31) or XOR reg, r/m (0x33)
            0x31 | 0x33 => {
                let modrm = self.fetch_u8()?;
                let (reg, rm) = self.decode_modrm_reg_reg(modrm, rex_r, rex_b)?;
                let (src, dst) = if byte == 0x31 { (reg, rm) } else { (rm, reg) };
                let val1 = self.cpu.get(dst);
                let val2 = self.cpu.get(src);
                let res = if rex_w {
                    val1 ^ val2
                } else {
                    (val1 as u32 ^ val2 as u32) as u64
                };
                self.cpu.set(dst, res);
                self.update_flags_logic(res, rex_w);
                Ok(None)
            }

            // ADD r/m, reg (0x01) or ADD reg, r/m (0x03)
            0x01 | 0x03 => {
                let modrm = self.fetch_u8()?;
                let (reg, rm) = self.decode_modrm_reg_reg(modrm, rex_r, rex_b)?;
                let (src, dst) = if byte == 0x01 { (reg, rm) } else { (rm, reg) };
                let val1 = self.cpu.get(dst);
                let val2 = self.cpu.get(src);
                let (res, flags) = if rex_w {
                    add64(val1, val2)
                } else {
                    let (r, f) = add32(val1 as u32, val2 as u32);
                    (r as u64, f)
                };
                self.cpu.set(dst, res);
                self.cpu.flags = flags;
                Ok(None)
            }

            // SUB r/m, reg (0x29) or SUB reg, r/m (0x2B)
            0x29 | 0x2B => {
                let modrm = self.fetch_u8()?;
                let (reg, rm) = self.decode_modrm_reg_reg(modrm, rex_r, rex_b)?;
                let (src, dst) = if byte == 0x29 { (reg, rm) } else { (rm, reg) };
                let val1 = self.cpu.get(dst);
                let val2 = self.cpu.get(src);
                let (res, flags) = if rex_w {
                    sub64(val1, val2)
                } else {
                    let (r, f) = sub32(val1 as u32, val2 as u32);
                    (r as u64, f)
                };
                self.cpu.set(dst, res);
                self.cpu.flags = flags;
                Ok(None)
            }

            // CMP reg, r/m (0x3B) or CMP r/m, reg (0x39)
            0x39 | 0x3B => {
                let modrm = self.fetch_u8()?;
                let (reg, rm) = self.decode_modrm_reg_reg(modrm, rex_r, rex_b)?;
                let (v1, v2) = if byte == 0x39 {
                    (self.cpu.get(rm), self.cpu.get(reg))
                } else {
                    (self.cpu.get(reg), self.cpu.get(rm))
                };
                let (_, flags) = if rex_w {
                    sub64(v1, v2)
                } else {
                    let (r, f) = sub32(v1 as u32, v2 as u32);
                    (r as u64, f)
                };
                self.cpu.flags = flags;
                Ok(None)
            }

            // Immediate group 1: 0x83 /op reg, imm8
            0x83 => {
                let modrm = self.fetch_u8()?;
                let op = (modrm >> 3) & 7;
                let (reg, rm) = self.decode_modrm_reg_reg(modrm, rex_r, rex_b)?;
                let imm8 = self.fetch_i8()? as i64;
                let imm = imm8 as u64;

                let val = self.cpu.get(rm);
                match op {
                    0 => {
                        // ADD
                        let (res, flags) = if rex_w {
                            add64(val, imm)
                        } else {
                            let (r, f) = add32(val as u32, imm as u32);
                            (r as u64, f)
                        };
                        self.cpu.set(rm, res);
                        self.cpu.flags = flags;
                    }
                    5 => {
                        // SUB
                        let (res, flags) = if rex_w {
                            sub64(val, imm)
                        } else {
                            let (r, f) = sub32(val as u32, imm as u32);
                            (r as u64, f)
                        };
                        self.cpu.set(rm, res);
                        self.cpu.flags = flags;
                    }
                    7 => {
                        // CMP
                        let (_, flags) = if rex_w {
                            sub64(val, imm)
                        } else {
                            let (r, f) = sub32(val as u32, imm as u32);
                            (r as u64, f)
                        };
                        self.cpu.flags = flags;
                    }
                    _ => return Err(ExitReason::IllegalInstruction(byte)),
                }
                let _ = reg;
                Ok(None)
            }

            // MOV r/m, reg (0x89) or MOV reg, r/m (0x8B)
            0x89 | 0x8B => {
                let modrm = self.fetch_u8()?;
                self.execute_mov_rm(modrm, rex_w, rex_r, rex_b, byte == 0x8B)
            }

            // JMP rel8 (0xEB)
            0xEB => {
                let offset = self.fetch_i8()? as i64;
                self.cpu.rip = (self.cpu.rip as i64 + offset) as u64;
                Ok(None)
            }

            // JMP rel32 (0xE9)
            0xE9 => {
                let offset = self.fetch_i32()? as i64;
                self.cpu.rip = (self.cpu.rip as i64 + offset) as u64;
                Ok(None)
            }

            // JZ/JE rel8 (0x74)
            0x74 => {
                let offset = self.fetch_i8()? as i64;
                if self.cpu.flags.zf {
                    self.cpu.rip = (self.cpu.rip as i64 + offset) as u64;
                }
                Ok(None)
            }

            // JNZ/JNE rel8 (0x75)
            0x75 => {
                let offset = self.fetch_i8()? as i64;
                if !self.cpu.flags.zf {
                    self.cpu.rip = (self.cpu.rip as i64 + offset) as u64;
                }
                Ok(None)
            }

            // CALL rel32 (0xE8)
            0xE8 => {
                let offset = self.fetch_i32()? as i64;
                let target = (self.cpu.rip as i64 + offset) as u64;
                let ret_addr = self.cpu.rip;
                self.push_u64(ret_addr)?;
                self.cpu.rip = target;
                Ok(None)
            }

            // RET (0xC3)
            0xC3 => {
                let ret_addr = self.pop_u64()?;
                self.cpu.rip = ret_addr;
                Ok(None)
            }

            // PUSH reg (0x50..=0x57)
            0x50..=0x57 => {
                let reg_idx = (byte - 0x50) + if rex_b { 8 } else { 0 };
                let reg = Register::from_index(reg_idx);
                let val = self.cpu.get(reg);
                self.push_u64(val)?;
                Ok(None)
            }

            // POP reg (0x58..=0x5F)
            0x58..=0x5F => {
                let reg_idx = (byte - 0x58) + if rex_b { 8 } else { 0 };
                let reg = Register::from_index(reg_idx);
                let val = self.pop_u64()?;
                self.cpu.set(reg, val);
                Ok(None)
            }

            _ => Err(ExitReason::IllegalInstruction(byte)),
        }
    }

    fn push_u64(&mut self, val: u64) -> Result<(), ExitReason> {
        let sp = self.cpu.get(Register::Rsp);
        let new_sp = sp.checked_sub(8).ok_or(ExitReason::StackUnderflow)?;
        self.memory
            .write_u64(new_sp, val)
            .map_err(ExitReason::MemoryFault)?;
        self.cpu.set(Register::Rsp, new_sp);
        Ok(())
    }

    fn pop_u64(&mut self) -> Result<u64, ExitReason> {
        let sp = self.cpu.get(Register::Rsp);
        let val = self.memory.read_u64(sp).map_err(ExitReason::MemoryFault)?;
        self.cpu.set(Register::Rsp, sp + 8);
        Ok(val)
    }

    fn decode_modrm_reg_reg(
        &self,
        modrm: u8,
        rex_r: bool,
        rex_b: bool,
    ) -> Result<(Register, Register), ExitReason> {
        let mode = (modrm >> 6) & 3;
        if mode != 3 {
            return Err(ExitReason::IllegalInstruction(modrm));
        }
        let reg_idx = ((modrm >> 3) & 7) + if rex_r { 8 } else { 0 };
        let rm_idx = (modrm & 7) + if rex_b { 8 } else { 0 };
        Ok((Register::from_index(reg_idx), Register::from_index(rm_idx)))
    }

    fn execute_mov_rm(
        &mut self,
        modrm: u8,
        rex_w: bool,
        rex_r: bool,
        rex_b: bool,
        is_load: bool,
    ) -> Result<Option<ExitReason>, ExitReason> {
        let mode = (modrm >> 6) & 3;
        let reg_idx = ((modrm >> 3) & 7) + if rex_r { 8 } else { 0 };
        let rm_idx = (modrm & 7) + if rex_b { 8 } else { 0 };
        let reg = Register::from_index(reg_idx);

        if mode == 3 {
            // Register-to-register move
            let rm = Register::from_index(rm_idx);
            if is_load {
                let val = self.cpu.get(rm);
                self.cpu
                    .set(reg, if rex_w { val } else { val as u32 as u64 });
            } else {
                let val = self.cpu.get(reg);
                self.cpu
                    .set(rm, if rex_w { val } else { val as u32 as u64 });
            }
            return Ok(None);
        }

        // Memory operand [rm + disp]
        let base_reg = Register::from_index(rm_idx);
        let base_val = self.cpu.get(base_reg);

        let disp: i64 = match mode {
            0 => {
                if (rm_idx & 7) == 5 {
                    // RIP-relative displacement disp32
                    self.fetch_i32()? as i64
                } else {
                    0
                }
            }
            1 => self.fetch_i8()? as i64,
            2 => self.fetch_i32()? as i64,
            _ => unreachable!(),
        };

        let effective_addr = if mode == 0 && (rm_idx & 7) == 5 {
            (self.cpu.rip as i64 + disp) as u64
        } else {
            (base_val as i64 + disp) as u64
        };

        if is_load {
            // Load from memory to reg
            if rex_w {
                let val = self
                    .memory
                    .read_u64(effective_addr)
                    .map_err(ExitReason::MemoryFault)?;
                self.cpu.set(reg, val);
            } else {
                let val = self
                    .memory
                    .read_u32(effective_addr)
                    .map_err(ExitReason::MemoryFault)?;
                self.cpu.set(reg, val as u64);
            }
        } else {
            // Store from reg to memory
            let val = self.cpu.get(reg);
            if rex_w {
                self.memory
                    .write_u64(effective_addr, val)
                    .map_err(ExitReason::MemoryFault)?;
            } else {
                self.memory
                    .write_u32(effective_addr, val as u32)
                    .map_err(ExitReason::MemoryFault)?;
            }
        }

        Ok(None)
    }

    fn update_flags_logic(&mut self, val: u64, is_64: bool) {
        self.cpu.flags.cf = false;
        self.cpu.flags.of = false;
        if is_64 {
            self.cpu.flags.zf = val == 0;
            self.cpu.flags.sf = (val as i64) < 0;
        } else {
            let val32 = val as u32;
            self.cpu.flags.zf = val32 == 0;
            self.cpu.flags.sf = (val32 as i32) < 0;
        }
    }
}

fn add64(a: u64, b: u64) -> (u64, RFlags) {
    let (res, cf) = a.overflowing_add(b);
    let of = ((a ^ res) & (b ^ res) & 0x8000_0000_0000_0000) != 0;
    let sf = (res as i64) < 0;
    let zf = res == 0;
    (res, RFlags { cf, zf, sf, of })
}

fn add32(a: u32, b: u32) -> (u32, RFlags) {
    let (res, cf) = a.overflowing_add(b);
    let of = ((a ^ res) & (b ^ res) & 0x8000_0000) != 0;
    let sf = (res as i32) < 0;
    let zf = res == 0;
    (res, RFlags { cf, zf, sf, of })
}

fn sub64(a: u64, b: u64) -> (u64, RFlags) {
    let (res, cf) = a.overflowing_sub(b);
    let of = ((a ^ b) & (a ^ res) & 0x8000_0000_0000_0000) != 0;
    let sf = (res as i64) < 0;
    let zf = res == 0;
    (res, RFlags { cf, zf, sf, of })
}

fn sub32(a: u32, b: u32) -> (u32, RFlags) {
    let (res, cf) = a.overflowing_sub(b);
    let of = ((a ^ b) & (a ^ res) & 0x8000_0000) != 0;
    let sf = (res as i32) < 0;
    let zf = res == 0;
    (res, RFlags { cf, zf, sf, of })
}

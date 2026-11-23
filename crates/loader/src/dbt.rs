// SPDX-License-Identifier: GPL-3.0-or-later

//! Bounded x86-64 to ARM64 Dynamic Binary Translation (DBT) engine for Xenora.
//!
//! Compiles decoded x86-64 basic blocks to native Apple Silicon ARM64 machine code,
//! executes them in a W^X code cache, and manages host-service dispatch and fallbacks.

use crate::arm64::{Arm64Assembler, SP, X0, X1, X2, X4, X9, X10, X19, X20, X29, X30};
use crate::cpu::{CpuState, ExecutionResult, ExitReason, Register};
use crate::jit::{JitError, JitMemory};
use crate::memory::GuestMemory;
use std::collections::HashMap;

/// Registered guest host-service handler function.
pub type HostServiceFn =
    Box<dyn Fn(&mut CpuState, &mut GuestMemory) -> Result<(), ExitReason> + Send + Sync>;

/// Diagnostic execution counters for DBT runtime profiling.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DbtMetrics {
    pub blocks_compiled: usize,
    pub native_bytes_emitted: usize,
    pub dbt_dispatches: usize,
    pub instructions_executed: usize,
    pub interpreter_fallbacks: usize,
    pub service_calls: usize,
}

/// Dynamic Binary Translator and native execution runtime.
pub struct DbtEngine {
    jit: JitMemory,
    block_cache: HashMap<u64, *const u8>,
    services: HashMap<u64, HostServiceFn>,
    pub metrics: DbtMetrics,
}

impl DbtEngine {
    /// Create a new DBT engine with specified JIT cache capacity (e.g. 1MB).
    pub fn new(capacity: usize) -> Result<Self, JitError> {
        Ok(Self {
            jit: JitMemory::new(capacity)?,
            block_cache: HashMap::new(),
            services: HashMap::new(),
            metrics: DbtMetrics::default(),
        })
    }

    /// Register a host service at a synthetic guest virtual address.
    pub fn register_service<F>(&mut self, address: u64, handler: F)
    where
        F: Fn(&mut CpuState, &mut GuestMemory) -> Result<(), ExitReason> + Send + Sync + 'static,
    {
        self.services.insert(address, Box::new(handler));
    }

    /// Execute guest code starting at CPU RIP until HLT, budget exhaustion, or fault.
    pub fn run(
        &mut self,
        cpu: &mut CpuState,
        memory: &mut GuestMemory,
        max_instructions: usize,
    ) -> ExecutionResult {
        let mut instructions_count = 0;

        while instructions_count < max_instructions {
            // Check if current RIP is a registered host service
            if let Some(service) = self.services.get(&cpu.rip) {
                self.metrics.service_calls += 1;
                if let Err(reason) = service(cpu, memory) {
                    return ExecutionResult {
                        instructions_executed: instructions_count,
                        final_state: cpu.clone(),
                        exit_reason: reason,
                    };
                }
                instructions_count += 1;
                continue;
            }

            // Look up or translate block at cpu.rip
            let block_fn_ptr = match self.get_or_compile_block(cpu.rip, memory) {
                Ok(ptr) => ptr,
                Err(reason) => {
                    return ExecutionResult {
                        instructions_executed: instructions_count,
                        final_state: cpu.clone(),
                        exit_reason: reason,
                    };
                }
            };

            // Execute native ARM64 translated block!
            // Signature: extern "C" fn(cpu_state_ptr: *mut u64, guest_memory_ctx: *mut (GuestMemory, ExitReason)) -> u32
            // Returns: number of guest instructions executed in this block
            type BlockFn = unsafe extern "C" fn(*mut u64, *mut BlockContext) -> u32;
            let block_fn: BlockFn = unsafe { std::mem::transmute(block_fn_ptr) };

            let mut ctx = BlockContext {
                memory_ptr: memory as *mut GuestMemory,
                exit_reason: None,
            };

            self.metrics.dbt_dispatches += 1;
            let insns_in_block = unsafe { block_fn(cpu.gpr.as_mut_ptr(), &mut ctx) };
            instructions_count += insns_in_block as usize;
            self.metrics.instructions_executed += insns_in_block as usize;

            if let Some(exit) = ctx.exit_reason {
                return ExecutionResult {
                    instructions_executed: instructions_count,
                    final_state: cpu.clone(),
                    exit_reason: exit,
                };
            }
        }

        ExecutionResult {
            instructions_executed: instructions_count,
            final_state: cpu.clone(),
            exit_reason: ExitReason::StepLimitExceeded,
        }
    }

    fn get_or_compile_block(
        &mut self,
        rip: u64,
        memory: &GuestMemory,
    ) -> Result<*const u8, ExitReason> {
        if let Some(&fn_ptr) = self.block_cache.get(&rip) {
            return Ok(fn_ptr);
        }

        let compiled_ptr = self.compile_block(rip, memory)?;
        self.block_cache.insert(rip, compiled_ptr);
        Ok(compiled_ptr)
    }

    fn compile_block(
        &mut self,
        start_rip: u64,
        memory: &GuestMemory,
    ) -> Result<*const u8, ExitReason> {
        let mut asm = Arm64Assembler::new();
        let mut cur_rip = start_rip;
        let mut insn_count = 0;

        // Function Prologue:
        // Inputs: X0 = &mut CpuState.gpr[0], X1 = &mut BlockContext
        // We preserve host callee-saved registers X19, X20, X21, X29 (FP), X30 (LR)
        asm.stp_pre_64(X29, X30, SP, -32);
        asm.str_uoff_64(X19, SP, 16);
        asm.str_uoff_64(X20, SP, 24);

        // Keep CPU GPR pointer in callee-saved X19
        asm.mov_reg_64(X19, X0);
        // Keep BlockContext pointer in callee-saved X20
        asm.mov_reg_64(X20, X1);

        // We decode and lower up to 16 instructions or until a basic block boundary (branch/call/ret/hlt)
        loop {
            let insn_rip = cur_rip;
            let b0 = memory
                .fetch_instruction_byte(cur_rip)
                .map_err(ExitReason::MemoryFault)?;
            cur_rip += 1;

            let mut rex_w = false;
            let mut rex_r = false;
            let mut rex_b = false;
            let mut op = b0;

            if (op & 0xf0) == 0x40 {
                rex_w = (op & 0x08) != 0;
                rex_r = (op & 0x04) != 0;
                rex_b = (op & 0x01) != 0;
                op = memory
                    .fetch_instruction_byte(cur_rip)
                    .map_err(ExitReason::MemoryFault)?;
                cur_rip += 1;
            }

            match op {
                // HLT (0xF4): Terminate block with ExitReason::Halt
                0xF4 => {
                    insn_count += 1;
                    self.emit_set_exit_reason(&mut asm, 1, cur_rip); // 1 = Halt
                    break;
                }

                // MOV r32, imm32 (0xB8 + reg) or MOV r64, imm64 (with REX.W)
                0xB8..=0xBF => {
                    insn_count += 1;
                    let reg_idx = (op - 0xB8) + if rex_b { 8 } else { 0 };
                    let imm = if rex_w {
                        let mut buf = [0u8; 8];
                        for b in &mut buf {
                            *b = memory
                                .fetch_instruction_byte(cur_rip)
                                .map_err(ExitReason::MemoryFault)?;
                            cur_rip += 1;
                        }
                        u64::from_le_bytes(buf)
                    } else {
                        let mut buf = [0u8; 4];
                        for b in &mut buf {
                            *b = memory
                                .fetch_instruction_byte(cur_rip)
                                .map_err(ExitReason::MemoryFault)?;
                            cur_rip += 1;
                        }
                        u32::from_le_bytes(buf) as u64
                    };

                    asm.load_imm_64(X9, imm);
                    asm.str_uoff_64(X9, X19, (reg_idx as u32) * 8);
                }

                // ADD r/m, reg (0x01) or ADD reg, r/m (0x03)
                0x01 | 0x03 => {
                    insn_count += 1;
                    let modrm = memory
                        .fetch_instruction_byte(cur_rip)
                        .map_err(ExitReason::MemoryFault)?;
                    cur_rip += 1;
                    let mode = (modrm >> 6) & 3;
                    if mode != 3 {
                        return Err(ExitReason::IllegalInstruction(op));
                    }
                    let reg_idx = ((modrm >> 3) & 7) + if rex_r { 8 } else { 0 };
                    let rm_idx = (modrm & 7) + if rex_b { 8 } else { 0 };
                    let (src, dst) = if op == 0x01 {
                        (reg_idx, rm_idx)
                    } else {
                        (rm_idx, reg_idx)
                    };

                    asm.ldr_uoff_64(X9, X19, (dst as u32) * 8);
                    asm.ldr_uoff_64(X10, X19, (src as u32) * 8);
                    if rex_w {
                        asm.add_reg_64(X9, X9, X10);
                    } else {
                        asm.add_reg_32(X9, X9, X10); // W9 = W9 + W10 (automatically zero-extended to X9 in AArch64)
                    }
                    asm.str_uoff_64(X9, X19, (dst as u32) * 8);
                }

                // SUB r/m, reg (0x29) or SUB reg, r/m (0x2B)
                0x29 | 0x2B => {
                    insn_count += 1;
                    let modrm = memory
                        .fetch_instruction_byte(cur_rip)
                        .map_err(ExitReason::MemoryFault)?;
                    cur_rip += 1;
                    let mode = (modrm >> 6) & 3;
                    if mode != 3 {
                        return Err(ExitReason::IllegalInstruction(op));
                    }
                    let reg_idx = ((modrm >> 3) & 7) + if rex_r { 8 } else { 0 };
                    let rm_idx = (modrm & 7) + if rex_b { 8 } else { 0 };
                    let (src, dst) = if op == 0x29 {
                        (reg_idx, rm_idx)
                    } else {
                        (rm_idx, reg_idx)
                    };

                    asm.ldr_uoff_64(X9, X19, (dst as u32) * 8);
                    asm.ldr_uoff_64(X10, X19, (src as u32) * 8);
                    if rex_w {
                        asm.sub_reg_64(X9, X9, X10);
                    } else {
                        asm.sub_reg_32(X9, X9, X10);
                    }
                    asm.str_uoff_64(X9, X19, (dst as u32) * 8);
                }

                // XOR r/m, reg (0x31) or XOR reg, r/m (0x33)
                0x31 | 0x33 => {
                    insn_count += 1;
                    let modrm = memory
                        .fetch_instruction_byte(cur_rip)
                        .map_err(ExitReason::MemoryFault)?;
                    cur_rip += 1;
                    let mode = (modrm >> 6) & 3;
                    if mode != 3 {
                        return Err(ExitReason::IllegalInstruction(op));
                    }
                    let reg_idx = ((modrm >> 3) & 7) + if rex_r { 8 } else { 0 };
                    let rm_idx = (modrm & 7) + if rex_b { 8 } else { 0 };
                    let (src, dst) = if op == 0x31 {
                        (reg_idx, rm_idx)
                    } else {
                        (rm_idx, reg_idx)
                    };

                    asm.ldr_uoff_64(X9, X19, (dst as u32) * 8);
                    asm.ldr_uoff_64(X10, X19, (src as u32) * 8);
                    asm.eor_reg_64(X9, X9, X10);
                    if !rex_w {
                        asm.mov_reg_32(X9, X9); // zero extend
                    }
                    asm.str_uoff_64(X9, X19, (dst as u32) * 8);
                }

                // Imm8 group (0x83): ADD/SUB/CMP reg, imm8
                0x83 => {
                    insn_count += 1;
                    let modrm = memory
                        .fetch_instruction_byte(cur_rip)
                        .map_err(ExitReason::MemoryFault)?;
                    cur_rip += 1;
                    let mode = (modrm >> 6) & 3;
                    if mode != 3 {
                        return Err(ExitReason::IllegalInstruction(op));
                    }
                    let sub_op = (modrm >> 3) & 7;
                    let rm_idx = (modrm & 7) + if rex_b { 8 } else { 0 };
                    let imm8 = memory
                        .fetch_instruction_byte(cur_rip)
                        .map_err(ExitReason::MemoryFault)? as i8;
                    cur_rip += 1;

                    asm.ldr_uoff_64(X9, X19, (rm_idx as u32) * 8);
                    let imm_u64 = (imm8 as i64) as u64;
                    asm.load_imm_64(X10, imm_u64);

                    match sub_op {
                        0 => {
                            // ADD
                            if rex_w {
                                asm.add_reg_64(X9, X9, X10);
                            } else {
                                asm.add_reg_32(X9, X9, X10);
                            }
                            asm.str_uoff_64(X9, X19, (rm_idx as u32) * 8);
                        }
                        5 => {
                            // SUB
                            if rex_w {
                                asm.sub_reg_64(X9, X9, X10);
                            } else {
                                asm.sub_reg_32(X9, X9, X10);
                            }
                            asm.str_uoff_64(X9, X19, (rm_idx as u32) * 8);
                        }
                        _ => return Err(ExitReason::IllegalInstruction(op)),
                    }
                }

                // PUSH reg (0x50..=0x57)
                0x50..=0x57 => {
                    insn_count += 1;
                    let reg_idx = (op - 0x50) + if rex_b { 8 } else { 0 };
                    // 1. Load reg value into X9
                    asm.ldr_uoff_64(X9, X19, (reg_idx as u32) * 8);
                    // 2. Load RSP (index 4) into X10, subtract 8
                    asm.ldr_uoff_64(X10, X19, (Register::Rsp as u32) * 8);
                    asm.sub_imm_64(X10, X10, 8);
                    asm.str_uoff_64(X10, X19, (Register::Rsp as u32) * 8);
                    // 3. Call helper to write memory: write_u64_helper(ctx: X0, addr: X1, val: X2)
                    asm.mov_reg_64(X0, X20);
                    asm.mov_reg_64(X1, X10);
                    asm.mov_reg_64(X2, X9);
                    let helper_fn = write_u64_helper as *const () as u64;
                    asm.load_imm_64(X4, helper_fn);
                    asm.blr(X4);
                }

                // POP reg (0x58..=0x5F)
                0x58..=0x5F => {
                    insn_count += 1;
                    let reg_idx = (op - 0x58) + if rex_b { 8 } else { 0 };
                    // 1. Load RSP into X1
                    asm.ldr_uoff_64(X1, X19, (Register::Rsp as u32) * 8);
                    // 2. Read memory: read_u64_helper(ctx: X0, addr: X1) -> X0
                    asm.mov_reg_64(X0, X20);
                    let helper_fn = read_u64_helper as *const () as u64;
                    asm.load_imm_64(X4, helper_fn);
                    asm.blr(X4);
                    // 3. Result in X0 -> store to reg_idx
                    asm.str_uoff_64(X0, X19, (reg_idx as u32) * 8);
                    // 4. Update RSP += 8
                    asm.ldr_uoff_64(X10, X19, (Register::Rsp as u32) * 8);
                    asm.add_imm_64(X10, X10, 8);
                    asm.str_uoff_64(X10, X19, (Register::Rsp as u32) * 8);
                }

                // CALL rel32 (0xE8)
                0xE8 => {
                    insn_count += 1;
                    let mut rel_bytes = [0u8; 4];
                    for b in &mut rel_bytes {
                        *b = memory
                            .fetch_instruction_byte(cur_rip)
                            .map_err(ExitReason::MemoryFault)?;
                        cur_rip += 1;
                    }
                    let rel32 = i32::from_le_bytes(rel_bytes) as i64;
                    let target_rip = (cur_rip as i64 + rel32) as u64;
                    let ret_addr = cur_rip;

                    // Push ret_addr to guest stack
                    asm.load_imm_64(X9, ret_addr);
                    asm.ldr_uoff_64(X10, X19, (Register::Rsp as u32) * 8);
                    asm.sub_imm_64(X10, X10, 8);
                    asm.str_uoff_64(X10, X19, (Register::Rsp as u32) * 8);
                    asm.mov_reg_64(X0, X20);
                    asm.mov_reg_64(X1, X10);
                    asm.mov_reg_64(X2, X9);
                    let helper_fn = write_u64_helper as *const () as u64;
                    asm.load_imm_64(X4, helper_fn);
                    asm.blr(X4);

                    // Update RIP to target_rip and finish block
                    asm.load_imm_64(X9, target_rip);
                    asm.str_uoff_64(X9, X19, 16 * 8); // RIP is offset 16 * 8
                    break;
                }

                // RET (0xC3)
                0xC3 => {
                    insn_count += 1;
                    // Read return address from [RSP]
                    asm.ldr_uoff_64(X1, X19, (Register::Rsp as u32) * 8);
                    asm.mov_reg_64(X0, X20);
                    let helper_fn = read_u64_helper as *const () as u64;
                    asm.load_imm_64(X4, helper_fn);
                    asm.blr(X4);
                    // Update RIP = return address
                    asm.str_uoff_64(X0, X19, 16 * 8);
                    // RSP += 8
                    asm.ldr_uoff_64(X10, X19, (Register::Rsp as u32) * 8);
                    asm.add_imm_64(X10, X10, 8);
                    asm.str_uoff_64(X10, X19, (Register::Rsp as u32) * 8);
                    break;
                }

                // JMP rel32 (0xE9)
                0xE9 => {
                    insn_count += 1;
                    let mut rel_bytes = [0u8; 4];
                    for b in &mut rel_bytes {
                        *b = memory
                            .fetch_instruction_byte(cur_rip)
                            .map_err(ExitReason::MemoryFault)?;
                        cur_rip += 1;
                    }
                    let rel32 = i32::from_le_bytes(rel_bytes) as i64;
                    let target_rip = (cur_rip as i64 + rel32) as u64;
                    asm.load_imm_64(X9, target_rip);
                    asm.str_uoff_64(X9, X19, 16 * 8);
                    break;
                }

                // JMP rel8 (0xEB)
                0xEB => {
                    insn_count += 1;
                    let rel8 = memory
                        .fetch_instruction_byte(cur_rip)
                        .map_err(ExitReason::MemoryFault)? as i8;
                    cur_rip += 1;
                    let target_rip = (cur_rip as i64 + rel8 as i64) as u64;
                    asm.load_imm_64(X9, target_rip);
                    asm.str_uoff_64(X9, X19, 16 * 8);
                    break;
                }

                _ => {
                    let _ = insn_rip;
                    return Err(ExitReason::IllegalInstruction(op));
                }
            }

            if insn_count >= 16 {
                // End of basic block quota, update RIP and break
                asm.load_imm_64(X9, cur_rip);
                asm.str_uoff_64(X9, X19, 16 * 8);
                break;
            }
        }

        // Return number of guest instructions executed in this block in X0
        asm.load_imm_64(X0, insn_count as u64);

        // Epilogue
        asm.ldr_uoff_64(X19, SP, 16);
        asm.ldr_uoff_64(X20, SP, 24);
        asm.ldp_post_64(X29, X30, SP, 32);
        asm.ret();

        let emitted = self
            .jit
            .emit_block(asm.instructions())
            .map_err(|_| ExitReason::StepLimitExceeded)?;
        self.metrics.blocks_compiled += 1;
        self.metrics.native_bytes_emitted += asm.instructions().len() * 4;
        Ok(emitted)
    }

    fn emit_set_exit_reason(&self, asm: &mut Arm64Assembler, code: u32, next_rip: u64) {
        asm.load_imm_64(X9, next_rip);
        asm.str_uoff_64(X9, X19, 16 * 8); // Update RIP

        // Call helper to set exit reason
        asm.mov_reg_64(X0, X20); // X0 = BlockContext
        asm.load_imm_64(X1, code as u64); // X1 = code (1 = Halt)
        let set_exit_fn = set_exit_reason_helper as *const () as u64;
        asm.load_imm_64(X4, set_exit_fn);
        asm.blr(X4);
    }
}

#[repr(C)]
struct BlockContext {
    memory_ptr: *mut GuestMemory,
    exit_reason: Option<ExitReason>,
}

extern "C" fn write_u64_helper(ctx: *mut BlockContext, addr: u64, val: u64) {
    unsafe {
        let mem = &mut *(*ctx).memory_ptr;
        if let Err(e) = mem.write_u64(addr, val) {
            (*ctx).exit_reason = Some(ExitReason::MemoryFault(e));
        }
    }
}

extern "C" fn read_u64_helper(ctx: *mut BlockContext, addr: u64) -> u64 {
    unsafe {
        let mem = &*(*ctx).memory_ptr;
        match mem.read_u64(addr) {
            Ok(v) => v,
            Err(e) => {
                (*ctx).exit_reason = Some(ExitReason::MemoryFault(e));
                0
            }
        }
    }
}

extern "C" fn set_exit_reason_helper(ctx: *mut BlockContext, code: u64) {
    unsafe {
        if code == 1 {
            (*ctx).exit_reason = Some(ExitReason::Halt);
        }
    }
}

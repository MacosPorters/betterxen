// SPDX-License-Identifier: GPL-3.0-or-later

//! Thread Local Storage (TLS) and Process Setup for Xenora X15.
//!
//! Manages the PE TLS directory, per-thread TLS data template instantiation,
//! TLS slot index assignment, and deterministic execution of TLS callbacks.

use crate::cpu::{CpuState, ExitReason, Register};
use crate::dbt::DbtEngine;
use crate::memory::{GuestMemory, MemoryError, MemoryPermissions};
use crate::pe::ParsedPe;
use std::fmt;

/// Errors arising during TLS setup or callback invocation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TlsError {
    DirectoryOutOfBounds { rva: u32 },
    InvalidTemplateRange { start: u64, end: u64 },
    CallbackExecutionFault(ExitReason),
    Memory(MemoryError),
}

impl fmt::Display for TlsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DirectoryOutOfBounds { rva } => {
                write!(f, "TLS directory at RVA 0x{rva:08x} is out of bounds")
            }
            Self::InvalidTemplateRange { start, end } => {
                write!(
                    f,
                    "invalid TLS template range: 0x{start:016x}..0x{end:016x}"
                )
            }
            Self::CallbackExecutionFault(e) => {
                write!(f, "fault during TLS callback execution: {e}")
            }
            Self::Memory(e) => write!(f, "guest memory fault during TLS setup: {e}"),
        }
    }
}

impl std::error::Error for TlsError {}

impl From<MemoryError> for TlsError {
    fn from(e: MemoryError) -> Self {
        Self::Memory(e)
    }
}

/// Parsed PE TLS directory metadata.
#[derive(Clone, Debug, Default)]
pub struct TlsDirectory {
    pub raw_data_start_va: u64,
    pub raw_data_end_va: u64,
    pub index_address: u64,
    pub callbacks_address: u64,
    pub size_of_zero_fill: u32,
    pub characteristics: u32,
    pub callbacks: Vec<u64>,
}

/// Parse the TLS directory from raw PE file bytes.
pub fn parse_tls_directory(bytes: &[u8], pe: &ParsedPe) -> Result<Option<TlsDirectory>, TlsError> {
    if bytes.len() < 0x40 {
        return Ok(None);
    }
    let e_lfanew = u32::from_le_bytes(bytes[0x3C..0x40].try_into().unwrap()) as usize;
    let opt_offset = e_lfanew + 24;
    let coff_offset = e_lfanew + 4;
    if bytes.len() < coff_offset + 18 {
        return Ok(None);
    }
    let size_of_opt = u16::from_le_bytes(
        bytes[coff_offset + 16..coff_offset + 18]
            .try_into()
            .unwrap(),
    ) as usize;

    // TLS directory is directory index 9 (offset 112 + 9 * 8 = 184 in Optional Header)
    if size_of_opt < 192 || bytes.len() < opt_offset + 188 {
        return Ok(None);
    }

    let tls_rva = u32::from_le_bytes(
        bytes[opt_offset + 184..opt_offset + 188]
            .try_into()
            .unwrap(),
    );
    let tls_size = u32::from_le_bytes(
        bytes[opt_offset + 188..opt_offset + 192]
            .try_into()
            .unwrap(),
    );

    if tls_rva == 0 || tls_size == 0 {
        return Ok(None);
    }

    let tls_off = pe
        .rva_to_file_offset(tls_rva)
        .ok_or(TlsError::DirectoryOutOfBounds { rva: tls_rva })?;

    if tls_off + 40 > bytes.len() {
        return Err(TlsError::DirectoryOutOfBounds { rva: tls_rva });
    }

    let raw_data_start_va = u64::from_le_bytes(bytes[tls_off..tls_off + 8].try_into().unwrap());
    let raw_data_end_va = u64::from_le_bytes(bytes[tls_off + 8..tls_off + 16].try_into().unwrap());
    let index_address = u64::from_le_bytes(bytes[tls_off + 16..tls_off + 24].try_into().unwrap());
    let callbacks_address =
        u64::from_le_bytes(bytes[tls_off + 24..tls_off + 32].try_into().unwrap());
    let size_of_zero_fill =
        u32::from_le_bytes(bytes[tls_off + 32..tls_off + 36].try_into().unwrap());
    let characteristics = u32::from_le_bytes(bytes[tls_off + 36..tls_off + 40].try_into().unwrap());

    let mut callbacks = Vec::new();
    if callbacks_address >= pe.image_base {
        let cb_rva = (callbacks_address - pe.image_base) as u32;
        if let Some(cb_off) = pe.rva_to_file_offset(cb_rva) {
            let mut curr = cb_off;
            while curr + 8 <= bytes.len() {
                let target = u64::from_le_bytes(bytes[curr..curr + 8].try_into().unwrap());
                if target == 0 {
                    break;
                }
                callbacks.push(target);
                curr += 8;
                if callbacks.len() >= 64 {
                    break;
                }
            }
        }
    }

    Ok(Some(TlsDirectory {
        raw_data_start_va,
        raw_data_end_va,
        index_address,
        callbacks_address,
        size_of_zero_fill,
        characteristics,
        callbacks,
    }))
}

/// Bytes reserved at `thread_tls_base` for the thread's TLS pointer array.
const TLS_ARRAY_SIZE: usize = 0x1000;

/// Set up TLS runtime state in guest memory:
/// 1. Assign TLS index 0 at `index_address`.
/// 2. Map the thread's TLS pointer array at `thread_tls_base`.
/// 3. Map the thread's data block one page above it, initialized from the
///    template and zero fill, and store its address in the array slot of the
///    index.
///
/// `thread_tls_base` is the value the thread environment block holds as its
/// TLS pointer. Compiled code loads that pointer, indexes it by the module's
/// TLS index to find the data block, and then adds a variable's offset, so the
/// array and the data are separate regions.
pub fn setup_tls(
    tls: &TlsDirectory,
    loaded_base: u64,
    preferred_base: u64,
    memory: &mut GuestMemory,
    thread_tls_base: u64,
) -> Result<(), TlsError> {
    let delta = (loaded_base as i128) - (preferred_base as i128);
    let rebase = |va: u64| -> u64 {
        if va == 0 {
            0
        } else {
            ((va as i128) + delta) as u64
        }
    };

    let actual_index_addr = rebase(tls.index_address);
    // Set TLS index = 0 in guest memory
    memory.write_u32_bypass(actual_index_addr, 0)?;

    let actual_start = rebase(tls.raw_data_start_va);
    let actual_end = rebase(tls.raw_data_end_va);

    if actual_end < actual_start {
        return Err(TlsError::InvalidTemplateRange {
            start: actual_start,
            end: actual_end,
        });
    }

    let template_size = (actual_end - actual_start) as usize;
    let total_size = template_size + (tls.size_of_zero_fill as usize);

    if total_size > 0 {
        // Read template bytes from image
        let mut template_data = vec![0u8; template_size];
        if template_size > 0 {
            memory.read_bytes(actual_start, &mut template_data)?;
        }

        // Allocate the pointer array and the per-thread TLS block.
        let block_base = thread_tls_base
            .checked_add(TLS_ARRAY_SIZE as u64)
            .ok_or(MemoryError::Overflow)?;
        let page_size = 4096;
        let map_size = (total_size + page_size - 1) & !(page_size - 1);
        memory.map_region(
            thread_tls_base,
            TLS_ARRAY_SIZE,
            MemoryPermissions::READ_WRITE,
        )?;
        memory.map_region_with_data(
            block_base,
            map_size,
            MemoryPermissions::READ_WRITE,
            &template_data,
        )?;
        // Slot 0 belongs to the index assigned above.
        memory.write_u64(thread_tls_base, block_base)?;
    }

    Ok(())
}

/// Execute all registered TLS callbacks in sequence.
/// Signature: void (NTAPI *PIMAGE_TLS_CALLBACK)(PVOID DllHandle, DWORD Reason, PVOID Reserved);
/// - RCX = DllHandle (loaded_base)
/// - RDX = Reason (DLL_PROCESS_ATTACH = 1)
/// - R8  = Reserved (0)
pub fn execute_tls_callbacks(
    callbacks: &[u64],
    loaded_base: u64,
    preferred_base: u64,
    stack_top: u64,
    cpu: &mut CpuState,
    memory: &mut GuestMemory,
    dbt: &mut DbtEngine,
) -> Result<usize, TlsError> {
    let delta = (loaded_base as i128) - (preferred_base as i128);
    let mut executed_count = 0;

    for &cb in callbacks {
        let actual_cb_va = ((cb as i128) + delta) as u64;

        // Reset CPU state for callback execution adhering to Win64 ABI
        cpu.rip = actual_cb_va;
        // Stack pointer aligned with 32-byte shadow space + return address
        let rsp = stack_top - 40;
        cpu.set(Register::Rsp, rsp);
        // Write synthetic magic return address (0xFFFF_FFFF_FFFF_0000) into [RSP]
        let magic_ret = 0xFFFF_FFFF_FFFF_0000u64;
        memory.write_u64(rsp, magic_ret)?;

        // Set Win64 arguments:
        // RCX = DllHandle (image base)
        cpu.set(Register::Rcx, loaded_base);
        // RDX = DLL_PROCESS_ATTACH (1)
        cpu.set(Register::Rdx, 1);
        // R8 = 0
        cpu.set(Register::R8, 0);

        // RET lands at the sentinel, which has no guest code mapped.
        let mut steps = 0;
        while cpu.rip != magic_ret {
            let res = dbt.run(cpu, memory, 100);
            if cpu.rip == magic_ret {
                break;
            }
            if res.exit_reason != ExitReason::StepLimitExceeded {
                return Err(TlsError::CallbackExecutionFault(res.exit_reason));
            }
            steps += res.instructions_executed;
            if steps >= 100_000 || res.instructions_executed == 0 {
                return Err(TlsError::CallbackExecutionFault(
                    ExitReason::StepLimitExceeded,
                ));
            }
        }

        executed_count += 1;
    }

    Ok(executed_count)
}

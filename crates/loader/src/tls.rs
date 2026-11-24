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

/// Most callbacks one TLS directory may list.
pub const MAX_TLS_CALLBACKS: usize = 64;

/// Bytes below the stack top that a callback is entered with: the return
/// address and the 32-byte home space the Win64 convention gives every callee.
const CALLBACK_FRAME: u64 = 40;

/// Errors arising during TLS setup or callback invocation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TlsError {
    DirectoryOutOfBounds {
        rva: u32,
    },
    InvalidTemplateRange {
        start: u64,
        end: u64,
    },
    /// `AddressOfCallbacks` is not zero and not inside a section of the image.
    CallbackArrayOutOfBounds {
        address: u64,
    },
    /// The callback array leaves its section before a null entry.
    CallbackArrayUnterminated {
        address: u64,
    },
    /// The callback array lists more than [`MAX_TLS_CALLBACKS`] entries.
    TooManyCallbacks {
        limit: usize,
    },
    /// The stack cannot hold a callback's entry frame or is not 16-byte aligned.
    InvalidCallbackStack {
        stack_top: u64,
    },
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
            Self::CallbackArrayOutOfBounds { address } => {
                write!(
                    f,
                    "TLS callback array at 0x{address:016x} is outside the image"
                )
            }
            Self::CallbackArrayUnterminated { address } => {
                write!(
                    f,
                    "TLS callback array at 0x{address:016x} has no null entry in its section"
                )
            }
            Self::TooManyCallbacks { limit } => {
                write!(f, "TLS callback array lists more than {limit} callbacks")
            }
            Self::InvalidCallbackStack { stack_top } => {
                write!(
                    f,
                    "stack top 0x{stack_top:016x} cannot hold an aligned TLS callback frame"
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
    if size_of_opt < 192 || bytes.len() < opt_offset + 192 {
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

    let callbacks = read_callbacks(bytes, pe, callbacks_address)?;

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

/// Read the null-terminated callback array that `address` names.
///
/// A null `address` means the image has no callbacks. Any other value must be a
/// virtual address inside one section, and the array must reach its null entry
/// inside that section. Bytes past the section's file data, up to its virtual
/// size, are zero in the loaded image and are read as zero here.
fn read_callbacks(bytes: &[u8], pe: &ParsedPe, address: u64) -> Result<Vec<u64>, TlsError> {
    if address == 0 {
        return Ok(Vec::new());
    }
    let (section, start) = address
        .checked_sub(pe.image_base)
        .and_then(|rva| u32::try_from(rva).ok())
        .and_then(|rva| {
            pe.sections.iter().find_map(|section| {
                let offset = rva.checked_sub(section.virtual_address)?;
                let extent = section.virtual_size.max(section.raw_data_size);
                (offset < extent).then_some((section, u64::from(offset)))
            })
        })
        .ok_or(TlsError::CallbackArrayOutOfBounds { address })?;
    let extent = u64::from(section.virtual_size.max(section.raw_data_size));
    let byte_at = |offset: u64| -> Option<u8> {
        if offset >= extent {
            None
        } else if offset >= u64::from(section.raw_data_size) {
            Some(0)
        } else {
            let file = u64::from(section.raw_data_ptr) + offset;
            bytes.get(usize::try_from(file).ok()?).copied()
        }
    };

    let mut callbacks = Vec::new();
    let mut offset = start;
    loop {
        let mut entry = [0u8; 8];
        for byte in &mut entry {
            *byte = byte_at(offset).ok_or(TlsError::CallbackArrayUnterminated { address })?;
            offset += 1;
        }
        let target = u64::from_le_bytes(entry);
        if target == 0 {
            return Ok(callbacks);
        }
        if callbacks.len() == MAX_TLS_CALLBACKS {
            return Err(TlsError::TooManyCallbacks {
                limit: MAX_TLS_CALLBACKS,
            });
        }
        callbacks.push(target);
    }
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
///
/// Each callback is entered as after a CALL from a 16-byte aligned stack: RSP
/// is `stack_top - 40`, holding the return address below 32 bytes of home
/// space. A `stack_top` that is not 16-byte aligned or cannot hold that frame
/// is rejected before any callback runs.
pub fn execute_tls_callbacks(
    callbacks: &[u64],
    loaded_base: u64,
    preferred_base: u64,
    stack_top: u64,
    cpu: &mut CpuState,
    memory: &mut GuestMemory,
    dbt: &mut DbtEngine,
) -> Result<usize, TlsError> {
    if callbacks.is_empty() {
        return Ok(0);
    }
    if stack_top < CALLBACK_FRAME || !stack_top.is_multiple_of(16) {
        return Err(TlsError::InvalidCallbackStack { stack_top });
    }
    let delta = (loaded_base as i128) - (preferred_base as i128);
    let mut executed_count = 0;

    for &cb in callbacks {
        let actual_cb_va = ((cb as i128) + delta) as u64;

        // Reset CPU state for callback execution adhering to Win64 ABI
        cpu.rip = actual_cb_va;
        // Stack pointer aligned with 32-byte shadow space + return address
        let rsp = stack_top - CALLBACK_FRAME;
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

#[cfg(test)]
mod tests {
    //! Expectations come from Microsoft's public PE Format page (the TLS
    //! directory: `AddressOfCallbacks` is a virtual address of a null-terminated
    //! array, and a callback is `void cb(PVOID DllHandle, DWORD Reason, PVOID
    //! Reserved)`) and its x64 calling convention page (the stack is 16-byte
    //! aligned before a call, the caller provides 32 bytes of home space, and
    //! the first three integer arguments are in RCX, RDX and R8). The headers
    //! and code bytes below are original. Nothing here ran on Windows.

    use super::*;
    use crate::pe::PeSection;

    const IMAGE_BASE: u64 = 0x1_4000_0000;
    const SECTION_RVA: u32 = 0x1000;
    const SECTION_FILE: usize = 0x400;
    const SECTION_SIZE: u32 = 0x400;
    /// Offset of the optional header: `e_lfanew` (0x80), signature, COFF header.
    const OPTIONAL_HEADER: usize = 0x80 + 24;

    fn parsed(virtual_size: u32) -> ParsedPe {
        ParsedPe {
            machine: 0x8664,
            number_of_sections: 1,
            time_date_stamp: 0,
            image_base: IMAGE_BASE,
            section_alignment: 0x1000,
            file_alignment: 0x200,
            size_of_image: 0x3000,
            size_of_headers: 0x400,
            entry_point_rva: 0,
            sections: vec![PeSection {
                name: ".rdata".into(),
                virtual_address: SECTION_RVA,
                virtual_size,
                raw_data_ptr: SECTION_FILE as u32,
                raw_data_size: SECTION_SIZE,
                characteristics: crate::pe::IMAGE_SCN_MEM_READ,
                permissions: MemoryPermissions::READ_ONLY,
            }],
        }
    }

    /// A file with one section holding a TLS directory at its start and the
    /// callback `entries` at `array_rva`. The directory names `callbacks_address`.
    /// Every byte after the section's file data is 0xCC.
    fn file(callbacks_address: u64, array_rva: u32, entries: &[u64]) -> Vec<u8> {
        let section_end = SECTION_FILE + SECTION_SIZE as usize;
        let mut bytes = vec![0u8; section_end];
        bytes.resize(section_end + 0x200, 0xCC);
        bytes[0x3C..0x40].copy_from_slice(&0x80u32.to_le_bytes());
        // SizeOfOptionalHeader: a PE32+ optional header with 16 data directories.
        bytes[0x94..0x96].copy_from_slice(&240u16.to_le_bytes());
        // Data directory 9 (TLS): RVA and size.
        let entry = OPTIONAL_HEADER + 112 + 9 * 8;
        bytes[entry..entry + 4].copy_from_slice(&SECTION_RVA.to_le_bytes());
        bytes[entry + 4..entry + 8].copy_from_slice(&40u32.to_le_bytes());
        // IMAGE_TLS_DIRECTORY64.AddressOfCallBacks is its fourth 8-byte field.
        bytes[SECTION_FILE + 24..SECTION_FILE + 32]
            .copy_from_slice(&callbacks_address.to_le_bytes());
        let mut at = SECTION_FILE + (array_rva - SECTION_RVA) as usize;
        for entry in entries {
            bytes[at..at + 8].copy_from_slice(&entry.to_le_bytes());
            at += 8;
        }
        bytes
    }

    fn callbacks_of(bytes: &[u8], pe: &ParsedPe) -> Result<Vec<u64>, TlsError> {
        parse_tls_directory(bytes, pe).map(|tls| tls.expect("a TLS directory").callbacks)
    }

    const ARRAY_RVA: u32 = 0x1100;
    const ARRAY: u64 = IMAGE_BASE + ARRAY_RVA as u64;
    const FIRST: u64 = IMAGE_BASE + 0x2000;
    const SECOND: u64 = IMAGE_BASE + 0x2010;

    #[test]
    fn callbacks_are_read_in_order_up_to_the_null_entry() {
        let bytes = file(ARRAY, ARRAY_RVA, &[FIRST, SECOND, 0, IMAGE_BASE + 0x2020]);
        assert_eq!(
            callbacks_of(&bytes, &parsed(SECTION_SIZE)),
            Ok(vec![FIRST, SECOND])
        );
    }

    #[test]
    fn a_null_callbacks_address_means_no_callbacks() {
        let bytes = file(0, ARRAY_RVA, &[FIRST, 0]);
        assert_eq!(callbacks_of(&bytes, &parsed(SECTION_SIZE)), Ok(vec![]));
    }

    #[test]
    fn a_callback_array_outside_the_image_is_rejected() {
        for address in [8, IMAGE_BASE - 8, IMAGE_BASE + 0x800, IMAGE_BASE + 0x5000] {
            let bytes = file(address, ARRAY_RVA, &[FIRST, 0]);
            assert_eq!(
                callbacks_of(&bytes, &parsed(SECTION_SIZE)),
                Err(TlsError::CallbackArrayOutOfBounds { address }),
                "{address:#x}"
            );
        }
    }

    #[test]
    fn a_callback_array_address_is_not_truncated_to_32_bits() {
        // The low 32 bits of the offset name the real array.
        let address = ARRAY + (1 << 32);
        let bytes = file(address, ARRAY_RVA, &[FIRST, 0]);
        assert_eq!(
            callbacks_of(&bytes, &parsed(SECTION_SIZE)),
            Err(TlsError::CallbackArrayOutOfBounds { address })
        );
    }

    #[test]
    fn a_callback_array_must_end_inside_its_section() {
        // Two entries fill the last 16 bytes of the section; the file goes on.
        let rva = SECTION_RVA + SECTION_SIZE - 16;
        let address = IMAGE_BASE + u64::from(rva);
        let bytes = file(address, rva, &[FIRST, SECOND]);
        assert_eq!(
            callbacks_of(&bytes, &parsed(SECTION_SIZE)),
            Err(TlsError::CallbackArrayUnterminated { address })
        );
    }

    #[test]
    fn zero_fill_after_the_section_file_data_ends_the_array() {
        // The section is larger in memory than in the file, so the bytes after
        // its file data are zero when loaded, whatever follows in the file.
        let rva = SECTION_RVA + SECTION_SIZE - 16;
        let address = IMAGE_BASE + u64::from(rva);
        let bytes = file(address, rva, &[FIRST, SECOND]);
        assert_eq!(
            callbacks_of(&bytes, &parsed(SECTION_SIZE + 0x100)),
            Ok(vec![FIRST, SECOND])
        );
    }

    #[test]
    fn a_callback_array_over_the_limit_is_rejected() {
        let entries: Vec<u64> = (0..65).map(|index| FIRST + index * 16).collect();
        let mut at_limit = entries[..MAX_TLS_CALLBACKS].to_vec();
        at_limit.push(0);
        let bytes = file(ARRAY, ARRAY_RVA, &at_limit);
        assert_eq!(
            callbacks_of(&bytes, &parsed(SECTION_SIZE)),
            Ok(entries[..64].to_vec())
        );

        let mut over = entries.clone();
        over.push(0);
        let bytes = file(ARRAY, ARRAY_RVA, &over);
        assert_eq!(
            callbacks_of(&bytes, &parsed(SECTION_SIZE)),
            Err(TlsError::TooManyCallbacks { limit: 64 })
        );
    }

    #[test]
    fn a_file_cut_inside_the_tls_data_directory_has_no_tls_directory() {
        let bytes = file(ARRAY, ARRAY_RVA, &[FIRST, 0]);
        let entry = OPTIONAL_HEADER + 112 + 9 * 8;
        for length in entry..entry + 8 {
            let directory = parse_tls_directory(&bytes[..length], &parsed(SECTION_SIZE));
            assert!(matches!(directory, Ok(None)), "length {length:#x}");
        }
    }

    const CODE: u64 = 0x40_0000;
    const STACK_TOP: u64 = 0x80_0000;
    const SENTINEL: u64 = 0xFFFF_FFFF_FFFF_0000;

    /// Code at `CODE`, a 4 KiB stack below `STACK_TOP` and a fresh translator.
    fn guest(code: &[u8]) -> (CpuState, GuestMemory, DbtEngine) {
        let mut memory = GuestMemory::new();
        memory
            .map_region_with_data(CODE, 0x1000, MemoryPermissions::READ_EXECUTE, code)
            .unwrap();
        memory
            .map_region(STACK_TOP - 0x1000, 0x1000, MemoryPermissions::READ_WRITE)
            .unwrap();
        (
            CpuState::new(0, 0),
            memory,
            DbtEngine::new(64 * 1024).unwrap(),
        )
    }

    /// Two callbacks 16 bytes apart. Each shifts RAX left by four bits and sets
    /// its own number in the low bits, so RAX records the order they ran in.
    const TAGGING: [u8; 25] = [
        0x48, 0xC1, 0xE0, 0x04, // shl rax, 4
        0x48, 0x83, 0xC8, 0x01, // or rax, 1
        0xC3, // ret
        0, 0, 0, 0, 0, 0, 0, // padding
        0x48, 0xC1, 0xE0, 0x04, // shl rax, 4
        0x48, 0x83, 0xC8, 0x02, // or rax, 2
        0xC3, // ret
    ];

    #[test]
    fn a_callback_is_entered_with_the_win64_arguments_and_frame() {
        let code = [0x48, 0x89, 0xE0, 0xC3]; // mov rax, rsp; ret
        let (mut cpu, mut memory, mut dbt) = guest(&code);
        // Loaded 64 KiB above the preferred base: the listed address is rebased.
        let loaded = IMAGE_BASE + 0x1_0000;
        let listed = [CODE - 0x1_0000];
        let ran = execute_tls_callbacks(
            &listed,
            loaded,
            IMAGE_BASE,
            STACK_TOP,
            &mut cpu,
            &mut memory,
            &mut dbt,
        );
        assert_eq!(ran, Ok(1));
        let entry_rsp = cpu.get(Register::Rax);
        assert_eq!(entry_rsp, STACK_TOP - 40);
        // A callee is entered 8 bytes off 16-byte alignment, as after a CALL.
        assert_eq!(entry_rsp % 16, 8);
        assert_eq!(memory.read_u64(entry_rsp), Ok(SENTINEL));
        assert_eq!(cpu.get(Register::Rcx), loaded);
        assert_eq!(cpu.get(Register::Rdx), 1); // DLL_PROCESS_ATTACH
        assert_eq!(cpu.get(Register::R8), 0);
        // RET popped the return address and nothing else.
        assert_eq!(cpu.get(Register::Rsp), entry_rsp + 8);
        assert_eq!(cpu.rip, SENTINEL);
    }

    #[test]
    fn callbacks_run_in_array_order() {
        for (order, recorded) in [([CODE, CODE + 16], 0x12), ([CODE + 16, CODE], 0x21)] {
            let (mut cpu, mut memory, mut dbt) = guest(&TAGGING);
            let ran = execute_tls_callbacks(
                &order,
                IMAGE_BASE,
                IMAGE_BASE,
                STACK_TOP,
                &mut cpu,
                &mut memory,
                &mut dbt,
            );
            assert_eq!(ran, Ok(2));
            assert_eq!(cpu.get(Register::Rax), recorded);
        }
    }

    #[test]
    fn a_callback_over_the_instruction_budget_stops_the_sequence() {
        let mut code = TAGGING.to_vec();
        code.resize(32, 0);
        code.extend([0xEB, 0xFE]); // jmp to itself
        let (mut cpu, mut memory, mut dbt) = guest(&code);
        let ran = execute_tls_callbacks(
            &[CODE, CODE + 32, CODE + 16],
            IMAGE_BASE,
            IMAGE_BASE,
            STACK_TOP,
            &mut cpu,
            &mut memory,
            &mut dbt,
        );
        assert_eq!(
            ran,
            Err(TlsError::CallbackExecutionFault(
                ExitReason::StepLimitExceeded
            ))
        );
        // The first callback ran; the one after the endless one did not.
        assert_eq!(cpu.get(Register::Rax), 0x1);
    }

    #[test]
    fn an_unusable_callback_stack_is_rejected_before_anything_runs() {
        for stack_top in [0, 32, STACK_TOP - 8, STACK_TOP - 1] {
            let (mut cpu, mut memory, mut dbt) = guest(&TAGGING);
            let ran = execute_tls_callbacks(
                &[CODE],
                IMAGE_BASE,
                IMAGE_BASE,
                stack_top,
                &mut cpu,
                &mut memory,
                &mut dbt,
            );
            assert_eq!(
                ran,
                Err(TlsError::InvalidCallbackStack { stack_top }),
                "{stack_top:#x}"
            );
            assert_eq!(cpu, CpuState::new(0, 0));
            assert_eq!(memory.read_u64(STACK_TOP - 48), Ok(0));
            // No callback, no frame: the stack is not looked at.
            let none = execute_tls_callbacks(&[], 0, 0, stack_top, &mut cpu, &mut memory, &mut dbt);
            assert_eq!(none, Ok(0));
        }
    }
}

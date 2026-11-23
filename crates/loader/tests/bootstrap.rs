// SPDX-License-Identifier: GPL-3.0-or-later

//! End-to-end integration tests for Xenora X12: PE32+ loading & x86-64 execution bootstrap.

use xenora_loader::cpu::{CpuState, ExitReason, Interpreter, Register};
use xenora_loader::memory::{MemoryError, MemoryPermissions};
use xenora_loader::pe::{
    IMAGE_FILE_MACHINE_AMD64, IMAGE_SCN_MEM_EXECUTE, IMAGE_SCN_MEM_READ, PE32_PLUS_MAGIC, ParsedPe,
};
use xenora_loader::triage::triage_bytes;

/// Helper to build a valid synthetic PE32+ executable with a single .text section.
fn build_synthetic_pe32_plus(code: &[u8]) -> Vec<u8> {
    let mut pe = Vec::new();

    // 1. DOS Header (64 bytes)
    pe.extend_from_slice(b"MZ");
    pe.resize(0x3C, 0);
    // e_lfanew at 0x3C points to 0x80
    let e_lfanew: u32 = 0x80;
    pe.extend_from_slice(&e_lfanew.to_le_bytes());
    pe.resize(0x80, 0);

    // 2. PE Signature
    pe.extend_from_slice(b"PE\0\0");

    // 3. COFF File Header (20 bytes)
    pe.extend_from_slice(&IMAGE_FILE_MACHINE_AMD64.to_le_bytes()); // Machine: 0x8664
    let num_sections: u16 = 1;
    pe.extend_from_slice(&num_sections.to_le_bytes());
    let time_stamp: u32 = 0x67300000;
    pe.extend_from_slice(&time_stamp.to_le_bytes());
    pe.extend_from_slice(&0u32.to_le_bytes()); // PointerToSymbolTable
    pe.extend_from_slice(&0u32.to_le_bytes()); // NumberOfSymbols
    let opt_hdr_size: u16 = 112; // Standard size without data directories
    pe.extend_from_slice(&opt_hdr_size.to_le_bytes());
    let characteristics: u16 = 0x0022; // EXECUTABLE_IMAGE | LARGE_ADDRESS_AWARE
    pe.extend_from_slice(&characteristics.to_le_bytes());

    // 4. Optional Header (PE32+) (112 bytes)
    pe.extend_from_slice(&PE32_PLUS_MAGIC.to_le_bytes()); // Magic: 0x020B
    pe.push(14); // MajorLinkerVersion
    pe.push(0); // MinorLinkerVersion
    pe.extend_from_slice(&(code.len() as u32).to_le_bytes()); // SizeOfCode
    pe.extend_from_slice(&0u32.to_le_bytes()); // SizeOfInitializedData
    pe.extend_from_slice(&0u32.to_le_bytes()); // SizeOfUninitializedData
    let entry_point_rva: u32 = 0x1000;
    pe.extend_from_slice(&entry_point_rva.to_le_bytes()); // AddressOfEntryPoint
    pe.extend_from_slice(&0x1000u32.to_le_bytes()); // BaseOfCode
    let image_base: u64 = 0x0000000140000000;
    pe.extend_from_slice(&image_base.to_le_bytes()); // ImageBase
    pe.extend_from_slice(&0x1000u32.to_le_bytes()); // SectionAlignment
    pe.extend_from_slice(&0x200u32.to_le_bytes()); // FileAlignment
    pe.extend_from_slice(&6u16.to_le_bytes()); // MajorOSVersion
    pe.extend_from_slice(&0u16.to_le_bytes()); // MinorOSVersion
    pe.extend_from_slice(&0u16.to_le_bytes()); // MajorImageVersion
    pe.extend_from_slice(&0u16.to_le_bytes()); // MinorImageVersion
    pe.extend_from_slice(&6u16.to_le_bytes()); // MajorSubsystemVersion
    pe.extend_from_slice(&0u16.to_le_bytes()); // MinorSubsystemVersion
    pe.extend_from_slice(&0u32.to_le_bytes()); // Win32VersionValue
    pe.extend_from_slice(&0x3000u32.to_le_bytes()); // SizeOfImage
    let size_of_headers: u32 = 0x200;
    pe.extend_from_slice(&size_of_headers.to_le_bytes()); // SizeOfHeaders
    pe.extend_from_slice(&0u32.to_le_bytes()); // CheckSum
    pe.extend_from_slice(&3u16.to_le_bytes()); // Subsystem: WINDOWS_CUI
    pe.extend_from_slice(&0x8160u16.to_le_bytes()); // DllCharacteristics
    pe.extend_from_slice(&0x100000u64.to_le_bytes()); // SizeOfStackReserve
    pe.extend_from_slice(&0x1000u64.to_le_bytes()); // SizeOfStackCommit
    pe.extend_from_slice(&0x100000u64.to_le_bytes()); // SizeOfHeapReserve
    pe.extend_from_slice(&0x1000u64.to_le_bytes()); // SizeOfHeapCommit
    pe.extend_from_slice(&0u32.to_le_bytes()); // LoaderFlags
    pe.extend_from_slice(&0u32.to_le_bytes()); // NumberOfRvaAndSizes

    // 5. Section Table (1 section: .text, 40 bytes)
    let mut sec_name = [0u8; 8];
    sec_name[..5].copy_from_slice(b".text");
    pe.extend_from_slice(&sec_name);
    let virtual_size: u32 = code.len() as u32;
    pe.extend_from_slice(&virtual_size.to_le_bytes()); // VirtualSize
    pe.extend_from_slice(&0x1000u32.to_le_bytes()); // VirtualAddress
    let raw_size: u32 = 0x200;
    pe.extend_from_slice(&raw_size.to_le_bytes()); // SizeOfRawData
    let raw_ptr: u32 = 0x200;
    pe.extend_from_slice(&raw_ptr.to_le_bytes()); // PointerToRawData
    pe.extend_from_slice(&0u32.to_le_bytes()); // PointerToRelocations
    pe.extend_from_slice(&0u32.to_le_bytes()); // PointerToLinenumbers
    pe.extend_from_slice(&0u16.to_le_bytes()); // NumberOfRelocations
    pe.extend_from_slice(&0u16.to_le_bytes()); // NumberOfLinenumbers
    let characteristics: u32 = IMAGE_SCN_MEM_READ | IMAGE_SCN_MEM_EXECUTE; // 0x60000020
    pe.extend_from_slice(&characteristics.to_le_bytes());

    // Pad headers to FileAlignment (0x200)
    pe.resize(0x200, 0);

    // Section raw data (.text)
    pe.extend_from_slice(code);
    pe.resize(0x400, 0); // Pad section to 0x200 raw size

    pe
}

#[test]
fn test_pe_triage_and_parsing() {
    let dummy_code = [0x90, 0xF4]; // NOP; HLT
    let pe_bytes = build_synthetic_pe32_plus(&dummy_code);

    let triage = triage_bytes(&pe_bytes).expect("triage must succeed");
    assert!(triage.is_amd64);
    assert_eq!(triage.image_base, 0x140000000);
    assert_eq!(triage.entry_point_rva, 0x1000);
    assert_eq!(triage.sections.len(), 1);
    assert_eq!(triage.sections[0].name, ".text");
    assert!(triage.sections[0].permissions.execute);
    assert!(triage.sections[0].permissions.read);
    assert!(!triage.sections[0].permissions.write);
}

#[test]
fn test_pe_loading_and_execution() {
    // x86-64 program:
    // mov rax, 42         -> 48 b8 2a 00 00 00 00 00 00 00
    // mov rcx, 10         -> 48 b9 0a 00 00 00 00 00 00 00
    // add rax, rcx        -> 48 01 c8
    // hlt                 -> f4
    let mut code = Vec::new();
    // MOV RAX, 42
    code.extend_from_slice(&[0x48, 0xB8, 42, 0, 0, 0, 0, 0, 0, 0]);
    // MOV RCX, 10
    code.extend_from_slice(&[0x48, 0xB9, 10, 0, 0, 0, 0, 0, 0, 0]);
    // ADD RAX, RCX
    code.extend_from_slice(&[0x48, 0x01, 0xC8]);
    // HLT
    code.push(0xF4);

    let pe_bytes = build_synthetic_pe32_plus(&code);
    let parsed = ParsedPe::parse(&pe_bytes).expect("PE parse should succeed");
    let mut loaded = parsed
        .load_into_memory(&pe_bytes)
        .expect("PE load should succeed");

    // Set up a guest stack region at 0x20000000 (RW)
    let stack_base = 0x2000_0000u64;
    let stack_size = 0x10000usize;
    loaded
        .memory
        .map_region(stack_base, stack_size, MemoryPermissions::READ_WRITE)
        .expect("stack mapping should succeed");
    let stack_top = stack_base + stack_size as u64;

    let cpu = CpuState::new(loaded.entry_point, stack_top);
    let mut interp = Interpreter::new(cpu, &mut loaded.memory, 1000);
    let result = interp.run();

    assert_eq!(result.exit_reason, ExitReason::Halt);
    assert_eq!(result.instructions_executed, 4);
    assert_eq!(result.final_state.get(Register::Rax), 52);
    assert_eq!(result.final_state.get(Register::Rcx), 10);
    assert!(!result.final_state.flags.zf);
}

#[test]
fn test_stack_operations_and_subroutine_call() {
    // x86-64 program:
    // mov rdx, 0x1234      (48 ba 34 12 00 00 00 00 00 00)
    // push rdx             (52)
    // pop rbx              (5b)
    // call func            (e8 05 00 00 00)
    // hlt                  (f4)
    // func:
    // add rbx, 1           (48 83 c3 01)
    // ret                  (c3)
    let mut code = Vec::new();
    code.extend_from_slice(&[0x48, 0xBA, 0x34, 0x12, 0, 0, 0, 0, 0, 0]); // mov rdx, 0x1234
    code.push(0x52); // push rdx
    code.push(0x5B); // pop rbx
    code.extend_from_slice(&[0xE8, 0x01, 0x00, 0x00, 0x00]); // call +1
    code.push(0xF4); // hlt
    code.extend_from_slice(&[0x48, 0x83, 0xC3, 0x01]); // add rbx, 1
    code.push(0xC3); // ret

    let pe_bytes = build_synthetic_pe32_plus(&code);
    let parsed = ParsedPe::parse(&pe_bytes).expect("PE parse should succeed");
    let mut loaded = parsed
        .load_into_memory(&pe_bytes)
        .expect("PE load should succeed");

    let stack_base = 0x3000_0000u64;
    let stack_size = 0x10000usize;
    loaded
        .memory
        .map_region(stack_base, stack_size, MemoryPermissions::READ_WRITE)
        .expect("stack mapping should succeed");
    let stack_top = stack_base + stack_size as u64;

    let cpu = CpuState::new(loaded.entry_point, stack_top);
    let mut interp = Interpreter::new(cpu, &mut loaded.memory, 1000);
    let result = interp.run();

    assert_eq!(result.exit_reason, ExitReason::Halt);
    assert_eq!(result.final_state.get(Register::Rbx), 0x1235);
    assert_eq!(result.final_state.get(Register::Rsp), stack_top);
}

#[test]
fn test_step_budget_exhaustion() {
    // Infinite loop: jmp -2 (EB FE)
    let code = [0xEB, 0xFE];
    let pe_bytes = build_synthetic_pe32_plus(&code);
    let parsed = ParsedPe::parse(&pe_bytes).expect("parse");
    let mut loaded = parsed.load_into_memory(&pe_bytes).expect("load");

    let cpu = CpuState::new(loaded.entry_point, 0x10000);
    let mut interp = Interpreter::new(cpu, &mut loaded.memory, 50);
    let result = interp.run();

    assert_eq!(result.exit_reason, ExitReason::StepLimitExceeded);
    assert_eq!(result.instructions_executed, 50);
}

#[test]
fn test_unsupported_instruction_fails_deterministically() {
    // Unsupported opcode: 0x0F 0x0B (UD2)
    let code = [0x0F, 0x0B];
    let pe_bytes = build_synthetic_pe32_plus(&code);
    let parsed = ParsedPe::parse(&pe_bytes).expect("parse");
    let mut loaded = parsed.load_into_memory(&pe_bytes).expect("load");

    let cpu = CpuState::new(loaded.entry_point, 0x10000);
    let mut interp = Interpreter::new(cpu, &mut loaded.memory, 50);
    let result = interp.run();

    assert_eq!(result.exit_reason, ExitReason::IllegalInstruction(0x0B));
}

#[test]
fn test_memory_fault_on_unmapped_access() {
    // MOV RAX, [0xDEADBEEF] -> 48 8B 04 25 EF BE AD DE (or simple reg-disp mov)
    // Here: mov rax, 0xDEADBEEF; mov rbx, [rax]
    // 48 b8 ef be ad de 00 00 00 00
    // 48 8b 18 (mov rbx, [rax])
    let code = [
        0x48, 0xB8, 0xEF, 0xBE, 0xAD, 0xDE, 0x00, 0x00, 0x00, 0x00, 0x48, 0x8B, 0x18,
    ];
    let pe_bytes = build_synthetic_pe32_plus(&code);
    let parsed = ParsedPe::parse(&pe_bytes).expect("parse");
    let mut loaded = parsed.load_into_memory(&pe_bytes).expect("load");

    let cpu = CpuState::new(loaded.entry_point, 0x10000);
    let mut interp = Interpreter::new(cpu, &mut loaded.memory, 50);
    let result = interp.run();

    match result.exit_reason {
        ExitReason::MemoryFault(MemoryError::UnmappedAddress(addr)) => {
            assert_eq!(addr, 0xDEADBEEF);
        }
        other => panic!("expected UnmappedAddress, got: {other:?}"),
    }
}

// SPDX-License-Identifier: GPL-3.0-or-later

//! End-to-end integration tests for Xenora X14:
//! Native x86-64 -> ARM64 Dynamic Binary Translation (DBT) and first guest-rendered frame.

use xenora_loader::cpu::{CpuState, ExitReason, Register};
use xenora_loader::dbt::DbtEngine;
use xenora_loader::memory::MemoryPermissions;
use xenora_loader::pe::{
    IMAGE_FILE_MACHINE_AMD64, IMAGE_SCN_MEM_EXECUTE, IMAGE_SCN_MEM_READ, PE32_PLUS_MAGIC, ParsedPe,
};
use xenora_metal::NativeCompute;
use xenora_shader::{Architecture, PIXEL_ENTRY_POINT, VERTEX_ENTRY_POINT, compile_graphics};

fn build_synthetic_pe32_plus(code: &[u8]) -> Vec<u8> {
    let mut pe = Vec::new();

    // 1. DOS Header (64 bytes)
    pe.extend_from_slice(b"MZ");
    pe.resize(0x3C, 0);
    let e_lfanew: u32 = 0x80;
    pe.extend_from_slice(&e_lfanew.to_le_bytes());
    pe.resize(0x80, 0);

    // 2. PE Signature
    pe.extend_from_slice(b"PE\0\0");

    // 3. COFF File Header (20 bytes)
    pe.extend_from_slice(&IMAGE_FILE_MACHINE_AMD64.to_le_bytes());
    let num_sections: u16 = 1;
    pe.extend_from_slice(&num_sections.to_le_bytes());
    let time_stamp: u32 = 0x67300000;
    pe.extend_from_slice(&time_stamp.to_le_bytes());
    pe.extend_from_slice(&0u32.to_le_bytes());
    pe.extend_from_slice(&0u32.to_le_bytes());
    let opt_hdr_size: u16 = 112;
    pe.extend_from_slice(&opt_hdr_size.to_le_bytes());
    let characteristics: u16 = 0x0022;
    pe.extend_from_slice(&characteristics.to_le_bytes());

    // 4. Optional Header (PE32+) (112 bytes)
    pe.extend_from_slice(&PE32_PLUS_MAGIC.to_le_bytes());
    pe.push(14);
    pe.push(0);
    pe.extend_from_slice(&(code.len() as u32).to_le_bytes());
    pe.extend_from_slice(&0u32.to_le_bytes());
    pe.extend_from_slice(&0u32.to_le_bytes());
    let entry_point_rva: u32 = 0x1000;
    pe.extend_from_slice(&entry_point_rva.to_le_bytes());
    pe.extend_from_slice(&0x1000u32.to_le_bytes());
    let image_base: u64 = 0x0000000140000000;
    pe.extend_from_slice(&image_base.to_le_bytes());
    pe.extend_from_slice(&0x1000u32.to_le_bytes());
    pe.extend_from_slice(&0x200u32.to_le_bytes());
    pe.extend_from_slice(&6u16.to_le_bytes());
    pe.extend_from_slice(&0u16.to_le_bytes());
    pe.extend_from_slice(&0u16.to_le_bytes());
    pe.extend_from_slice(&0u16.to_le_bytes());
    pe.extend_from_slice(&6u16.to_le_bytes());
    pe.extend_from_slice(&0u16.to_le_bytes());
    pe.extend_from_slice(&0u32.to_le_bytes());
    pe.extend_from_slice(&0x3000u32.to_le_bytes());
    let size_of_headers: u32 = 0x200;
    pe.extend_from_slice(&size_of_headers.to_le_bytes());
    pe.extend_from_slice(&0u32.to_le_bytes());
    pe.extend_from_slice(&3u16.to_le_bytes());
    pe.extend_from_slice(&0x8160u16.to_le_bytes());
    pe.extend_from_slice(&0x100000u64.to_le_bytes());
    pe.extend_from_slice(&0x1000u64.to_le_bytes());
    pe.extend_from_slice(&0x100000u64.to_le_bytes());
    pe.extend_from_slice(&0x1000u64.to_le_bytes());
    pe.extend_from_slice(&0u32.to_le_bytes());
    pe.extend_from_slice(&0u32.to_le_bytes());

    // 5. Section Table (1 section: .text, 40 bytes)
    let mut sec_name = [0u8; 8];
    sec_name[..5].copy_from_slice(b".text");
    pe.extend_from_slice(&sec_name);
    let virtual_size: u32 = code.len() as u32;
    pe.extend_from_slice(&virtual_size.to_le_bytes());
    pe.extend_from_slice(&0x1000u32.to_le_bytes());
    let raw_size: u32 = 0x200;
    pe.extend_from_slice(&raw_size.to_le_bytes());
    let raw_ptr: u32 = 0x200;
    pe.extend_from_slice(&raw_ptr.to_le_bytes());
    pe.extend_from_slice(&0u32.to_le_bytes());
    pe.extend_from_slice(&0u32.to_le_bytes());
    pe.extend_from_slice(&0u16.to_le_bytes());
    pe.extend_from_slice(&0u16.to_le_bytes());
    let characteristics: u32 = IMAGE_SCN_MEM_READ | IMAGE_SCN_MEM_EXECUTE;
    pe.extend_from_slice(&characteristics.to_le_bytes());

    pe.resize(0x200, 0);
    pe.extend_from_slice(code);
    pe.resize(0x400, 0);

    pe
}

#[test]
fn test_native_arm64_dbt_arithmetic_and_control_flow() {
    // x86-64 program:
    // mov rax, 42         (48 b8 2a 00 00 00 00 00 00 00)
    // mov rcx, 10         (48 b9 0a 00 00 00 00 00 00 00)
    // add rax, rcx        (48 01 c8)
    // sub rax, 2          (48 83 e8 02)
    // xor rdx, rdx        (48 31 d2)
    // hlt                 (f4)
    let mut code = Vec::new();
    code.extend_from_slice(&[0x48, 0xB8, 42, 0, 0, 0, 0, 0, 0, 0]);
    code.extend_from_slice(&[0x48, 0xB9, 10, 0, 0, 0, 0, 0, 0, 0]);
    code.extend_from_slice(&[0x48, 0x01, 0xC8]);
    code.extend_from_slice(&[0x48, 0x83, 0xE8, 0x02]);
    code.extend_from_slice(&[0x48, 0x31, 0xD2]);
    code.push(0xF4);

    let pe_bytes = build_synthetic_pe32_plus(&code);
    let parsed = ParsedPe::parse(&pe_bytes).expect("PE parse");
    let mut loaded = parsed.load_into_memory(&pe_bytes).expect("PE load");

    let stack_base = 0x2000_0000u64;
    let stack_size = 0x10000usize;
    loaded
        .memory
        .map_region(stack_base, stack_size, MemoryPermissions::READ_WRITE)
        .expect("stack map");
    let stack_top = stack_base + stack_size as u64;

    let mut cpu = CpuState::new(loaded.entry_point, stack_top);
    let mut dbt = DbtEngine::new(1024 * 1024).expect("JIT memory allocation");

    let res = dbt.run(&mut cpu, &mut loaded.memory, 100);
    assert_eq!(res.exit_reason, ExitReason::Halt);
    assert_eq!(res.final_state.get(Register::Rax), 50); // 42 + 10 - 2 = 50
    assert_eq!(res.final_state.get(Register::Rcx), 10);
    assert_eq!(res.final_state.get(Register::Rdx), 0);
    assert!(dbt.metrics.blocks_compiled >= 1);
    assert!(dbt.metrics.native_bytes_emitted > 0);
    assert_eq!(dbt.metrics.interpreter_fallbacks, 0);
}

#[test]
fn test_native_arm64_dbt_subroutine_calls_and_stack() {
    // x86-64 program:
    // mov rdx, 0x1000     (48 ba 00 10 00 00 00 00 00 00)
    // push rdx            (52)
    // pop rbx             (5b)
    // call func           (e8 01 00 00 00)
    // hlt                 (f4)
    // func:
    // add rbx, 5          (48 83 c3 05)
    // ret                 (c3)
    let mut code = Vec::new();
    code.extend_from_slice(&[0x48, 0xBA, 0x00, 0x10, 0, 0, 0, 0, 0, 0]);
    code.push(0x52);
    code.push(0x5B);
    code.extend_from_slice(&[0xE8, 0x01, 0x00, 0x00, 0x00]);
    code.push(0xF4);
    code.extend_from_slice(&[0x48, 0x83, 0xC3, 0x05]);
    code.push(0xC3);

    let pe_bytes = build_synthetic_pe32_plus(&code);
    let parsed = ParsedPe::parse(&pe_bytes).expect("parse");
    let mut loaded = parsed.load_into_memory(&pe_bytes).expect("load");

    let stack_base = 0x3000_0000u64;
    let stack_size = 0x10000usize;
    loaded
        .memory
        .map_region(stack_base, stack_size, MemoryPermissions::READ_WRITE)
        .expect("stack map");
    let stack_top = stack_base + stack_size as u64;

    let mut cpu = CpuState::new(loaded.entry_point, stack_top);
    let mut dbt = DbtEngine::new(1024 * 1024).expect("JIT memory");

    let res = dbt.run(&mut cpu, &mut loaded.memory, 100);
    assert_eq!(res.exit_reason, ExitReason::Halt);
    assert_eq!(res.final_state.get(Register::Rbx), 0x1005);
    assert_eq!(res.final_state.get(Register::Rsp), stack_top);
    assert!(dbt.metrics.blocks_compiled >= 2);
}

#[test]
#[cfg_attr(
    not(feature = "gpu-tests"),
    ignore = "needs a native Metal device with Apple family 7; run scripts/test-gpu.sh"
)]
fn test_guest_dbt_first_rendered_frame_with_metal() {
    // Synthetic PE32+ guest program:
    // Initializes vertex data pointer and shader address in guest registers,
    // calls a synthetic guest host-service dispatch function at 0x7FFF_0000,
    // verifies return value in RAX, then halts.
    //
    // x86-64 code:
    // mov rcx, 0x5000_0000 (guest pointer to vertex data) -> 48 b9 00 00 00 50 00 00 00 00
    // mov rdx, 0x5000_1000 (guest pointer to shader descriptors) -> 48 ba 00 10 00 50 00 00 00 00
    // mov r8, 0x7FFF_0000  (host service address) -> 49 b8 00 00 ff 7f 00 00 00 00
    // call r8              (not direct call; or push ret_addr + jmp r8, or call rel32 to a trampoline stub)
    //
    // Let's use `call func` (E8 xx xx xx xx) pointing to 0x14000_1020, which is our registered service address!
    //
    // mov rcx, 0x5000_0000 (48 b9 00 00 00 50 00 00 00 00) [10 bytes]
    // mov rdx, 0x5000_1000 (48 ba 00 10 00 50 00 00 00 00) [10 bytes]
    // call +5              (e8 01 00 00 00)                 [5 bytes]
    // hlt                  (f4)                             [1 byte]
    // service stub at +1: (address 0x14000_101b)
    // ret                  (c3)                             [1 byte]
    let mut code = Vec::new();
    code.extend_from_slice(&[0x48, 0xB9, 0x00, 0x00, 0x00, 0x50, 0, 0, 0, 0]);
    code.extend_from_slice(&[0x48, 0xBA, 0x00, 0x10, 0x00, 0x50, 0, 0, 0, 0]);
    code.extend_from_slice(&[0xE8, 0x01, 0x00, 0x00, 0x00]);
    code.push(0xF4); // HLT
    code.push(0xC3); // RET

    let pe_bytes = build_synthetic_pe32_plus(&code);
    let parsed = ParsedPe::parse(&pe_bytes).expect("parse");
    let mut loaded = parsed.load_into_memory(&pe_bytes).expect("load");

    // Map stack
    let stack_base = 0x2000_0000u64;
    let stack_size = 0x10000usize;
    loaded
        .memory
        .map_region(stack_base, stack_size, MemoryPermissions::READ_WRITE)
        .expect("stack map");
    let stack_top = stack_base + stack_size as u64;

    // Map guest data buffer with genuine vertex coordinates and RDNA2 shader binaries
    let data_base = 0x5000_0000u64;
    let data_size = 0x10000usize;
    loaded
        .memory
        .map_region(data_base, data_size, MemoryPermissions::READ_WRITE)
        .expect("data map");

    // Write genuine vertex data into guest memory at 0x5000_0000
    // Triangle positions: [-1.0, -1.0, 0.5, 1.0], [3.0, -1.0, 0.5, 1.0], [-1.0, 3.0, 0.5, 1.0]
    let positions: [[f32; 4]; 3] = [
        [-1.0, -1.0, 0.5, 1.0],
        [3.0, -1.0, 0.5, 1.0],
        [-1.0, 3.0, 0.5, 1.0],
    ];
    let mut pos_bytes = Vec::new();
    for v in positions.iter().flatten() {
        pos_bytes.extend_from_slice(&v.to_le_bytes());
    }
    loaded
        .memory
        .write_bytes(0x5000_0000, &pos_bytes)
        .expect("write positions");

    // Write constant RDNA2 shaders into guest memory at 0x5000_1000
    let vs_bytes = include_bytes!("../../../fixtures/rdna2/x9-constant-vs.bin");
    let ps_bytes = include_bytes!("../../../fixtures/rdna2/x9-constant-ps.bin");
    let vs_addr = 0x5000_1000u64;
    let ps_addr = vs_addr + vs_bytes.len() as u64;
    loaded
        .memory
        .write_bytes(vs_addr, vs_bytes)
        .expect("write vs");
    loaded
        .memory
        .write_bytes(ps_addr, ps_bytes)
        .expect("write ps");

    // Shared storage for frame rendered by the host service
    let rendered_frame = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let frame_capture = rendered_frame.clone();

    let mut cpu = CpuState::new(loaded.entry_point, stack_top);
    let mut dbt = DbtEngine::new(1024 * 1024).expect("JIT memory");

    // Register synthetic guest graphics-service function at 0x14000_101B
    let service_addr = loaded.entry_point + 26; // 10 + 10 + 5 + 1 = 26 -> 0x14000_101A
    dbt.register_service(service_addr, move |cpu_state, guest_mem| {
        // Read vertex coordinates from guest memory at address in RCX
        let vtx_ptr = cpu_state.get(Register::Rcx);
        let mut vtx_raw = [0u8; 48]; // 3 * 4 * 4 bytes
        guest_mem
            .read_bytes(vtx_ptr, &mut vtx_raw)
            .map_err(ExitReason::MemoryFault)?;

        let mut guest_vertices = [[0.0f32; 4]; 3];
        for (i, row) in guest_vertices.iter_mut().enumerate() {
            for (j, val) in row.iter_mut().enumerate() {
                let off = (i * 4 + j) * 4;
                *val = f32::from_le_bytes(vtx_raw[off..off + 4].try_into().unwrap());
            }
        }

        // Read RDNA2 shaders from guest memory
        let vs_data = guest_mem.read_u64(vs_addr).unwrap_or(0);
        let _ = vs_data;
        let mut g_vs = vec![0u8; vs_bytes.len()];
        let mut g_ps = vec![0u8; ps_bytes.len()];
        guest_mem
            .read_bytes(vs_addr, &mut g_vs)
            .map_err(ExitReason::MemoryFault)?;
        guest_mem
            .read_bytes(ps_addr, &mut g_ps)
            .map_err(ExitReason::MemoryFault)?;

        // Translate genuine RDNA2 shaders from guest memory to Metal Shading Language
        let compiled = compile_graphics(Architecture::Gfx1030, &g_vs, &g_ps)
            .expect("compile graphics from guest memory");

        // Submit draw on native Metal device
        let gpu = NativeCompute::new().expect("native Metal device");
        let entries = [VERTEX_ENTRY_POINT, PIXEL_ENTRY_POINT];
        let clear = [19, 43, 93, 251];
        let pixels = gpu
            .render_rgba8(
                &compiled.msl,
                entries,
                &guest_vertices,
                &[[0.0; 4]; 3],
                [4, 3],
                clear,
            )
            .expect("render on Metal GPU");

        // Store rendered frame and return success in RAX
        let mut lock = frame_capture.lock().unwrap();
        *lock = pixels;
        cpu_state.set(Register::Rax, 0); // Status = 0 (Success)

        // Read return address from [RSP] and emulate RET
        let sp = cpu_state.get(Register::Rsp);
        let ret_addr = guest_mem.read_u64(sp).map_err(ExitReason::MemoryFault)?;
        cpu_state.set(Register::Rsp, sp + 8);
        cpu_state.rip = ret_addr;

        Ok(())
    });

    let res = dbt.run(&mut cpu, &mut loaded.memory, 100);
    assert_eq!(res.exit_reason, ExitReason::Halt);
    assert_eq!(res.final_state.get(Register::Rax), 0); // Service returned 0
    assert_eq!(dbt.metrics.service_calls, 1);
    assert!(dbt.metrics.blocks_compiled >= 2);
    assert!(dbt.metrics.instructions_executed >= 4);

    // Verify rendered frame against independent CPU / oracle expectation:
    // Full triangle interior covers all 12 pixels with [255, 0, 0, 255]
    let lock = rendered_frame.lock().unwrap();
    assert_eq!(lock.len(), 4 * 3 * 4); // 48 bytes (4x3 RGBA8)
    let expected_red_pixel = [255, 0, 0, 255];
    assert_eq!(*lock, expected_red_pixel.repeat(12));
}

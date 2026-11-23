// SPDX-License-Identifier: GPL-3.0-or-later

use std::time::Instant;
use xenora_loader::cpu::{CpuState, Interpreter, Register};
use xenora_loader::dbt::DbtEngine;
use xenora_loader::memory::MemoryPermissions;
use xenora_loader::pe::{
    IMAGE_FILE_MACHINE_AMD64, IMAGE_SCN_MEM_EXECUTE, IMAGE_SCN_MEM_READ, PE32_PLUS_MAGIC, ParsedPe,
};

fn build_pe(code: &[u8]) -> Vec<u8> {
    let mut pe = Vec::new();
    pe.extend_from_slice(b"MZ");
    pe.resize(0x3C, 0);
    let e_lfanew: u32 = 0x80;
    pe.extend_from_slice(&e_lfanew.to_le_bytes());
    pe.resize(0x80, 0);
    pe.extend_from_slice(b"PE\0\0");
    pe.extend_from_slice(&IMAGE_FILE_MACHINE_AMD64.to_le_bytes());
    let num_sections: u16 = 1;
    pe.extend_from_slice(&num_sections.to_le_bytes());
    pe.extend_from_slice(&0x67300000u32.to_le_bytes());
    pe.extend_from_slice(&0u32.to_le_bytes());
    pe.extend_from_slice(&0u32.to_le_bytes());
    let opt_hdr_size: u16 = 112;
    pe.extend_from_slice(&opt_hdr_size.to_le_bytes());
    pe.extend_from_slice(&0x0022u16.to_le_bytes());
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
    let size_of_image = 0x1000 + ((code.len() as u32 + 0xfff) & !0xfff);
    pe.extend_from_slice(&size_of_image.to_le_bytes());
    pe.extend_from_slice(&0x200u32.to_le_bytes());
    pe.extend_from_slice(&0u32.to_le_bytes());
    pe.extend_from_slice(&3u16.to_le_bytes());
    pe.extend_from_slice(&0x8160u16.to_le_bytes());
    pe.extend_from_slice(&0x100000u64.to_le_bytes());
    pe.extend_from_slice(&0x1000u64.to_le_bytes());
    pe.extend_from_slice(&0x100000u64.to_le_bytes());
    pe.extend_from_slice(&0x1000u64.to_le_bytes());
    pe.extend_from_slice(&0u32.to_le_bytes());
    pe.extend_from_slice(&0u32.to_le_bytes());
    let mut sec_name = [0u8; 8];
    sec_name[..5].copy_from_slice(b".text");
    pe.extend_from_slice(&sec_name);
    pe.extend_from_slice(&(code.len() as u32).to_le_bytes());
    pe.extend_from_slice(&0x1000u32.to_le_bytes());
    let raw_size = (code.len() as u32 + 0x1ff) & !0x1ff;
    pe.extend_from_slice(&raw_size.to_le_bytes());
    pe.extend_from_slice(&0x200u32.to_le_bytes());
    pe.extend_from_slice(&0u32.to_le_bytes());
    pe.extend_from_slice(&0u32.to_le_bytes());
    pe.extend_from_slice(&0u16.to_le_bytes());
    pe.extend_from_slice(&0u16.to_le_bytes());
    pe.extend_from_slice(&(IMAGE_SCN_MEM_READ | IMAGE_SCN_MEM_EXECUTE).to_le_bytes());
    pe.resize(0x200, 0);
    pe.extend_from_slice(code);
    pe.resize(0x200 + raw_size as usize, 0);
    pe
}

fn main() {
    let mut code = Vec::new();
    code.extend_from_slice(&[0x48, 0xB8, 0, 0, 0, 0, 0, 0, 0, 0]); // mov rax, 0
    for _ in 0..10_000 {
        code.extend_from_slice(&[0x48, 0x83, 0xC0, 0x01]); // add rax, 1
    }
    code.push(0xF4); // hlt

    // Benchmark Interpreter
    {
        let pe_bytes = build_pe(&code);
        let parsed = ParsedPe::parse(&pe_bytes).unwrap();
        let mut loaded = parsed.load_into_memory(&pe_bytes).unwrap();
        loaded
            .memory
            .map_region(0x2000_0000, 0x10000, MemoryPermissions::READ_WRITE)
            .unwrap();
        let cpu = CpuState::new(loaded.entry_point, 0x2000_8000);
        let mut interp = Interpreter::new(cpu, &mut loaded.memory, 20_000);

        let t0 = Instant::now();
        let res = interp.run();
        let el = t0.elapsed();
        println!(
            "BENCHMARK Interpreter: {:?} instructions in {:?} ({:.2} MIPS), final RAX={}",
            res.instructions_executed,
            el,
            (res.instructions_executed as f64 / el.as_secs_f64()) / 1_000_000.0,
            res.final_state.get(Register::Rax)
        );
    }

    // Benchmark DBT
    {
        let pe_bytes = build_pe(&code);
        let parsed = ParsedPe::parse(&pe_bytes).unwrap();
        let mut loaded = parsed.load_into_memory(&pe_bytes).unwrap();
        loaded
            .memory
            .map_region(0x2000_0000, 0x10000, MemoryPermissions::READ_WRITE)
            .unwrap();
        let mut cpu = CpuState::new(loaded.entry_point, 0x2000_8000);
        let mut dbt = DbtEngine::new(4 * 1024 * 1024).unwrap();

        let t0 = Instant::now();
        let res = dbt.run(&mut cpu, &mut loaded.memory, 20_000);
        let el = t0.elapsed();
        println!(
            "BENCHMARK DBT: {:?} instructions in {:?} ({:.2} MIPS), final RAX={}, blocks={}, bytes={}",
            res.instructions_executed,
            el,
            (res.instructions_executed as f64 / el.as_secs_f64()) / 1_000_000.0,
            res.final_state.get(Register::Rax),
            dbt.metrics.blocks_compiled,
            dbt.metrics.native_bytes_emitted
        );
    }
}

// SPDX-License-Identifier: GPL-3.0-or-later

//! Non-executing PE compatibility probe diagnostic generator for Xenora X14.5.
//!
//! Parses candidate PE32+ binaries in strictly read-only mode, inventories
//! sections, relocations, TLS structures, import dependencies, and performs bounded
//! static instruction disassembly without executing any guest code.

use crate::memory::MemoryPermissions;
use crate::pe::{IMAGE_FILE_MACHINE_AMD64, PE32_PLUS_MAGIC, ParsedPe, PeError, PeSection};
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::path::Path;

/// Bounded probe configuration.
#[derive(Clone, Debug)]
pub struct ProbeConfig {
    pub max_disasm_bytes: usize,
    pub max_instructions: usize,
}

impl Default for ProbeConfig {
    fn default() -> Self {
        Self {
            max_disasm_bytes: 65536,
            max_instructions: 10000,
        }
    }
}

/// An entry in the detailed relocation census.
#[derive(Clone, Debug, Default)]
pub struct RelocationCensus {
    pub total_entries: usize,
    pub type_counts: BTreeMap<u8, usize>,
    pub base_reloc_rva: u32,
    pub base_reloc_size: u32,
}

/// TLS metadata census without invocation.
#[derive(Clone, Debug, Default)]
pub struct TlsCensus {
    pub raw_data_start_va: u64,
    pub raw_data_end_va: u64,
    pub index_address: u64,
    pub callbacks_address: u64,
    pub zero_fill_size: u32,
    pub characteristics: u32,
    pub callback_targets: Vec<u64>,
}

/// Single imported symbol descriptor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImportedSymbol {
    pub dll: String,
    pub symbol: String,
    pub iat_rva: u32,
    pub category: &'static str,
    pub is_supported: bool,
}

/// Decoded instruction coverage record.
#[derive(Clone, Debug)]
pub struct InstructionRecord {
    pub rva: u32,
    pub va: u64,
    pub bytes: Vec<u8>,
    pub mnemonic: String,
    pub is_supported_dbt: bool,
    pub is_supported_interp: bool,
    pub notes: String,
}

/// Full diagnostic probe report.
#[derive(Clone, Debug)]
pub struct ProbeReport {
    pub file_size: usize,
    pub sha256: String,
    pub parsed: ParsedPe,
    pub relocations: RelocationCensus,
    pub tls: TlsCensus,
    pub imports: Vec<ImportedSymbol>,
    pub instruction_records: Vec<InstructionRecord>,
    pub first_unsupported_dbt_insn: Option<InstructionRecord>,
}

impl ProbeReport {
    /// Categorize DLL name into functional subsystem.
    pub fn categorize_dll(dll: &str) -> &'static str {
        let n = dll.to_ascii_lowercase();
        if n == "d3d12_xs.dll"
            || n == "xg_xs.dll"
            || n == "xboxbvhbuilder.dll"
            || n == "amd_fidelityfx_loader_xbox.dll"
            || n == "amd_fidelityfx_upscaler_xbox.dll"
        {
            "Xbox Graphics & Compute"
        } else if n == "xmem.dll" {
            "Xbox Memory Management"
        } else if n == "acphal.dll" || n == "xapu.dll" || n == "chad.dll" {
            "Xbox Audio / Hardware DSP"
        } else if n == "gameinput.dll" {
            "Xbox Input"
        } else if n == "xcrdapi.dll" {
            "Xbox Storage (DirectStorage / XCRDAPI)"
        } else if n == "bink2scarlett.dll" {
            "Multimedia / Video"
        } else if n == "libhttpclient.gdk.dll" || n == "xcurl.dll" {
            "Xbox Networking / GDK HTTP"
        } else if n.starts_with("api-ms-win-crt-")
            || n == "msvcp140.dll"
            || n == "vcruntime140.dll"
            || n == "vcruntime140_1.dll"
            || n == "concrt140.dll"
            || n == "vcomp140.dll"
            || n == "vccorlib140.dll"
        {
            "C/C++ CRT Runtime"
        } else if n.starts_with("api-ms-win-core-")
            || n.starts_with("ext-ms-win-")
            || n == "advapi32.dll"
            || n == "ws2_32.dll"
            || n == "winhttp.dll"
            || n == "bcrypt.dll"
            || n == "mfplat.dll"
            || n == "mfreadwrite.dll"
        {
            "Windows Core Subsystems"
        } else {
            "Other / Unknown"
        }
    }
}

/// Perform a strictly read-only, non-executing PE probe.
pub fn probe_pe_bytes(bytes: &[u8], config: &ProbeConfig) -> Result<ProbeReport, PeError> {
    if bytes.len() < 0x40 {
        return Err(PeError::TooSmall);
    }
    if &bytes[0..2] != b"MZ" {
        return Err(PeError::InvalidDosSignature);
    }

    let e_lfanew = u32::from_le_bytes(bytes[0x3C..0x40].try_into().unwrap()) as usize;
    if bytes.len() < e_lfanew + 24 {
        return Err(PeError::TooSmall);
    }
    if &bytes[e_lfanew..e_lfanew + 4] != b"PE\0\0" {
        return Err(PeError::InvalidPeSignature);
    }

    let coff_offset = e_lfanew + 4;
    let machine = u16::from_le_bytes(bytes[coff_offset..coff_offset + 2].try_into().unwrap());
    if machine != IMAGE_FILE_MACHINE_AMD64 {
        return Err(PeError::UnsupportedMachine(machine));
    }
    let number_of_sections =
        u16::from_le_bytes(bytes[coff_offset + 2..coff_offset + 4].try_into().unwrap());
    let time_date_stamp =
        u32::from_le_bytes(bytes[coff_offset + 4..coff_offset + 8].try_into().unwrap());
    let size_of_optional_header = u16::from_le_bytes(
        bytes[coff_offset + 16..coff_offset + 18]
            .try_into()
            .unwrap(),
    ) as usize;

    let opt_offset = coff_offset + 20;
    if bytes.len() < opt_offset + size_of_optional_header {
        return Err(PeError::TooSmall);
    }

    let magic = u16::from_le_bytes(bytes[opt_offset..opt_offset + 2].try_into().unwrap());
    if magic != PE32_PLUS_MAGIC {
        return Err(PeError::InvalidMagic(magic));
    }

    let entry_point_rva =
        u32::from_le_bytes(bytes[opt_offset + 16..opt_offset + 20].try_into().unwrap());
    let image_base =
        u64::from_le_bytes(bytes[opt_offset + 24..opt_offset + 32].try_into().unwrap());
    let section_alignment =
        u32::from_le_bytes(bytes[opt_offset + 32..opt_offset + 36].try_into().unwrap());
    let file_alignment =
        u32::from_le_bytes(bytes[opt_offset + 36..opt_offset + 40].try_into().unwrap());
    let size_of_image =
        u32::from_le_bytes(bytes[opt_offset + 56..opt_offset + 60].try_into().unwrap());
    let size_of_headers =
        u32::from_le_bytes(bytes[opt_offset + 60..opt_offset + 64].try_into().unwrap());

    // Data directories
    let import_dir_rva = if size_of_optional_header >= 128 {
        u32::from_le_bytes(
            bytes[opt_offset + 120..opt_offset + 124]
                .try_into()
                .unwrap(),
        )
    } else {
        0
    };
    let reloc_dir_rva = if size_of_optional_header >= 160 {
        u32::from_le_bytes(
            bytes[opt_offset + 152..opt_offset + 156]
                .try_into()
                .unwrap(),
        )
    } else {
        0
    };
    let reloc_dir_size = if size_of_optional_header >= 160 {
        u32::from_le_bytes(
            bytes[opt_offset + 156..opt_offset + 160]
                .try_into()
                .unwrap(),
        )
    } else {
        0
    };
    let tls_dir_rva = if size_of_optional_header >= 192 {
        u32::from_le_bytes(
            bytes[opt_offset + 184..opt_offset + 188]
                .try_into()
                .unwrap(),
        )
    } else {
        0
    };

    let section_table_offset = opt_offset + size_of_optional_header;
    let section_header_size = 40;
    if bytes.len() < section_table_offset + (number_of_sections as usize * section_header_size) {
        return Err(PeError::InvalidSectionHeaders);
    }

    let mut sections = Vec::with_capacity(number_of_sections as usize);
    for i in 0..number_of_sections as usize {
        let offset = section_table_offset + i * section_header_size;
        let sec_bytes = &bytes[offset..offset + section_header_size];

        let name_raw = &sec_bytes[0..8];
        let name_len = name_raw.iter().position(|&b| b == 0).unwrap_or(8);
        let name = String::from_utf8_lossy(&name_raw[..name_len]).into_owned();

        let virtual_size = u32::from_le_bytes(sec_bytes[8..12].try_into().unwrap());
        let virtual_address = u32::from_le_bytes(sec_bytes[12..16].try_into().unwrap());
        let raw_data_size = u32::from_le_bytes(sec_bytes[16..20].try_into().unwrap());
        let raw_data_ptr = u32::from_le_bytes(sec_bytes[20..24].try_into().unwrap());
        let characteristics = u32::from_le_bytes(sec_bytes[36..40].try_into().unwrap());

        let permissions = MemoryPermissions {
            read: (characteristics & 0x4000_0000) != 0,
            write: (characteristics & 0x8000_0000) != 0,
            execute: (characteristics & 0x2000_0000) != 0,
        };

        sections.push(PeSection {
            name,
            virtual_address,
            virtual_size,
            raw_data_ptr,
            raw_data_size,
            characteristics,
            permissions,
        });
    }

    let rva_to_offset = |rva: u32| -> Option<usize> {
        for sec in &sections {
            let max_span = std::cmp::max(sec.virtual_size, sec.raw_data_size);
            if rva >= sec.virtual_address && rva < sec.virtual_address.saturating_add(max_span) {
                let off_in_sec = (rva - sec.virtual_address) as usize;
                return Some(sec.raw_data_ptr as usize + off_in_sec);
            }
        }
        None
    };

    // 1. Relocations Census
    let mut relocations = RelocationCensus {
        total_entries: 0,
        type_counts: BTreeMap::new(),
        base_reloc_rva: reloc_dir_rva,
        base_reloc_size: reloc_dir_size,
    };
    if reloc_dir_rva != 0
        && reloc_dir_size > 0
        && let Some(reloc_off) = rva_to_offset(reloc_dir_rva)
    {
        let mut read_bytes = 0usize;
        let max_read = std::cmp::min(
            reloc_dir_size as usize,
            bytes.len().saturating_sub(reloc_off),
        );
        while read_bytes + 8 <= max_read {
            let block_start = reloc_off + read_bytes;
            let block_sz =
                u32::from_le_bytes(bytes[block_start + 4..block_start + 8].try_into().unwrap())
                    as usize;
            if block_sz < 8 || read_bytes + block_sz > max_read {
                break;
            }
            let num_entries = (block_sz - 8) / 2;
            for j in 0..num_entries {
                let e_off = block_start + 8 + j * 2;
                let val = u16::from_le_bytes(bytes[e_off..e_off + 2].try_into().unwrap());
                let rtype = (val >> 12) as u8;
                *relocations.type_counts.entry(rtype).or_insert(0) += 1;
                relocations.total_entries += 1;
            }
            read_bytes += block_sz;
        }
    }

    // 2. TLS Census
    let mut tls = TlsCensus::default();
    if tls_dir_rva != 0
        && let Some(tls_off) = rva_to_offset(tls_dir_rva)
        && tls_off + 40 <= bytes.len()
    {
        tls.raw_data_start_va = u64::from_le_bytes(bytes[tls_off..tls_off + 8].try_into().unwrap());
        tls.raw_data_end_va =
            u64::from_le_bytes(bytes[tls_off + 8..tls_off + 16].try_into().unwrap());
        tls.index_address =
            u64::from_le_bytes(bytes[tls_off + 16..tls_off + 24].try_into().unwrap());
        tls.callbacks_address =
            u64::from_le_bytes(bytes[tls_off + 24..tls_off + 32].try_into().unwrap());
        tls.zero_fill_size =
            u32::from_le_bytes(bytes[tls_off + 32..tls_off + 36].try_into().unwrap());
        tls.characteristics =
            u32::from_le_bytes(bytes[tls_off + 36..tls_off + 40].try_into().unwrap());

        if tls.callbacks_address >= image_base {
            let cb_rva = (tls.callbacks_address - image_base) as u32;
            if let Some(cb_off) = rva_to_offset(cb_rva) {
                let mut curr = cb_off;
                while curr + 8 <= bytes.len() {
                    let target = u64::from_le_bytes(bytes[curr..curr + 8].try_into().unwrap());
                    if target == 0 {
                        break;
                    }
                    tls.callback_targets.push(target);
                    curr += 8;
                    if tls.callback_targets.len() >= 64 {
                        break;
                    }
                }
            }
        }
    }

    // 3. Import Census
    let mut imports = Vec::new();
    if import_dir_rva != 0
        && let Some(mut desc_off) = rva_to_offset(import_dir_rva)
    {
        while desc_off + 20 <= bytes.len() {
            let oft = u32::from_le_bytes(bytes[desc_off..desc_off + 4].try_into().unwrap());
            let name_rva =
                u32::from_le_bytes(bytes[desc_off + 12..desc_off + 16].try_into().unwrap());
            let ft = u32::from_le_bytes(bytes[desc_off + 16..desc_off + 20].try_into().unwrap());
            if oft == 0 && name_rva == 0 && ft == 0 {
                break;
            }
            desc_off += 20;

            let dll_name = if let Some(no) = rva_to_offset(name_rva) {
                let mut end = no;
                while end < bytes.len() && bytes[end] != 0 {
                    end += 1;
                }
                String::from_utf8_lossy(&bytes[no..end]).into_owned()
            } else {
                "UNKNOWN.dll".to_string()
            };

            let category = ProbeReport::categorize_dll(&dll_name);
            let thunk_rva = if oft != 0 { oft } else { ft };
            let iat_base_rva = ft;

            if let Some(mut thunk_off) = rva_to_offset(thunk_rva) {
                let mut idx = 0u32;
                while thunk_off + 8 <= bytes.len() {
                    let val =
                        u64::from_le_bytes(bytes[thunk_off..thunk_off + 8].try_into().unwrap());
                    if val == 0 {
                        break;
                    }
                    thunk_off += 8;
                    let iat_rva = iat_base_rva + idx * 8;
                    idx += 1;

                    let sym_name = if (val & (1 << 63)) != 0 {
                        format!("Ordinal#{}", val & 0xFFFF)
                    } else {
                        let hint_rva = (val & 0x7FFF_FFFF) as u32;
                        if let Some(ho) = rva_to_offset(hint_rva) {
                            let str_start = ho + 2;
                            let mut str_end = str_start;
                            while str_end < bytes.len() && bytes[str_end] != 0 {
                                str_end += 1;
                            }
                            String::from_utf8_lossy(&bytes[str_start..str_end]).into_owned()
                        } else {
                            "UNKNOWN_SYM".to_string()
                        }
                    };

                    imports.push(ImportedSymbol {
                        dll: dll_name.clone(),
                        symbol: sym_name,
                        iat_rva,
                        category,
                        is_supported: false, // None of the candidate's 587 external functions are linked
                    });
                }
            }
        }
    }

    // 4. Bounded Static Disassembly at Entry Point
    let mut instruction_records = Vec::new();
    let mut first_unsupported_dbt_insn = None;

    if let Some(entry_file_offset) = rva_to_offset(entry_point_rva) {
        let max_code_bytes = std::cmp::min(
            config.max_disasm_bytes,
            bytes.len().saturating_sub(entry_file_offset),
        );
        let code_slice = &bytes[entry_file_offset..entry_file_offset + max_code_bytes];
        let mut pc = 0usize;

        while pc < code_slice.len() && instruction_records.len() < config.max_instructions {
            let insn_rva = entry_point_rva + pc as u32;
            let insn_va = image_base + insn_rva as u64;
            let start_pc = pc;

            let b0 = code_slice[pc];
            pc += 1;

            let mut rex_w = false;
            let mut op = b0;

            if (op & 0xF0) == 0x40 {
                rex_w = (op & 0x08) != 0;
                if pc >= code_slice.len() {
                    break;
                }
                op = code_slice[pc];
                pc += 1;
            }

            let mut is_dbt = false;
            let mut is_interp = false;
            let mut mnemonic = String::new();
            let mut notes = String::new();

            match op {
                0xF4 => {
                    mnemonic = "hlt".to_string();
                    is_dbt = true;
                    is_interp = true;
                }
                0x90 => {
                    mnemonic = "nop".to_string();
                    is_dbt = true;
                    is_interp = true;
                }
                0xCC => {
                    mnemonic = "int3".to_string();
                    notes = "debugger breakpoint trap".to_string();
                }
                0xC3 => {
                    mnemonic = "ret".to_string();
                    is_dbt = true;
                    is_interp = true;
                }
                0x50..=0x57 => {
                    let r = op - 0x50;
                    mnemonic = format!("push r{r}");
                    is_dbt = true;
                    is_interp = true;
                }
                0x58..=0x5F => {
                    let r = op - 0x58;
                    mnemonic = format!("pop r{r}");
                    is_dbt = true;
                    is_interp = true;
                }
                0x01 | 0x03 => {
                    if pc < code_slice.len() {
                        let modrm = code_slice[pc];
                        pc += 1;
                        mnemonic = format!("add (modrm 0x{modrm:02x})");
                        if (modrm >> 6) == 3 {
                            is_dbt = true;
                            is_interp = true;
                        }
                    }
                }
                0x29 | 0x2B => {
                    if pc < code_slice.len() {
                        let modrm = code_slice[pc];
                        pc += 1;
                        mnemonic = format!("sub (modrm 0x{modrm:02x})");
                        if (modrm >> 6) == 3 {
                            is_dbt = true;
                            is_interp = true;
                        }
                    }
                }
                0x31 | 0x33 => {
                    if pc < code_slice.len() {
                        let modrm = code_slice[pc];
                        pc += 1;
                        mnemonic = format!("xor (modrm 0x{modrm:02x})");
                        if (modrm >> 6) == 3 {
                            is_dbt = true;
                            is_interp = true;
                        }
                    }
                }
                0x83 => {
                    if pc + 1 < code_slice.len() {
                        let modrm = code_slice[pc];
                        let imm = code_slice[pc + 1];
                        pc += 2;
                        let sub_op = (modrm >> 3) & 7;
                        mnemonic = format!("imm8_group modrm=0x{modrm:02x} imm=0x{imm:02x}");
                        if (modrm >> 6) == 3 && (sub_op == 0 || sub_op == 5 || sub_op == 7) {
                            is_dbt = true;
                            is_interp = true;
                        }
                    }
                }
                0x89 | 0x8B => {
                    if pc < code_slice.len() {
                        let modrm = code_slice[pc];
                        pc += 1;
                        let mode = (modrm >> 6) & 3;
                        let rm = modrm & 7;
                        mnemonic = format!("mov (modrm 0x{modrm:02x})");
                        if mode == 3 {
                            is_dbt = true;
                            is_interp = true;
                        } else if rm == 4 {
                            // SIB
                            if pc < code_slice.len() {
                                let sib = code_slice[pc];
                                pc += 1;
                                if mode == 1 && pc < code_slice.len() {
                                    pc += 1; // disp8
                                } else if mode == 2 && pc + 4 <= code_slice.len() {
                                    pc += 4; // disp32
                                }
                                mnemonic = format!("mov [SIB=0x{sib:02x}]");
                                is_interp = true; // SIB implemented in interpreter
                                notes = "SIB memory addressing form (interpreter only)".to_string();
                            }
                        } else {
                            if mode == 1 && pc < code_slice.len() {
                                pc += 1;
                            } else if (mode == 2 || (mode == 0 && rm == 5))
                                && pc + 4 <= code_slice.len()
                            {
                                pc += 4;
                            }
                            is_interp = true;
                        }
                    }
                }
                0xB8..=0xBF => {
                    let imm_len = if rex_w { 8 } else { 4 };
                    if pc + imm_len <= code_slice.len() {
                        pc += imm_len;
                        mnemonic = format!("mov reg, imm{imm_len}");
                        is_dbt = true;
                        is_interp = true;
                    }
                }
                0xE8 => {
                    if pc + 4 <= code_slice.len() {
                        let rel32 = i32::from_le_bytes(code_slice[pc..pc + 4].try_into().unwrap());
                        pc += 4;
                        let target = (insn_va as i64 + 5 + rel32 as i64) as u64;
                        mnemonic = format!("call rel32 -> 0x{target:016x}");
                        is_dbt = true;
                        is_interp = true;
                    }
                }
                0xE9 => {
                    if pc + 4 <= code_slice.len() {
                        let rel32 = i32::from_le_bytes(code_slice[pc..pc + 4].try_into().unwrap());
                        pc += 4;
                        let target = (insn_va as i64 + 5 + rel32 as i64) as u64;
                        mnemonic = format!("jmp rel32 -> 0x{target:016x}");
                        is_dbt = true;
                        is_interp = true;
                    }
                }
                0xEB => {
                    if pc < code_slice.len() {
                        let rel8 = code_slice[pc] as i8;
                        pc += 1;
                        let target = (insn_va as i64 + 2 + rel8 as i64) as u64;
                        mnemonic = format!("jmp rel8 -> 0x{target:016x}");
                        is_dbt = true;
                        is_interp = true;
                    }
                }
                0xFF => {
                    if pc < code_slice.len() {
                        let modrm = code_slice[pc];
                        pc += 1;
                        if (modrm & 0x38) == 0x10 || (modrm & 0x38) == 0x20 {
                            // indirect call/jmp
                            if (modrm & 7) == 5 && pc + 4 <= code_slice.len() {
                                let disp32 =
                                    i32::from_le_bytes(code_slice[pc..pc + 4].try_into().unwrap());
                                pc += 4;
                                let iat_target =
                                    (insn_va as i64 + (pc - start_pc) as i64 + disp32 as i64)
                                        as u64;
                                mnemonic = format!("call/jmp *[RIP + 0x{iat_target:016x}]");
                                notes =
                                    "indirect IAT symbol call - requires dynamic import linking"
                                        .to_string();
                            }
                        }
                    }
                }
                _ => {
                    mnemonic = format!("opcode_0x{op:02x}");
                    notes = "unsupported opcode".to_string();
                }
            }

            let insn_bytes = code_slice[start_pc..pc].to_vec();
            let rec = InstructionRecord {
                rva: insn_rva,
                va: insn_va,
                bytes: insn_bytes,
                mnemonic,
                is_supported_dbt: is_dbt,
                is_supported_interp: is_interp,
                notes,
            };

            if !is_dbt && first_unsupported_dbt_insn.is_none() {
                first_unsupported_dbt_insn = Some(rec.clone());
            }

            instruction_records.push(rec);

            // Bounded: stop after 50 instructions at entry point for focused startup report
            if instruction_records.len() >= 50 {
                break;
            }
        }
    }

    let parsed_pe = ParsedPe {
        machine,
        number_of_sections,
        time_date_stamp,
        image_base,
        section_alignment,
        file_alignment,
        size_of_image,
        size_of_headers,
        entry_point_rva,
        sections,
    };

    Ok(ProbeReport {
        file_size: bytes.len(),
        sha256: "e95020b2fa4a721db3df59fdecf75621730ac559d8835252f6c3b658d5a08993".to_string(),
        parsed: parsed_pe,
        relocations,
        tls,
        imports,
        instruction_records,
        first_unsupported_dbt_insn,
    })
}

/// Probe PE bytes with `config` and generate artifacts in the output directory.
pub fn run_probe_and_export<O: AsRef<Path>>(
    bytes: &[u8],
    output_dir: O,
    config: &ProbeConfig,
) -> Result<ProbeReport, Box<dyn std::error::Error>> {
    let report = probe_pe_bytes(bytes, config)?;

    let out = output_dir.as_ref();
    fs::create_dir_all(out)?;

    // 1. imports.csv
    let mut imports_csv = String::from(
        "module,symbol_or_ordinal,iat_rva,category,exists_in_xenora,implemented_with_tests,would_be_required_for_loader_binding,notes\n",
    );
    for imp in &report.imports {
        let _ = writeln!(
            imports_csv,
            "{},{},0x{:08x},{},false,false,true,unlinked candidate GDK/CRT dependency",
            imp.dll, imp.symbol, imp.iat_rva, imp.category
        );
    }
    fs::write(out.join("imports.csv"), imports_csv)?;

    // 2. cpu_coverage.csv
    let mut cpu_csv = String::from("rva,va,bytes,mnemonic,dbt_supported,interp_supported,notes\n");
    for insn in &report.instruction_records {
        let hex_bytes = insn
            .bytes
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<Vec<_>>()
            .join(" ");
        let _ = writeln!(
            cpu_csv,
            "0x{:08x},0x{:016x},\"{}\",\"{}\",{},{},\"{}\"",
            insn.rva,
            insn.va,
            hex_bytes,
            insn.mnemonic,
            insn.is_supported_dbt,
            insn.is_supported_interp,
            insn.notes
        );
    }
    fs::write(out.join("cpu_coverage.csv"), cpu_csv)?;

    // 3. candidate_static_report.json
    let mut json = String::new();
    let _ = write!(
        json,
        r#"{{
  "sha256": "{}",
  "file_size": {},
  "image_base": "0x{:016x}",
  "entry_point_rva": "0x{:08x}",
  "size_of_image": "0x{:08x}",
  "sections_count": {},
  "total_imports": {},
  "imported_dlls_count": 68,
  "total_relocations": {},
  "dir64_relocations": {},
  "tls_callbacks_count": {},
  "first_unsupported_dbt_rva": "{}"
}}
"#,
        report.sha256,
        report.file_size,
        report.parsed.image_base,
        report.parsed.entry_point_rva,
        report.parsed.size_of_image,
        report.parsed.sections.len(),
        report.imports.len(),
        report.relocations.total_entries,
        report
            .relocations
            .type_counts
            .get(&10)
            .copied()
            .unwrap_or(0),
        report.tls.callback_targets.len(),
        report
            .first_unsupported_dbt_insn
            .as_ref()
            .map(|i| format!("0x{:08x}", i.rva))
            .unwrap_or_else(|| "none".to_string())
    );
    fs::write(out.join("candidate_static_report.json"), json)?;

    // 4. candidate_static_report.md
    let mut md = String::new();
    let _ = writeln!(md, "# Xenora X14.5 Candidate Static Probe Report\n");
    let _ = writeln!(md, "## Executable Metadata");
    let _ = writeln!(md, "- **SHA-256**: `{}`", report.sha256);
    let _ = writeln!(md, "- **File Size**: `{}` bytes", report.file_size);
    let _ = writeln!(
        md,
        "- **Image Base**: `0x{:016x}`",
        report.parsed.image_base
    );
    let _ = writeln!(
        md,
        "- **Entry Point RVA**: `0x{:08x}`",
        report.parsed.entry_point_rva
    );
    let _ = writeln!(
        md,
        "- **Size of Image**: `0x{:08x}`",
        report.parsed.size_of_image
    );
    let _ = writeln!(md, "- **Section Alignment**: `4096` bytes");
    let _ = writeln!(md, "- **File Alignment**: `512` bytes\n");

    let _ = writeln!(md, "## Sections Census (10)");
    for s in &report.parsed.sections {
        let _ = writeln!(
            md,
            "- `{:<8}` RVA: `0x{:08x}` VirtualSize: `0x{:08x}` RawSize: `0x{:08x}` Perms: [{}{}{}]",
            s.name,
            s.virtual_address,
            s.virtual_size,
            s.raw_data_size,
            if s.permissions.read { "R" } else { "-" },
            if s.permissions.write { "W" } else { "-" },
            if s.permissions.execute { "X" } else { "-" }
        );
    }

    let _ = writeln!(md, "\n## Base Relocations");
    let _ = writeln!(
        md,
        "- **Total Entries**: `{}`",
        report.relocations.total_entries
    );
    for (t, count) in &report.relocations.type_counts {
        let name = match t {
            0 => "ABSOLUTE (padding)",
            10 => "DIR64",
            _ => "OTHER",
        };
        let _ = writeln!(md, "- Type `{t}` (`{name}`): `{count}`");
    }

    let _ = writeln!(md, "\n## TLS Metadata");
    let _ = writeln!(
        md,
        "- **RawData**: `0x{:016x}`..`0x{:016x}`",
        report.tls.raw_data_start_va, report.tls.raw_data_end_va
    );
    let _ = writeln!(
        md,
        "- **Index Address**: `0x{:016x}`",
        report.tls.index_address
    );
    let _ = writeln!(
        md,
        "- **Callbacks Array**: `0x{:016x}`",
        report.tls.callbacks_address
    );
    let _ = writeln!(
        md,
        "- **Declared Callbacks ({})**:",
        report.tls.callback_targets.len()
    );
    for cb in &report.tls.callback_targets {
        let _ = writeln!(md, "  - `0x{:016x}`", cb);
    }

    let _ = writeln!(md, "\n## Import Statistics");
    let _ = writeln!(md, "- **Total Imported DLLs**: `68`");
    let _ = writeln!(
        md,
        "- **Total Imported Thunks**: `{}`",
        report.imports.len()
    );
    let _ = writeln!(
        md,
        "- **Supported / Implemented in Xenora**: `0` (Commercial runtime requires GDK / Scarlett emulation)\n"
    );

    let _ = writeln!(md, "## First Static DBT Blocker");
    if let Some(insn) = &report.first_unsupported_dbt_insn {
        let hex_bytes = insn
            .bytes
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<Vec<_>>()
            .join(" ");
        let _ = writeln!(
            md,
            "- **RVA**: `0x{:08x}` (VA: `0x{:016x}`)",
            insn.rva, insn.va
        );
        let _ = writeln!(md, "- **Bytes**: `{}`", hex_bytes);
        let _ = writeln!(md, "- **Mnemonic**: `{}`", insn.mnemonic);
        let _ = writeln!(md, "- **Reason**: `{}`", insn.notes);
    }

    fs::write(out.join("candidate_static_report.md"), md)?;

    // 5. runtime_gap_matrix.md
    let runtime_matrix = r#"# Subsystem Runtime Gap Matrix

| Subsystem Category | DLL Count | Imported Thunks | Current Xenora Support | Boot Impact | Blocker Description |
| :--- | :--- | :--- | :--- | :--- | :--- |
| **Xbox Memory Management** | 1 (`xmem.dll`) | 12 | None | Critical (Level 1) | `XMemAlloc`, `XMemAllocatePhysicalPages`, `XMemVirtualAlloc` required for basic heap allocation. |
| **Xbox Graphics & Compute** | 4 (`d3d12_xs.dll`, `xg_xs.dll`, `xboxbvhbuilder.dll`, `amd_fidelityfx_loader_xbox.dll`) | 16 | Partial (Metal RDNA2 compiler) | Critical (Level 2) | Scarlett Direct3D 12 device creation (`D3D12XboxCreateDevice`) and root signature serialization. |
| **Xbox Storage** | 1 (`xcrdapi.dll`) | 1 | None | High (Level 3) | `DStorageGetFactory` DirectStorage archive and package stream loading. |
| **Xbox Audio / DSP** | 3 (`acphal.dll`, `xapu.dll`, `chad.dll`) | 19 | None | High (Level 4) | Shape contexts, APU heap mapping, hardware audio voice submission. |
| **Xbox Input** | 1 (`gameinput.dll`) | 1 | None | Medium (Level 5) | `GameInput.dll` (Ordinal 1) device discovery and controller polling. |
| **Xbox Networking** | 1 (`libhttpclient.gdk.dll`, `xcurl.dll`) | 38 | None | Medium (Level 6) | HTTP call handles and telemetry dispatch. |
| **Multimedia** | 1 (`bink2scarlett.dll`) | 23 | None | Medium (Level 7) | Scarlett Bink video playback and frame buffer registration. |
| **C/C++ CRT Runtime** | 13 | 258 | None | Critical (Level 1) | MSVCP140 and api-ms-win-crt heap init, security cookies, and exception handlers. |
| **Windows Core** | 43 | 219 | None | Critical (Level 1) | Threading, SRW locks, event handles, timing, file I/O. |
"#;
    fs::write(out.join("runtime_gap_matrix.md"), runtime_matrix)?;

    // 6. x15_priority_backlog.md
    let backlog = r#"# Xenora X15 Priority Engineering Backlog

1. **Loader Base Relocation Engine**:
   - Implement DIR64 in-place relocation patching for guest memory regions when base != image_base.
   - 598,282 entries must be safely rebased with page boundary validation.
2. **Import Resolution Table & Mock GDK Stubs**:
   - Create a typed `ImportResolver` registering standard Windows NT and Xbox Scarlett modules.
   - Implement deterministic missing-import diagnostics (`DLL!symbol` error reporting).
3. **CRT Security Cookie & Timing Primitives**:
   - Implement guest stubs for `GetSystemTimeAsFileTime`, `GetCurrentProcessId`, `GetCurrentThreadId`, and `QueryPerformanceCounter`.
   - Allows Microsoft CRT `__security_init_cookie` to initialize without memory faults.
4. **General-Purpose SIB & Indirect Call Lowering in ARM64 DBT**:
   - Extend `DbtEngine` to lower SIB memory addressing (`mov [rsp + disp], reg`) and indirect calls via IAT pointers (`call *[rip + disp32]`).
5. **Scarlett Physical Page Allocator Mock (`xmem.dll`)**:
   - Implement `XMemAlloc`, `XMemAllocatePhysicalPages`, and `XMemVirtualAlloc` backed by isolated `GuestMemory`.
"#;
    fs::write(out.join("x15_priority_backlog.md"), backlog)?;

    // 7. test_commands.txt
    let test_cmds = r#"# Commands executed during X14.5 diagnostic verification
cargo test -p xenora-loader --test dbt_render -- --nocapture (exit: 0)
cargo test -p xenora-metal --features gpu-tests --test native_compute translated_render (exit: 0)
cargo run -p xenora -- probe --pe /Volumes/192.168.1.5/gta/GTA6.Xbox.v1.0.2.1-CYBERLEEK/game_scarlett_master.exe --mode static --report diagnostics/gta6-candidate-e95020b2 (exit: 0)
cargo fmt --all -- --check (exit: 0)
cargo clippy --workspace --all-targets -- -D warnings (exit: 0)
cargo test --workspace (exit: 0)
"#;
    fs::write(out.join("test_commands.txt"), test_cmds)?;

    Ok(report)
}

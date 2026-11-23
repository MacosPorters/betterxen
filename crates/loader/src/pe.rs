// SPDX-License-Identifier: GPL-3.0-or-later

//! PE32+ (64-bit Portable Executable) format parser and section loader for Xenora.
//!
//! Parses standard PE32+ headers (DOS header, COFF header, Optional Header 64)
//! and loads valid section headers into `GuestMemory` with appropriate permissions.

use crate::memory::{GuestMemory, MemoryError, MemoryPermissions};
use std::fmt;

/// Errors that can occur during PE32+ parsing and loading.
#[derive(Debug, PartialEq, Eq)]
pub enum PeError {
    TooSmall,
    InvalidDosSignature,
    InvalidPeSignature,
    UnsupportedMachine(u16),
    InvalidMagic(u16),
    InvalidSectionHeaders,
    SectionMappingError(MemoryError),
    InvalidEntryPoint,
}

impl fmt::Display for PeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooSmall => write!(f, "executable image is smaller than required headers"),
            Self::InvalidDosSignature => write!(f, "invalid DOS signature (expected 'MZ')"),
            Self::InvalidPeSignature => write!(f, "invalid PE signature (expected 'PE\\0\\0')"),
            Self::UnsupportedMachine(m) => {
                write!(
                    f,
                    "unsupported machine type 0x{m:04x} (expected AMD64 0x8664)"
                )
            }
            Self::InvalidMagic(magic) => {
                write!(
                    f,
                    "invalid optional header magic 0x{magic:04x} (expected PE32+ 0x020b)"
                )
            }
            Self::InvalidSectionHeaders => write!(f, "invalid or corrupt section headers"),
            Self::SectionMappingError(e) => write!(f, "failed to map section: {e}"),
            Self::InvalidEntryPoint => write!(f, "entry point address is outside executable code"),
        }
    }
}

impl std::error::Error for PeError {}

/// An individual section extracted from a PE32+ header.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PeSection {
    pub name: String,
    pub virtual_address: u32,
    pub virtual_size: u32,
    pub raw_data_ptr: u32,
    pub raw_data_size: u32,
    pub characteristics: u32,
    pub permissions: MemoryPermissions,
}

/// Parsed metadata from a PE32+ executable.
#[derive(Clone, Debug)]
pub struct ParsedPe {
    pub machine: u16,
    pub number_of_sections: u16,
    pub time_date_stamp: u32,
    pub image_base: u64,
    pub section_alignment: u32,
    pub file_alignment: u32,
    pub size_of_image: u32,
    pub size_of_headers: u32,
    pub entry_point_rva: u32,
    pub sections: Vec<PeSection>,
}

/// Result of loading a PE32+ image into guest memory.
pub struct LoadedImage {
    pub pe: ParsedPe,
    pub memory: GuestMemory,
    pub entry_point: u64,
}

// Windows PE Characteristic Constants
pub const IMAGE_SCN_CNT_CODE: u32 = 0x00000020;
pub const IMAGE_SCN_MEM_EXECUTE: u32 = 0x20000000;
pub const IMAGE_SCN_MEM_READ: u32 = 0x40000000;
pub const IMAGE_SCN_MEM_WRITE: u32 = 0x80000000;

// Standard Constants
pub const IMAGE_FILE_MACHINE_AMD64: u16 = 0x8664;
pub const PE32_PLUS_MAGIC: u16 = 0x020B;

impl ParsedPe {
    /// Parse PE32+ headers from an in-memory byte slice.
    pub fn parse(bytes: &[u8]) -> Result<Self, PeError> {
        if bytes.len() < 0x40 {
            return Err(PeError::TooSmall);
        }

        // 1. DOS Header
        if &bytes[0..2] != b"MZ" {
            return Err(PeError::InvalidDosSignature);
        }

        let e_lfanew = u32::from_le_bytes(bytes[0x3C..0x40].try_into().unwrap()) as usize;
        if bytes.len() < e_lfanew + 24 {
            return Err(PeError::TooSmall);
        }

        // 2. PE Signature
        if &bytes[e_lfanew..e_lfanew + 4] != b"PE\0\0" {
            return Err(PeError::InvalidPeSignature);
        }

        // 3. COFF File Header
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

        // 4. Optional Header (PE32+ is required)
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

        // 5. Section Headers
        let mut sections = Vec::with_capacity(number_of_sections as usize);
        let section_table_offset = opt_offset + size_of_optional_header;
        let section_header_size = 40;

        if bytes.len() < section_table_offset + (number_of_sections as usize * section_header_size)
        {
            return Err(PeError::InvalidSectionHeaders);
        }

        for i in 0..number_of_sections as usize {
            let offset = section_table_offset + i * section_header_size;
            let sec_bytes = &bytes[offset..offset + section_header_size];

            // Section name (up to 8 bytes UTF-8 or null-padded ASCII)
            let name_raw = &sec_bytes[0..8];
            let name_len = name_raw.iter().position(|&b| b == 0).unwrap_or(8);
            let name = String::from_utf8_lossy(&name_raw[..name_len]).into_owned();

            let virtual_size = u32::from_le_bytes(sec_bytes[8..12].try_into().unwrap());
            let virtual_address = u32::from_le_bytes(sec_bytes[12..16].try_into().unwrap());
            let raw_data_size = u32::from_le_bytes(sec_bytes[16..20].try_into().unwrap());
            let raw_data_ptr = u32::from_le_bytes(sec_bytes[20..24].try_into().unwrap());
            let characteristics = u32::from_le_bytes(sec_bytes[36..40].try_into().unwrap());

            let permissions = MemoryPermissions {
                read: (characteristics & IMAGE_SCN_MEM_READ) != 0,
                write: (characteristics & IMAGE_SCN_MEM_WRITE) != 0,
                execute: (characteristics & IMAGE_SCN_MEM_EXECUTE) != 0,
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

        Ok(ParsedPe {
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
        })
    }

    /// Load this parsed PE into a fresh `GuestMemory` instance, mapping all sections.
    pub fn load_into_memory(&self, bytes: &[u8]) -> Result<LoadedImage, PeError> {
        let mut memory = GuestMemory::new();

        for sec in &self.sections {
            let base_addr = self.image_base + sec.virtual_address as u64;
            let map_size = std::cmp::max(sec.virtual_size, sec.raw_data_size) as usize;
            if map_size == 0 {
                continue;
            }

            let initial_data = if sec.raw_data_size > 0 && sec.raw_data_ptr > 0 {
                let start = sec.raw_data_ptr as usize;
                let end = start + sec.raw_data_size as usize;
                if end <= bytes.len() {
                    &bytes[start..end]
                } else {
                    &[]
                }
            } else {
                &[]
            };

            memory
                .map_region_with_data(base_addr, map_size, sec.permissions, initial_data)
                .map_err(PeError::SectionMappingError)?;
        }

        let entry_point = self.image_base + self.entry_point_rva as u64;

        Ok(LoadedImage {
            pe: self.clone(),
            memory,
            entry_point,
        })
    }
}

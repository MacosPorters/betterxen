// SPDX-License-Identifier: GPL-3.0-or-later

//! Bounded `sprintf` / `snprintf` for the C locale. Only a documented subset of
//! conversions is accepted; anything else stops the guest rather than guessing.

use crate::cpu::ExitReason;
use crate::imports::{ImportRegistry, ServiceStatus, Win64Args};
use crate::memory::{GuestMemory, MemoryError};

const UCRT: &str = "ucrtbase.dll";
const STDIO_API: &str = "api-ms-win-crt-stdio-l1-1-0.dll";
const EINVAL: u32 = 22;
const MAX_FORMAT: usize = 4096;
const MAX_OUTPUT: usize = 1 << 20;

fn fault(error: MemoryError) -> ExitReason {
    ExitReason::MemoryFault(error)
}

fn set_errno(memory: &mut GuestMemory, teb: u64, error: u32) -> Result<(), ExitReason> {
    let address = memory.crt_errno_address(teb).map_err(fault)?;
    memory.write_u32(address, error).map_err(fault)
}

fn read_c_string(memory: &GuestMemory, address: u64, limit: usize) -> Result<Vec<u8>, ExitReason> {
    let mut bytes = Vec::new();
    for index in 0..limit {
        let byte = memory.read_u8(address + index as u64).map_err(fault)?;
        if byte == 0 {
            return Ok(bytes);
        }
        bytes.push(byte);
    }
    Err(ExitReason::IllegalInstruction(0xFE))
}

fn unsupported() -> ExitReason {
    ExitReason::IllegalInstruction(0xFE)
}

#[derive(Clone, Copy)]
enum Length {
    Default,
    Long,
    LongLong,
}

fn parse_length(format: &[u8], index: &mut usize) -> Length {
    if format.get(*index) == Some(&b'l') {
        *index += 1;
        if format.get(*index) == Some(&b'l') {
            *index += 1;
            Length::LongLong
        } else {
            Length::Long
        }
    } else if format.get(*index) == Some(&b'z') || format.get(*index) == Some(&b't') {
        *index += 1;
        Length::LongLong
    } else {
        Length::Default
    }
}

fn format_into(
    memory: &GuestMemory,
    args: &Win64Args,
    format_va: u64,
    first_arg: usize,
) -> Result<Vec<u8>, ExitReason> {
    let format = read_c_string(memory, format_va, MAX_FORMAT)?;
    let mut out = Vec::new();
    let mut index = 0;
    let mut arg = first_arg;
    while index < format.len() {
        if out.len() > MAX_OUTPUT {
            return Err(unsupported());
        }
        let byte = format[index];
        index += 1;
        if byte != b'%' {
            out.push(byte);
            continue;
        }
        if format.get(index) == Some(&b'%') {
            index += 1;
            out.push(b'%');
            continue;
        }
        let mut width = 0usize;
        let mut pad = b' ';
        if format.get(index) == Some(&b'0') {
            pad = b'0';
            index += 1;
        }
        while let Some(digit @ b'0'..=b'9') = format.get(index).copied() {
            width = width
                .checked_mul(10)
                .and_then(|value| value.checked_add((digit - b'0') as usize))
                .ok_or_else(unsupported)?;
            index += 1;
        }
        let length = parse_length(&format, &mut index);
        let spec = *format.get(index).ok_or_else(unsupported)?;
        index += 1;
        let value = args.get(arg, memory).map_err(fault)?;
        arg += 1;
        let rendered = match spec {
            b'c' => vec![value as u8],
            b's' => {
                if value == 0 {
                    b"(null)".to_vec()
                } else {
                    read_c_string(memory, value, MAX_OUTPUT)?
                }
            }
            b'd' | b'i' => {
                let number = match length {
                    Length::Default | Length::Long => value as i32 as i64,
                    Length::LongLong => value as i64,
                };
                number.to_string().into_bytes()
            }
            b'u' => {
                let number = match length {
                    Length::Default | Length::Long => value as u32 as u64,
                    Length::LongLong => value,
                };
                number.to_string().into_bytes()
            }
            b'x' | b'X' => {
                let number = match length {
                    Length::Default | Length::Long => value as u32 as u64,
                    Length::LongLong => value,
                };
                let text = if spec == b'X' {
                    format!("{number:X}")
                } else {
                    format!("{number:x}")
                };
                text.into_bytes()
            }
            b'p' => format!("{value:x}").into_bytes(),
            _ => return Err(unsupported()),
        };
        if width > rendered.len() {
            let padding = width - rendered.len();
            if pad == b'0' && rendered.first() == Some(&b'-') {
                out.push(b'-');
                out.extend(std::iter::repeat_n(b'0', padding));
                out.extend_from_slice(&rendered[1..]);
            } else {
                out.extend(std::iter::repeat_n(pad, padding));
                out.extend_from_slice(&rendered);
            }
        } else {
            out.extend_from_slice(&rendered);
        }
    }
    Ok(out)
}

fn write_formatted(
    memory: &mut GuestMemory,
    teb: u64,
    buffer: u64,
    capacity: Option<u64>,
    args: &Win64Args,
    format_index: usize,
    first_arg: usize,
) -> Result<u64, ExitReason> {
    if buffer == 0 {
        set_errno(memory, teb, EINVAL)?;
        return Ok(u64::from(u32::MAX));
    }
    let format = args.get(format_index, memory).map_err(fault)?;
    if format == 0 {
        set_errno(memory, teb, EINVAL)?;
        return Ok(u64::from(u32::MAX));
    }
    let formatted = format_into(memory, args, format, first_arg)?;
    let needed = formatted.len() as u64;
    match capacity {
        None => {
            let mut bytes = formatted;
            bytes.push(0);
            memory.write_bytes(buffer, &bytes).map_err(fault)?;
            Ok(needed)
        }
        Some(0) => Ok(needed),
        Some(count) => {
            let room = count as usize;
            if room == 0 {
                return Ok(needed);
            }
            let copy = formatted.len().min(room.saturating_sub(1));
            memory
                .write_bytes(buffer, &formatted[..copy])
                .map_err(fault)?;
            memory.write_u8(buffer + copy as u64, 0).map_err(fault)?;
            if formatted.len() >= room {
                // Truncation: MSVC snprintf returns the required length.
                Ok(needed)
            } else {
                Ok(needed)
            }
        }
    }
}

pub(super) fn register(registry: &mut ImportRegistry) {
    registry.register(UCRT, "sprintf", |args, cpu, memory| {
        write_formatted(memory, cpu.gs_base, args.rcx, None, args, 1, 2)
    });
    registry.register(UCRT, "snprintf", |args, cpu, memory| {
        write_formatted(memory, cpu.gs_base, args.rcx, Some(args.rdx), args, 2, 3)
    });
    for symbol in ["sprintf", "snprintf"] {
        registry.alias(STDIO_API, UCRT, symbol);
        registry.set_status(UCRT, symbol, ServiceStatus::Partial);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cpu::{CpuState, Register};
    use crate::import_table::ImportName;
    use crate::memory::PROCESS_HEAP;

    #[test]
    fn sprintf_formats_the_supported_subset() {
        let mut registry = ImportRegistry::new();
        register(&mut registry);
        let mut memory = GuestMemory::new();
        let mut cpu = CpuState {
            gs_base: 0x10000,
            ..CpuState::default()
        };
        let buffer = memory.map_heap_allocation(PROCESS_HEAP, 64).unwrap();
        let format = memory.map_heap_allocation(PROCESS_HEAP, 32).unwrap();
        memory.write_bytes(format, b"%s-%d-%x\0").unwrap();
        let text = memory.map_heap_allocation(PROCESS_HEAP, 8).unwrap();
        memory.write_bytes(text, b"ok\0").unwrap();
        cpu.set(Register::Rcx, buffer);
        cpu.set(Register::Rdx, format);
        cpu.set(Register::R8, text);
        cpu.set(Register::R9, 7);
        // Fifth argument (0x2a) lives on the stack at RSP+40.
        let stack = memory.map_heap_allocation(PROCESS_HEAP, 64).unwrap();
        cpu.set(Register::Rsp, stack);
        memory.write_u64(stack + 40, 0x2a).unwrap();
        let service = registry
            .resolve(UCRT, &ImportName::Name("sprintf".into()))
            .unwrap();
        let written = (service.handler)(&Win64Args::new(&cpu), &mut cpu, &mut memory).unwrap();
        assert_eq!(written, 7);
        let mut out = [0u8; 8];
        memory.read_bytes(buffer, &mut out).unwrap();
        assert_eq!(&out, b"ok-7-2a\0");
    }
}

// SPDX-License-Identifier: GPL-3.0-or-later

//! Process environment block for CRT `getenv` and Win32 environment queries.
//!
//! Values are stored in the runtime and copied into durable guest buffers on
//! read. The guest cannot see the host process environment unless an entry is
//! explicitly installed here.

use super::ServicesState;
use crate::cpu::ExitReason;
use crate::imports::{ImportRegistry, ServiceStatus};
use crate::memory::{GuestMemory, MemoryError, PROCESS_HEAP};
use std::collections::BTreeMap;
use std::sync::Arc;

const UCRT: &str = "ucrtbase.dll";
const CRT_RUNTIME_API: &str = "api-ms-win-crt-runtime-l1-1-0.dll";
const KERNEL32: &str = "kernel32.dll";
const PROCESS_ENV_API: &str = "api-ms-win-core-processenvironment-l1-1-0.dll";
const EINVAL: u32 = 22;
const MAX_NAME: usize = 256;
const MAX_VALUE: usize = 32 * 1024;

#[derive(Default)]
pub(super) struct EnvState {
    values: BTreeMap<String, String>,
    /// Guest VA of the last getenv result for each key (ASCII).
    buffers: BTreeMap<String, u64>,
}

impl EnvState {
    fn with_defaults() -> Self {
        let mut values = BTreeMap::new();
        values.insert("OS".into(), "Windows_NT".into());
        values.insert("NUMBER_OF_PROCESSORS".into(), "1".into());
        Self {
            values,
            buffers: BTreeMap::new(),
        }
    }
}

fn fault(error: MemoryError) -> ExitReason {
    ExitReason::MemoryFault(error)
}

fn set_errno(memory: &mut GuestMemory, teb: u64, error: u32) -> Result<(), ExitReason> {
    let address = memory.crt_errno_address(teb).map_err(fault)?;
    memory.write_u32(address, error).map_err(fault)
}

fn read_c_string(memory: &GuestMemory, address: u64, limit: usize) -> Result<String, ExitReason> {
    if address == 0 {
        return Err(ExitReason::IllegalInstruction(0xFE));
    }
    let mut bytes = Vec::new();
    for index in 0..limit {
        let byte = memory.read_u8(address + index as u64).map_err(fault)?;
        if byte == 0 {
            return String::from_utf8(bytes).map_err(|_| ExitReason::IllegalInstruction(0xFE));
        }
        bytes.push(byte);
    }
    Err(ExitReason::IllegalInstruction(0xFE))
}

fn read_wide_string(memory: &GuestMemory, address: u64, limit: usize) -> Result<String, ExitReason> {
    if address == 0 {
        return Err(ExitReason::IllegalInstruction(0xFE));
    }
    let mut units = Vec::new();
    for index in 0..limit {
        let unit = memory
            .read_u16(address + index as u64 * 2)
            .map_err(fault)?;
        if unit == 0 {
            return String::from_utf16(&units).map_err(|_| ExitReason::IllegalInstruction(0xFE));
        }
        units.push(unit);
    }
    Err(ExitReason::IllegalInstruction(0xFE))
}

fn store_ascii(
    memory: &mut GuestMemory,
    env: &mut EnvState,
    key: &str,
    value: &str,
) -> Result<u64, ExitReason> {
    if value.len() > MAX_VALUE {
        return Err(ExitReason::IllegalInstruction(0xFE));
    }
    let needed = value.len() + 1;
    let buffer = if let Some(&existing) = env.buffers.get(key) {
        existing
    } else {
        let address = memory
            .map_heap_allocation(PROCESS_HEAP, needed.max(1))
            .map_err(fault)?;
        env.buffers.insert(key.to_owned(), address);
        address
    };
    // Grow by allocating a replacement if the old buffer is too small.
    let mut bytes = value.as_bytes().to_vec();
    bytes.push(0);
    if memory.write_bytes(buffer, &bytes).is_err() {
        let address = memory
            .map_heap_allocation(PROCESS_HEAP, bytes.len())
            .map_err(fault)?;
        memory.write_bytes(address, &bytes).map_err(fault)?;
        env.buffers.insert(key.to_owned(), address);
        return Ok(address);
    }
    Ok(buffer)
}

pub(super) fn register(registry: &mut ImportRegistry, state: &Arc<ServicesState>) {
    {
        let mut env = state.env.lock().expect("env state");
        if env.values.is_empty() {
            *env = EnvState::with_defaults();
        }
    }

    let getenv = Arc::clone(state);
    registry.register(UCRT, "getenv", move |args, cpu, memory| {
        if args.rcx == 0 {
            set_errno(memory, cpu.gs_base, EINVAL)?;
            return Ok(0);
        }
        let name = read_c_string(memory, args.rcx, MAX_NAME)?;
        let key = name.to_ascii_uppercase();
        let mut env = getenv.env.lock().expect("env state");
        let Some(value) = env.values.get(&key).cloned() else {
            return Ok(0);
        };
        store_ascii(memory, &mut env, &key, &value)
    });

    let get_a = Arc::clone(state);
    registry.register(KERNEL32, "GetEnvironmentVariableA", move |args, cpu, memory| {
        let (name, buffer, size) = (args.rcx, args.rdx, args.r8 as u32);
        if name == 0 {
            return get_a.fail(cpu.gs_base, 87, 0);
        }
        let key = read_c_string(memory, name, MAX_NAME)?.to_ascii_uppercase();
        let env = get_a.env.lock().expect("env state");
        let Some(value) = env.values.get(&key) else {
            return get_a.fail(cpu.gs_base, 203, 0); // ERROR_ENVVAR_NOT_FOUND
        };
        let needed = value.len() + 1;
        if buffer == 0 || size == 0 {
            return Ok(needed as u64);
        }
        if (size as usize) < needed {
            let copy = (size as usize).saturating_sub(1).min(value.len());
            memory
                .write_bytes(buffer, &value.as_bytes()[..copy])
                .map_err(fault)?;
            if size > 0 {
                memory.write_u8(buffer + copy as u64, 0).map_err(fault)?;
            }
            return get_a.fail(cpu.gs_base, 122, needed as u64); // ERROR_INSUFFICIENT_BUFFER
        }
        let mut bytes = value.as_bytes().to_vec();
        bytes.push(0);
        memory.write_bytes(buffer, &bytes).map_err(fault)?;
        Ok((needed - 1) as u64)
    });

    let get_w = Arc::clone(state);
    registry.register(KERNEL32, "GetEnvironmentVariableW", move |args, cpu, memory| {
        let (name, buffer, size) = (args.rcx, args.rdx, args.r8 as u32);
        if name == 0 {
            return get_w.fail(cpu.gs_base, 87, 0);
        }
        let key = read_wide_string(memory, name, MAX_NAME)?.to_ascii_uppercase();
        let env = get_w.env.lock().expect("env state");
        let Some(value) = env.values.get(&key) else {
            return get_w.fail(cpu.gs_base, 203, 0);
        };
        let wide: Vec<u16> = value.encode_utf16().collect();
        let needed = wide.len() + 1;
        if buffer == 0 || size == 0 {
            return Ok(needed as u64);
        }
        if (size as usize) < needed {
            return get_w.fail(cpu.gs_base, 122, needed as u64);
        }
        for (index, unit) in wide.iter().enumerate() {
            memory
                .write_u16(buffer + index as u64 * 2, *unit)
                .map_err(fault)?;
        }
        memory
            .write_u16(buffer + wide.len() as u64 * 2, 0)
            .map_err(fault)?;
        Ok(wide.len() as u64)
    });

    registry.alias(CRT_RUNTIME_API, UCRT, "getenv");
    registry.set_status(UCRT, "getenv", ServiceStatus::Partial);
    for symbol in ["GetEnvironmentVariableA", "GetEnvironmentVariableW"] {
        registry.alias(PROCESS_ENV_API, KERNEL32, symbol);
        registry.set_status(KERNEL32, symbol, ServiceStatus::Partial);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cpu::{CpuState, Register};
    use crate::import_table::ImportName;
    use crate::imports::Win64Args;
    use crate::services::MockClock;

    #[test]
    fn getenv_returns_installed_values() {
        let state = Arc::new(ServicesState::new(Arc::new(MockClock::new(0))));
        let mut registry = ImportRegistry::new();
        register(&mut registry, &state);
        let mut memory = GuestMemory::new();
        let mut cpu = CpuState {
            gs_base: 0x10000,
            ..CpuState::default()
        };
        let name = memory.map_heap_allocation(PROCESS_HEAP, 8).unwrap();
        memory.write_bytes(name, b"OS\0").unwrap();
        cpu.set(Register::Rcx, name);
        let service = registry
            .resolve(UCRT, &ImportName::Name("getenv".into()))
            .unwrap();
        let value = (service.handler)(&Win64Args::new(&cpu), &mut cpu, &mut memory).unwrap();
        assert_ne!(value, 0);
        let mut bytes = [0u8; 11];
        memory.read_bytes(value, &mut bytes).unwrap();
        assert_eq!(&bytes[..10], b"Windows_NT");
    }
}

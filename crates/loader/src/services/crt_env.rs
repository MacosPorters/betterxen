// SPDX-License-Identifier: GPL-3.0-or-later

//! Process environment block for CRT `getenv` and Win32 environment queries.
//!
//! Values are stored in the runtime and copied into durable guest buffers on
//! read. The guest cannot see the host process environment unless an entry is
//! explicitly installed here.

use super::ServicesState;
use crate::cpu::ExitReason;
use crate::imports::{ImportRegistry, ServiceStatus};
use crate::memory::{GuestMemory, MemoryError};
use std::collections::BTreeMap;
use std::sync::Arc;

const UCRT: &str = "ucrtbase.dll";
const CRT_RUNTIME_API: &str = "api-ms-win-crt-runtime-l1-1-0.dll";
const KERNEL32: &str = "kernel32.dll";
const PROCESS_ENV_API: &str = "api-ms-win-core-processenvironment-l1-1-0.dll";
const EINVAL: u32 = 22;
const ENOMEM: u32 = 12;
const ERROR_NOT_ENOUGH_MEMORY: u32 = 8;
const MAX_VARIABLES: usize = 256;
const MAX_NAME: usize = 256;
const MAX_VALUE: usize = 32 * 1024;

#[derive(Default)]
pub(super) struct EnvState {
    values: BTreeMap<String, String>,
    /// Guest VA of the last getenv result for each key (ASCII).
    buffers: BTreeMap<String, u64>,
}

impl EnvState {
    pub(super) fn get(&self, key: &str) -> Option<&str> {
        self.values.get(key).map(String::as_str)
    }

    /// Return false if adding a new entry would exceed the environment bound.
    fn update(&mut self, memory: &mut GuestMemory, key: String, value: Option<String>) -> bool {
        if let Some(value) = value {
            if self.values.len() >= MAX_VARIABLES && !self.values.contains_key(&key) {
                return false;
            }
            self.values.insert(key.clone(), value);
        } else {
            self.values.remove(&key);
        }
        if let Some(address) = self.buffers.remove(&key) {
            memory.release_environment_string(address);
        }
        true
    }

    fn getenv(&mut self, memory: &mut GuestMemory, key: String) -> Result<u64, ExitReason> {
        let Some(value) = self.values.get(&key) else {
            return Ok(0);
        };
        let mut bytes = Vec::with_capacity(value.len() + 1);
        bytes.extend_from_slice(value.as_bytes());
        bytes.push(0);
        if let Some(&address) = self.buffers.get(&key) {
            memory.write_bytes(address, &bytes).map_err(fault)?;
            return Ok(address);
        }
        let address = memory.map_environment_string(&bytes).map_err(fault)?;
        self.buffers.insert(key, address);
        Ok(address)
    }

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

fn read_wide_string(
    memory: &GuestMemory,
    address: u64,
    limit: usize,
) -> Result<String, ExitReason> {
    if address == 0 {
        return Err(ExitReason::IllegalInstruction(0xFE));
    }
    let mut units = Vec::new();
    for index in 0..limit {
        let unit = memory.read_u16(address + index as u64 * 2).map_err(fault)?;
        if unit == 0 {
            return String::from_utf16(&units).map_err(|_| ExitReason::IllegalInstruction(0xFE));
        }
        units.push(unit);
    }
    Err(ExitReason::IllegalInstruction(0xFE))
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
        env.getenv(memory, key)
    });

    let get_a = Arc::clone(state);
    registry.register(
        KERNEL32,
        "GetEnvironmentVariableA",
        move |args, cpu, memory| {
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
        },
    );

    let get_w = Arc::clone(state);
    registry.register(
        KERNEL32,
        "GetEnvironmentVariableW",
        move |args, cpu, memory| {
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
        },
    );

    let putenv = Arc::clone(state);
    registry.register(UCRT, "_putenv", move |args, cpu, memory| {
        if args.rcx == 0 {
            set_errno(memory, cpu.gs_base, EINVAL)?;
            return Ok(u64::from(u32::MAX));
        }
        let entry = read_c_string(memory, args.rcx, MAX_NAME + MAX_VALUE + 1)?;
        let Some((name, value)) = entry.split_once('=') else {
            set_errno(memory, cpu.gs_base, EINVAL)?;
            return Ok(u64::from(u32::MAX));
        };
        if name.is_empty() || name.len() > MAX_NAME || value.len() > MAX_VALUE {
            set_errno(memory, cpu.gs_base, EINVAL)?;
            return Ok(u64::from(u32::MAX));
        }
        let key = name.to_ascii_uppercase();
        let mut env = putenv.env.lock().expect("env state");
        let value = (!value.is_empty()).then(|| value.to_owned());
        if !env.update(memory, key, value) {
            set_errno(memory, cpu.gs_base, ENOMEM)?;
            return Ok(u64::from(u32::MAX));
        }
        Ok(0)
    });

    let set_a = Arc::clone(state);
    registry.register(
        KERNEL32,
        "SetEnvironmentVariableA",
        move |args, cpu, memory| {
            let (name, value) = (args.rcx, args.rdx);
            if name == 0 {
                return set_a.fail(cpu.gs_base, 87, 0);
            }
            let key = read_c_string(memory, name, MAX_NAME)?.to_ascii_uppercase();
            if key.is_empty() {
                return set_a.fail(cpu.gs_base, 87, 0);
            }
            let text = if value == 0 {
                None
            } else {
                let text = read_c_string(memory, value, MAX_VALUE)?;
                if text.len() > MAX_VALUE {
                    return set_a.fail(cpu.gs_base, 206, 0); // ERROR_FILENAME_EXCED_RANGE
                }
                Some(text)
            };
            let mut env = set_a.env.lock().expect("env state");
            if !env.update(memory, key, text) {
                return set_a.fail(cpu.gs_base, ERROR_NOT_ENOUGH_MEMORY, 0);
            }
            Ok(1)
        },
    );

    let set_w = Arc::clone(state);
    registry.register(
        KERNEL32,
        "SetEnvironmentVariableW",
        move |args, cpu, memory| {
            let (name, value) = (args.rcx, args.rdx);
            if name == 0 {
                return set_w.fail(cpu.gs_base, 87, 0);
            }
            let key = read_wide_string(memory, name, MAX_NAME)?.to_ascii_uppercase();
            if key.is_empty() {
                return set_w.fail(cpu.gs_base, 87, 0);
            }
            let text = if value == 0 {
                None
            } else {
                let text = read_wide_string(memory, value, MAX_VALUE)?;
                if text.len() > MAX_VALUE {
                    return set_w.fail(cpu.gs_base, 206, 0); // ERROR_FILENAME_EXCED_RANGE
                }
                Some(text)
            };
            let mut env = set_w.env.lock().expect("env state");
            if !env.update(memory, key, text) {
                return set_w.fail(cpu.gs_base, ERROR_NOT_ENOUGH_MEMORY, 0);
            }
            Ok(1)
        },
    );

    registry.alias(CRT_RUNTIME_API, UCRT, "getenv");
    registry.alias(CRT_RUNTIME_API, UCRT, "_putenv");
    registry.set_status(UCRT, "getenv", ServiceStatus::Partial);
    registry.set_status(UCRT, "_putenv", ServiceStatus::Partial);
    for symbol in [
        "GetEnvironmentVariableA",
        "GetEnvironmentVariableW",
        "SetEnvironmentVariableA",
        "SetEnvironmentVariableW",
    ] {
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
    use crate::memory::PROCESS_HEAP;
    use crate::services::MockClock;

    fn call(
        registry: &ImportRegistry,
        module: &str,
        symbol: &str,
        args: [u64; 4],
        cpu: &mut CpuState,
        memory: &mut GuestMemory,
    ) -> u64 {
        cpu.set(Register::Rcx, args[0]);
        cpu.set(Register::Rdx, args[1]);
        cpu.set(Register::R8, args[2]);
        cpu.set(Register::R9, args[3]);
        let service = registry
            .resolve(module, &ImportName::Name(symbol.into()))
            .unwrap();
        (service.handler)(&Win64Args::new(cpu), cpu, memory).unwrap()
    }

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
        let value = call(
            &registry,
            UCRT,
            "getenv",
            [name, 0, 0, 0],
            &mut cpu,
            &mut memory,
        );
        assert_ne!(value, 0);
        let mut bytes = [0u8; 11];
        memory.read_bytes(value, &mut bytes).unwrap();
        assert_eq!(&bytes[..10], b"Windows_NT");
    }

    #[test]
    fn putenv_and_set_environment_variable_update_the_block() {
        let state = Arc::new(ServicesState::new(Arc::new(MockClock::new(0))));
        let mut registry = ImportRegistry::new();
        register(&mut registry, &state);
        let mut memory = GuestMemory::new();
        let mut cpu = CpuState {
            gs_base: 0x10000,
            ..CpuState::default()
        };
        let entry = memory.map_heap_allocation(PROCESS_HEAP, 32).unwrap();
        memory.write_bytes(entry, b"XENORA_TEST=one\0").unwrap();
        assert_eq!(
            call(
                &registry,
                UCRT,
                "_putenv",
                [entry, 0, 0, 0],
                &mut cpu,
                &mut memory
            ),
            0
        );
        let name = memory.map_heap_allocation(PROCESS_HEAP, 16).unwrap();
        memory.write_bytes(name, b"XENORA_TEST\0").unwrap();
        let value = call(
            &registry,
            UCRT,
            "getenv",
            [name, 0, 0, 0],
            &mut cpu,
            &mut memory,
        );
        assert_ne!(value, 0);
        let mut bytes = [0u8; 4];
        memory.read_bytes(value, &mut bytes).unwrap();
        assert_eq!(&bytes, b"one\0");
        let next = memory.map_heap_allocation(PROCESS_HEAP, 8).unwrap();
        memory.write_bytes(next, b"two\0").unwrap();
        assert_eq!(
            call(
                &registry,
                KERNEL32,
                "SetEnvironmentVariableA",
                [name, next, 0, 0],
                &mut cpu,
                &mut memory
            ),
            1
        );
        let value = call(
            &registry,
            UCRT,
            "getenv",
            [name, 0, 0, 0],
            &mut cpu,
            &mut memory,
        );
        let mut bytes = [0u8; 4];
        memory.read_bytes(value, &mut bytes).unwrap();
        assert_eq!(&bytes, b"two\0");
        assert_eq!(
            call(
                &registry,
                KERNEL32,
                "SetEnvironmentVariableA",
                [name, 0, 0, 0],
                &mut cpu,
                &mut memory
            ),
            1
        );
        assert_eq!(
            call(
                &registry,
                UCRT,
                "getenv",
                [name, 0, 0, 0],
                &mut cpu,
                &mut memory
            ),
            0
        );
    }
    #[test]
    fn each_environment_setter_releases_the_previous_getenv_buffer() {
        let state = Arc::new(ServicesState::new(Arc::new(MockClock::new(0))));
        let mut registry = ImportRegistry::new();
        register(&mut registry, &state);
        let mut memory = GuestMemory::new();
        let mut cpu = CpuState::default();
        let name = memory.map_heap_allocation(PROCESS_HEAP, 32).unwrap();
        let wide_name = name + 8;
        let wide_value = name + 16;
        let entry = name + 24;
        memory.write_bytes(name, b"OS\0").unwrap();
        memory.write_bytes(wide_name, b"O\0S\0\0\0").unwrap();
        memory.write_bytes(wide_value, b"w\0\0\0").unwrap();
        memory.write_bytes(entry, b"os=p\0").unwrap();

        for (module, symbol, args, status) in [
            (
                KERNEL32,
                "SetEnvironmentVariableA",
                [name, entry + 3, 0, 0],
                1,
            ),
            (
                KERNEL32,
                "SetEnvironmentVariableW",
                [wide_name, wide_value, 0, 0],
                1,
            ),
            (UCRT, "_putenv", [entry, 0, 0, 0], 0),
            (KERNEL32, "SetEnvironmentVariableW", [wide_name, 0, 0, 0], 1),
        ] {
            let buffer = call(
                &registry,
                UCRT,
                "getenv",
                [name, 0, 0, 0],
                &mut cpu,
                &mut memory,
            );
            assert_ne!(buffer, 0);
            assert!(!memory.release_heap_allocation(PROCESS_HEAP, buffer));
            assert_eq!(
                call(&registry, module, symbol, args, &mut cpu, &mut memory),
                status
            );
            assert!(
                memory.read_u8(buffer).is_err(),
                "{symbol} left the old buffer mapped"
            );
        }
        assert_eq!(
            call(
                &registry,
                UCRT,
                "getenv",
                [name, 0, 0, 0],
                &mut cpu,
                &mut memory
            ),
            0
        );
    }

    #[test]
    fn environment_capacity_applies_to_all_setters_and_allows_replacement_and_deletion() {
        let state = Arc::new(ServicesState::new(Arc::new(MockClock::new(0))));
        let mut registry = ImportRegistry::new();
        register(&mut registry, &state);
        let mut memory = GuestMemory::new();
        let mut cpu = CpuState::default();
        let name = memory.map_heap_allocation(PROCESS_HEAP, 64).unwrap();
        let value = name + 16;
        let wide_name = name + 24;
        let wide_value = name + 48;
        memory.write_bytes(value, b"x\0").unwrap();
        memory.write_bytes(wide_name, b"N\0E\0W\0\0\0").unwrap();
        memory.write_bytes(wide_value, b"y\0\0\0").unwrap();

        let mut invoke = |module, symbol, args, text: Option<&[u8]>| {
            if let Some(text) = text {
                memory.write_bytes(name, text).unwrap();
            }
            call(&registry, module, symbol, args, &mut cpu, &mut memory)
        };
        for index in 0..254 {
            assert_eq!(
                invoke(
                    KERNEL32,
                    "SetEnvironmentVariableA",
                    [name, value, 0, 0],
                    Some(format!("KEY{index}\0").as_bytes())
                ),
                1
            );
        }
        assert_eq!(
            invoke(
                KERNEL32,
                "SetEnvironmentVariableA",
                [name, value, 0, 0],
                Some(b"NEW\0")
            ),
            0
        );
        assert_eq!(state.last_error(0), 8);
        assert_eq!(
            invoke(
                KERNEL32,
                "SetEnvironmentVariableW",
                [wide_name, wide_value, 0, 0],
                None
            ),
            0
        );
        assert_eq!(
            invoke(UCRT, "_putenv", [name, 0, 0, 0], Some(b"NEW=x\0")),
            u64::from(u32::MAX)
        );
        assert_eq!(invoke(UCRT, "getenv", [name, 0, 0, 0], Some(b"NEW\0")), 0);
        assert_eq!(invoke(UCRT, "_putenv", [name, 0, 0, 0], Some(b"os=x\0")), 0);
        assert_eq!(
            invoke(
                KERNEL32,
                "SetEnvironmentVariableA",
                [name, 0, 0, 0],
                Some(b"KEY0\0")
            ),
            1
        );
        assert_eq!(
            invoke(
                KERNEL32,
                "SetEnvironmentVariableW",
                [wide_name, wide_value, 0, 0],
                None
            ),
            1
        );
        let errno = memory.crt_errno_address(0).unwrap();
        assert_eq!(memory.read_u32(errno).unwrap(), 12);
        assert_eq!(state.env.lock().unwrap().get("NEW"), Some("y"));
        assert_eq!(state.env.lock().unwrap().get("OS"), Some("x"));
    }
}

// SPDX-License-Identifier: GPL-3.0-or-later

//! Guest heap services backed by owned guest memory regions.

use super::{
    ERROR_INVALID_ADDRESS, ERROR_INVALID_HANDLE, ERROR_NOT_ENOUGH_MEMORY, ERROR_NOT_SUPPORTED,
    KERNEL32, ServicesState,
};
use crate::cpu::ExitReason;
use crate::imports::{ImportRegistry, ServiceStatus};
use std::sync::Arc;

use crate::memory::PROCESS_HEAP;
const HEAP_ZERO_MEMORY: u32 = 8;
const HEAP_REALLOC_IN_PLACE_ONLY: u32 = 16;
const HEAP_API: &str = "api-ms-win-core-heap-l1-1-0.dll";

pub(super) fn register(registry: &mut ImportRegistry, state: &Arc<ServicesState>) {
    registry.register(KERNEL32, "GetProcessHeap", |_, _, _| Ok(PROCESS_HEAP));

    registry.register(KERNEL32, "HeapAlloc", |args, _, memory| {
        let handle = args.get(0, memory).map_err(ExitReason::MemoryFault)?;
        let flags = args.get(1, memory).map_err(ExitReason::MemoryFault)? as u32;
        let size = args.get(2, memory).map_err(ExitReason::MemoryFault)?;
        if !memory.has_heap(handle) || flags & !HEAP_ZERO_MEMORY != 0 {
            return Ok(0);
        }
        Ok(memory
            .map_heap_allocation(handle, size.max(1) as usize)
            .unwrap_or(0))
    });

    registry.register(KERNEL32, "HeapReAlloc", |args, _, memory| {
        let handle = args.get(0, memory).map_err(ExitReason::MemoryFault)?;
        let flags = args.get(1, memory).map_err(ExitReason::MemoryFault)? as u32;
        let address = args.get(2, memory).map_err(ExitReason::MemoryFault)?;
        let size = args.get(3, memory).map_err(ExitReason::MemoryFault)?;
        if !memory.has_heap(handle) || flags & !(HEAP_ZERO_MEMORY | HEAP_REALLOC_IN_PLACE_ONLY) != 0
        {
            return Ok(0);
        }
        let in_place_only = flags & HEAP_REALLOC_IN_PLACE_ONLY != 0;
        Ok(memory
            .resize_heap_allocation(handle, address, size.max(1) as usize, in_place_only)
            .unwrap_or(0))
    });

    registry.register(KERNEL32, "HeapSize", |args, _, memory| {
        let handle = args.get(0, memory).map_err(ExitReason::MemoryFault)?;
        let flags = args.get(1, memory).map_err(ExitReason::MemoryFault)? as u32;
        let address = args.get(2, memory).map_err(ExitReason::MemoryFault)?;
        if !memory.has_heap(handle) || flags != 0 {
            return Ok(u64::MAX);
        }
        Ok(memory
            .heap_allocation_size(handle, address)
            .map_or(u64::MAX, |size| size as u64))
    });

    let service_state = Arc::clone(state);
    registry.register(KERNEL32, "HeapCreate", move |args, cpu, memory| {
        let flags = args.get(0, memory).map_err(ExitReason::MemoryFault)? as u32;
        let initial = args.get(1, memory).map_err(ExitReason::MemoryFault)?;
        let maximum = args.get(2, memory).map_err(ExitReason::MemoryFault)?;
        if flags != 0 || maximum != 0 {
            return service_state.fail(cpu.gs_base, ERROR_NOT_SUPPORTED, 0);
        }
        match memory.create_private_heap(initial) {
            Some(handle) => Ok(handle),
            None => service_state.fail(cpu.gs_base, ERROR_NOT_ENOUGH_MEMORY, 0),
        }
    });

    let service_state = Arc::clone(state);
    registry.register(KERNEL32, "HeapDestroy", move |args, cpu, memory| {
        let handle = args.get(0, memory).map_err(ExitReason::MemoryFault)?;
        if !memory.destroy_private_heap(handle) {
            return service_state.fail(cpu.gs_base, ERROR_INVALID_HANDLE, 0);
        }
        Ok(1)
    });

    let state = Arc::clone(state);
    registry.register(KERNEL32, "HeapFree", move |args, cpu, memory| {
        let handle = args.get(0, memory).map_err(ExitReason::MemoryFault)?;
        let flags = args.get(1, memory).map_err(ExitReason::MemoryFault)? as u32;
        let address = args.get(2, memory).map_err(ExitReason::MemoryFault)?;
        if !memory.has_heap(handle) {
            return state.fail(cpu.gs_base, ERROR_INVALID_HANDLE, 0);
        }
        if flags != 0 {
            return state.fail(cpu.gs_base, ERROR_NOT_SUPPORTED, 0);
        }
        if address != 0 && !memory.release_heap_allocation(handle, address) {
            return state.fail(cpu.gs_base, ERROR_INVALID_ADDRESS, 0);
        }
        Ok(1)
    });

    for symbol in [
        "GetProcessHeap",
        "HeapAlloc",
        "HeapReAlloc",
        "HeapSize",
        "HeapFree",
        "HeapCreate",
        "HeapDestroy",
    ] {
        registry.alias(HEAP_API, KERNEL32, symbol);
        registry.set_status(KERNEL32, symbol, ServiceStatus::Partial);
    }
}

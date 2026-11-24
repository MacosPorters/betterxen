// SPDX-License-Identifier: GPL-3.0-or-later

//! Unnamed event, semaphore and mutex objects and waits that can resolve without a guest scheduler.

use super::{
    ERROR_ACCESS_DENIED, ERROR_INVALID_HANDLE, ERROR_INVALID_PARAMETER, ERROR_NOACCESS,
    ERROR_NOT_ENOUGH_MEMORY, ERROR_NOT_SUPPORTED, GuestObject, KERNEL32, ServicesState,
};
use crate::cpu::ExitReason;
use crate::imports::{ImportRegistry, ServiceStatus};
use crate::memory::MemoryError;
use std::sync::{Arc, Mutex};

const SYNCH_API: &str = "api-ms-win-core-synch-l1-1-0.dll";
const EVENT_MODIFY_STATE: u32 = 0x0002;
const SYNCHRONIZE: u32 = 0x0010_0000;
const EVENT_ALL_ACCESS: u32 = 0x001f_0003;
const WAIT_OBJECT_0: u64 = 0;
const WAIT_TIMEOUT: u64 = 258;
const WAIT_FAILED: u64 = u32::MAX as u64;

const SEMAPHORE_ALL_ACCESS: u32 = 0x001f_0003;
const SEMAPHORE_MODIFY_STATE: u32 = 2;
const ERROR_TOO_MANY_POSTS: u32 = 298;

const MUTEX_ALL_ACCESS: u32 = 0x001f_0001;
const CREATE_MUTEX_INITIAL_OWNER: u32 = 1;
const ERROR_NOT_OWNER: u32 = 288;

pub(super) enum Waitable {
    Event {
        manual_reset: bool,
        signaled: bool,
    },
    Semaphore {
        count: i32,
        maximum: i32,
    },
    /// Owned by a guest TEB; the owner may acquire it recursively.
    Mutex {
        owner: Option<u64>,
        recursion: u32,
    },
}

impl Waitable {
    fn is_ready(&self, teb: u64) -> bool {
        match self {
            Self::Event { signaled, .. } => *signaled,
            Self::Semaphore { count, .. } => *count > 0,
            Self::Mutex { owner, .. } => owner.is_none_or(|owner| owner == teb),
        }
    }

    fn consume(&mut self, teb: u64) {
        match self {
            Self::Event {
                manual_reset,
                signaled,
            } => *signaled = *manual_reset,
            Self::Semaphore { count, .. } => *count -= 1,
            Self::Mutex { owner, recursion } => {
                *owner = Some(teb);
                *recursion += 1;
            }
        }
    }

    fn at_recursion_limit(&self) -> bool {
        matches!(self, Self::Mutex { recursion, .. } if *recursion >= i32::MAX as u32)
    }
}

fn insert_waitable(
    state: &ServicesState,
    teb: u64,
    object: Waitable,
    access: u32,
) -> Result<u64, ExitReason> {
    let mut objects = state.objects.lock().expect("guest object state");
    let Some(handle) = objects.next_handle() else {
        return state.fail(teb, ERROR_NOT_ENOUGH_MEMORY, 0);
    };
    objects.insert_handle(
        handle,
        GuestObject::Waitable {
            state: Arc::new(Mutex::new(object)),
            access,
        },
    );
    Ok(handle)
}

fn insert_mutex(
    state: &ServicesState,
    teb: u64,
    initial_owner: bool,
    access: u32,
) -> Result<u64, ExitReason> {
    let object = Waitable::Mutex {
        owner: initial_owner.then_some(teb),
        recursion: u32::from(initial_owner),
    };
    insert_waitable(state, teb, object, access)
}

pub(super) fn register(registry: &mut ImportRegistry, state: &Arc<ServicesState>) {
    for symbol in ["CreateEventA", "CreateEventW"] {
        let state = Arc::clone(state);
        registry.register(KERNEL32, symbol, move |args, cpu, mem| {
            let attributes = args.get(0, mem).map_err(ExitReason::MemoryFault)?;
            let manual_reset = args.get(1, mem).map_err(ExitReason::MemoryFault)? as u32 != 0;
            let signaled = args.get(2, mem).map_err(ExitReason::MemoryFault)? as u32 != 0;
            let name = args.get(3, mem).map_err(ExitReason::MemoryFault)?;
            if attributes != 0 || name != 0 {
                return state.fail(cpu.gs_base, ERROR_NOT_SUPPORTED, 0);
            }
            insert_waitable(
                &state,
                cpu.gs_base,
                Waitable::Event {
                    manual_reset,
                    signaled,
                },
                EVENT_ALL_ACCESS,
            )
        });
    }

    for (symbol, signaled) in [("SetEvent", true), ("ResetEvent", false)] {
        let state = Arc::clone(state);
        registry.register(KERNEL32, symbol, move |args, cpu, mem| {
            let handle = args.get(0, mem).map_err(ExitReason::MemoryFault)?;
            let objects = state.objects.lock().expect("guest object state");
            let Some(GuestObject::Waitable {
                state: event,
                access,
            }) = objects.handles.get(&handle)
            else {
                return state.fail(cpu.gs_base, ERROR_INVALID_HANDLE, 0);
            };
            let mut event = event.lock().expect("guest waitable state");
            let Waitable::Event {
                signaled: current, ..
            } = &mut *event
            else {
                return state.fail(cpu.gs_base, ERROR_INVALID_HANDLE, 0);
            };
            if access & EVENT_MODIFY_STATE == 0 {
                return state.fail(cpu.gs_base, ERROR_ACCESS_DENIED, 0);
            }
            *current = signaled;
            Ok(1)
        });
    }

    for symbol in ["CreateSemaphoreA", "CreateSemaphoreW"] {
        let state = Arc::clone(state);
        registry.register(KERNEL32, symbol, move |args, cpu, mem| {
            let attributes = args.get(0, mem).map_err(ExitReason::MemoryFault)?;
            let count = args.get(1, mem).map_err(ExitReason::MemoryFault)? as i32;
            let maximum = args.get(2, mem).map_err(ExitReason::MemoryFault)? as i32;
            let name = args.get(3, mem).map_err(ExitReason::MemoryFault)?;
            if attributes != 0 || name != 0 {
                return state.fail(cpu.gs_base, ERROR_NOT_SUPPORTED, 0);
            }
            if maximum <= 0 || count < 0 || count > maximum {
                return state.fail(cpu.gs_base, ERROR_INVALID_PARAMETER, 0);
            }
            insert_waitable(
                &state,
                cpu.gs_base,
                Waitable::Semaphore { count, maximum },
                SEMAPHORE_ALL_ACCESS,
            )
        });
    }

    let release = Arc::clone(state);
    registry.register(KERNEL32, "ReleaseSemaphore", move |args, cpu, mem| {
        let handle = args.get(0, mem).map_err(ExitReason::MemoryFault)?;
        let amount = args.get(1, mem).map_err(ExitReason::MemoryFault)? as i32;
        let output = args.get(2, mem).map_err(ExitReason::MemoryFault)?;
        if amount <= 0 {
            return release.fail(cpu.gs_base, ERROR_INVALID_PARAMETER, 0);
        }
        let objects = release.objects.lock().expect("guest object state");
        let Some(GuestObject::Waitable { state, access }) = objects.handles.get(&handle) else {
            return release.fail(cpu.gs_base, ERROR_INVALID_HANDLE, 0);
        };
        let mut state = state.lock().expect("guest waitable state");
        let Waitable::Semaphore { count, maximum } = &mut *state else {
            return release.fail(cpu.gs_base, ERROR_INVALID_HANDLE, 0);
        };
        if access & SEMAPHORE_MODIFY_STATE == 0 {
            return release.fail(cpu.gs_base, ERROR_ACCESS_DENIED, 0);
        }
        if amount > *maximum - *count {
            return release.fail(cpu.gs_base, ERROR_TOO_MANY_POSTS, 0);
        }
        if output != 0 && mem.write_u32(output, *count as u32).is_err() {
            return release.fail(cpu.gs_base, ERROR_NOACCESS, 0);
        }
        *count += amount;
        Ok(1)
    });

    for symbol in ["CreateMutexA", "CreateMutexW"] {
        let state = Arc::clone(state);
        registry.register(KERNEL32, symbol, move |args, cpu, mem| {
            let attributes = args.get(0, mem).map_err(ExitReason::MemoryFault)?;
            let initial_owner = args.get(1, mem).map_err(ExitReason::MemoryFault)? as u32 != 0;
            let name = args.get(2, mem).map_err(ExitReason::MemoryFault)?;
            if attributes != 0 || name != 0 {
                return state.fail(cpu.gs_base, ERROR_NOT_SUPPORTED, 0);
            }
            insert_mutex(&state, cpu.gs_base, initial_owner, MUTEX_ALL_ACCESS)
        });
    }

    for symbol in ["CreateMutexExA", "CreateMutexExW"] {
        let state = Arc::clone(state);
        registry.register(KERNEL32, symbol, move |args, cpu, mem| {
            let attributes = args.get(0, mem).map_err(ExitReason::MemoryFault)?;
            let name = args.get(1, mem).map_err(ExitReason::MemoryFault)?;
            let flags = args.get(2, mem).map_err(ExitReason::MemoryFault)? as u32;
            let access = args.get(3, mem).map_err(ExitReason::MemoryFault)? as u32;
            if attributes != 0 || name != 0 || access & !MUTEX_ALL_ACCESS != 0 {
                return state.fail(cpu.gs_base, ERROR_NOT_SUPPORTED, 0);
            }
            if flags & !CREATE_MUTEX_INITIAL_OWNER != 0 {
                return state.fail(cpu.gs_base, ERROR_INVALID_PARAMETER, 0);
            }
            let initial_owner = flags & CREATE_MUTEX_INITIAL_OWNER != 0;
            insert_mutex(&state, cpu.gs_base, initial_owner, access)
        });
    }

    let release = Arc::clone(state);
    registry.register(KERNEL32, "ReleaseMutex", move |args, cpu, mem| {
        let handle = args.get(0, mem).map_err(ExitReason::MemoryFault)?;
        let objects = release.objects.lock().expect("guest object state");
        let Some(GuestObject::Waitable { state, .. }) = objects.handles.get(&handle) else {
            return release.fail(cpu.gs_base, ERROR_INVALID_HANDLE, 0);
        };
        let mut state = state.lock().expect("guest waitable state");
        let Waitable::Mutex { owner, recursion } = &mut *state else {
            return release.fail(cpu.gs_base, ERROR_INVALID_HANDLE, 0);
        };
        if *owner != Some(cpu.gs_base) {
            return release.fail(cpu.gs_base, ERROR_NOT_OWNER, 0);
        }
        *recursion -= 1;
        if *recursion == 0 {
            *owner = None;
        }
        Ok(1)
    });

    for (symbol, extended) in [
        ("WaitForSingleObject", false),
        ("WaitForSingleObjectEx", true),
    ] {
        let single = Arc::clone(state);
        registry.register(KERNEL32, symbol, move |args, cpu, mem| {
            let handle = args.get(0, mem).map_err(ExitReason::MemoryFault)?;
            let timeout = args.get(1, mem).map_err(ExitReason::MemoryFault)? as u32;
            if extended && args.get(2, mem).map_err(ExitReason::MemoryFault)? as u32 != 0 {
                return single.fail(cpu.gs_base, ERROR_NOT_SUPPORTED, WAIT_FAILED);
            }
            wait_for_objects(&single, cpu.gs_base, &[handle], false, timeout)
        });
    }
    for (symbol, extended) in [
        ("WaitForMultipleObjects", false),
        ("WaitForMultipleObjectsEx", true),
    ] {
        let multiple = Arc::clone(state);
        registry.register(KERNEL32, symbol, move |args, cpu, mem| {
            let count = args.get(0, mem).map_err(ExitReason::MemoryFault)? as u32;
            let address = args.get(1, mem).map_err(ExitReason::MemoryFault)?;
            let all = args.get(2, mem).map_err(ExitReason::MemoryFault)? as u32 != 0;
            let timeout = args.get(3, mem).map_err(ExitReason::MemoryFault)? as u32;
            if extended && args.get(4, mem).map_err(ExitReason::MemoryFault)? as u32 != 0 {
                return multiple.fail(cpu.gs_base, ERROR_NOT_SUPPORTED, WAIT_FAILED);
            }
            if !(1..=64).contains(&count) || address == 0 {
                return multiple.fail(cpu.gs_base, ERROR_INVALID_PARAMETER, WAIT_FAILED);
            }
            let mut handles = [0; 64];
            let handles = &mut handles[..count as usize];
            for (index, handle) in handles.iter_mut().enumerate() {
                let slot = address
                    .checked_add(index as u64 * 8)
                    .ok_or(ExitReason::MemoryFault(MemoryError::Overflow))?;
                *handle = mem.read_u64(slot).map_err(ExitReason::MemoryFault)?;
            }
            wait_for_objects(&multiple, cpu.gs_base, handles, all, timeout)
        });
    }

    for symbol in [
        "CreateEventA",
        "CreateEventW",
        "SetEvent",
        "ResetEvent",
        "CreateSemaphoreA",
        "CreateSemaphoreW",
        "ReleaseSemaphore",
        "CreateMutexA",
        "CreateMutexW",
        "CreateMutexExA",
        "CreateMutexExW",
        "ReleaseMutex",
        "WaitForSingleObject",
        "WaitForSingleObjectEx",
        "WaitForMultipleObjects",
        "WaitForMultipleObjectsEx",
    ] {
        registry.alias(SYNCH_API, KERNEL32, symbol);
        registry.set_status(KERNEL32, symbol, ServiceStatus::Partial);
    }
    for symbol in ["WaitForMultipleObjects", "WaitForMultipleObjectsEx"] {
        registry.alias("api-ms-win-core-synch-l1-2-1.dll", KERNEL32, symbol);
    }
}

fn wait_for_objects(
    state: &ServicesState,
    teb: u64,
    handles: &[u64],
    all: bool,
    timeout: u32,
) -> Result<u64, ExitReason> {
    // The object table remains locked through validation and consumption.
    let objects = state.objects.lock().expect("guest object state");
    let mut waitables: Vec<&Arc<Mutex<Waitable>>> = Vec::with_capacity(handles.len());
    for (index, handle) in handles.iter().enumerate() {
        if handles[..index].contains(handle) {
            return state.fail(teb, ERROR_INVALID_PARAMETER, WAIT_FAILED);
        }
        match objects.handles.get(handle) {
            Some(GuestObject::Waitable {
                state: event,
                access,
            }) => {
                if access & SYNCHRONIZE == 0 {
                    return state.fail(teb, ERROR_ACCESS_DENIED, WAIT_FAILED);
                }
                if all
                    && waitables
                        .iter()
                        .any(|existing| Arc::ptr_eq(existing, event))
                {
                    return state.fail(teb, ERROR_INVALID_PARAMETER, WAIT_FAILED);
                }
                waitables.push(event);
            }
            Some(_) => return state.fail(teb, ERROR_NOT_SUPPORTED, WAIT_FAILED),
            None if matches!(*handle, super::CURRENT_THREAD | super::CURRENT_PROCESS) => {
                return state.fail(teb, ERROR_NOT_SUPPORTED, WAIT_FAILED);
            }
            None => return state.fail(teb, ERROR_INVALID_HANDLE, WAIT_FAILED),
        }
    }
    let mut ready = Vec::with_capacity(handles.len());
    for (index, event) in waitables.into_iter().enumerate() {
        if event.lock().expect("guest waitable state").is_ready(teb) {
            ready.push((index, event));
            if !all {
                break;
            }
        }
    }
    if ready.is_empty() || (all && ready.len() != handles.len()) {
        return if timeout == 0 {
            Ok(WAIT_TIMEOUT)
        } else {
            state.fail(teb, ERROR_NOT_SUPPORTED, WAIT_FAILED)
        };
    }
    if ready.iter().any(|(_, object)| {
        object
            .lock()
            .expect("guest waitable state")
            .at_recursion_limit()
    }) {
        // Windows raises STATUS_MUTANT_LIMIT_EXCEEDED; exceptions are unsupported.
        return Err(ExitReason::IllegalInstruction(0xFE));
    }
    let first = ready[0].0;
    for (_, event) in ready {
        event.lock().expect("guest waitable state").consume(teb);
    }
    Ok(WAIT_OBJECT_0 + first as u64)
}

// SPDX-License-Identifier: GPL-3.0-or-later

//! Documented XAsync helpers on top of the manual GDK task-queue subset.
//!
//! This first cut runs `XAsyncRun` work and completion on the calling thread
//! before the service returns (immediate dispatch). Status is still tracked so
//! `XAsyncGetStatus` / `XAsyncCancel` behave for callers that poll after the
//! run returns. Queue-port scheduling remains future work.

use super::ServicesState;
use crate::cpu::ExitReason;
use crate::imports::{
    CallbackStart, CallbackStep, CallbackTask, ImportRegistry, ServiceStatus, Win64Args,
};
use crate::memory::{GuestMemory, MemoryError};
use crate::runtime::HResult;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

const MODULE: &str = "xgameruntime.dll";
const ASYNC_BLOCK_SIZE: u64 = 56;
const E_PENDING: u64 = 0x8000_000A;
const E_ABORT: u64 = 0x8000_4004;
const E_INVALIDARG: u64 = 0x8007_0057;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Status {
    Pending,
    Done(u32),
}

#[derive(Default)]
pub(super) struct AsyncOps {
    ops: BTreeMap<u64, Status>,
}

fn fault(error: MemoryError) -> ExitReason {
    ExitReason::MemoryFault(error)
}

fn guest_hresult(code: u32) -> u64 {
    HResult::from_win32(code).guest_return()
}

struct AsyncRun {
    block: u64,
    completion: u64,
    ops: Arc<Mutex<AsyncOps>>,
    phase: Phase,
}

enum Phase {
    Work,
    Complete,
}

impl CallbackTask for AsyncRun {
    fn resume(
        &mut self,
        result: u64,
        _: u64,
        _: &mut GuestMemory,
    ) -> Result<CallbackStep, ExitReason> {
        match self.phase {
            Phase::Work => {
                let status = result as u32;
                self.ops
                    .lock()
                    .expect("async ops")
                    .ops
                    .insert(self.block, Status::Done(status));
                if self.completion != 0 {
                    self.phase = Phase::Complete;
                    return Ok(CallbackStep::Call {
                        entry: self.completion,
                        args: [self.block, 0, 0, 0],
                    });
                }
                Ok(CallbackStep::Return(HResult::OK.guest_return()))
            }
            Phase::Complete => Ok(CallbackStep::Return(HResult::OK.guest_return())),
        }
    }
}

pub(super) fn register(registry: &mut ImportRegistry, state: &Arc<ServicesState>) {
    let run = Arc::clone(state);
    registry.register_callback_task(
        MODULE,
        "XAsyncRun",
        ServiceStatus::Partial,
        move |args, _, memory, _| start_run(&run, args, memory),
    );

    let status = Arc::clone(state);
    registry.register_with_status(
        MODULE,
        "XAsyncGetStatus",
        ServiceStatus::Partial,
        move |args, _, memory| {
            let block = args.rcx;
            if block == 0 || memory.check_write(block, ASYNC_BLOCK_SIZE as usize).is_err() {
                return Ok(E_INVALIDARG);
            }
            if args.rdx != 0 {
                // Waiting would block the guest; the sync subset never leaves
                // work pending after XAsyncRun returns.
                return Ok(guest_hresult(50)); // ERROR_NOT_SUPPORTED as HRESULT
            }
            let ops = status.gdk_async.lock().expect("async ops");
            Ok(match ops.ops.get(&block) {
                Some(Status::Pending) => E_PENDING,
                Some(Status::Done(code)) => u64::from(*code),
                None => E_INVALIDARG,
            })
        },
    );

    let cancel = Arc::clone(state);
    registry.register_with_status(
        MODULE,
        "XAsyncCancel",
        ServiceStatus::Partial,
        move |args, _, memory| {
            let block = args.rcx;
            if block == 0 || memory.check_write(block, ASYNC_BLOCK_SIZE as usize).is_err() {
                return Ok(0);
            }
            let mut ops = cancel.gdk_async.lock().expect("async ops");
            if let Some(Status::Pending) = ops.ops.get(&block) {
                ops.ops.insert(block, Status::Done(E_ABORT as u32));
            }
            Ok(0)
        },
    );

    registry.register_with_status(
        MODULE,
        "XAsyncGetResultSize",
        ServiceStatus::Partial,
        move |args, _, memory| {
            if args.rcx == 0
                || args.rdx == 0
                || memory.check_write(args.rcx, ASYNC_BLOCK_SIZE as usize).is_err()
            {
                return Ok(E_INVALIDARG);
            }
            memory.write_u64(args.rdx, 0).map_err(fault)?;
            Ok(HResult::OK.guest_return())
        },
    );
}

fn start_run(
    state: &Arc<ServicesState>,
    args: &Win64Args,
    memory: &mut GuestMemory,
) -> Result<CallbackStart, ExitReason> {
    let block = args.rcx;
    let work = args.rdx;
    if block == 0 || work == 0 || memory.check_write(block, ASYNC_BLOCK_SIZE as usize).is_err() {
        return Ok(CallbackStart::Return(E_INVALIDARG));
    }
    memory
        .fetch_instruction_byte(work)
        .map_err(ExitReason::MemoryFault)?;
    let completion = memory.read_u64(block + 16).map_err(fault)?;
    if completion != 0 {
        memory
            .fetch_instruction_byte(completion)
            .map_err(ExitReason::MemoryFault)?;
    }
    {
        let mut ops = state.gdk_async.lock().expect("async ops");
        if matches!(ops.ops.get(&block), Some(Status::Pending)) {
            return Ok(CallbackStart::Return(E_INVALIDARG));
        }
        ops.ops.insert(block, Status::Pending);
    }
    Ok(CallbackStart::Call {
        task: Box::new(AsyncRun {
            block,
            completion,
            ops: Arc::clone(&state.gdk_async),
            phase: Phase::Work,
        }),
        entry: work,
        args: [block, 0, 0, 0],
    })
}

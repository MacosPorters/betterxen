// SPDX-License-Identifier: GPL-3.0-or-later

//! Process termination.

use super::{KERNEL32, ServicesState};
use crate::cpu::ExitReason;
use crate::imports::{ImportRegistry, ServiceStatus};
use std::sync::Arc;

const PROCESS_THREADS_API: &str = "api-ms-win-core-processthreads-l1-1-0.dll";

pub(super) fn register(registry: &mut ImportRegistry, state: &Arc<ServicesState>) {
    // ExitProcess does not return (processthreadsapi.h). It stops the guest
    // with its status and never ends the host. It is partial: there are no
    // other threads or loaded DLLs to notify.
    let s = Arc::clone(state);
    registry.register(KERNEL32, "ExitProcess", move |args, _, _| {
        // The status is a UINT, the low half of the first argument.
        Err(ExitReason::ProcessExit(s.exit_process(args.rcx as u32)))
    });
    registry.alias(PROCESS_THREADS_API, KERNEL32, "ExitProcess");
    registry.set_status(KERNEL32, "ExitProcess", ServiceStatus::Partial);
}

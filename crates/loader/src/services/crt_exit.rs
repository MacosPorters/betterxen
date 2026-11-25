// SPDX-License-Identifier: GPL-3.0-or-later

//! Abnormal CRT termination: `abort`, `_exit`, `terminate`,
//! `__std_terminate` and `_purecall`.
//!
//! Expected behaviour is from the public Microsoft C runtime pages. `abort`
//! ends the process with exit code 3 when no `SIGABRT` handler takes over.
//! `_exit` ends it with the given status and runs no `atexit` functions. The
//! default `terminate` handler calls `abort`, and so does the default
//! pure-virtual-call handler behind `_purecall`.
//!
//! No handler can be installed in Xenora yet: `signal`, `set_terminate` and
//! `_set_purecall_handler` are not runtime services, so a guest that tries
//! stops there. The default path is therefore the whole behaviour. Every exit
//! goes through the same process exit as `ExitProcess`; the host process is
//! never ended. Debug-CRT message boxes and Windows Error Reporting are not
//! modelled.

use super::ServicesState;
use crate::cpu::ExitReason;
use crate::imports::{ImportRegistry, ServiceStatus};
use std::sync::Arc;

const UCRT: &str = "ucrtbase.dll";
const CRT_RUNTIME_API: &str = "api-ms-win-crt-runtime-l1-1-0.dll";
const VCRUNTIME: &str = "vcruntime140.dll";

/// The exit code `abort` reports without a `SIGABRT` handler.
const ABORT_EXIT_CODE: u32 = 3;

pub(super) fn register(registry: &mut ImportRegistry, state: &Arc<ServicesState>) {
    // Each of these reaches abort's default path.
    for (module, symbol) in [
        (UCRT, "abort"),
        (UCRT, "terminate"),
        (UCRT, "_purecall"),
        (VCRUNTIME, "__std_terminate"),
        (VCRUNTIME, "_purecall"),
    ] {
        let s = Arc::clone(state);
        registry.register_with_status(module, symbol, ServiceStatus::Partial, move |_, _, _| {
            Err(ExitReason::ProcessExit(s.exit_process(ABORT_EXIT_CODE)))
        });
    }

    let s = Arc::clone(state);
    registry.register_with_status(UCRT, "_exit", ServiceStatus::Partial, move |args, _, _| {
        // The status is an int; the process exit code is its 32 bits.
        Err(ExitReason::ProcessExit(s.exit_process(args.rcx as u32)))
    });

    for symbol in ["abort", "terminate", "_purecall", "_exit"] {
        registry.alias(CRT_RUNTIME_API, UCRT, symbol);
    }
}

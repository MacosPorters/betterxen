// SPDX-License-Identifier: GPL-3.0-or-later

//! Abnormal CRT termination through the guest import table. Expectations are
//! from the public Microsoft C runtime pages: `abort` ends the process with
//! exit code 3 when no SIGABRT handler takes over, `_exit` ends it with its
//! status, and the default `terminate` and pure-virtual-call handlers call
//! `abort`. Each stub is `call [slot]; hlt`, so a service that returned would
//! report Halt instead of a process exit.

mod support;

use support::*;
use xenora_loader::cpu::ExitReason;

const UCRT: &str = "ucrtbase.dll";
const RUNTIME: &str = "api-ms-win-crt-runtime-l1-1-0.dll";
const VCRUNTIME: &str = "vcruntime140.dll";

const IMPORTS: &[(&str, &str)] = &[
    (UCRT, "abort"),
    (UCRT, "terminate"),
    (UCRT, "_purecall"),
    (VCRUNTIME, "__std_terminate"),
    (UCRT, "_exit"),
    ("kernel32.dll", "SetLastError"),
    ("kernel32.dll", "CreateEventW"),
];
const RUNTIME_IMPORTS: &[(&str, &str)] = &[
    (RUNTIME, "abort"),
    (RUNTIME, "terminate"),
    (RUNTIME, "_purecall"),
    (VCRUNTIME, "_purecall"),
    (RUNTIME, "_exit"),
];

#[test]
fn abort_terminate_and_purecall_exit_with_code_three() {
    for imports in [IMPORTS, RUNTIME_IMPORTS] {
        for &(module, symbol) in imports.iter().filter(|(_, s)| *s != "_exit") {
            if module == "kernel32.dll" {
                continue;
            }
            let mut guest = Guest::new(imports, &[]);
            // Arguments are ignored: these functions take none.
            let (_, exit) = guest.run(symbol, &[0x55]);
            assert_eq!(exit, ExitReason::ProcessExit(3), "{module}!{symbol}");
            assert_eq!(guest.state.exit_status(), Some(3), "{module}!{symbol}");
        }
    }
}

#[test]
fn underscore_exit_uses_its_whole_32_bit_status() {
    for imports in [IMPORTS, RUNTIME_IMPORTS] {
        for (argument, status) in [
            (0, 0),
            (7, 7),
            (0xFFFF_FFFF, 0xFFFF_FFFF),
            (0x1_0000_0002, 2),
        ] {
            let mut guest = Guest::new(imports, &[]);
            let (_, exit) = guest.run("_exit", &[argument]);
            assert_eq!(exit, ExitReason::ProcessExit(status), "{argument:#x}");
        }
    }
}

#[test]
fn abort_closes_handles_and_nothing_runs_afterwards() {
    let mut guest = Guest::new(IMPORTS, &[]);
    assert_ne!(guest.call("CreateEventW", &[0, 1, 0, 0]), 0);
    assert_eq!(guest.state.open_handle_count(), 1);
    assert_eq!(guest.run("abort", &[]).1, ExitReason::ProcessExit(3));
    assert_eq!(guest.state.open_handle_count(), 0);
    // A later exit does not change the status, and no service runs.
    assert_eq!(guest.run("_exit", &[9]).1, ExitReason::ProcessExit(3));
    assert_eq!(
        guest.run("SetLastError", &[5]).1,
        ExitReason::ProcessExit(3)
    );
    assert_eq!(guest.state.last_error(THREAD_A), 0);
}

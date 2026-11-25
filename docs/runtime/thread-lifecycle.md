# Static TLS and guest worker lifecycle

Proposed contract for [#305](https://github.com/MacosPorters/betterxen/issues/305).
The current [scheduler](threads.md) gives each worker its own CPU context,
stack, thread environment block (TEB), and dynamic thread-local storage (TLS)
and fiber-local storage (FLS) values. It drops those values when a worker ends.
It does not instantiate PE static TLS for workers or run their TLS, DLL or FLS
exit callbacks. The current [PE TLS setup](../../crates/loader/src/tls.rs)
creates one initial-thread block at module index zero. The
[fixture recipe](../../tests/compat_fixtures/thread-lifecycle/README.md) describes
future CPU tests for the extended contract.

## Module and thread ownership

The module loader supplies a fixed, dependency-ordered set of admitted PE32+
modules, including the executable. Assign a stable index to each module with a
TLS directory and write it once to that module’s checked index location. Keep
the template and relocated callback metadata owned by the module. The worker
scheduler owns each thread’s TLS pointer array and data blocks alongside its
stack and TEB. Dynamic TLS slots and FLS values keep their separate namespaces.

Before a worker may run, check every module template range, index, alignment
and size calculation. Copy each initialized template into distinct worker
memory and append its declared zero fill. Build the entire pointer array and
set the TEB pointer before invoking any callback. A worker receives the
initial bytes from the module template even when the initial thread or another
worker has modified its own copy. Charge TLS allocations to the existing 64
MiB aggregate thread-memory budget, and retain the existing 64-live-thread
limit. A failed reservation releases the worker’s partial allocations without
publishing a runnable worker. [Microsoft PE format, TLS section](https://learn.microsoft.com/en-us/windows/win32/debug/pe-format#the-tls-section)

## Notifications and normal exit

Process attach and module initialization precede any worker notifications.
The initial thread receives process initialization. Do not replay it as
worker attach. On a worker’s first scheduled start, run thread attach before
its entry routine. A worker created suspended stays outside this phase until
its first resume. Returning from the routine or calling `ExitThread` records
the requested exit code and begins normal teardown. Keep the worker’s TLS,
stack and TEB mapped until teardown ends, then publish completion, release
waiters and reclaim thread-owned state. The thread handle retains its exit
status until it is closed. [Microsoft DllMain](https://learn.microsoft.com/en-us/windows/win32/dlls/dllmain)
[Microsoft ExitThread](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-exitthread)

The initial BetterXen fixture uses this deterministic order. The public
references give the notification reasons, arguments and TLS callback array
order. They do not establish the cross-module or FLS ordering used here.

| Stage | Proposed order |
| --- | --- |
| Worker attach | Visit modules in the loader’s dependency order. For each module, invoke TLS callbacks in array order, then its DLL entry point if present. Use the thread-attach reason. |
| Normal worker exit | Visit populated FLS slots with registered callbacks once in ascending slot order. Take the saved value out of the slot immediately before calling its registered function. Skip empty slots and slots without callbacks. |
| Worker detach | Visit the modules in reverse dependency order. For each module, invoke its DLL entry point if present, then its TLS callbacks in array order. Use the thread-detach reason. A completed worker exit is published after this pass. |

The executable can have TLS callbacks and has no DLL entry point. TLS and DLL
notification calls receive the loaded module base, reason and zero reserved
argument for worker notifications. A TLS callback returns no value and the
DLL entry point’s return value is ignored for worker notifications. A DLL
returning false during process attach is a module-loader failure under
[#301](https://github.com/MacosPorters/betterxen/issues/301), before any worker
may run. Each FLS callback receives its worker’s previous slot value and
returns no value. [PE TLS callback functions](https://learn.microsoft.com/en-us/windows/win32/debug/pe-format#tls-callback-functions)
[Microsoft FLS callback contract](https://learn.microsoft.com/en-us/windows/win32/api/winnt/nc-winnt-pfls_callback_function)

## Checked callbacks and faults

- Invoke only executable guest addresses. Check that module notification
  targets belong to the expected module and that an FLS target belongs to an
  admitted executable mapping. Revalidate through checked guest memory.
  Use the scheduled worker’s CPU and checked stack with a guest return frame,
  32 bytes of home space, and RSP equal to 8 modulo 16 on callback entry.
  Preserve Win64 nonvolatile general-purpose registers and XMM6–XMM15,
  and validate stack restoration on return. Guest callbacks and TLS never use
  host callback pointers or host TLS.
  [Microsoft x64 calling convention](https://learn.microsoft.com/en-us/cpp/build/x64-calling-convention)
- Keep attach and detach serial with module lifecycle work. Allow a bounded
  callback to read its own TLS/FLS state and update checked fixture memory.
  Reject operations that would reenter module or worker lifecycle, block, or
  mutate the FLS callback list during teardown. The first contract excludes
  fibers, FLS destructor retries and Windows loader-lock equivalence.
- Retain the 64-callback directory limit and existing 128-slot FLS limit. Charge
  all callback instructions against both a bounded lifecycle allowance and the
  remaining run budget. Consume one allowance over the whole attach or exit
  pass, and stop dispatching callbacks on the first budget or execution fault.
  The budget value and scheduler outcome representation belong to the
  implementation review under [#228](https://github.com/MacosPorters/betterxen/issues/228).
- On a failing attach, skip worker entry. On a failing teardown, preserve the
  primary fault and completed callback prefix. In both cases stop the run
  through the scheduler’s fault path and reclaim runtime-owned worker state.
  A fault does not undo guest writes or certify that guest cleanup completed.
- The first contract covers normal worker return and explicit `ExitThread`.
  Coordinate `ExitThread` with live waiters and forced worker termination
  through the [scheduler issue](https://github.com/MacosPorters/betterxen/issues/228).
  Process exit with live workers also depends on the
  [process-exit contract](https://github.com/MacosPorters/betterxen/issues/229).
  Dynamic module attach to existing workers and fiber switching require
  separate contracts. Forced reclamation drops FLS values without running
  their destructors.

The scheduler and module-loader owners should agree on module lifetime,
callback frames and fault reporting before any production change. A runnable
integration remains dependent on #228 and #301. The eventual startup-to-frame
fixture in [#256](https://github.com/MacosPorters/betterxen/issues/256) can then
reuse the lifecycle probes. This design establishes no guest compatibility or
execution result.

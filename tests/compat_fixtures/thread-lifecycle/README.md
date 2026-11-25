# Worker TLS and callback fixture recipe

This is a proposed original PE32+ fixture for
[the worker lifecycle contract](../../../docs/runtime/thread-lifecycle.md).
The fixture sources and execution harness are follow-up work after the scheduler
and module-loader contracts are agreed.

## Build inputs and inspection

Use two freestanding inputs, `worker-lifecycle.exe` and `worker-probe.dll`.
Keep the TLS directories, callback arrays and small register-checking stubs in
original assembly and write the log, control flow and result checks in original
C. The executable imports one log-registration export from the companion DLL
and the required scheduler, event, wait and FLS services from the synthetic
registry. Register the shared log before launching a worker. Record process
notifications separately in module-owned storage before a log is registered.

Follow the [existing PE32+ build recipe](../README.md). Use native ARM64
`clang`, `lld-link` and `llvm-readobj` on macOS. Verify the tools and their
loadable dependencies before invoking them. Compile for
`x86_64-pc-windows-msvc` with `-ffreestanding -fno-stack-protector`, and use
`lld-link /nodefaultlib /timestamp:0` with explicit EXE and DLL entry points.
Generate import libraries from the authored export/service declarations.
The fixture uses no linked support runtime or system DLL. Any future support
object needs per-file license review before inclusion. Put generated objects,
images and evidence under ignored `artifacts/`.

Inspect each image with `llvm-readobj --file-headers --coff-imports
--coff-exports --coff-tls-directory --coff-basereloc`. Require AMD64 PE32+,
the intended imports, distinct valid TLS index locations, a terminated callback
array, and base relocations for TLS addresses that store virtual addresses.
Use two distinguishable callbacks in the DLL array so reversed or skipped
callbacks are observable. Preserve the relocations and test one legal rebase.

## Expected observations

Set the executable template to initialized bytes `11 22` followed by two
zero-fill bytes and the DLL template to `33 44` followed by three zero-fill
bytes. The initial thread overwrites its own test bytes before creating any
worker. Workers A and B each start with `11 22 00 00` and `33 44 00 00 00`.
They overwrite their own initialized and zero-fill bytes with disjoint patterns.
Use events to keep A alive while B writes and exits. A then verifies that its
bytes retained their assigned values. After joining A and B, the initial
thread checks its own bytes and creates a third worker to check fresh contents
after reclamation and a distinct worker ID. Record a bounded per-thread event
log in ordinary shared guest memory and compare each worker’s event sequence
independently of scheduling interleavings.

At normal exit the callback gets the worker’s nonzero FLS value. Use distinct
values for A and B, exercise a normal return and an explicit `ExitThread`,
and require each published exit code and the worker’s callback prefix to match.
Use a third worker with an empty FLS value to check omission. For a worker
with a nonzero value, the expected sequence is the following.

```text
dll.tls[0].attach  dll.tls[1].attach  dll.entry.attach  exe.tls.attach
worker.entry       worker.exit-request
fls.callback       exe.tls.detach    dll.entry.detach
dll.tls[0].detach  dll.tls[1].detach  worker.completed
```

Each notification records its reason and loaded module base. At FLS and detach
callbacks the worker reads its own TLS values, proving teardown has not unmapped
them. Waiters can observe completion only after the successful callback
sequence. The harness records peak and final worker resources during repeated
start/join cycles and checks that the final footprint returns to baseline.
Run the same cases at two distinct scheduler quanta and compare values and
per-worker order. A short assembly probe pins ABI nonvolatile register and
XMM values, aligned entry stack state, and the callback stack’s home area.
Derive the oracle from the byte constants and the contract above.

| Original variant | Expected observation |
| --- | --- |
| Change one initialized DLL template byte, leaving the oracle unchanged | The first worker’s initial TLS comparison fails at that byte. |
| Remove one DLL TLS callback entry | The per-worker event sequence differs at the missing callback. |
| Create a suspended worker | No attach record appears before its first resume. After resume the worker follows the normal sequence. |
| Return false from the DLL’s thread-attach notification | The worker still reaches its entry. A false process-attach return has a separate failure case. |
| Make process attach fail, or fault during worker attach | No affected worker entry. Preserve the fault and only the completed notification prefix. |
| Use a callback target outside the admitted executable range | Report invalid target before entering that callback. |
| Exhaust a lifecycle budget in one callback | Later callbacks do not run. The reported fault and prefix survive reclamation. |
| Fault during FLS or detach | Reclaim runtime-owned state and preserve the primary fault. No normal completion marker or claim that all guest cleanup ran. |
| Violate the callback return frame or clobber a nonvolatile sentinel | Reject callback completion and identify the ABI/stack failure. |

No fixture or worker lifecycle test has been executed for this design.

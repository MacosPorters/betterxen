# Security review of the hybrid path and native boundaries (2026-11-25)

Written by the author of the hybrid changes; not an independent review, and not a penetration test.

## A CrossOver bottle is not a security boundary

In the `betterxen-test` bottle, `Z:` maps to `/`, `Y:` to the user's home directory, and `D:` and `E:` to other mounted volumes, including raw device nodes in `dosdevices`. A Windows program run this way executes as the user, with the user's files and network. Removing drive letters would not make it a sandbox, because Wine is not designed to confine hostile code. **Process supervision (memory, time, output, signals) limits runaway behaviour only.** Run only programs you trust. Untrusted programs need a separate user account or a virtual machine, neither of which BetterXen provides.

## Findings

| Area | Finding | State |
| --- | --- | --- |
| Environment | The child inherited every host variable; a probe saw an arbitrary secret and `SSH_AUTH_SOCK` | **Fixed**: allowlist plus explicit `--env`; `DYLD_*` and backend variables refused; `--inherit-env` restores the old behaviour deliberately. Verified with a gate that fails when the variable is visible |
| Filesystem | See above | Open by nature |
| Command injection | The runner starts processes with argument vectors, no shell. Bottle names that are paths or options are refused; program paths starting with `-` are refused; arguments after `--` are passed through to the program | No injection found. CrossOver's `wine` launcher is a Perl script that was not reviewed |
| Executable input | PE headers are parsed with bounds checks and tested with truncated and corrupted input. Not fuzzed. The file is launched by path after identification, so it can be swapped in between | Open |
| Resource exhaustion | Free-memory floor, wall-clock and stall limits, 64 MiB output limit. No limit on CPU, GPU memory, disk writes by the guest or process count | Open |
| Cleanup | SIGINT, SIGTERM, SIGHUP stop the process group and bottle; leftovers are recorded and killed | Verified on CrossOver. SIGKILL of the supervisor leaves the program running until `betterxen hybrid stop` |
| Concurrency | Stopping a run stops every process in its bottle, including another run's. Memory sampling and leftover detection match by executable name | Open: one run per bottle |
| Evidence files | Reports are created exclusively and never overwrite; records contain program paths, `--env` names (not values) and output tails, which may include whatever the program printed | Do not publish records unread |
| Dependencies | 25 third-party packages across four lock files queried against OSV: no advisories on 2026-11-25. `cargo-deny` licence and source policy was not run | Point in time |
| `unsafe` in the hybrid crate | Five sites: signal installation, `kill`, `killpg` (two) and a test; each has a safety comment | Reviewed by the author only |
| FFI (`crates/ffi`) | Exports catch panics and check null pointers and sizes at the sites sampled. 71 of 91 `unsafe` blocks have no comment nearby (some rely on a function-level `# Safety` section). A panic while the global session lock is held poisons it, after which every call returns an internal error | Sampled, **not reviewed block by block**; native code is paused |
| Translator and JIT (`loader`) | No nearby safety comment on any of the 16 blocks in `dbt.rs` or the 9 in `jit.rs` | **Not reviewed** |
| Metal (`metal`) | All but one `unsafe` block carry a safety comment | Comments not verified |

## Not covered

CrossOver, Wine, Rosetta, D3DMetal and the graphics backends are closed or external and were not assessed. No fuzzing campaign was run in this review.

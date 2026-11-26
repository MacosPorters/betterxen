# Workstream B append-only audit: B1 stopped-run hash discrepancy and B2 target-PID trace inputs

Author: Workstream B AI research agent under @atesahmet0's authorization (2026-11-27).
Scope: Resolves Workstream A's two concrete review findings in [#473 (comment)](https://github.com/MacosPorters/betterxen/issues/473#issuecomment-7204728260) against B commit `a40228747470f0d184375e451b0173769c90aef5` without rewriting any historical receipt or archived output.

## 1. B1 — Forensic timeline and audit of `b-trace-01/00-wb/stderr.txt`

### Retained evidence in `/private/tmp/b473-trace-01` and published `2026-11-27-apple-m5-atesahmet0-b-trace-01`

All original files from the stopped run remain preserved on the M5 host. Subsecond APFS `stat` timestamps (`btime`, `mtime_ns`, `ctime_ns`) establish the exact sequence:

| File | Bytes | Birth (`s`) | Modify (`ns` since epoch) | SHA-256 |
| --- | ---: | ---: | ---: | --- |
| `00-wb/stdout.txt` | 121 | `1795730535.214490175` | `1795730537091268055` | `dca5ad5caa4f0940d193ea3ef60d56b9b98adde3a47470c1f28338ccbd072307` |
| `00-wb/stderr.txt` | 40 | `1795730535.214524984` | `1795730540932343519` | `b7eab1251cc975b771929b1c6dfe6cb8511bb1ec75b3241336abe9651a151f83` |
| `00-wb/case.json` | 3943 | `1795730540.928579330` | `1795730540928687009` | `2380bf2e49963014...` |
| `receipt.json` | 23377 | `1795730532.366539478` | `1795730542894906836` | `2e537107cb1ae7e9...` |
| `00-wb/xctrace-output.txt` | 257 | `1795730537.167738199` | `1795730551460369485` | `b5769eac9dd93b50f0bfbac3ae33b3a0e30f066f6005c96e6bc40bc5ec184f7e` |
| `00-wb/xctrace.json` | 391 | `1795730551.524590492` | `1795730551524795927` | `27a6e0d0500fd5a9...` |

### Established cause from timestamps and code inspection

1. `run_case()` in `B-local-1` (`copy-detection/run.py`) launched CrossOver's Perl wrapper `bin/wine` with `stdin=subprocess.PIPE`, `stdout=out`, `stderr=err`, and `start_new_session=True`.
2. At `1795730537.091`, the Windows guest (`b-coherence.exe`, `guest_pid=57987`) printed `BACKEND_READY` to `stdout.txt` and blocked in `receipt_pause()` (`std::getchar()` on `stdin`, `support.h:214`).
3. `b473-trace-launch.py` verified `guest_pid=57987` and attached `xctrace` (`1795730537.168`). Immediately afterward, `pressure_ready` observed `kern.memorystatus_vm_pressure_level=2` (Urgent) and raised `RuntimeError('pressure gate at paused device before workload')`.
4. Exiting the `with` block closed `run.py`'s parent file descriptors `out` and `err`, **not** the guest's inherited `stderr` descriptor, and **did not close `process.stdin`**, which remained open on local variable `process` (`subprocess.Popen`).
5. In `except BaseException:`, `os.killpg(process.pid, signal.SIGKILL)` killed the `bin/wine` launcher process group and `process.wait()` reaped `exit=-9`, **without** signaling `guest_pid=57987` directly or stopping the bottle's `wineserver` (`wineserver -k` ran later in `main()`'s outer `finally:`). Because `process.stdin` was still open, `b-coherence.exe` (`57987`) remained alive and blocked in `std::getchar()`.
6. In `run_case()`'s `finally:`, `stderr.txt` was hashed while still genuinely 0 bytes (`e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855`), and `case.json` finished writing at `1795730540.928687009`.
7. Immediately upon `return result` from `run_case()`, local `process` went out of scope and CPython closed `process.stdin`. That pipe EOF woke `std::getchar() == EOF` in `b-coherence.exe`, which threw `std::runtime_error("backend receipt handshake")`, caught it in `main()`, and wrote `END failure: backend receipt handshake\r\n` (40 bytes) to `stderr.txt` at `1795730540.932343519` — **3,656,510 ns (3.657 ms) after `case.json` was written**. `receipt.json` (`1795730542.895`) serialized the already-computed in-memory `result['output_hashes']` dictionary rather than re-hashing on disk.

### Full audit across all 20 published Workstream B bundles (165 `output_hashes` entries)

- Across all 156 guest/shader entries (`64` `stdout.txt`, `64` `stderr.txt`, `16` `access.hlsl`, `12` `access.dxbc`), **155/156 match**; `b-trace-01/00-wb/stderr.txt` is the sole mismatch. Zero completed runs are affected because normal completion writes `\n` to `process.stdin`, waits for guest `END complete`, and reaps clean exit `0` before hashing.
- Across the remaining 9 `xctrace-output.txt` entries in traced runs, 8 of 9 `case.json` hashes recorded the mid-save `xctrace-output.txt` digest at the end of `run_case()` because `trace.py` awaited `trace.wait()` in its outer `finally:` after `runner.main()` returned (for the 7 archived trace runs, `archive.json` separately recorded both post-`trace.wait()` `raw_sha256` and `<HOME>`-redacted `archive_sha256`).

### Harness correction (`B-local-1.1`)

`copy-detection/run.py` (`7a4599fcb8f3ebf46c830eda1334d76beed3f89c3fd05c30fb6b54d4ef64e930`) and `copy-detection/trace.py` (`e82533b3abe4eee45978bd549cdc0a648744568cc6fdfcf3b607a65a692ebc88`) are updated to:
- Close `process.stdin` explicitly on both normal and error paths before process reaping;
- Send `SIGKILL` to both `process.pid`'s process group and any verified `guest_pid`, followed by `wineserver -k` and `wineserver -w` on the private bottle before hashing error outputs;
- Wait on `wineserver -w` after normal case exit and finalize `xctrace` via `runner.after_guest` **before** computing `output_hashes` and writing `case.json`.

A controlled forced stop at `pressure_ready` (`max_level=-1`) verified `recorded_sha == actual_sha` (`True`) after full bottle teardown. Historical `B-local-1` receipts remain untouched.

## 2. B2 — Sanitized target-PID Metal trace XML inputs

To distinguish arithmetic re-evaluation of `gpu-samples.json` from independent validation of `summarize-trace.py`'s XML `id`/`ref` resolution, target-PID filtering, staging/readback `Blit Command` exclusion, `Active` GPU-state selection, and `(cmdbuffer-id, encoder-id)` sequence join, `copy-detection/export-target-trace.py` (`0195ca28b0a6f7d8d30e732f4b741b19804814e00645a83ab1f4edc69ae94b6c`) exports minimal target-PID-only XML inputs from the preserved `/private/tmp/b473-*` `xctrace` captures into [the append-only trace-input bundle](../results/2026-11-27-apple-m5-atesahmet0-b-trace-inputs-01/receipt.json).

Each export retains:
- Full `<schema>` column mnemonics;
- A `<referenced-nodes>` block holding every transitive `id="..."` XML node referenced via `ref="..."` from outside the target rows (exercising `resolve()`'s `elements[element.attrib['ref']]` path);
- Every `<row>` matching the verified `guest_pid`, including all 270 staging/readback `Blit Command` rows in `b-metal-default` (`345` total target encoder/GPU rows -> `75` matched `Compute Command` intervals) while stripping all unrelated host processes and verifying zero personal paths.

Running unmodified `summarize-trace.py` on each published run directory plus its sanitized `target-gpu-intervals.xml` and `target-metal-encoders.xml` reproduces all 7 published `gpu-samples.json` outputs (300 shader intervals + 45 copy intervals) with exact JSON equality.

# D3DMetal unified-memory and zero-copy research

Collaborative research for [#473](https://github.com/MacosPorters/betterxen/issues/473), informing [#464](https://github.com/MacosPorters/betterxen/issues/464). Maintainer-agreed participants are @Arths17, @skakwy and @CarReal8917; participation is not an exclusive claim. The shared branch is [`research/d3dmetal-zero-copy`](https://github.com/MacosPorters/betterxen/tree/research/d3dmetal-zero-copy). It is based on `origin/main` commit `f7fb52bc56f671f9da7109f8aef8d3f04de59aee` and is not being merged into main.

This directory contains an original synthetic probe, a runner, separate run records and [consolidated findings](findings.md). It neither imports nor tests the production `xmem` or `d3d12_xs` implementation. @devil8blade retains the implementation of #464. Findings here do not certify that implementation or establish Xbox compatibility.

## Build and run

Requirements: Apple Silicon macOS, an externally installed licensed CrossOver with D3DMetal, supported Rosetta 2, Python 3.11+, and native ARM64 MinGW-w64 cross-compiler tools. Nothing proprietary is downloaded or bundled. The compiler is an ARM64 host executable; its output is a Windows x86-64 guest executable.

Use an existing **win64** CrossOver bottle. The runner copies it into a fresh scratch directory, changes only that temporary copy, builds outside the checkout, selects D3DMetal, runs with `ROSETTA_ADVERTISE_AVX=1`, and enforces a **30-second** process timeout. No BetterXen shim directory is added to the search path. It prints the resolved `d3d12.dll` path and checks that `d3d12_xs.dll` is absent. The original bottle and the checkout are not written by the runner.

```sh
# Fetch this branch even in a clone that normally fetches only main.
git fetch origin research/d3dmetal-zero-copy:refs/remotes/origin/research/d3dmetal-zero-copy
git worktree add --detach ../betterxen-zero-copy origin/research/d3dmetal-zero-copy
cd ../betterxen-zero-copy
python3 docs/research/d3dmetal-zero-copy/run.py \
  --bottle "/absolute/path/to/an/existing/CrossOver/bottle" \
  --scratch /tmp/bx-uma-verify/run-01
```

The scratch directory must not exist. `--crossover /Applications/CrossOver.app` and `--compiler x86_64-w64-mingw32-g++` select non-default installations. Existing bottle configuration can affect a run; record relevant settings and verify the loaded backend. The runner uses a small child-environment allowlist and never forwards host credential variables.

The runner invokes this compilation, with absolute scratch paths in the actual receipt:

```sh
x86_64-w64-mingw32-g++ -std=c++20 -O2 -static -static-libgcc -static-libstdc++ \
  -Wall -Wextra -Werror probe.cpp -o probe.exe -lkernel32
```

Outputs are `probe.cpp`, `probe.exe`, `receipt.json`, `stdout.txt` and `stderr.txt`. The receipt records source/executable SHA-256, compiler command/result, PE format/imports, host, software, environment and run outcome. Temporary bottle contents are removed after its private wineserver is stopped. The caller removes the remaining scratch files **after preserving the selected research records**.

Two launch attempts in the initial experiment stopped before the probe because the host ClamAV database was absent. For the original executable compiled locally from this source, the temporary bottle uses `AllowUntrusted=1` and `AntiVirusScan=untrusted`; the source bottle's scanner settings are unchanged. This does not bypass any dependency license or activation requirement.

## Fixed six-check baseline

Do not change these checks to improve an outcome. Unsupported HRESULTs and not-run paths are evidence, not passes. Exit 0 means no observed data/submission mismatch; it does **not** mean every capability is supported. Inspect each printed result.

| Check | Fixed experiment |
| --- | --- |
| 1: device | Obtain `ID3D12Device3`; query `ARCHITECTURE1` (`UMA`, `CacheCoherentUMA`, `TileBasedRenderer`, `IsolatedMMU`) and `D3D12_OPTIONS.ResourceHeapTier`. |
| 2: existing memory | `OpenExistingHeapFromAddress` on 64 KiB `VirtualAlloc` (`MEM_COMMIT\|MEM_RESERVE`, `PAGE_READWRITE`); `OpenExistingHeapFromFileMapping` on 64 KiB anonymous read/write mapping. Print HRESULTs; if supported, place a buffer and check CPU writes by GPU readback with no staging upload. |
| 3: custom/placement | Six heap cases: WRITE_BACK/L0/ALLOW_ALL; WRITE_COMBINE/L0/ALLOW_ALL; WRITE_BACK/L0 with each of the three ALLOW_ONLY restrictions; WRITE_BACK/L1/ALLOW_ALL control. Place a UAV-capable buffer and 64×64 RGBA8 texture together on one WRITE_BACK/L0 heap at disjoint allocation-info-aligned offsets; run `WriteToSubresource`/`ReadFromSubresource` round-trip. |
| 4: persistent Map | Two CPU→GPU copies with different magic pairs, followed by UPLOAD→placed GPU copy and direct CPU read through the original pointer. Fence after each; no buffer Unmap/remap or CPU→GPU staging upload in the forward direction. |
| 5: latency | WRITE_BACK/L0 at 64 KiB and 256 MiB; WRITE_COMBINE/L0 at 256 MiB; UPLOAD at 256 MiB, in that order. Touch each 64 KiB interval before timing, then 5 warmups and 40 timed submissions. Every iteration updates offsets 0 and size−64, copies 64 bytes from each end to READBACK and checks all 32 words after its fence. Print total ms, average ms/submit, N/40 and warmup matches. |
| 6: dirty pages | 64 KiB `MEM_WRITE_WATCH`; write bytes at offsets 0 and 16384; `GetWriteWatch(WRITE_WATCH_FLAG_RESET,...)`; print return, count, granularity and dirty offsets. |

All CUSTOM test heaps use explicit CPU-page and memory-pool properties; UPLOAD uses the standard heap type. The six creation cases use 2 MiB heaps, mixed placement uses 2 MiB, and the persistent coherence buffer uses a 64 KiB heap. The latency tests use exactly the named buffer/heap sizes and copy only 128 bytes per submit. READBACK buffers are 128 bytes. Latency includes CPU writes, command-list reset/encoding, transitions, queue submission, fence wait, device-status query and comparison; it is not GPU-only time. Prefault and warmups are excluded. No API-validation or verbose loader tracing is enabled in the timed baseline.

The 64 KiB prefault stride follows the requested experiment; it does not touch every physical 16 KiB host page in the 256 MiB region. A separately named, fully resident memory-overhead experiment is needed before interpreting resident allocation sizes.

## Synchronization and lifetime

The CPU finishes stores and executes an x86 store fence before submission. Each submission has its own D3D12 fence value and a bounded event wait. CUSTOM buffers transition from COMMON to COPY_SOURCE/COPY_DEST and back; UPLOAD stays GENERIC_READ and READBACK stays COPY_DEST. The CPU overwrites or reads GPU-used memory only after the previous fence. Resources remain mapped until release, with placed resources destroyed before their heaps. Wrapping allocations, mapping views and mapping handles are retained through any successful visibility test and released after resources and GPU completion.

The texture is in COMMON, mapped with a null output pointer before the CPU subresource methods, then unmapped after that separate round-trip. A CPU texture round-trip is not a GPU texture coherence test. The baseline does not test simultaneous CPU/GPU access, atomic interoperability, console flush APIs, multiple queues, aliasing barriers, resource reuse while work is in flight, render-target/depth texture placement, other formats or texture GPU readback.

These rules follow the public [Map contract](https://learn.microsoft.com/en-us/windows/win32/api/d3d12/nf-d3d12-id3d12resource-map), [resource barriers](https://learn.microsoft.com/en-us/windows/win32/direct3d12/using-resource-barriers-to-synchronize-resource-states-in-direct3d-12) and [CPU texture access requirements](https://learn.microsoft.com/en-us/windows/win32/api/d3d12/nf-d3d12-id3d12resource-writetosubresource). They do not establish console semantics.

## Backend receipt and supplemental investigations

A selected setting is insufficient to identify the backend. For a **separate, untimed diagnostic** use a new scratch directory:

```sh
python3 docs/research/d3dmetal-zero-copy/run.py \
  --bottle "/absolute/path/to/an/existing/CrossOver/bottle" \
  --scratch /tmp/bx-uma-verify/backend-receipt --receipt-only
```

This mode only creates a device, repeats existing-memory wrapping, and pauses for 10 seconds to permit OS loaded-library inspection; it does not replace or change the six-check baseline. The runner enables `--debugmsg -all,+loaddll,+d3d12`. Inspect the specific probe PID with `lsof -n -p <pid>` or `vmmap <pid>` and preserve the app's `apple_gptk/.../d3d12.dll`, `dxgi.dll` and `D3DMetal.framework` paths. Sample only the intended process, not another contributor's Wine session. Avoid including account names or personal paths in committed diagnostics; disclose any excerpts/redactions. The initial runtime inspection is recorded as an excerpt in its run directory.

Supplemental experiments must be versioned separately and must not alter baseline sizes, checks or iteration counts. Follow-up work includes:

- Resident/physical memory at allocation, prefault, submission and retirement; record measurement tool, baselines, page residency, compression, pressure and variability. Virtual size alone is insufficient.
- Copy profiling that distinguishes shared storage, full-buffer shadow copies and dirty-page copies. Similar small-copy latency is consistent with sharing but does not prove physical aliasing or absence of internal copies.
- GPU texture readback/coherence by format/layout and render/depth usage. Buffer success does not establish these.
- Dirty-page fallback with reset/rearming races, alignment, coalescing, explicit copies, budget accounting and concurrent writes. Baseline `GetWriteWatch` success does not establish a working fallback.

## Independent results and acceptance

Create a new `results/<date>-<chip>-<contributor>-<run>/` directory for every run. Retain raw benchmark stdout/stderr unedited (the local `.gitattributes` preserves their Windows CRLF bytes), exact software/hardware receipts, timeout/exit status, backend evidence, source revision/hash and run limitations. Do not commit the executable, bottle, caches, proprietary frameworks or credentials. A result owner may append new runs; do not rewrite another contributor's measurements. DCO-sign commits with the contributor's existing Git identity and push normally without force. If another contributor updated the shared branch, fetch and reconcile before pushing.

An independent reviewer must re-derive expected values and API legality from primary sources. The author of this probe has not certified it independently. Require independent results from at least two contributors on at least two Apple Silicon generations, with repeated sessions before scaling claims. M1–M5 are coverage targets, not measured support. Follow the full evidence checklist in #473; unresolved mechanisms remain class U. Hardware readbacks are local class A observations, not independent acceptance or Xbox hardware validation.

## Provenance and primary sources

`probe.cpp` and `run.py` are newly authored synthetic research under the repository's GPL-3.0-or-later license. No upstream implementation or emulator code was copied. MinGW-w64 14.0.0 headers and GCC 16.2.0 are externally installed build tools on the initial host. CrossOver 26.3.0.39832, D3DMetal 3.0 and Rosetta 2 are externally installed runtime components; nothing proprietary is redistributed. Existing [upstream registry](../upstream-registry.md) and [reuse matrix](../component-reuse-matrix.md) cover runtime research; this work adds no vendored dependency. Review exact file licenses and update those records before adopting any future third-party source.

Public API references used to author and review the experiments:

- [Architecture1 flags](https://learn.microsoft.com/en-us/windows/win32/api/d3d12/ns-d3d12-d3d12_feature_data_architecture1) and [heap properties](https://learn.microsoft.com/en-us/windows/win32/api/d3d12/ns-d3d12-d3d12_heap_properties).
- [OpenExistingHeapFromAddress](https://learn.microsoft.com/en-us/windows/win32/api/d3d12/nf-d3d12-id3d12device3-openexistingheapfromaddress) and [OpenExistingHeapFromFileMapping](https://learn.microsoft.com/en-us/windows/win32/api/d3d12/nf-d3d12-id3d12device3-openexistingheapfromfilemapping).
- [CreatePlacedResource](https://learn.microsoft.com/en-us/windows/win32/api/d3d12/nf-d3d12-id3d12device-createplacedresource), [Map](https://learn.microsoft.com/en-us/windows/win32/api/d3d12/nf-d3d12-id3d12resource-map), [WriteToSubresource](https://learn.microsoft.com/en-us/windows/win32/api/d3d12/nf-d3d12-id3d12resource-writetosubresource) and [ReadFromSubresource](https://learn.microsoft.com/en-us/windows/win32/api/d3d12/nf-d3d12-id3d12resource-readfromsubresource).
- [GetWriteWatch](https://learn.microsoft.com/en-us/windows/win32/api/memoryapi/nf-memoryapi-getwritewatch).

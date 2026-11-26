# Phase-two standardized supplemental protocol 1

Research [#473](https://github.com/MacosPorters/betterxen/issues/473). Read the [source/API review](review.md) and [separate concurrent-experiment assessment](concurrent-review.md). The original six experiments in `../probe.cpp` and their runner are frozen and unchanged. These additions do not touch #464, production shims, ABI or main.

## Identical reproduction

Use native ARM64 MinGW-w64, Python 3.11+, Apple Command Line Tools, licensed CrossOver/D3DMetal and supported Rosetta. The runner builds a Windows x86-64 probe and a native ARM64 process-accounting observer, uses a private copy of an existing win64 bottle, and writes only to a new scratch directory. Preserve source revision/hash, compiler/runtime receipts and raw logs; remove scratch after archiving evidence. No proprietary binaries are committed.

```sh
git fetch origin research/d3dmetal-zero-copy:refs/remotes/origin/research/d3dmetal-zero-copy
git worktree add --detach ../betterxen-zero-copy-p2 origin/research/d3dmetal-zero-copy
cd ../betterxen-zero-copy-p2
python3 docs/research/d3dmetal-zero-copy/phase2/run.py \
  --bottle "/absolute/path/to/an/existing/CrossOver/bottle" \
  --scratch /tmp/bx-uma-verify/phase2-run
```

Default modes are identity, shader coherence, omitted-dispatch negative control, write-watch, textures, dirty-page scaling and memory footprint. Expected exit is 1 only for the deliberate negative control, and it must specifically report a GPU checksum mismatch. Other failures/timeouts are retained and never treated as passes. Source hash, PE/observer hashes, exact compiler commands, host/software versions and child environment are in receipt.json; every case has unedited stdout/stderr and case.json. `--modes identity,coherence,negative-control,watch,texture,dirty` and `--modes memory` split the suite into serial batches when convenient; do not run GPU batches concurrently.

The common profile is identical on **16 GiB and 24 GiB** hosts: 64, 256 and 1024 MiB, one allocation per fresh process, two repeats per size/type. CPU-only VirtualAlloc is the accounting control; CUSTOM/WRITE_BACK/L0 and standard UPLOAD are the graphics cases. `--large` adds 2048 MiB only when installed RAM is at least 24 GiB; never extrapolate an unrun case. A 4 GiB allocation is intentionally outside the common profile. Review recorded pressure/swap/compression and stop/reduce the study rather than forcing an allocation under pressure; label reduced profiles, do not mix them silently. Close unrelated heavy applications when practical and record background load. Tools run outside timed latency windows.

Compilation (the receipt records actual absolute paths):

```sh
x86_64-w64-mingw32-g++ -std=c++20 -O2 -static -static-libgcc -static-libstdc++ \
  -Wall -Wextra -Werror phase2/probe.cpp -o phase2.exe -lkernel32
xcrun clang++ -std=c++20 -O2 -arch arm64 -Wall -Wextra -Werror \
  phase2/observe.cpp -o observe
file phase2.exe observe
lipo -archs observe
otool -L observe
```

The HLSL is original, embedded in the source, compiled at runtime through the externally installed d3dcompiler_47.dll (cs_5_0; optimization-level-3 requested). Preserve compiler diagnostics: this Wine compiler reports that it ignores flag 0x8000, so no claim is made that the requested optimization was applied. The application loads host d3d12.dll directly and rejects a loaded d3d12_xs.dll. Environment uses an allowlist, ROSETTA_ADVERTISE_AVX=1, CX_GRAPHICS_BACKEND=d3dmetal and WINEDEBUG=-all. The copied bottle alone receives the locally compiled executable's trust/scanner configuration. Original bottle configuration is checked unchanged.

## 1. Physical footprint and overhead

Each memory process warms the shader, queue and fixed 64 KiB output before the baseline sample. The requested large source is then created, mapped, **fully written word-by-word** using index-mixed data, consumed by a full-allocation GPU shader read, fence-waited and released. A CPU-only control skips the GPU source pass explicitly. The shader reduces the entire allocation into 256 checksum words; validation is a checksum test, not a byte-for-byte proof for every source word.

At each handshake point—baseline, created, mapped, fully_initialized, gpu_complete, released—the guest pauses with no GPU work pending. The host resolves the process mapping the unique scratch PE, excluding wineserver's open file handle. It records:

- Three proc_pid_rusage(RUSAGE_INFO_V4) snapshots: physical footprint, resident/wired size, pageins and lifetime maximum; native observer return codes are retained.
- `footprint -p PID -f bytes` and `vmmap -summary PID`, each with bounded timeout and full stdout/stderr.
- `vm_stat` and `memory_pressure -Q`, to identify changing compression/swap/pressure.
- A separate loaded-library excerpt proving the app's D3DMetal DLL/framework; unrelated personal paths are not needed for this receipt.

These are API/process accounting measurements, not a physical-page identity test. Report deltas from each process's baseline and slopes across sizes, with both repeats and the CPU control. Do not add shared/RSS/footprint categories together or call a region label proof of sharing. Allocation caches and driver accounting may survive release. A near-1x slope supports a small *observed process-accounted* overhead; it does not exclude transient copies or memory charged elsewhere. Permission failures of footprint/vmmap remain visible; proc_pid_rusage is a separate signal, not a fabricated replacement for a failed tool.

## 2. Dirty-page scaling

Use a 256 MiB source in WB/L0, WC/L0 and UPLOAD, with 4096- and 16384-byte strides. Fully initialize once. For each stride modify k={0,1,64,4096,all} page-first words, evenly spread including endpoints when k>1. Thus all=65536 for 4 KiB and all=16384 for 16 KiB. Here “dirty” means application-modified strides, not a claim about the driver's actual tracking granularity. Each new write uses an iteration-specific pattern; k=0 changes nothing. Changes from previous k cases remain part of the expected state, so there is no hidden reset that dirties the full allocation between cases.

Three fresh-process sessions rotate k order and alternate heap order. Each k has 5 warmups + 30 recorded iterations. All sources use GENERIC_READ, including CUSTOM, with the same pre-recorded two-copy command list and no source transitions. CPU writes/store fence occur outside the timed window, and their cost is recorded separately as dirty_cpu_ms. Only ExecuteCommandLists + Signal + fence wait are timed as ms; command encoding, device-status check and validation are outside it. Readback checks the first and final stride's first 64 bytes. An additional untimed shader reads all page-first words and validates 256 independent output words against a CPU checksum, so modifying interior pages is not ignored.

Publish all per-iteration samples, median and nearest-rank p10/p90; compare per-session medians rather than pooling correlated samples without disclosure. The untimed verification uses another serialized queue; each submission completes before any subsequent CPU access or queue use. No concurrent queue accesses occur. Flat timing cannot exclude copies on CPU faults, at other API points or below noise; a slope cannot by itself distinguish copies from tracking/cache costs. No lsof/ps/vmmap polling runs during these cases.

## 3. Direct shader access and synchronized coherence

Persistent WB/L0 input (64 KiB) and UAV-capable WB/L0 output (64 KiB). Three rounds overwrite all 16384 input words and poison the 256-word output. After CPU store completion, 256 shader invocations read the input directly through a root SRV and write checksums through a root UAV. Output transitions COMMON→UNORDERED_ACCESS→COMMON; a queue fence completes before direct CPU reads from the same original output pointer. All 256 outputs must match each round. No CopyBufferRegion or CPU→GPU staging upload is used in this mode. The omitted-dispatch control must fail against the poison pattern. This establishes synchronized shader usability, not physical zero-copy or concurrent atomic coherence.

## 4. GPU texture readback and layout limits

Use an intentionally non-aligned width: 65×37 RGBA8, one mip/slice. UNKNOWN texture on CUSTOM WB/L0 is mapped with ppData=nullptr, filled with WriteToSubresource in COMMON, transitioned to COPY_SOURCE and GPU-copied into a READBACK buffer using GetCopyableFootprints. Respect returned row pitch/offset; check exactly 2405 pixel words after the fence. Padding is not interpreted as pixels. A single cold elapsed time is recorded but is not a throughput benchmark or a comparison of layout speed.

The ROW_MAJOR control uses a DEFAULT heap with SHARED|SHARED_CROSS_ADAPTER and a resource with ALLOW_CROSS_ADAPTER. It is filled from a footprint-pitched UPLOAD buffer by GPU copy, then GPU-copied to READBACK and compared. Map is explicitly not called on this CPU-inaccessible control. If shared/cross-adapter creation is unsupported, record the HRESULT and not-run path. No portability inference comes from the earlier development variant that omitted required flags; that variant was corrected before protocol-1 acceptance evidence.

This does not establish texture sampling/render/depth correctness, opaque-layout pointer aliasing, arbitrary formats/mips or a requirement that all textures internally copy. Public restrictions and buffer/texture ownership implications are in the review.

## 5. Adapter and heap-property identification

Match DXGI to **the actual D3D12 device LUID** using GetAdapterLuid and EnumAdapterByLuid, not an assumed adapter index. Record GetDesc2 name, IDs, flags and exact byte counts; these are advertised values, not physical memory measurements. Query architecture flags, heap tier, cross-adapter row-major and standard-swizzle support. Log GetCustomHeapProperties(0, DEFAULT/UPLOAD/READBACK), including Type, CPUPageProperty, MemoryPoolPreference and both node masks. Record guest page size/allocation granularity alongside the native observer's host page size. Loaded-library evidence, not adapter branding alone, identifies the backend.

## 6. Write-watch boundaries

Allocate a separate 64 KiB VirtualAlloc MEM_WRITE_WATCH region. Reset before each disjoint case; write one byte at {0}, {4096}, {12288}, {16383}, {16384}, {0,4096}, {0,4096,8192,12288}, {12288,16384}, {16383,16384}. Report dirty offsets, return/count/granularity and an immediate second query showing the reset state. There are no other threads writing during reset/query. These tests cover separate 4 KiB subpages in one 16 KiB interval and both sides of the 16 KiB boundary. They exercise CPU tracking only; GPU write-watch tracking remains untested and cannot be inferred from IsolatedMMU=false.

## Evidence and provenance

Source and native observer are original GPL-3.0-or-later research, using externally installed Microsoft-compatible headers/runtime APIs and Apple's public local SDK interfaces. No upstream implementation, restricted SDK, shader corpus, game code, proprietary compiler or runtime binary is copied into this directory. Existing runtime/toolchain research provenance remains in the [registry](../../upstream-registry.md) and [reuse matrix](../../component-reuse-matrix.md); any future vendored third-party code requires a per-file review before adoption.

Archive results by chip/contributor/run, preserving numerical logs; disclose any home-path redaction in host diagnostics. Keep the exact input source hashes and execution outcome. Hardware results are local class A evidence; public D3D12 contracts class B; physical aliasing/internal mechanisms and unmeasured generations remain class U. New probe authors do not independently certify their own code. Another contributor must review the implementation and run the same revision before independent acceptance.

# Version-scoped findings

Research issue [#473](https://github.com/MacosPorters/betterxen/issues/473); methodology and limitations are in the [research README](README.md). This is a one-host observation, independent of #464's production implementation. The newly written probe still needs independent source review and reproduction by another contributor.

## Recorded host and runtime

Both sessions used Apple M4; unified RAM 25769803776 bytes (24 GiB); macOS 27.0, build 26A428; CrossOver public 26.3.0, product 26.3.0.39832, build tag cxoffice-26.3.0rc2, timestamp 20260715T103801Z; D3DMetal framework short/build version 3.0, SourceVersion 32047000000000; Rosetta package 1.0.0.0.1788432274; Windows 11 template win11_64 / WineArch win64; native ARM64 GCC 16.2.0 / MinGW-w64 14.0.0_3. The source bottle was copied into a temporary private prefix. ROSETTA_ADVERTISE_AVX=1 and CX_GRAPHICS_BACKEND=d3dmetal were set; timed runs had WINEDEBUG=-all.

Loaded-library inspection in a separate diagnostic confirmed the app's apple_gptk D3D12 DLL, DXGI DLL, D3DMetal framework and Rosetta runtime. The guest reported no d3d12_xs.dll. No timed-run stderr or method-specific D3DMetal log message was emitted. Unsupported calls returned their printed HRESULTs.

## Baseline outcomes

These exact outcomes agreed in both local sessions:

| Experiment | Observation | Evidence scope |
| --- | --- | --- |
| Architecture | UMA=0, CacheCoherentUMA=0, TileBasedRenderer=0, IsolatedMMU=0; ResourceHeapTier=2 | API-reported values, not a description of the Mac's physical topology |
| Existing-memory opens | Address and mapping both 0x80004001 (E_NOTIMPL) | CPU visibility through these heaps not tested because no heap opened |
| CUSTOM heaps | All five WRITE_BACK/WRITE_COMBINE L0 cases 0x00000000; WRITE_BACK/L1 control 0x80070057 (E_INVALIDARG) | Creation only for restricted heap cases; no render/depth texture usage tested |
| Mixed placement | UAV-capable 64 KiB buffer at 0; 64×64 RGBA8 texture at 65536; both alive on one heap | Texture CPU round-trip 4096/4096 words; no GPU texture readback |
| CPU→GPU persistent map | 11223344/A1B2C3D4 then 55AA00FF/F00DBA5E both matched | Fence-separated buffer copies; no Unmap/remap or staging upload |
| GPU→CPU persistent map | C001D00D/9E3779B9 matched at original CPU pointer | GPU copy from UPLOAD, fence, direct CPU read |
| Dirty-page tracking | Return 0; count 2; granularity 4096; offsets 0 and 16384 | Single allocation/reset query; fallback correctness not established |

Hardware execution is local class A evidence within these workloads. Public D3D12 API contracts are class B; internal sharing/copy mechanism and cross-generation behavior are class U. These are not independent acceptance or Xbox compatibility claims.

## Latency observations

Each row has 5/5 warmup matches and 40/40 timed matches. Size increases by 4096× between the two WRITE_BACK cases. Each submission copies just 128 bytes; timings include CPU encoding, fence synchronization and validation.

| Session | Case | Total ms / 40 | Average ms/submit | Exact matches |
| --- | --- | ---: | ---: | ---: |
| Initial | WRITE_BACK/L0 64 KiB | 20.635900 | 0.515898 | 40/40 |
| Initial | WRITE_BACK/L0 256 MiB | 30.937300 | 0.773432 | 40/40 |
| Initial | WRITE_COMBINE/L0 256 MiB | 11.762100 | 0.294052 | 40/40 |
| Initial | UPLOAD 256 MiB | 10.118800 | 0.252970 | 40/40 |
| Shared-runner verification | WRITE_BACK/L0 64 KiB | 11.558300 | 0.288957 | 40/40 |
| Shared-runner verification | WRITE_BACK/L0 256 MiB | 11.961700 | 0.299042 | 40/40 |
| Shared-runner verification | WRITE_COMBINE/L0 256 MiB | 9.973900 | 0.249347 | 40/40 |
| Shared-runner verification | UPLOAD 256 MiB | 8.198200 | 0.204955 | 40/40 |

The two sizes do not show latency proportional to the 4096× size difference in these sessions. This is consistent with shared storage or a sparse-copy mechanism; it does not establish O(1) behavior generally, physical aliasing, or guaranteed zero-copy. The machine was not idle; a background C++ language-service process was observed using about one CPU core before the initial run. Session variation is substantial, and no statistical confidence or performance improvement is claimed.

## Raw records

- [Initial stdout](results/2026-11-26-apple-m4-arths17-initial/stdout.txt), [stderr](results/2026-11-26-apple-m4-arths17-initial/stderr.txt), [receipt](results/2026-11-26-apple-m4-arths17-initial/receipt.json), [backend excerpt and launch notes](results/2026-11-26-apple-m4-arths17-initial/backend-receipt.txt).
- [Shared-runner stdout](results/2026-11-26-apple-m4-arths17-runner/stdout.txt), [stderr](results/2026-11-26-apple-m4-arths17-runner/stderr.txt), [receipt](results/2026-11-26-apple-m4-arths17-runner/receipt.json). This run records the exact published source hash and executed PE hash. The initial executable was not retained/hashed before the diagnostic extension was added; its incomplete receipt is explicitly marked.

## Unresolved work for the collaborators

1. Independently review the probe against public contracts and reproduce the same source revision on another Apple Silicon generation; append independent raw logs and repeated timing sessions.
2. Measure resident/physical memory and internal copy activity in a separately named supplemental experiment. Neither was measured here. A 64 KiB prefault stride does not make every host page resident; do not infer a second 256 MiB allocation from virtual size or these timings.
3. Establish GPU texture coherence/readback, format/layout and render/depth restrictions separately. The current texture check is only a CPU round-trip.
4. Verify lifetime/reuse, dirty-page rearming/reset races and explicit fallback copying where necessary. The baseline only serializes accesses with per-submission fences.
5. Give #464 evidence-scoped recommendations: these runtime versions reject both caller-memory wrapping APIs, while host-allocated custom L0 heaps support the observed buffer access. This does not supply an arbitrary-xmem-to-heap bridge, justify silent fallbacks or permit console flush operations to become no-ops.

# Direct compute coherence and write-watch

The [B runner](../../copy-detection/README.md) builds this original C++20 guest and runs the original HLSL through the installed d3dcompiler_47 DLL with `main`, `cs_5_0` and requests optimization level 3. The tested compiler emits a diagnostic that it ignores flag 0x8000. The runner records compiler/module identity and DXBC hashes. It retains stdout/stderr, pipeline and device errors, the exact HLSL and every comparison.

## Correctness and throughput

The shader consumes a CPU-written dataset through a raw root SRV and writes a raw root UAV. WB and WC trials keep each pointer mapped through dataset changes and fence-separated GPU output reads. An UPLOAD-to-WB trial and a fully staged DEFAULT-to-DEFAULT trial provide controls. DEFAULT staging and poison copies finish before the measured dispatch. Any output staging copy is ordered by a UAV-to-COPY_SOURCE transition and sits outside the GPU timestamp pair.

For a result slot `i`, the CPU independently enumerates the conceptual input word indices, regenerates input values from the seed, performs a widened sum and reduces it modulo 2^32. GPU arithmetic is unsigned 32-bit. Sample extremes, mismatch positions and exact match counts are retained. Poisoning each prior output detects stale data. The omitted dispatch control compares against the poison so the predicted negative behavior is explicit. The separate frozen historical shader probe uses an intentionally failing omitted-dispatch oracle.

The four configurations use identical dispatch shapes, but staging and command encoding differ between direct and DEFAULT output. Compare useful GPU payload rates using the GPU timestamp interval. Submission/fence timings include each configuration's actual transfer and barrier commands.

| Workload | Input reads | Output writes |
| --- | --- | --- |
| Both | Every word in 4 MiB | Every result word in 4 MiB |
| Full read | Every word in 4 MiB | 256 KiB of dependent output |
| Sparse 4 KiB | One word per 4 KiB of 4 MiB | 4 KiB of dependent output |
| Sparse 16 KiB | One word per 16 KiB of 4 MiB | 1 KiB of dependent output |
| Write | Zero input loads by uniform zero iteration count | Every result word in 4 MiB |
| Omitted | Zero | Zero, and CPU verifies unchanged poison |

The read workloads include shader indexing, accumulation and an observable output. Rates apply to those workloads and do not measure isolated memory-pin bandwidth or hardware counters for physical transactions.

## Write tracking

A CPU-only guest tests a fresh allocation, one subpage, nonresetting queries, adjacent 4 KiB writes, both sides of a 16 KiB boundary, automatic and separate reset, rearming, read-only and same-value store. A separate D3D12 process attempts to wrap the actual watch-enabled region and reports capability and exact HRESULT. GPU tracking remains untested if the region cannot be imported. The fixture does not apply GetWriteWatch to a foreign pointer. [Tracking and synchronization contract](../contracts/synchronization-mapping-tracking.md).

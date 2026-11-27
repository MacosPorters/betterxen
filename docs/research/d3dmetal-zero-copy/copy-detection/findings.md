# Workstream B findings for #473

Two M5 sessions reproduced synchronized direct SRV/UAV access on the tested CUSTOM WB and WC heaps. The placement and tracking failures impose API boundaries on a BetterXen design. The copy timing shows configuration- and session-dependent latency and leaves physical page identity and internal copies unresolved.

## Host, runtime and receipts

Apple M5, 16 GiB unified RAM, 16 KiB host pages, macOS 27.2.1 build 26B131, CrossOver 26.3.0.39832, Wine 11.0-8726-g2e2f5fca349, D3DMetal 3.0 and Rosetta 1.0.0.0.1793943009 with `ROSETTA_ADVERTISE_AVX=1`. Compiled with native ARM64 MinGW GCC 16.2.0 to x86-64 PE32+. The loaded GPTK D3D12 DLL, D3DMetal framework and Rosetta runtime were verified for each graphics probe process. The write-watch-only probe did not load D3D12. Runs used a temporary clone of a Win64 bottle. Pressures were Warning with substantial compression and swap, including about 6–8 GiB of swap at performance-group start. The sessions describe this host under those conditions.

| Group | Archive |
| --- | --- |
| Normal-pressure preflight | [No workload started](../results/2026-11-27-apple-m5-atesahmet0-b-small-01/receipt.json) |
| Placement and tracking | [First session](../results/2026-11-27-apple-m5-atesahmet0-b-small-02/receipt.json), [second and clock check](../results/2026-11-27-apple-m5-atesahmet0-b-small-03/receipt.json), [clock confirmation](../results/2026-11-27-apple-m5-atesahmet0-b-small-05/03-clock/stdout.txt) |
| Copy and CPU rates | [Seed 473](../results/2026-11-27-apple-m5-atesahmet0-b-sweep-01/receipt.json), [seed 947](../results/2026-11-27-apple-m5-atesahmet0-b-sweep-02/receipt.json) |
| Compute | [Seed 473](../results/2026-11-27-apple-m5-atesahmet0-b-compute-01/receipt.json), [seed 947](../results/2026-11-27-apple-m5-atesahmet0-b-compute-02/receipt.json) |
| Metal System Trace | [Urgent stop before guest workload](../results/2026-11-27-apple-m5-atesahmet0-b-trace-01/receipt.json), [sanitized target-PID XML inputs](../results/2026-11-27-apple-m5-atesahmet0-b-trace-inputs-01/receipt.json), [B1/B2 audit](b1-b2-audit.md) |
| A same-source reproduction | [Alias](../results/2026-11-27-apple-m5-atesahmet0-b-a-alias-01/receipt.json), [texture](../results/2026-11-27-apple-m5-atesahmet0-b-a-texture-01/receipt.json), [review](review-a.md) |

## Hidden-copy detection

The two seeded sweeps used 1, 16 and 64 MiB across WB/WC/UPLOAD and both strides, 18 guest processes per session. All 5040 measured copy/control iterations matched their oracle, including poison checks. Each session also recorded 216 CPU measurements and 216 empty/signal-only measurements. Warmups and individual distributions are in the archives.

The following rows show 64 MiB WB with a 12-byte GPU copy and two distinct write strides. Times are medians in microseconds. Full rewrite touches every byte. A word per page writes 64 KiB of data at 4 KiB stride or 16 KiB at 16 KiB stride.

| Seed | Stride | No writes | One page | 64 pages | One word per page | Full rewrite | CPU write, word per page / full rewrite |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 473 | 4 KiB | 184 | 364 | 224 | 364 | 463 | 165 / 1296 |
| 947 | 4 KiB | 181 | 393 | 383 | 188 | 377 | 160 / 1241 |
| 473 | 16 KiB | 249 | 364 | 236 | 377 | 446 | 42 / 1618 |
| 947 | 16 KiB | 360 | 355 | 371 | 373 | 406 | 38 / 1286 |

WB 64 MiB full-rewrite small-copy completion increased versus the no-write case in these four comparisons. The increase ranged from 46 to 279 µs, with different baselines. The word-per-page cases show substantial differences between seeds and strides, including nearly flat repetition and nonmonotonic behavior. Across all 64 MiB sources and both seeds the small clean-copy medians span 181–401 µs. A measured explicit full-allocation GPU copy at that size took about 2.09–2.36 ms median in the no-write cases. The small-copy path therefore does not show a serialized copy with the cost of that explicit full READBACK workload. That comparison does not rule out a faster or overlapped backend copy, copy outside the measured interval, or work restricted to referenced pages. The returns also fit cache ownership, page activity, queue scheduling, and host pressure as alternative contributors. Full-byte CPU write time is visible before submission and varies independently of it.

The earlier M5 report touched one word per 4 KiB and verified only endpoints. The historical M4 timing likewise used endpoint copies, with a separate checksum run. The current 64 MiB sweep validates endpoint/interior sentinels or every copied word and distinguishes whole-byte rewrites. There is no validated upper bound at 256 MiB or across other D3DMetal versions. There is also no direct evidence that the CPU and GPU mappings identify the same physical pages.

## Timing and effective bandwidth

Each table entry is the range of case medians across both seeds, both strides and three source heap types. Full GPU copies use the no-write case with an identical READBACK destination. Rates are decimal GB/s.

| Size | CPU checksum read | CPU memset write | GPU copy payload / fence interval |
| --- | ---: | ---: | ---: |
| 1 MiB | 8.91–17.53 | 32.36–88.12 | 2.07–4.60 |
| 16 MiB | 16.03–16.55 | 58.90–76.30 | 5.56–20.41 |
| 64 MiB | 15.74–16.51 | 47.70–56.38 | 28.38–32.33 |

Empty submit medians across both sessions were 0.30–0.60 µs for reset/encode/close, 1.80–2.95 µs for Execute, 0.20–0.30 µs for Signal, 40.25–55.05 µs for fence arm/wait and 42.60–58.30 µs total. Signal-only was 43.00–59.90 µs. These controls expose the fixed host synchronization scale and are not blindly subtracted from each workload.

D3D12 timestamp duration was unavailable. The runtime reported a 60 Hz timestamp query frequency. Regular query pairs were zero-initialized and read back 0/0. A separate fence-separated clock check at roughly 153 ms left **both distinct poison slots unchanged** even when their respective query was resolved. The old diagnostic printed the arithmetic difference of those sentinels as 16666.667 µs. The corrected clock confirmation reproduced the unchanged sentinels after about 158 ms and reported no GPU duration. The first Metal System Trace recorded a paused target and stopped at Urgent before the workload. After pressure recovered, separate captures succeeded. Fence-interval rates include all queue work and scheduling in the interval and are not device-only bandwidth. See the [clock stdout](../results/2026-11-27-apple-m5-atesahmet0-b-small-03/03-clock/stdout.txt).

## Direct shader access and persistent mapping

Each of four configurations ran six modes, twelve measured iterations and three warmups per mode, in each of two sessions. All 576 measured comparisons and their warmups were exact. That includes the two direct CUSTOM outputs and the UPLOAD→WB control using the original mapped pointers through every round. The DEFAULT control used explicit staging. Output remained poisoned when dispatch was omitted.

The table reports median submission-to-fence completion in µs for seed 473 / 947. The full-read kernel reads 4 MiB and writes a compact result. Store-only writes 4 MiB. The sparse kernels read 4096 or 1024 bytes across the same 4 MiB allocation and write equally sized results.

| Source → destination | Full read | Sparse 4 KiB | Sparse 16 KiB | Store only | Omitted dispatch |
| --- | ---: | ---: | ---: | ---: | ---: |
| WB → WB | 314 / 572 | 204 / 189 | 188 / 200 | 308 / 488 | 60 / 48 |
| WC → WC | 535 / 502 | 178 / 175 | 191 / 180 | 506 / 393 | 57 / 52 |
| UPLOAD → WB | 322 / 524 | 203 / 191 | 185 / 186 | 258 / 500 | 56 / 51 |
| DEFAULT → DEFAULT | 275 / 217 | 228 / 180 | 222 / 167 | 326 / 253 | 268 / 233 |

WB's full-read rate is approximately 13.3 / 7.3 GB/s and its store-only rate is 13.6 / 8.6 GB/s on the fence interval. WC's are approximately 7.8 / 8.4 and 8.3 / 10.7 GB/s. DEFAULT includes dependent readback in its fence interval. The staged and direct cases share shader commands while differing in transfer and barrier work. Sparse operations are completion-latency dominated. Useful-byte rates in these cases cannot be converted into physical transaction bandwidth or an intrinsic ranking of heap types. The shader compiler reported ignoring requested optimization flag 0x8000, with the same DXBC hash in all four modes and both sessions. An independent Python expression regenerated the two reported word values in each of the 720 samples including warmups and confirmed each pair.

The CPU finishes stores before GPU access, and observes writes after a queue fence. No test relies on concurrent CPU/GPU access, mapped-pointer atomics, or a console cache contract. [Synchronization and mapping contracts](../gpu-correctness/contracts/synchronization-mapping-tracking.md).

## GPU intervals from Metal System Trace

Each configuration used a separate attached Instruments capture. The analysis joins the verified probe PID's GPU work to the Metal application's encoder IDs. All 75 shader dispatches and 15 omitted controls per configuration were accounted for. Staging and readback blits remain outside the reported shader intervals. Some blits run on a trace channel named Compute, so the analysis distinguishes blits and shaders by encoder type and matching command-buffer and encoder IDs.

The table reports GPU duration median and median absolute deviation in µs for 12 measured iterations, followed by median useful payload rates in decimal GB/s. Each case has three additional warmups. Full read includes the compact dependent output. Store-only has no shader input loads.

| Heap source → output | Full read µs median ± MAD | Full read GB/s | Store-only µs median ± MAD | Store-only GB/s | Sparse read 4 / 16 KiB stride µs |
| --- | ---: | ---: | ---: | ---: | ---: |
| WB → WB | 110.23 ± 1.56 | 38.05 | 51.02 ± 3.08 | 82.21 | 6.75 / 3.92 |
| WC → WC | 106.00 ± 0.81 | 39.57 | 39.85 ± 8.40 | 105.25 | 4.98 / 1.98 |
| UPLOAD → WB | 104.54 ± 5.10 | 40.12 | 28.23 ± 4.29 | 148.62 | 6.60 / 5.71 |
| DEFAULT → DEFAULT | 32.12 ± 0.92 | 130.56 | 18.83 ± 0.04 | 222.70 | 1.83 / 1.25 |

The 64 MiB full-copy profile used the same 15 copy workloads per source heap, identical READBACK destinations and explicit fences. It verified all 16777216 words per copy. WB, WC and UPLOAD median GPU intervals were 1319.44, 1236.90 and 1476.63 µs, respectively. Median payload rates were 50.88, 54.26 and 45.87 GB/s. The MADs were 180.56, 191.69 and 299.42 µs. The first UPLOAD guest completed its checks but Instruments did not save its capture within the original timeout. The separate retry saved and matched all 15 intervals. The instrumented copy fixture uses a different data initialization from the uninstrumented sweep. Compare only the same kind of measurement.

These are per-encoder device-side timings, with GPU execution, cache and trace effects included. Application payload divided by duration does not reveal physical memory transactions. An individual interval can be much longer than its case's median, and cells come from separate single captures on a pressured host. Inspect the individual samples before using the rates for design decisions. The substantial difference between the two write-only workloads with WB output is a direct cross-capture reproducibility limit.

[WB trace](../results/2026-11-27-apple-m5-atesahmet0-b-trace-02/gpu-samples.json), [WC trace](../results/2026-11-27-apple-m5-atesahmet0-b-metal-wc/gpu-samples.json), [UPLOAD→WB trace](../results/2026-11-27-apple-m5-atesahmet0-b-metal-upload-wb/gpu-samples.json), [DEFAULT trace](../results/2026-11-27-apple-m5-atesahmet0-b-metal-default/gpu-samples.json), [WB copy trace](../results/2026-11-27-apple-m5-atesahmet0-b-metal-copy-wb/gpu-samples.json), [WC copy trace](../results/2026-11-27-apple-m5-atesahmet0-b-metal-copy-wc/gpu-samples.json), [UPLOAD copy trace](../results/2026-11-27-apple-m5-atesahmet0-b-metal-copy-upload-retry/gpu-samples.json).

## Alignment and write tracking

The first independent M5 placement probe found CPU Map remainders of 16384 for WB/WC/UPLOAD and 0 for the VirtualAlloc control. A later fresh process returned 0 for the valid maps. The same-source alias runs returned 32768 and 16384. All checked valid GPU VAs remained 64 KiB aligned. For invalid creation only, a 64 KiB heap accepted offsets 1 and 65536, and a 128 KiB heap accepted offset 1 with the allocation span otherwise in bounds. None of these resources was used after the creation call. Caller alignment and bounds validation remain required.

A CPU-only watch-enabled allocation gave exact 4096-byte page sets through eleven phases in each small group. Same-host-page subpages were distinguishable, boundary stores appeared at 12288 and 16384, query and separate reset cleared tracking, and rearming captured later stores including a same-value write. A read did not set a dirty bit. The watch-enabled allocation could not be wrapped by D3D12 (E_NOTIMPL). IsolatedMMU was false. General GPU-write tracking and coherence on a foreign Map pointer remain unestablished.

## BetterXen handoff

1. Use D3D12-owned CPU-visible buffers as a candidate for allocations whose alignment, lifetime and ownership contracts match the actual returned pointer. Keep heap-relative validation, CPU pointer checks and fence-delayed reuse explicit. A mapped aligned slice does not automatically give a legal corresponding placed-resource offset.
2. Use the ordinary texture layout, initialization, state and copy rules for opaque textures. The M4/M5 mixed and alias fixtures establish the reported functional subcases. They do not establish raw pointer equivalence or general texture inheritance.
3. Keep device-owned and predevice CPU-owned memory paths explicit. Existing-memory import returned E_NOTIMPL in the tested cases. Write-watch may reduce CPU-to-GPU explicit-copy work with controlled writer ownership, rearming and retry handling. GPU-to-CPU synchronization requires a fence and a supported visibility/transfer contract.
4. Use one logical budget and monitor separate process-accounting or runtime-retention measurements. A's large runs observe retained charges and incomplete repeats. No Class A evidence here proves a leak or a universal retention amount.
5. Preserve #464's scope. [PR #486](https://github.com/MacosPorters/betterxen/pull/486) reports vkd3d execution and already guards placement. Its statements about physical sharing and absence of copies exceed the scope established by this research.

Outstanding items include repeated GPU traces at normal pressure, independent repetition of B on M4, normal-pressure sensitivity, same-host accounting repeats for large allocations, generalized GPU-write tracking, any stronger physical-page or copy-mechanism claim, and reciprocal acceptance of the architecture handoff. Workstream B's own runs are author-run. The A review records separate source analysis, same-source M5 reproduction, and separately implemented placement verification.

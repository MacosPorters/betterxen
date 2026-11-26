# Physical-footprint observations and limits

The historical data inventory and corrected A2 data below are author observations on one Apple M4 with 24 GiB, macOS 27.0 (26A428), CrossOver 26.3.0.39832 and loaded D3DMetal 3.0. They are not independent acceptance. The original six baseline experiments remain unchanged.

## Existing data and correction

The historical 18-case profile has two fresh-process repeats at 64/256/1024 MiB for graphics-warmed CPU allocation, CUSTOM/WB/L0 and UPLOAD. Its baseline is about 340–341 MiB, dropping about 40 MiB by the next checkpoint. It fully writes the source and runs a checksum shader across all source words. However, it leaves command/pipeline/output objects live at its release checkpoint. It does not establish retention after their disposal. Exact historical sources/receipts and the earlier timeout are preserved in the [inventory](inventory/README.md).

The A2 correction disposes warm-up command objects before its settled baseline and separates resource release, command/pipeline/output disposal, a 500 ms settled interval and device release. It adds CPU-only processes without any D3D12 device, alongside matched graphics-warmed CPU allocation controls. Loaded-library evidence is collected separately at a paused stage, and exact-pointer vmmap is added.

The [first A2 profile](../results/2026-11-26-apple-m4-arths17-a2-memory-partial/receipt.json) completed 12 cases: four types at three sizes. The second session was interrupted after the host reported kernel pressure level 2; it is incomplete, not a pass. Operator stop, raw logs, shader bytes/hashes, failed/missing diagnostic and exact executed source snapshots are preserved. Later checking identified the actual Wine engine as `wine-11.0-8726-g2e2f5fca349`; the original runner's nonexistent `wine64 --version` lookup failed with exit 127, so its CrossOver version was known but its engine-version field was missing. Do not rewrite that receipt as a success.

## First-session accounting

MiB are 2^20 bytes. Each number below is the median of three correlated proc_pid_rusage readings at one paused checkpoint, not three independent sessions. Deltas are from that process's settled baseline. Values are rounded only in this table; raw bytes remain in the logs.

| Type | Requested MiB | Baseline MiB | After full initialization delta MiB | After full GPU consumption delta MiB | After settled complete retirement delta MiB |
| --- | ---: | ---: | ---: | ---: | ---: |
| CPU-only | 64 | 13.26 | 64.16 | 64.17 (GPU not run) | 0.03 |
| CPU with graphics warmed | 64 | 300.78 | 64.71 | 64.71 (GPU not run) | 0.57 |
| CUSTOM WB/L0 | 64 | 300.62 | 0.09 | 40.50 | 0.19 |
| UPLOAD | 64 | 300.21 | 0.09 | 40.54 | 0.22 |
| CPU-only | 256 | 13.15 | 256.59 | 256.59 (GPU not run) | 0.07 |
| CPU with graphics warmed | 256 | 300.46 | 256.54 | 257.18 (GPU not run) | 0.78 |
| CUSTOM WB/L0 | 256 | 300.38 | 256.58 | 297.04 | 256.74 |
| UPLOAD | 256 | 301.12 | 256.52 | 296.86 | 256.54 |
| CPU-only | 1024 | 13.19 | 1026.28 | 1026.28 (GPU not run) | 0.26 |
| CPU with graphics warmed | 1024 | 300.17 | 1026.73 | 1026.73 (GPU not run) | 0.90 |
| CUSTOM WB/L0 | 1024 | 301.36 | 1026.51 | 1066.96 | 1026.66 |
| UPLOAD | 1024 | 301.04 | 1025.98 | 1066.34 | 1026.04 |

The CPU control releases its large charge; larger graphics allocations remain mostly process-accounted at complete retirement and device release. This is stronger retirement scoping than the historical resource-release checkpoint, but still does not isolate runtime/driver caching or prove a leak. The device-release checkpoint leaves the process and loaded DLL/framework alive.

The 64 MiB graphics allocations add little charge at creation/initialization. Their mapped pointers lie in a 256 MiB IOAccelerator virtual region in this run. Larger allocations have separate pointer-specific regions. This supports an existing-pool explanation, rather than a linear assumption that every allocation creates a new equally sized physical charge. It does not prove which physical pages back the data.

## Accounting is not residency or page identity

The same first-session 1 GiB WB case reports approximately 1368 MiB physical-footprint accounting at GPU completion, but only 448 MiB resident size. The graphics-warmed CPU control has about 1327 MiB footprint accounting and 53 MiB RSS at that checkpoint. System compression/swap and the sequential sampling windows confound physical-residency interpretation. A near-1x size slope is an accounting observation, not proof of unique physical allocation, no duplication, or absence of copies elsewhere.

The exact mapped CPU pointers also refute an unqualified alignment assumption: the 64 MiB WB/UPLOAD pointers were 16384 modulo 65536, and the 256/1024 MiB WB/UPLOAD pointers were 49152 modulo 65536. Heap/resource placement alignment is not CPU-pointer alignment. See the [ownership matrix](inverted-ownership.md).

`footprint`, `vmmap`, native accounting and system-pressure tools measure different categories at different times. Virtual address ranges, resident pages, process ledger charges and driver-retained memory must not be summed or relabeled as physical page identity. Profiler permissions/timeouts and missing tools remain explicit outcomes.

## Pressure correction and remaining evidence

The initial guard used memory_pressure -Q free percentage only. The kernel subsequently reported pressure level 2 even with 51% reported free. Apple's [XNU documentation](https://github.com/apple-oss-distributions/xnu/blob/main/doc/vm/memorystatus_notify.md) calls level 2 urgent/warning-equivalent. This host's pressure state during earlier stages was not recorded by that stronger signal, so it is unknown. Existing swap and substantial background CPU load were recorded and limit interpretation.

A-local-2.1 adds kernel-level receipts and requires normal pressure at case and stage boundaries, with retained free-percentage guards. Fresh normal-pressure sessions and reciprocal review are required before treating the profile as reproducibly settled. The partial A2 results remain valid records of the workload under their stated conditions, not a completed repeated profile or a controlled benchmark.

Physical page identity remains unestablished. No result here certifies physical zero-copy, console coherence or #464. Independent review must inspect pinned source, exact shader/PE identities, full raw snapshots, failure paths and controls rather than accepting this interpretation.

## Bounded continuation and remaining repeats

After the requester freed memory and allowed bounded warning-pressure work under a 23 GB ceiling, [A-local-2.2 continuation](../results/2026-11-26-apple-m4-arths17-a22-memory-pressure-stop/receipt.json) completed six processes: all four 64 MiB controls/types and both CPU 256 MiB controls. The next WB 256 MiB process was killed at a paused checkpoint when kernel pressure returned to 2. Its exit -9 and harness stop reason are preserved, not a completed GPU result. The private Wine server was stopped and the source bottle configuration remained unchanged. No additional 1 GiB source was started in this continuation.

The maximum recorded conservative non-file-cache occupied estimate in completed snapshots was 21,187,198,976 bytes, below the 23,000,000,000-byte ceiling; this is a sampled metric, not a guarantee about unobserved intervals or an exact Activity Monitor value. The urgent-pressure guard stopped the profile independently of that ceiling. Both profiles remain explicitly partial. Further repeated large/normal-pressure characterization remains open rather than forcing execution under urgent pressure.

# Allocation, pooling, fragmentation and retirement

Local author observations on the M4/CrossOver/D3DMetal environment recorded by the [corrected trace receipt](../results/2026-11-26-apple-m4-arths17-a22-pool-corrected/receipt.json). Independent acceptance is pending. The trace is an original bounded experimental allocator, not production code or an evaluation run of AMD's allocator.

## Comparison

Six fresh processes form three matched seeded pairs (473, 474, 475). Each uses 24 buffer slots, two warmup cycles, ten timed cycles, and exactly the same live sizes/data/GPU readback work in both modes. Individual mode creates a CUSTOM/WB/L0 heap per allocation; pooled mode places resources in a 32 MiB slab. Each measured process has 360 allocation samples and 360 frees; samples within cycles are correlated. Reported distributions, raw samples, seeds and executed sources are retained. No background profiler runs inside the API windows.

| Seed | Individual heap creation median ms | Pool suballocation median ms | Individual placement median ms | Pool placement median ms | Individual cycle median ms | Pool cycle median ms |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| 473 | 0.01250 | 0.00010 | 0.00070 | 0.00100 | 12.62715 | 12.47335 |
| 474 | 0.01230 | 0.00010 | 0.00070 | 0.00090 | 12.33830 | 11.90645 |
| 475 | 0.01265 | 0.00010 | 0.00070 | 0.00100 | 12.68280 | 12.33040 |

Cold pooled heap setup was separately 6.5561, 6.8396 and 6.5303 ms. Do not omit that cost when discussing first use. Map median was near the reporting floor in both modes. Whole cycles include initialization, raw-log output, GPU copies/fence waits, validation and free-list operations, so they are not isolated allocator timings. The live snapshot pause is subtracted. Zero/near-zero API samples reflect measurement precision and call overhead, not zero execution cost.

Within this workload, replacing repeated heap creation with a free-list operation reduces the measured allocation-call component. The cycle difference is small, placement is slightly costlier, the host is busy and uses the explicit warning-pressure profile. An earlier preserved pair even had the opposite whole-cycle ordering (12.2991 versus 15.8891 ms). These data do not establish a stable application-level speedup or a production benefit. No title/framerate or cross-generation claim is supported.

## Memory and fragmentation

The first trace version created its pooled heap before the baseline. Its raw runs remain [preserved](../results/2026-11-26-apple-m4-arths17-a22-pool-first-pair/receipt.json), with [continuation pairs](../results/2026-11-26-apple-m4-arths17-a22-pool-pre-correction/receipt.json). Their API samples are inspectable, but baseline deltas hide slab creation. The corrected source moves slab creation after a matched baseline and adds heap_ready. Its initial TRACE line still contains the pre-allocation setup value 0; actual cost is the subsequent POOL_SETUP record. Current source renames that initial field pre_setup_ms only; the timing results above are tied to the exact archived executed source. The later omitted-copy control exercises the source after that logging-only rename.

In corrected seed 473, individual baseline was 34.58 MiB and heap_ready 34.67 MiB; pool baseline was 34.49 MiB and heap_ready 66.73 MiB. Thus the one-time slab charge is visible. At the live checkpoint both are about 332 MiB, and after command/slab/readback release both remain about 292 MiB. This shared accounting behavior does not establish a memory saving from pooling or identify the retained backing. The three pairs have similar absolute readings; all raw tool categories and RSS/footprint distinctions remain available.

The pool reserves 32 MiB irrespective of live payload. Individual heaps reserve the sum of resource allocation sizes. In seed 473's first measured holes checkpoint, both have 5,963,824 live requested bytes and 6,750,208 live allocation-aligned bytes, so alignment waste is 786,384 bytes. The pool has 26,804,224 free bytes in 13 spans, with largest span 22,675,456 bytes. Individual heaps do not have a reusable cross-heap free span; their printed zeros are not evidence of zero fragmentation.

Every cycle records full/holes/refill/empty snapshots and checks that all free spans coalesce back to one 32 MiB range. The trace demonstrates bounded recycling and quantifies logical slack/waste. It does not saturate arbitrary allocation patterns or establish a general fragmentation bound. The fixed slab is deliberately oversized for this trace; an adaptive production allocator could behave differently. Logical payload/slab/slack statistics are not physical memory measurements.

## Resource retirement and checks

Each process has 24 successful guard/interior readbacks (two phases in each of 12 cycles), plus a separate real gate-delayed copy. Before releasing the gate, the completion fence is below the retirement target and the allocation is withheld. After opening the gate and waiting with a timeout, readback matches before any release/reuse. A separate future-value policy control is also retained. This is a bounded one-queue retirement fixture, not exhaustive multi-queue lifetime certification.

The [omitted-copy negative control](../results/2026-11-26-apple-m4-arths17-a22-pool-negative/receipt.json) deliberately leaves poisoned readback in place and must exit 1 specifically for a guard/interior mismatch. It is a control success only if that exact failure is observed; it is not a passing GPU data path. Production changes and AMD allocator integration remain outside this task.

Recommendation: pooling is a plausible candidate for amortizing heap-creation overhead, with a measured cold reservation/setup cost and a required bounded slack/retirement policy. It is not yet a demonstrated overall memory/performance improvement. Prefer evaluating the [established allocator candidate](allocator-assessment.md) against the same trace before designing production machinery.

# Logical fragmentation and delayed-retirement model

This small native ARM64 experiment fills a gap in the original 32 MiB pooled GPU trace, which did not saturate the pool. It models bounded spans and fence-delayed eligibility, without allocating a 32 MiB backing buffer or using a GPU. Logical fragmentation is not driver fragmentation or physical memory usage.

```sh
python3 docs/research/d3dmetal-zero-copy/allocation-profiling/fragmentation/run.py --scratch /tmp/bx473-fragmentation-review
```

The original C++ trace maintains its free-list; a separate Python oracle derives free gaps from the union of live and pending allocations. It validates bounds, alignment, overlap, failed-allocation feasibility, payload/reservation waste, total/largest usable free spans and delayed retirement at every snapshot. It does not import or reuse the C++ allocation algorithm. Both implementations are still author work and require independent contributor review.

The fixed negative case fills eight 4 MiB slots, retires alternating slots and leaves 16 MiB free in four 4 MiB gaps. An 8 MiB request fails. Marking the remaining slots free but leaving their retirement fence incomplete still cannot satisfy it; only completing that fence coalesces a full 32 MiB range and permits the request. Three seeded traces then exercise 2000 operations apiece, varied 4 KiB-to-3 MiB requested sizes, 64 KiB/2 MiB logical alignment and varied retirement delays. These alignments are chosen allocator-policy inputs, not host pointer/page guarantees.

[Raw traces and separate oracle outputs](../../results/2026-11-26-apple-m4-arths17-logical-fragmentation/receipt.json) record seeds 473/474/475, each with 2004 checked statistics snapshots (6012 total). Failed logical allocation requests were 180, 93 and 142 respectively; they are expected capacity/fragmentation outcomes, not test failures or physical out-of-memory events. Every trace coalesced to the full empty slab. A [separate control run](../../results/2026-11-26-apple-m4-arths17-logical-oracle-control/receipt.json) changed one statistic in a copy of the log; the oracle rejected it. Original logs remain unchanged.

This verifies the recorded logical traces, not universal allocator correctness, actual GPU fence implementation, driver pooling, physical memory efficiency or Xbox behavior. The existing GPU trace remains the separate evidence for guard readback and a real delayed queued copy.

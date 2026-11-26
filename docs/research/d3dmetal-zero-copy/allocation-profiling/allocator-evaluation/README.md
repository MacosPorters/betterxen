# External allocator compatibility experiment

A separately versioned, original Workstream A harness evaluates AMD D3D12 Memory Allocator 3.3.0 at `600949266be004553f74c9ef809384380a9d44cf`. The [assessment](../allocator-assessment.md), upstream registry and reuse matrix record per-file MIT notices and exact Git blob identities before obtaining the external files. Vendor source remains unchanged in ignored artifacts; no production dependency or vendored implementation is added.

Supply the exact pinned `D3D12MemAlloc.h`, `D3D12MemAlloc.cpp` and `LICENSE.txt` in an external directory. The runner checks all three Git blob hashes, preserves the original notices and records SHA-256 identities. It uses native ARM64 MinGW-w64 host tools to build a Windows guest, with `-O2 -static -static-libgcc -static-libstdc++ -Wall -Wextra -Werror`. Only the original probe/harness and metadata are part of the research source tree. Build/runtime executables remain local.

```sh
python3 docs/research/d3dmetal-zero-copy/allocation-profiling/allocator-evaluation/run.py --external /absolute/path/to/pinned/files --scratch /tmp/bx473-allocator-review --bottle /absolute/path/to/existing/win64/bottle
```

Default execution requires normal pressure; the explicitly authorized `--allow-warning` profile permits warning but stops on urgent pressure. The conservative 23 GB host metric reserves 512 MiB headroom for this small workload. The pool is exactly one 32 MiB block (minimum and maximum block counts both 1); source buffers and GPU work are bounded. `--build-only` compiles without launching a guest. All command arrays, source/PE hashes, native observer architecture, backend evidence, pressure/footprint/vmmap checkpoints, exit/timeout status and raw outputs are retained in new scratch paths.

Two warmup and ten measured cycles use the same 24-slot payload sequence as the original seed-matched raw pooling trace. The original trace consumes one random draw for its gate control, so this reference deliberately advances that one draw before its comparable allocation sequence. Every live buffer is initialized and its leading/interior/trailing regions GPU-copied to readback. Releases occur after the completed queue fence and command-list reset. The library does not thereby become an automatic fence-retirement system.

A valid request for 33,554,436 bytes exceeds this explicit one-block pool. It must fail without growing the pool; an unexpected success is a fixture failure. Allocation statistics are checked after full/holes/empty phases; an empty pool can retain its one block by configuration. Statistics are logical library accounting, not physical residency. The pilot and repeated-bound fixture have distinct executed source snapshots. Timing comparisons must account for combined allocator+placement API versus the original separate calls, cold setup, different sessions and CPU load; these data alone do not establish an application benefit.

The [six-case bounded raw bundle](../../results/2026-11-26-apple-m4-arths17-external-allocator-bounded/receipt.json) can be rechecked without a GPU:

```sh
python3 docs/research/d3dmetal-zero-copy/allocation-profiling/allocator-evaluation/analyze.py docs/research/d3dmetal-zero-copy/results/2026-11-26-apple-m4-arths17-external-allocator-bounded --raw-pool-results docs/research/d3dmetal-zero-copy/results/2026-11-26-apple-m4-arths17-a22-pool-corrected --output /tmp/bx473-allocator-analysis.json
```

This separately derives free gaps from recorded live intervals, checks every reported statistic and compares exact request sequences; it does not reuse the library's allocation algorithm. Both producer/checker are author work. The [assessment](../allocator-assessment.md) states the measured scope and unqualified features.

# Workstream A memory and allocation research

Research for [#473](https://github.com/MacosPorters/betterxen/issues/473), on an isolated worktree based on `9ae6d1ee7092b6021aad075531f50ba60de4fd53`. This work does not change production shims, ABI, main or the six-check baseline. No commit/push has been authorized. Local runs are author observations, not independent acceptance or Xbox compatibility.

Start with the [research report](report.md) and [existing evidence inventory](inventory/README.md), [ownership feasibility matrix](inverted-ownership.md), [resource/lifetime contract](../gpu-correctness/contracts/resource-placement-lifetime.md), [optional allocator assessment](allocator-assessment.md) and [texture fixtures](../gpu-correctness/textures-and-resources/README.md). See [footprint findings](footprint-findings.md), [pooling findings](pooling-findings.md) and [texture/alias findings](../gpu-correctness/textures-and-resources/findings.md). Findings and review status are tracked in #473.

## Local protocol A2

`run.py` is an A-local harness; shared helpers and shared protocol files remain unchanged pending reciprocal coordination. `support.h` reuses the preserved original supplemental queue/buffer and full-consumption shader logic, formatted for readability, with shader capture added. It does not run B's dirty-page/coherence/tracking suite. The historical code's exact original bytes/hashes remain in `inventory/frozen-phase2/`.

Requirements: native ARM64 macOS, licensed externally installed CrossOver/D3DMetal/Rosetta, existing win64 bottle, native MinGW-w64 C++20 compiler, Python 3.11+, public Apple Command Line Tools. Compilation uses `-O2 -static -static-libgcc -static-libstdc++ -Wall -Wextra -Werror`; observer uses `xcrun clang++ -arch arm64`. Exact commands, native-tool architecture/linkage, source/executable/shader hashes and environment are retained in each receipt. The observed Wine compiler warns that optimization flag 0x8000 is ignored; no compiler optimization claim is inferred.

```sh
python3 docs/research/d3dmetal-zero-copy/allocation-profiling/run.py \
  --bottle "/absolute/path/to/an/existing/win64/bottle" \
  --scratch /tmp/bx473-a-memory-run --suite memory
python3 docs/research/d3dmetal-zero-copy/allocation-profiling/run.py \
  --bottle "/absolute/path/to/an/existing/win64/bottle" \
  --scratch /tmp/bx473-a-pool-run --suite pool
python3 docs/research/d3dmetal-zero-copy/allocation-profiling/run.py \
  --bottle "/absolute/path/to/an/existing/win64/bottle" \
  --scratch /tmp/bx473-a-texture-run --suite texture
```

Each scratch directory must be new and outside the checkout. Add `--smoke` for a reduced harness check, recorded separately. The runner copies the source bottle privately, selects D3DMetal, sets ROSETTA_ADVERTISE_AVX=1 and WINEDEBUG=-all, rejects a loaded d3d12_xs shim in the guest and captures loaded framework/DLL/Rosetta paths at a paused checkpoint. It records missing tools and failures. It stops only its private Wine server and checks the source configuration unchanged. It does not download/install or redistribute proprietary components. Disposable bottle scanner/trust settings permit the original locally built probe; source bottle settings are untouched.

One A process owns `/tmp/betterxen-473-gpu.lock` for a batch. Other contributors must coordinate the single-GPU-workload rule; this local advisory lock cannot exclude unrelated applications. No profiling/polling is performed inside allocation API timing windows. A process timeout is 240 seconds; GPU fence waits are bounded at 10 seconds. The initial A2 sweep used free-percentage guards only and was interrupted after observing kernel pressure level 2. A-local-2.1 additionally required kern.memorystatus_vm_pressure_level=0 before each case and at each snapshot. After the requester freed memory and explicitly allowed bounded continuation, A-local-2.2 added an explicit --allow-warning profile that permits level 1 while still stopping at >=2; its receipts record that selection. Default behavior still requires level 0. Warning-profile observations are not relabeled as normal-pressure evidence. It retains the 30%/25% free-percentage guards, but does not interpret those percentages as an OS pressure class. Unreadable pressure also stops the run. The partial A2 profile is not a completed two-session sweep. Existing swap/background load is retained in receipts; no idle-machine benchmark claim is made.

## Footprint correction

For two sessions, run each of 64/256/1024 MiB in a fresh process using CPU-only without graphics, graphics-warmed CPU allocation control, CUSTOM/WB/L0 and UPLOAD. Reverse size/type order in the second session. No optional >1 GiB allocation is part of this run. The graphics cases reuse the historical full-consumption checksum shader, write every source word and validate 256 checksum words after the fence; checksum agreement is not byte-exact evidence for every source word.

Checkpoints: warmed baseline, settled baseline, allocation/placement, Map, full initialization, full GPU consumption, resource release, disposal of command/output/pipeline objects, settled retirement, device release. CPU controls explicitly report GPU consumption not run. DLL/framework handles remain loaded at device release; it is not process exit or a guarantee of runtime-pool reclamation.

At each paused stage retain three proc_pid_rusage snapshots, `footprint -f bytes --swapped`, `vmmap -summary`, mapped-pointer `vmmap -w PID ADDRESS` where applicable, `vm_stat`, memory-pressure and swap readings. Tools execute sequentially, so readings can differ. Three samples at one pause are correlated snapshots, not three independent sessions. Virtual range size, RSS, physical-footprint accounting and graphics retention are reported separately; do not add categories or infer physical page identity.

## Pooling and retirement trace

Compare per-resource CUSTOM/WB/L0 heaps against one 32 MiB heap with placed suballocations, using matched seeds, buffer descriptions/alignment and GPU work. Three fresh-process pairs alternate case order. Each has two warmup cycles and ten measured cycles over 24 live slots with non-aligned requested sizes 65540, 262148, 393220 and 1048580 bytes. Allocate all, verify, free even slots, refill with seeded varied sizes, verify again, free all and check free-list coalescing. No grow/unbounded allocation occurs.

Each ALLOC retains heap/suballocation, placement and Map API timings; FREE retains resource and backing/free-list costs. Initialization, I/O and GPU work are outside these API samples. Cycle time includes them and is descriptive; the live snapshot pause is explicitly subtracted. A one-time pooled heap setup is separately recorded and must be included when comparing first-use costs. No benchmark improvement is promised.

Logical records track live payload, allocation-aligned live bytes, reserved backing, alignment waste, free space and largest free span. Individual heaps have no reusable cross-heap free span; report this as not applicable. Pool slack and logical fragmentation are not process footprint. GPU readback checks leading/trailing guards and an interior region for each live buffer, not every byte. A real gate-delayed queued copy verifies that the allocator withholds a pending range; it opens the gate before waiting and frees only after completion. The trace also rejects a fictitious future retirement value as a policy control. Every reused placed resource is activated; command list reset occurs after completed work and before resources are released. B's independent barrier/fence review remains required.

## Host ceiling and reduced profiles

The requester set a 23 GB RAM-use ceiling. A-local-2.2 records a conservative non-file-cache occupied estimate derived from physical RAM minus free and file-backed pages in vm_stat; this is not an exact Activity Monitor value. Before starting a case it reserves headroom below 23,000,000,000 bytes for twice the source plus 512 MiB (memory suite), or 512 MiB (small suites). It stops at paused checkpoints if the remaining margin falls below 256 MiB. This observation-based guard cannot control concurrent applications; the fixed source/slab caps and urgent-pressure stop remain necessary. A skipped case is recorded as not-run and makes the profile incomplete. No 2 GiB extension is used.

Use --allow-warning only with the requester's explicitly permitted warning-pressure conditions; otherwise default normal-pressure gating remains in force. A ceiling is not a target. Keep all actual reservations, pressure signals, swap changes and raw failures visible.

For the pool omitted-copy control, add --negative-control to --suite pool; the guest must fail with the specific trace guard/interior mismatch. The harness validates that expected failure separately. The texture matrix currently fails its negative-rejection gate on this runtime; all two sessions still run, and the batch returns failure. --suite alias performs the separate valid buffer-inheritance follow-up.

## Bottle receipt correction

Source review found that the historical/plain string insertion of AllowUntrusted matched the commented Bottle heading before the actual section. A22 run receipts preserve that malformed requested-setting line rather than claiming it was applied correctly; the appended scanner setting and actual loaded backend are separately recorded. Current run.py anchors the insertion to the actual section line and checks its construction without writing the source bottle. This configuration-only correction was made after the GPU runs; a post-correction launch is not claimed. Exact prior executed runner snapshots remain in every run bundle.

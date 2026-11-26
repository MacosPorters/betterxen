# Existing evidence inventory

This is Workstream A's inventory for [#473](https://github.com/MacosPorters/betterxen/issues/473), based on research revision `9ae6d1ee7092b6021aad075531f50ba60de4fd53`. The baseline source hash is `33e90bfee89e812f48990613cd74ced56473e5c3fc5d973f2ba1116090646a0c` and has not changed. [manifest.json](manifest.json) records original and archived SHA-256 hashes for each preserved historical file. The exact final supplemental source is in [frozen-phase2](frozen-phase2/README.md). Its historical proposed commands do not constitute a new shared protocol or authorization to run B's modes.

## Available records

| Evidence | Availability and review limits |
| --- | --- |
| Original M4 baseline, two sessions | Pinned branch source, two raw runs and receipts already published; first executable hash missing; no new independent certification |
| M4 `p2-memory-final` | 18 cases: CPU allocation control, WB/L0, UPLOAD; 64/256/1024 MiB, two fresh-process repeats; all recorded exit 0. Source/runner hashes match frozen final bytes. All footprint/vmmap stage commands recorded exit 0. CPU control initializes a graphics context before its baseline: it is not a process without graphics. |
| M4 `p2-functional-final` | 23 cases: identity, shader coherence, omitted-dispatch negative control, CPU write-watch, texture, 18 dirty-stride cases. All expected exits recorded, including negative control exit 1. A inventories but does not take ownership of B's tests or accept their conclusions. |
| M4 `p2-acceptance` | Historical name only: 22 earlier author-run cases, not independent acceptance. Probe/runner source hashes differ from the available final source. Executable retained locally; exact earlier source bytes not recovered. Not final-source reproducibility evidence. |
| M4 `p2-memory` | One CPU 64 MiB process killed at timeout (exit -9). Failure preserved. Earlier source bytes not recovered; method corrected in later runs. |
| Development logs | Identity/coherence/texture/watch stdout/stderr remain at the original scratch root; inventory hashes supplement the archived bundles. No complete development launch receipt; do not promote to final evidence. |
| 2elliti M5 report | [Comment](https://github.com/MacosPorters/betterxen/issues/473#issuecomment-7201953364): five reported baseline runs; launcher/bottle/timeout differ, later runs polled, backend evidence partial. Separately written private probe unavailable. Source/raw evidence requested. |
| atesahmet0 M5 report | [Comment](https://github.com/MacosPorters/betterxen/issues/473#issuecomment-7202142335): 1 GiB footprint/region excerpts, dirty-stride timings, adapter/heap/write-watch data. No accessible pinned supplemental source or complete raw distributions linked; requested. Physical-sharing conclusions exceed the accounting/timing evidence. |
| CarReal8917 B implementation | No pinned B revision or raw handoff in the five comments read at start. Confirmation and source/receipts requested in #473. No alias with another account assumed. |

Archives under `../../results/2026-11-26-apple-m4-arths17-historical-*` contain original stdout/stderr bytes, receipts and tool outputs, excluding executables and bottles. JSON diagnostics replace the original home-directory prefix with `<HOME>`; manifest retains both original and redacted hashes. Original evidence and executable bytes remain locally unmodified. Embedded source paths are historical, not a promise that those paths exist on another host. No proprietary DLL/framework is archived.

Two new harness preparation failures are also preserved: the first nonexistent wine64 lookup interrupted setup, and the next strict -Werror build detected unused header helpers. No guest ran in either. The first setup attempt has source hashes but no copied source snapshot; later attempts save exact source bytes before compiling. The fixed inline helpers and correct wineloader version command are exercised in subsequent runs.

## Objective classification before new experiments

| Objective | Classification | Next justified work |
| --- | --- | --- |
| Fixed six baseline checks | Already tested; requires independent source review/reproduction | Preserve unchanged; obtain M5 source/raw receipts and B review |
| Footprint sizes/CPU controls | Already tested; requires methodological correction | Baseline drops ~40 MiB; small allocations can use existing pools; add settled baseline and full retirement instead of repeating identical protocol |
| Physical page identity / excluded copies | Unsupported or currently untestable with these tools | Process accounting and region labels cannot identify all physical backing or copies outside the sampled process/window |
| Pointer-specific region evidence | Requires additional experimentation | Old M4 vmmap is summary-only; retain exact mapped-pointer region at paused checkpoints |
| Full resource retirement | Requires additional experimentation | Historical release leaves command/shader objects alive; dispose them and separately record device release |
| Inverted ownership/xmem requirements | Requires additional experimentation and requirement derivation | Inspect pinned public in-tree contracts; requested VA, reservation/decommit, attributes, aliases/scatter and pre-device allocations need explicit limits |
| Pooling/allocation latency | Requires additional experimentation | Matched individual/pool resource traces; raw allocation/placement/Map/free samples, retained memory and logical fragmentation |
| Fence-safe recycling/aliasing | Requires additional experimentation | Fixed traces, guards, readback, delayed reuse; no intentional in-flight destruction or unsafe writes |
| RGBA8 65x37 UNKNOWN CPU write + GPU readback | Already tested; requires independent review/reproduction | 2405/2405 pixels recorded, padded pitch 512; preserve original evidence |
| ROW_MAJOR DEFAULT cross-adapter control | Already tested; requires independent review/reproduction | Creation and GPU upload/readback recorded; do not rerun identical case without a review need |
| Mixed placement, RT/depth, formats/mips, storage classes | Requires additional experimentation | Extend texture matrix using public API restrictions and exact readbacks; do not infer sampling from copies |
| AMD allocator | Requires additional research | Exact-pin, per-file license/provenance and feature review; no integration authorized by evaluation |
| B copy/bandwidth/coherence/tracking | Requires independent reproduction after pinned handoff | B implementation responsibility; no duplicate fixture development by A |

Hardware logs are local class A observations within their workloads, public API contracts class B, unobserved mechanisms class U. Historical shaders were compiled at runtime but their exact compiled bytecode hashes were not archived; this is a receipt gap, not filled by new builds. The new memory fixture saves its own shader source and bytecode. All new A implementation remains author-run and independently unaccepted until reciprocal review. No Xbox hardware or game compatibility is established.

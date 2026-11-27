# Workstream A continuation after initial publication

Research issue [#473](https://github.com/MacosPorters/betterxen/issues/473), 2026-11-26. The requester manually published the initial A work at `8cc2f8ffbc4368e38950c2e037fee4ce12fa88b8` on `research/d3dmetal-zero-copy`. The final-stage request authorizes DCO-signed publication on the dedicated `research/473-workstream-a-memory` branch; earlier local-only checkpoints below remain historical. Production, ABI, main and the original six-check baseline remain untouched. This page records evidence scope, including unresolved work, rather than declaring the research complete.

The [pinned handoff to Asufte](handoff-asufte.md) and [issue handoff](https://github.com/MacosPorters/betterxen/issues/473#issuecomment-7204146451) identify historical phase-two sources, corrected A2/A22 snapshots, exact hashes, commands, raw results and failures. The current B lead is Asufte/@atesahmet0 and his agent; CarReal is unavailable and skakwy is not participating. No identities are inferred from aliases.

## Inventory and remaining objectives

| Objective | Available evidence | Remaining limitation/dependency |
| --- | --- | --- |
| Physical accounting | Initial corrected 12-case size/type sweep; A22 six completed smaller/control cases and one urgent-pressure interruption; distinct creation, initialization, consumption, retirement and device-release accounting | Both repeated profiles remain incomplete. WB/UPLOAD 256 MiB continuation and larger repeats need sufficient headroom and acceptable kernel pressure. Earlier sampling cannot identify physical pages or a leak. |
| Matched pooling/retirement | Published three seeded pairs, separate cold setup and API costs, guards, real delayed queued-copy retirement and omitted-copy failure control | No general application speedup; multi-queue stress and independent review remain open. |
| Fragmentation | [Logical trace](fragmentation/README.md): 6012 checked snapshots across three seeds; capacity/fragmentation failures and pending-fence eligibility; corrupted-statistic rejection | Logical spans are not driver/physical fragmentation. The small published GPU trace did not saturate its slab. |
| External allocator | [Pinned 3.3.0 evaluation](allocator-evaluation/README.md): WB/WC pilot plus six repeated, bounded-buffer cases; source/license checks before use; exact matched request traces and logical statistics | Buffers only, single queue/thread and one block. No runtime qualification of textures, every heap tier, defragmentation, migration or device loss. Caller supplies retirement. No production adoption. |
| Inverted ownership | [Requirement matrix](inverted-ownership.md), now separating CPU alignment from resource placement, contents from pointer identity, migration peak from payload, pending retirement and driver retention | Console semantics not established by public evidence remain unknown. Requested VA, sections/scatter, reserve/decommit and pre-device allocations cannot be supplied by ordinary Map alone. |
| Texture and placement | Published mip/mixed-heap/alias readbacks; invalid alignment/bounds creation accepted and rejection gate failed; valid buffer inheritance | Accepted invalid creation is not permission to submit it, nor independently a conformance finding. General format/layout coverage remains open. |
| Sampling/render/depth/stencil | [One corrected graphics smoke](../gpu-correctness/textures-and-resources/findings.md): seven exact 512-pixel checks; initial author fixture bug preserved | Repeated two-dataset profile stopped before launching a guest on urgent pressure. New packed-frame checking and omitted-draw control remain pending; smoke predates frame capture. |
| Reciprocal independent review | [B expectations](b-review-expectations.md) re-derived from public contracts; interpretation response to B; B reports A source reproduction/representative accounting audit | B subsequently published a402287; A source/data review is recorded below. M4 hardware reproduction, trace-input validation and a stopped-run hash discrepancy remain open. B has not reviewed new A graphics/allocator/fragmentation work. No final independent acceptance. |

## New records and reproducibility

All paths below are relative to the shared research root. Each raw bundle retains its own source snapshots, hashes and receipts; a later source revision never retroactively replaces an executed snapshot.

- `results/2026-11-26-apple-m4-arths17-logical-fragmentation/`: three seeded logical traces and separate interval-union oracle results.
- `results/2026-11-26-apple-m4-arths17-logical-oracle-control/`: deliberate copied-log statistic corruption rejected.
- `results/2026-11-26-apple-m4-arths17-external-allocator-pilot/`: WB/WC seed-473 pilot without the later oversized control.
- `results/2026-11-26-apple-m4-arths17-external-allocator-bounded/`: six fresh WB/WC processes, seeds 473–475, valid oversized-request rejection and per-cycle data. A separate interval calculation checks all 216 statistics records and all six 432-request sequences against published raw pooling traces.
- `results/2026-11-26-apple-m4-arths17-a23-render-lifetime-failure/`: original compiler-DLL lifetime bug; interrupted failure, not GPU acceptance.
- `results/2026-11-26-apple-m4-arths17-a23-render-fixed-smoke/`: corrected single-process seven-frame comparisons, shader source/bytecode, no packed-frame dumps yet.
- `results/2026-11-26-apple-m4-arths17-a23-render-pressure-stop/`: newer frame-capture source built, zero guest processes launched; urgent-pressure gate.

The external allocator source is unchanged and remains in ignored artifacts. Public records contain the original evaluation harness, exact file Git blob/SHA-256 identities, preserved license assessment and diagnostics; they do not redistribute proprietary runtimes or native/PE executables. Host tooling is ARM64; Windows guest probes are x86-64 PE32+ by design.

The graphics recipe and negative-control expectations are in the [fixture README](../gpu-correctness/textures-and-resources/README.md). The allocator recipe and separate statistical re-derivation command are in its [README](allocator-evaluation/README.md). A-local-2.3's explicit reduced memory selectors fill a missing case only when safety permits; selected sizes, types, session index and order are recorded. Reduced profiles cannot satisfy the full two-session common-profile gate.

## Architectural decision boundary

| Resource/requirement class | Evidence-supported candidate | What cannot be inferred |
| --- | --- | --- |
| Ordinary CPU-only, pre-device, fixed-address or section/scatter allocation | Conventional CPU/section ownership with explicit graphics transfer where required | Existing-memory wrapping, arbitrary CPU aliases or preservation of a pointer after moving storage |
| Eligible CPU-visible graphics buffer | Bounded D3D12-owned persistent buffer/slab, only after checking actual CPU pointer alignment, legal resource/view offsets, owner and final-use fences | Universal xmem provider, physical zero-copy or automatic retirement from an allocator library |
| Opaque textures and render/depth resources | Format-aware CPU subresource operations or DEFAULT upload/readback paths within tested contracts; explicit alias activation/initialization | Raw CPU texel pointers, console tiling, all formats/filters or physical backing representation |
| Storage-class selection | Combine A correctness/accounting with independently reviewed B matched bandwidth/copy/coherence data | A heap-type performance ranking from unmatched barriers, non-idle sessions or a single profiled capture |

One logical payload budget must distinguish reserved slab backing, alignment waste, pending retirement and migration peak. Physical-footprint/RSS/compression/swap/driver observations remain separate accounting layers. Successful logical reuse does not imply that the OS releases a physical charge.

The initial claims that small allocations might fit precharged pools and that pooling might reduce API overhead remain hypotheses constrained by measurements; neither is promoted into a universal architecture claim. The invalid-placement acceptances, probe-owned DLL lifetime bug, failed negative gate and repeated urgent-pressure stops are retained as contrary or limiting evidence.

## Safety and independent acceptance

The user permits at most 23,000,000,000 bytes of the recorded conservative host occupied-memory estimate, with explicit margins; this is a ceiling, not a target or a guarantee about unrelated applications. Kernel urgent pressure independently blocks new cases even when that estimate is below the ceiling. Default runs require normal pressure; explicitly authorized warning runs remain labeled. No optional 2 GiB extension or concurrent CPU/GPU polling experiment was run. No pressure setting, baseline gate or tolerance was weakened to complete a profile.

Next authorized execution requires pressure recovery: finish tiny graphics repetition/control first, then only missing footprint cases if their size-dependent reserves fit. B source/raw evidence is now accessible; [actual A review](review-b.md) records source/data checks and open trace/finalization questions. Critical M4 reproduction still needs pressure recovery. The proposed A-local-2.4 fence-error patch also needs reciprocal review before dependent GPU execution. Publication authority is limited to the dedicated A research branch; main and B remain untouched. The report remains author evidence until those dependencies are satisfied; the #464 implementation is not certified here.

## Subsequent reciprocal review

A read-only export of B's newly published commit enabled independent source/raw review: 98 source snapshots matched, 5040 measured copy rows and 576 compute rows were checked, and published trace distributions recalculated. One stopped-run stderr hash mismatch and the missing target-only XML inputs remain explicit review findings, with requests on #473. A prepared a failure-2 disposable-process patch for its own post-submit fence errors; all five A probes compile, but no new GPU/fault-injection outcome is claimed. See [review details](review-b.md). The publication-access dependency above is resolved for B's source; pressure, trace provenance and reciprocal acceptance remain open.

Reproducible review/build records are retained separately in [B review data](../results/2026-11-26-apple-m4-arths17-a-b-reciprocal-review/review-receipt.json), [exact B small-02 build-only receipt](../results/2026-11-26-apple-m4-arths17-a-b-source-build/receipt.json), [five A probe builds after the fence patch](../results/2026-11-26-apple-m4-arths17-a24-fence-build/receipt.json), and [external allocator build after the patch](../results/2026-11-26-apple-m4-arths17-a24-allocator-build/receipt.json). All are explicitly separate from actual GPU execution. [Data-analysis controls](../results/2026-11-26-apple-m4-arths17-a23-data-analysis/receipt.json) rejected a corrupted allocator statistic and an empty pressure-stopped graphics profile. Existing archived logs are unchanged.

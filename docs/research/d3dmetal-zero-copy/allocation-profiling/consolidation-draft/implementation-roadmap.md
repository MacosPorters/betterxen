# Evidence-supported implementation roadmap — proposal

This is a proposed sequence for review, not authorization to modify production, ABI contracts, existing issue ownership or milestones. No implementation issues are created. A prepared it and B [signed off the d3ae607 revision](https://github.com/MacosPorters/betterxen/issues/473#issuecomment-7206014568). Research branch integration is authorized; main and production changes are not. The [register](evidence-register.json) supplies exact pins/raw evidence for the IDs below; the [report](report.md) supplies the eight decision-gate answers.

“Ready for implementation” below means a bounded correctness/diagnostic task can be specified from available contracts without assuming physical zero-copy or a performance gain. It still requires separate maintainer authorization, coordination with the affected owner and independent verification. Items requiring unresolved compatibility/performance evidence stay in the research-dependent group. Nothing here requires #464 to wait for a universal mechanism proof.

## Ready to scope after separate authorization

Priority update: R3 combined-admission correctness is now highest priority alongside R2 lifetime correctness, ahead of allocator/performance selection. This is a technical finding for existing owners, not an unauthorized PR blocker, new issue or implementation change.

### R1 — Extend owned placement and allocation-eligibility regression coverage

- **Technical justification/evidence:** A-5–A-8 show that successful creation is not sufficient validation and CPU pointer alignment differs from placement alignment. #486 reviewed head c13f9c7 (vmem unchanged from c44ae4e) already contains backing-heap checks and ordinary-request forwarding; reuse/review those checks rather than create a parallel provider.
- **Expected benefit:** Reject ineligible provider requests before unsafe use and preserve unrelated D3D12 behavior. No throughput gain is promised.
- **Dependencies:** #464 implementation owner's agreement; current head/source review; public allocation-info contracts. Any new cross-DLL contract requires separate ABI authorization.
- **Affected components:** Existing provider's private validation/metadata, resource-class eligibility and its synthetic conformance tests; no assumed ABI change.
- **Compatibility risks:** A global 64 KiB rule can incorrectly reject small aligned textures; forcing replacement of existing/fixed CPU pointers can violate caller contracts; overriding forwarded ordinary calls can change host behavior.
- **Acceptance criteria:** Check the actual required CPU alignment and returned pointer; verify a valid queried resource alignment, overflow-safe span and required properties at the owned boundary. Keep pre-device/existing-content/requested-address/section cases explicitly eligible or rejected/fallback. Unsupported combinations report a reason.
- **Regression tests:** Offset 0; misalignment isolated from bounds with a larger heap; end-of-heap/size overflow; offsets near UINT64_MAX; buffer and available small-texture alignment; invalid allocation-info results; source pointer lifetime and existing contents; ordinary forwarded requests matching the host. Never submit caller-invalid objects to the GPU.
- **Independent verification:** Reviewer derives expectations from Microsoft contracts and inspects actual provider source; execute original synthetic tests on stated backend/runtime, with failed/skipped configurations distinct. Direct #473 host probes do not certify the shim path.

### R2 — Specify and test ownership, retirement and failure supervision

- **Technical justification/evidence:** A-2/A-8 and B-3 depend on successful final-use fences. B independently found A's historical post-submit exception hazard; B independently source-reviews and runtime-checks A-13 within the bounded disposable-guest scope; production API-failure/device-loss and multithreaded teardown remain unqualified.
- **Expected benefit:** Prevent reuse/free while GPU ownership is pending, and make incomplete work a visible failure. No recovery or device-loss support is presumed.
- **Dependencies:** Reuse the completed narrow peer review/fault verification; separately qualify production failure supervision; owner-approved queue/lifetime design; explicit final-use dependencies for each queue.
- **Affected components:** Provider resource/heap owners, mapped-pointer metadata, command/descriptor retention and process supervision; existing conformance harnesses.
- **Compatibility risks:** DLL detach can deadlock in ExitProcess; killing a launcher group may not prove its Wine guest has ended; command objects can retain resources; a signal from the CPU is not GPU completion.
- **Acceptance criteria:** Completed final-use values on every participating queue before recycle/destruction. Failed signal, event, timeout, device removal or incomplete completion never allows reuse. The parent bounds termination, confirms owned private guest/server completion and records failure without claiming a successful GPU result.
- **Regression tests:** Real bounded delayed work with guards; free while pending withheld; safe reuse after completion; event wake with incomplete fence; signal/event/wait failures; device-removed sentinel; multi-queue producer/consumer retirement; parent timeout and private-process cleanup. CPU fault-policy tests remain distinct from actual GPU synchronization tests.
- **Independent verification:** Reviewer re-derives queue ordering and object lifetime from public contracts; independently runs critical positive and failure cases with external process-state evidence. Do not count a mocked completion as GPU verification.

### R3 — Correct combined budget admission and expose distinct accounting layers

- **Technical justification/evidence:** A-1/A-4/A-5 separate payload, reservation and retained host charge; A-6 and B's version-scoped results require actual capabilities/addresses. At c13f9c7, A and B independently identify one-way private-heap visibility. A's source-derived private6/CPU3 example exceeds cap8; a stale-snapshot interleaving also exceeds the cap. Reuse current counters, but correct combined admission rather than claiming the sum is enforced.
- **Expected benefit:** Enforce the combined title cap in either allocation order and under concurrent admission, while explaining distinct logical/backing/retained measures. No physical release or performance gain is presumed.
- **Dependencies:** Existing budget owner's agreement and lock/transaction review across CPU and graphics records. If current internal interfaces cannot maintain coherent accounting, design/approve the required contract separately before implementation.
- **Affected components:** Existing private allocation/budget metadata and diagnostics; runtime capability/version/backend receipts. Host tooling remains native ARM64.
- **Compatibility risks:** Double charging a slice plus slab, missing pending or migration backing, racing independent budget snapshots, overflowing totals, or relying on a driver estimate as a title cap.
- **Acceptance criteria:** One coherent reserve/commit/release authority with atomic admission against all counted owners, plus separate reserved/pending/migration measures. Graphics-first and xmem-first sequences must obey the same cap; a statistics snapshot followed by a separate private charge is insufficient. Update/rollback failures coherently; old plus new migration backing remains charged through retirement. Diagnostics separately label footprint/RSS/compression/swap and unknown driver domains; no fixed-generation hardware assumptions.
- **Regression tests:** Both allocation orders, the 6+3/cap8 counterexample and concurrent stale-snapshot schedule (scaled bounded fixtures, not actual GiB allocations); heap/virtual/physical admission; allocate/free/retire; rollback on host failure; repeated config; slab/slice nonduplication; fragmented logical free space; pending and migration peaks; concurrent request budget bounds; overflow and underflow; runtime capability changes. Driver retention is logged, not asserted to vanish.
- **Independent verification:** Reviewer checks every allocation path and concurrency boundary, recalculates ledger transitions, and compares actual host observations without claiming page identity. A full unified budget is not accepted from one counter test.

### R4 — Seal reproducible research receipts after owned-process finalization

- **Technical justification/evidence:** B-8 identifies one stale stopped-run stderr hash. B's timestamp table and A's exact CPU process-race reproduction support sealing before the final child write. B-local-1.1 corrects the tested ordering; A additionally demonstrates that normal wineserver wait failure is ignored and cannot certify finality.
- **Expected benefit:** Make future successful and failed run receipts auditable without silent post-seal output changes.
- **Dependencies:** Preserve B's forensic reconciliation and A's controlled process check; obtain the minimal original stat/forced-stop transcripts for provenance; one agreed harness owner per patch. Preserve existing receipt/output and append a reconciliation record.
- **Affected components:** Research launch, stream closure, private Wine supervision, output manifest and archival code; coordinate B-owned changes with B.
- **Compatibility risks:** A wrapper's exit may not imply guest exit; arbitrary global Wine termination could affect other work; timeouts or missing process evidence can leave finality unknown.
- **Acceptance criteria:** Identify only owned processes, bound their termination/drain, close owned streams and seal a final manifest afterward. Propagate nonzero/timeout cleanup results into a failed or unknown finalization; do not discard the normal wineserver wait result or treat an error-path drain failure as finalized. Original bytes/receipts are never rewritten to make hashes agree.
- **Regression tests:** Normal completion, paused-backend stop, late stderr/stdout, timeout/interruption, surviving child, private server stop failure and archive redaction. A final digest covers archived bytes and identifies deliberate metadata redaction.
- **Independent verification:** Other workstream replays a bounded synthetic late-output case and independently hashes final outputs after process completion; no claims about completed historical benchmarks without matching evidence.

## Requires further evidence before production selection

### P1 — Close reciprocal review and the remaining safe verification cases

- **Technical justification/evidence:** A-1/A-12/A-13, B-1/B-3/B-7/B-8 and X-2 retain explicit missing gates.
- **Expected benefit:** Establish reproducible functional/synchronization evidence and provenance for the recommendations, not optimistic agreement.
- **Dependencies:** Kernel pressure recovery and size-dependent reserve below 23 GB; the now-supplied B target-only XML/schema/reference inputs (CPU verification completed); acknowledgment of review ownership. Urgent pressure remains a blocker and is never bypassed.
- **Affected components:** Research-only sources, append-only results, the register and independent review notes.
- **Compatibility risks:** Treating a reduced profile as full, combining correlated stage samples as sessions, mixing old/current helpers, losing trace outliers or changing a negative gate.
- **Acceptance criteria:** Finish changing-dataset graphics repetition and omitted-draw control; independently check packed frames; safe failed-fence behavior; only missing footprint repeats when eligible; critical same-source B small/representative compute/copy reproduction on M4. Seven exact trace extractions with independent PID/ID/unit/denominator/exclusion checks are completed; do not repeat unless source/inputs change. B's disposable-helper runtime scope is accepted, but production fault recovery remains separate.
- **Regression tests:** Existing exact controls, including failed texture rejection outcomes, retained stop/timeout receipts and unmatched source declarations. No unnecessary complete B sweep is commissioned.
- **Independent verification:** A/B exchange pinned actual sources, primary derivations, raw results and discrepancies. Reviewer records an explicit decision per criterion; no item becomes accepted from source publication or agent consensus.

### P2 — Qualify a bounded AMD allocator-backed buffer provider candidate

- **Technical justification/evidence:** A-4 demonstrates limited CUSTOM WB/WC pool compatibility; A-2/A-3 establish why setup/slack/fragmentation/retirement matter. Reuse is preferable to promoting the experimental allocator.
- **Expected benefit:** Established allocation/statistics machinery and potentially lower allocation-call overhead; application benefit remains to be measured.
- **Dependencies:** P1 critical reviews; per-file license/provenance registry preserved; existing provider owner and separately authorized integration; actual workload eligibility and stable-pointer policy.
- **Affected components:** Future private graphics-buffer pool provider and its tests; no default universal xmem replacement or implicit ABI changes.
- **Compatibility risks:** Fixed mapped pointers prohibit arbitrary defragmentation, bounded pools reject fragmented/oversized requests, retained blocks can consume headroom, and library object ownership does not provide queue retirement.
- **Acceptance criteria:** Explicit feature whitelist (initially only qualified buffer/property combinations), bounded reservations, correct statistics/rollback, safe fences and lifecycle. Compare cold setup, allocation/placement/Map, retirement, payload/slack and full-workload cost against the existing implementation. No application speedup claim without production measurements.
- **Regression tests:** Existing six seeded traces plus capacity/fragmentation, varied lifetimes, alignment, failed requests, pool release/retention and actual GPU guards. Multi-thread/queue, texture and device-loss features require their own added qualification before enabling.
- **Independent verification:** Reviewer evaluates exact upstream files/notices and wrapper source, re-derives span/statistic expectations and reproduces critical runs; performance assessment uses matched workload distributions and permitted host conditions.

### P3 — Select buffer and texture storage by matched workload evidence

- **Technical justification/evidence:** A-9–A-12/B-3/B-7 show functional paths but unmatched transfer work and single captures prevent a general heap ranking.
- **Expected benefit:** Workload-specific storage choices where measured costs justify them; no assumed Metal mode or bandwidth gain.
- **Dependencies:** P1 trace-input validation and normal-pressure sensitivity; agreed timing boundaries and equal useful work. Existing correct explicit transfer path remains available.
- **Affected components:** Future resource-class eligibility/policy and original benchmarks; retain B's ownership of bandwidth/compute methods.
- **Compatibility risks:** Comparing direct output with DEFAULT readback as identical GPU work, cache effects, deferred copies outside windows, and overgeneralizing point sampling or clears.
- **Acceptance criteria:** Separately compare CPU work, encoding, submission/fence and device intervals. Match resource descriptions/data/concurrency and account explicitly for necessary transfers/barriers. Retain per-iteration distributions/outliers and unknown mechanisms.
- **Regression tests:** CPU-visible buffers, staged DEFAULT buffers, tested opaque textures, correct RT/depth/stencil paths and required negative controls. Extend MSAA/BC/planar/arrays/3D only when an actual current-phase need is demonstrated.
- **Independent verification:** Other workstream re-derives payloads, checks raw trace selection and reproduces representative configurations; a version-specific policy never becomes a universal chip guarantee.

### P4 — Evaluate CPU write-watch as an optional selective-copy optimization

- **Technical justification/evidence:** BASE-6/B-4 establish ordinary CPU tracking; B-5 leaves foreign Map/GPU tracking unresolved.
- **Expected benefit:** Potentially reduce explicit CPU-to-GPU copied ranges for a demonstrated CPU-owned workload. Copy cost and tracker overhead must both be measured.
- **Dependencies:** A valid watch-enabled reservation, known writer ownership, synchronization/retry design and baseline full-copy fallback. No GPU-write-watch assumption.
- **Affected components:** Future explicit CPU transfer planner and bounded tests; existing caller pointer ownership stays unchanged.
- **Compatibility risks:** Query/reset/copy races, missing rearmed writes, same-value writes, address-array limits, unsupported allocation kinds and incorrectly converting host page size into returned tracking granularity.
- **Acceptance criteria:** No lost dirty coverage under the supported writer protocol; all relevant bytes copied/validated; errors or uncertain ownership choose explicit conservative handling. Report tracker/write/copy overhead separately.
- **Regression tests:** Subpages, boundaries, reset/rearm/read/same-value cases; writer quiescence, writes around reset/copy, failed or partial transfer, retry and allocation teardown. No unsupported concurrent protocol is accepted on a single-thread test.
- **Independent verification:** Reviewer derives dirty sets and race timelines from public API contracts, independently compares final bytes and measures the complete supported workload. GPU-written memory tracking stays Unknown unless separately demonstrated.

## Deferred conclusions

Physical backing-page identity, a universal absence of hidden copies, commercial game/FPS gains, broad console ABI semantics and general M1–M5 compatibility are not implementation acceptance criteria satisfied by this research. Stronger claims need discriminating authorized evidence. This roadmap neither creates distant-phase trackers nor expands native-only work; it supplies bounded proposals for existing hybrid ownership after separate authorization.

## Current closure boundary

BASE-1/3/4/5/6 and A-3/4/6/8/9/10/13 have B's explicit narrow acceptance; B-7 supplied-input extraction is accepted by A. A-12 remains incomplete despite M5 positive/omitted-control evidence, and B-1–B-4 still lack M4 hardware reproduction. The historical B-8 receipt stays Failed, its mechanism is independently supported, and unsuccessful cleanup handling remains open. Raw-frame/checker/stat transcript requests are provenance follow-ups rather than GPU blockers. No source-level combined-budget fix has been implemented or authorized.

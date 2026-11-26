# Final research acceptance checklist

This checklist separates publication integrity, narrowly accepted findings, unresolved research and permission to act. It does not close #473 or accept an implementation. A consolidates the report; Asufte/@atesahmet0 independently reviewed the prior published draft. The newly published delta requires B's final review. The current request authorizes research-only commits/pushes on A's dedicated branch, not branch integration, main/ABI/production changes or implementation issues.

## Publication and evidence integrity

- [x] Keep the six-check baseline probe and runner identical to `9ae6d1ee7092b6021aad075531f50ba60de4fd53`; probe SHA-256 remains `33e90bfee89e812f48990613cd74ced56473e5c3fc5d973f2ba1116090646a0c`.
- [x] Preserve all historical raw results, including invalid-rejection failures, urgent stops, the initial graphics bug and the stopped-run hash discrepancy. New verification uses append-only A result bundles.
- [x] Verify A archive manifests and retained source identities. Five declared references in already-published historical/setup runs have no matching retained source bytes; they remain unverified, rather than reconstructed or relabeled.
- [x] Verify all 29 register entries, reviewer attribution, acceptance scope and pinned raw-evidence paths. Match B review/trace inputs to `4ba4926d2f491517c01b1e94fb8539184055dc22` and the inspected budget source to `c13f9c76650edded41774528b50f40798244a6f3`.
- [x] Check documentation, Python sources, credential/personal-path scans and excluded executable/cache artifacts. Preserve byte-exact source snapshots; any retained whitespace warning is documented rather than silently reformatted.
- [x] Preserve per-file allocator MIT provenance/notices and external-only vendor/runtime status; no proprietary runtime or executable payload is published.
- [x] Confirm no production, ABI, main or shared-branch source modification. Research publication is confined to `research/473-workstream-a-memory`.
- [ ] Complete this publication's DCO sign-off, remote-HEAD verification and pinned issue handoff. The issue handoff, not a self-referential hash in this document, records the actual published SHA.
- [ ] Obtain Asufte's final review of this exact new consolidation revision. Prior endorsement is not automatically an endorsement of later changes.

The first seven checks are mechanical/source checks, not self-certification of experimental results. The final two are completed by the recorded publication and independent review respectively; an unchecked item is not a passing result.

## Thirteen independently accepted scopes

| Register ID | Accepting reviewer | Accepted scope and boundary |
| --- | --- | --- |
| BASE-1 | B | Reported flags/heap tier on the tested versions; not physical topology. |
| BASE-3 | B | Five L0 heaps, L1 rejection, initial mixed placement and CPU subresource round-trip. |
| BASE-4 | B | Fence-separated persistent-map copy visibility, not concurrent atomic interoperability. |
| BASE-5 | B | Negative methodological conclusion: baseline small-copy latency does not prove absence of copies. |
| BASE-6 | B | Ordinary CPU VirtualAlloc MEM_WRITE_WATCH behavior, not tracking foreign Map/GPU memory. |
| A-3 | B | Bounded CPU logical spans, not driver fragmentation. B reports its independent bitmap/rerun; those raw/checker artifacts are not included in the current B pin. |
| A-4 | B | Single-block, single-queue CUSTOM WB/WC buffer compatibility/statistics. B reviews six M4 cases and reproduces the two-case M5 seed-473 subset with the corrected helper. |
| A-6 | B | Placement alignment does not guarantee 64 KiB CPU Map alignment in the M4/M5 observations. |
| A-8 | B | Advanced-model whole-buffer data inheritance and disjoint guards in tested states. |
| A-9 | B | Three-format, three-mip transfer/guard positives; the full process still exits 1 at the invalid-rejection gate. |
| A-10 | B | Post-activation RT/depth clear/readback, not raw texture pointers or texture data inheritance. |
| A-13 | B | Single-threaded disposable research-helper behavior: real positive gated copy, direct failure-dispatch exit and empty unsignaled-fence timeout. Not actual failure injection of every queue API or arbitrary production teardown. |
| B-7 | A | All seven exact CPU extractions from supplied sanitized XML; independent PID/ref/join/unit/distribution checks. Not independent GPU recapture or physical traffic measurement. |

These are the explicit [register](evidence-register.json) scopes, not thirteen universal component certifications. B's exact shared-runner functional baseline reproduction closes that earlier gap, while its lsof backend receipt remains a separate receipt-only PID.

## Unresolved, failed and unsupported outcomes

| State | Required treatment / next evidence |
| --- | --- |
| Urgent-pressure blocked | M4 B hardware reproduction; repeated A graphics datasets, omitted draw and actual packed-frame inspection; missing footprint repeats; local fault-case repetition. Do not launch under urgent pressure or exceed the 23 GB ceiling/reserve. |
| Partial graphics evidence | B variant0 and omitted draw are reproduced; variant1 completes depth and stops at STAGE stencil. Hash expectations match, but B raw RGBA bytes are absent. Overall A-12 remains incomplete. |
| Failed historical gates | A-7 invalid-rejection gate, B-6 timestamp-query/resolve path and B-8 historical sealed-output digest remain Failed. Do not rewrite those receipts to turn them into passes. |
| Unsupported routes | Existing-memory opens and watch-allocation import return E_NOTIMPL in their stated configurations; GPU visibility/tracking through unsupported routes remains untested. |
| Budget correctness defect | At c13f9c7, one-way visibility and non-atomic cross-DLL admission permit a source-derived combined-cap violation. Production execution/repair is not performed; existing owners must review a separately authorized design and regression plan. |
| Cleanup-timeout handling gap | B-local-1.1 fixes the demonstrated late-write ordering, but normal wineserver wait errors are discarded. A's controlled CPU test exposes exit0/no-error despite a simulated wait124; failed drain cannot establish receipt finality. |
| Provenance follow-ups | B bitmap/M5 logical rerun records, raw frames, raw stat/real-Wine forced-stop transcript and full fault-build/PE receipt remain requested. Their absence is recorded; reported reviewer decisions are attributed rather than invented as A executions. |
| Unknown mechanisms / generality | Physical backing identity, universal no-copy behavior, GPU dirty tracking, unmeasured resource classes/generations, production performance and commercial game compatibility are unproven. |

Unknown mechanisms do not erase scoped functional results, and a scoped acceptance does not erase failed or blocked neighboring gates. The eight decision questions are answered in the [joint report](report.md); final scientific acceptance may include an explicit unknown answer rather than a fabricated mechanism conclusion.

## Integration and implementation permission gates

- [ ] Obtain explicit authorization for exact A/B commits and destination `research/d3dmetal-zero-copy` before any merge or shared-branch push. Follow the [integration proposal](integration-proposal.md); no merge has been performed.
- [ ] Have the independent reviewer examine the integrated result and its mechanical checks if integration is later authorized.
- [ ] Obtain separate implementation/ABI authorization and affected-owner agreement before acting on production recommendations.
- [ ] Obtain separate authorization before opening implementation issues or closing #473. This publication does neither.

Priority recommendations are R3 combined-admission correctness, R2 production lifetime/failure handling, R1 eligibility validation and R4 receipt finalization, followed by the evidence-dependent P2/P3/P4 work. The [roadmap](implementation-roadmap.md) specifies evidence, affected components, risks, dependencies, regressions and independent verification for every task. No production performance improvement is promised.

# Proposed integration of the two research branches

**Proposal only. No merge, merge-tree construction, shared-branch update or main integration has been executed.** The requester authorized publication of A's research, while reserving integration for separate authorization. This plan preserves both contributors' histories and leaves #473 open.

## Inputs and ownership

| Input | Exact identity / rule |
| --- | --- |
| Shared destination before integration | `research/d3dmetal-zero-copy` at `8cc2f8ffbc4368e38950c2e037fee4ce12fa88b8` |
| A | Exact verified remote SHA in this publication's #473 handoff; branch `research/473-workstream-a-memory`. Do not use an unverified moving tip. |
| B | Reviewed `4ba4926d2f491517c01b1e94fb8539184055dc22` on `research/473-workstream-b` |
| Common research ancestor | `8cc2f8ffbc4368e38950c2e037fee4ce12fa88b8` |
| Original six-check baseline | `9ae6d1ee7092b6021aad075531f50ba60de4fd53`; preserve original source and parameters |
| Proposed integrator | A, only after maintainer authorization and confirmation through #473 |
| Proposed independent integration reviewer | Asufte/Workstream B, subject to acknowledgment; prior reviews remain attributed to their actual pins |

A read-only changed-path comparison between the published A parent plus this candidate and B found no overlapping changed paths. This is a scope observation, not proof that Git can merge cleanly or that the combined documentation/code is semantically consistent. The report and register stay under A's owned directory so existing shared root findings are not edited simultaneously.

## Sequence to authorize later

1. Agree exact A/B input SHAs, expected old destination SHA and integrator/reviewer in #473. Request authorization explicitly for integration and any destination push; current publication authority does not suffice.
2. Fetch and verify all three refs again. If any changes, preserve the observed identity and review the delta before proceeding; no force push, rebase or history overwrite.
3. Use an isolated research integration worktree based on the approved shared revision. Preserve the two research histories using ordinary DCO-signed integration commits. Do not merge current main or production changes into the research result as an incidental update.
4. Preserve append-only raw directories, exact executed snapshots, all failures and original receipts. Keep B's sanitized XML and its provenance with the corresponding extract outputs. Do not replace older experiment sources with the latest helper or reconcile a stale hash by editing historical data.
5. Let one agreed author handle shared index/findings/changelog references; the other independently reviews that patch. Link the final consolidated report/register/roadmap/checklist without claiming all work accepted or manufacturing unavailable evidence.
6. Audit the integrated tree: protected production/ABI trees against the old shared research base, baseline source/runner identity, manifests/source hashes, private-path/credential/binary scans, Git source refs, documentation and relevant CPU processing/layout checks. Keep urgent-pressure GPU work blocked. Read-only path nonoverlap does not substitute for this step.
7. Obtain the independent reviewer's explicit findings on the actual integrated SHA. Record scoped acceptance, historical failures, unresolved cleanup/budget/provenance findings and pressure-blocked studies separately. Follow up on new differences rather than treating a merge as certification.
8. Only with the separately granted destination-push authorization, publish a normal non-force update to the shared research branch after checking its expected old SHA. Verify the remote result and post the actual integrated SHA plus reviewer status on #473.

## Invariants and stop conditions

Production, frozen ABI and the original baseline must remain unchanged relative to the approved shared research base; current main has its own later history and is not an integration target. A and B branches must not be overwritten. Any source/provenance conflict, changed destination, unexpected protected-file diff, failed mechanical check or missing independent review pauses integration for review; it does not justify discarding evidence or weakening gates.

Research findings are preserved even when they preclude an optimistic conclusion. The source-confirmed combined-budget defect is not fixed by merging research branches; the cleanup-timeout handling gap is not fixed by publishing an explanation. Physical zero-copy and production performance remain unproven. Hardware studies remain blocked under urgent pressure. No implementation issue is created, no contributor is reassigned and #473 is not closed by this plan.

## Prioritized production recommendations after separate authorization

| Priority | Existing roadmap item | Why now / independent acceptance required |
| --- | --- | --- |
| P0 | R3 — combined admission correctness | A/B source review and A's bounded arithmetic demonstrate one-way visibility and stale snapshots. Existing budget/provider owners need one atomic authority or equivalent design, both allocation orders, concurrency, rollback and overflow regressions. ABI work requires its own approval. |
| P0 | R2 — lifetime and production failure handling | Research-helper scope is accepted; production multi-queue, actual API failures, device loss and DLL/process recovery are not. Verify final-use ownership and failed-completion supervision independently. |
| P1 | R1 — eligibility and placement validation | Reuse existing #464/#486 checks; validate actual CPU alignment, adapter allocation alignment, bounds and pointer/content contracts. Preserve ordinary forwarding and explicit fallbacks. |
| P1 | R4 — receipt sealing / cleanup failure propagation | The specific race is reproduced and corrected; failed drain status must prevent a false finalized-success claim. Coordinate the B-owned harness patch rather than duplicate it. |
| P2 | P2 — bounded allocator provider qualification | Pinned allocator compatibility/statistics is accepted only for the tested buffer scope. Require correct budget/lifetime integration and matched end-to-end production measurement before selection or benefit claims. |
| P3 | P3/P4 — storage selection and CPU write-watch optimization | Require workload need, matched timing/transfer scope and race-safe eligible CPU ownership. Unknown GPU tracking or physical mechanisms cannot be silently assumed. |

These recommendations describe technically justified work, not implementation authorization, new blocking dependencies for #464 or guaranteed improvements. The detailed [roadmap](implementation-roadmap.md) contains acceptance criteria and independent regression requirements; the [research acceptance checklist](acceptance-checklist.md) controls evidence status rather than counting commits or closed issues.

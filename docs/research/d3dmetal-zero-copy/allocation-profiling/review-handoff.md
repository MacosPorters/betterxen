# Reciprocal review status and reproduction handoff

Workstream A owns allocation/footprint and resource/lifetime fixtures. Workstream B owns copy/bandwidth, compute/coherence, synchronization and tracking implementation. B's implementation has not been re-created by A. A's footprint shader/queue support reuses the frozen historical implementation; it is not a newly commissioned B workstream.

## What has been reviewed

A read all five #473 comments present at start, including the complete revised plan, the six-check pinned baseline, the actual retained M4 supplemental sources/raw receipts, M5 report/review notes and the architectural discussion. M5 claim analysis distinguishes public contracts from attributed observations. The common baseline timings copy only endpoints, and different CUSTOM/UPLOAD barriers prevent ranking heap types. Process accounting, region labels and flat timings do not prove page identity. UMA=0 does not invalidate CPU-visible L0 heaps. The historical UNKNOWN texture Map requested no pointer, so it did not test a pointer-return failure.

These are A's source/documentation assessments, not independent certification of A's own baseline or supplemental implementation. See the [inventory](inventory/README.md) for the missing earlier source bytes, compiled historical shader identities, partial M5 backend receipt and unavailable private probe.

## B source/data review and reproduction status

The requester designated Asufte/@atesahmet0 as B lead; CarReal is unavailable. B has now published a40228747470f0d184375e451b0173769c90aef5, and A has reviewed actual executed sources and raw data. [A's B review](review-b.md) records verified source identities, data/statistical recalculation, a stopped-run stderr hash discrepancy and the target-only XML inputs needed to validate trace selection. M4 hardware reproduction is blocked by urgent pressure. Publication access is no longer the B blocker; final independent acceptance remains open.

B's published review independently derives the earlier A expectations and records M5 same-source alias/texture reproduction, preserving the texture negative gate's exit 1. Those scoped outcomes are distinct from the resumed A allocator, fragmentation and graphics work, which B has not reviewed. A prepared an A-local-2.4 fence-error patch after B identified unsafe exception unwinding without proven completion. It is compile-checked and submitted through #473 for review before dependent GPU runs. No account identity is inferred beyond the requester's explicit confirmation, and no B-owned implementation was modified.

## Review A independently

The exact executed sources, commands, source/PE/shader hashes and raw stdout/stderr are in append-only `../results/2026-11-26-apple-m4-arths17-*` bundles. Each source snapshot/receipt pins bytes independently of the mutable working tree. Archives preserve raw stdout/stderr; JSON diagnostic copies explicitly redact the local home prefix, retaining original and archived hashes. Build/runtime binaries and bottle copies are not published. The requester manually published the initial A research at 8cc2f8ffbc4368e38950c2e037fee4ce12fa88b8. The [pinned evidence handoff](handoff-asufte.md) and [pre-review expectations](b-review-expectations.md) support reciprocal review. The final-stage request authorizes publication of resumed sources/results on the dedicated A research branch; independent acceptance remains separate.

One-command harness reproduction is in [README.md](README.md); choose the default normal-pressure profile or explicitly record an authorized warning-pressure profile. Respect the 23 GB conservative host-headroom guard and the single-GPU-workload rule. Do not turn the texture matrix's observed nonzero exits into passing status. The omitted-copy pool control must fail specifically for the expected mismatch.

Independent B review requests:

- Re-derive placed-resource allocation alignment/bounds, texture transfer layouts/mip extents and per-subresource CPU access from Microsoft documentation.
- Inspect resource states, alias activation/post-activation RT/depth initialization, full-buffer inheritance under the advanced model, CPU store completion, queue-gate cleanup and fence/command retirement.
- Reproduce critical pointer-alignment, buffer-inheritance, invalid-creation observations and representative mip/RT/depth cases on M5; retain unsupported/failure outcomes.
- Audit footprint baseline/retirement definitions, correlated snapshots versus fresh sessions, compression/RSS/process-ledger distinctions, pool setup/reservation/slack and cold/warm timing boundaries.
- Check raw per-allocation/per-free samples and per-session distributions; do not treat aggregate sample counts as independent machines or rank heaps from unmatched barriers.
- Independently evaluate whether the requirement matrix justifies any proposed architecture, with all required fallbacks and unresolved console semantics explicit.

Author-run, reported, reproduced and independently accepted are distinct statuses. The resumed A results remain author-run/unaccepted. The earlier alias/texture observations have scoped B review/reproduction; see the actual review records. Critical M4 reproduction and remaining reciprocal acceptance are still dependencies.

# Reciprocal review status and reproduction handoff

Workstream A owns allocation/footprint and resource/lifetime fixtures. Workstream B owns copy/bandwidth, compute/coherence, synchronization and tracking implementation. B's implementation has not been re-created by A. A's footprint shader/queue support reuses the frozen historical implementation; it is not a newly commissioned B workstream.

## What has been reviewed

A read all five #473 comments present at start, including the complete revised plan, the six-check pinned baseline, the actual retained M4 supplemental sources/raw receipts, M5 report/review notes and the architectural discussion. M5 claim analysis distinguishes public contracts from attributed observations. The common baseline timings copy only endpoints, and different CUSTOM/UPLOAD barriers prevent ranking heap types. Process accounting, region labels and flat timings do not prove page identity. UMA=0 does not invalidate CPU-visible L0 heaps. The historical UNKNOWN texture Map requested no pointer, so it did not test a pointer-return failure.

These are A's source/documentation assessments, not independent certification of A's own baseline or supplemental implementation. See the [inventory](inventory/README.md) for the missing earlier source bytes, compiled historical shader identities, partial M5 backend receipt and unavailable private probe.

## B review is pending

As of the last #473 poll, no accessible pinned CarReal8917 B source/raw handoff or contributor confirmation was available. A requested pinned source, raw logs/distributions, active-backend receipts and review notes through #473. Other contributors' M5 comments are not assumed to belong to CarReal. Therefore A has not completed independent implementation review or critical M4 reproduction of B. No agreement or independent acceptance is inferred from matching prose or reported numbers.

After a B handoff arrives: inspect the pinned actual source, independently derive API/data expectations, inspect individual samples/failures and backend evidence, identify confounds, reproduce only critical cases that resolve a specific uncertainty, and record disagreements and unknowns separately. Do not silently modify B's fixtures or overlap B-owned paths.

## Review A independently

The exact executed sources, commands, source/PE/shader hashes and raw stdout/stderr are in append-only `../results/2026-11-26-apple-m4-arths17-*` bundles. Each source snapshot/receipt pins bytes independently of the mutable working tree. Archives preserve raw stdout/stderr; JSON diagnostic copies explicitly redact the local home prefix, retaining original and archived hashes. Build/runtime binaries and bottle copies are not published. No commit/push has yet been authorized, so new sources are currently local; a remote fetchable revision is pending separate requester authorization.

One-command harness reproduction is in [README.md](README.md); choose the default normal-pressure profile or explicitly record an authorized warning-pressure profile. Respect the 23 GB conservative host-headroom guard and the single-GPU-workload rule. Do not turn the texture matrix's observed nonzero exits into passing status. The omitted-copy pool control must fail specifically for the expected mismatch.

Independent B review requests:

- Re-derive placed-resource allocation alignment/bounds, texture transfer layouts/mip extents and per-subresource CPU access from Microsoft documentation.
- Inspect resource states, alias activation/post-activation RT/depth initialization, full-buffer inheritance under the advanced model, CPU store completion, queue-gate cleanup and fence/command retirement.
- Reproduce critical pointer-alignment, buffer-inheritance, invalid-creation observations and representative mip/RT/depth cases on M5; retain unsupported/failure outcomes.
- Audit footprint baseline/retirement definitions, correlated snapshots versus fresh sessions, compression/RSS/process-ledger distinctions, pool setup/reservation/slack and cold/warm timing boundaries.
- Check raw per-allocation/per-free samples and per-session distributions; do not treat aggregate sample counts as independent machines or rank heaps from unmatched barriers.
- Independently evaluate whether the requirement matrix justifies any proposed architecture, with all required fallbacks and unresolved console semantics explicit.

Author-run, reported, reproduced and independently accepted are distinct statuses. Every new A result remains author-run/unaccepted. Peer review and source publication are outstanding dependencies, not completed deliverables.

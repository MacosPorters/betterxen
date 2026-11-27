# Joint research record for #473

Both workstreams of [#473](https://github.com/MacosPorters/betterxen/issues/473) in one tree: how the D3D12 memory interfaces of CrossOver/D3DMetal behaved under original synthetic workloads. It informs [#464](https://github.com/MacosPorters/betterxen/issues/464). It does not show physical zero-copy, a production performance gain, game execution or anything verified on Xbox hardware.

## Deliverables

| Deliverable | Document |
| --- | --- |
| Joint report | [report](allocation-profiling/consolidation-draft/report.md) |
| Evidence register (29 entries) | [table](allocation-profiling/consolidation-draft/evidence-register.md), [full records](allocation-profiling/consolidation-draft/evidence-register.json) |
| Acceptance checklist | [checklist](allocation-profiling/consolidation-draft/acceptance-checklist.md) |
| Implementation roadmap (recommendations only) | [roadmap](allocation-profiling/consolidation-draft/implementation-roadmap.md) |
| Evidence handoff for #464 | [handoff](allocation-profiling/handoff-464.md) |

The directory name `consolidation-draft` is kept so that paths cited in reviews stay valid.

## Inputs and review

| Input | Commit |
| --- | --- |
| Shared research base | `8cc2f8ffbc4368e38950c2e037fee4ce12fa88b8` |
| Workstream A (Arths17), `research/473-workstream-a-memory` | `62c7702e507ee1cb7c7d72c62263f37a4fa5a8d8` |
| Workstream B (atesahmet0), `research/473-workstream-b` | `4ba4926d2f491517c01b1e94fb8539184055dc22` |
| Six-check baseline | `9ae6d1ee7092b6021aad075531f50ba60de4fd53` |

Both histories are merged without squash or rebase. B [signed off A at `d3ae607`](https://github.com/MacosPorters/betterxen/issues/473#issuecomment-7206014568) and accepted the integration plan. `62c7702` differs from `d3ae607` in two publication-checklist lines of the acceptance checklist. After the merges, only status, authorization and navigation text was edited, the #464 handoff gained a post-review section, and this record was added. No finding, register record, experiment source or raw result was changed. B's review of the exact integrated commit is recorded in #473, not in this file.

## Reproduction

None of the GPU routes was rerun for integration. Do not run them under urgent memory pressure.

| Scope | Instructions |
| --- | --- |
| Six-check baseline | [README](README.md) |
| A: footprint, pooling, texture, alias, graphics | [A guide](allocation-profiling/README.md) |
| A: fragmentation model (CPU only) | [fragmentation](allocation-profiling/fragmentation/README.md) |
| A: external allocator (pinned, not vendored) | [allocator evaluation](allocation-profiling/allocator-evaluation/README.md) |
| B: copy sweeps, compute, traces | [B guide](copy-detection/README.md), [trace audit](copy-detection/b1-b2-audit.md) |
| CPU-only replays of traces, finalization, budget and peer data | [final verification](allocation-profiling/final-verification/README.md) |

## Accepted scopes

Thirteen entries have independent acceptance, each only within the boundary stated in the [checklist](allocation-profiling/consolidation-draft/acceptance-checklist.md): BASE-1, BASE-3, BASE-4, BASE-5, BASE-6, A-3, A-4, A-6, A-8, A-9, A-10 and A-13 (accepted by B), and B-7 (accepted by A, for extraction from the supplied sanitized XML only).

## Outstanding limitations

- **Defect in unmerged PR #486 at `c13f9c7`:** combined budget admission can exceed the cap. Derived from source; production code was not run or changed.
- **Research harness gap:** a failed or timed-out `wineserver -w` on the normal path is ignored, so such a receipt cannot be treated as final.
- **Failed:** A-7 (invalid placements accepted), B-6 (timestamp resolves), B-8 (historical sealed-output digest).
- **Unsupported on the tested versions:** opening existing memory as a heap (BASE-2) and importing the write-watch allocation (B-5).
- **Blocked by memory pressure:** M4 reproduction of B's hardware cases, the repeated graphics datasets, missing footprint repeats and local fault repetition.
- **Missing provenance:** B's bitmap checker records, raw frame bytes, forced-stop transcript and fault-build receipt; five historical source references of A have no retained bytes.
- **Unknown:** physical page identity, hidden copies, GPU write tracking, other Apple Silicon generations and resource classes, multi-queue and device loss, production and game performance.

## Integration checks

Mechanical checks run on the integrator's side (CPU only). They are not an independent review and do not repeat any experiment.

- Shared base, both inputs, `d3ae607` and the baseline are ancestors; every added commit carries its author's sign-off.
- The two inputs change no common path (343 for A, 667 for B), and the merged tree equals their union before the documentation edits.
- Only `docs/` and `CHANGELOG.md` differ from the shared base: no production, shim, ABI or tool code.
- Baseline `probe.cpp` (`33e90bfe…`) and `run.py` are identical to `9ae6d1e`.
- A: 29 archive manifests, 2,124 file digests match. The 480 entries of the reciprocal-review input manifest match their pinned commit.
- The register has 29 entries, 13 accepted, 11 unknown, 3 failed and 2 unsupported; its records equal those at `62c7702`. Its 96 raw-evidence links resolve in Git.
- B: digests in the result bundles were compared with bundled files. Many name artifacts that are not published, such as executables, and were not verified. Of those naming sources or outputs, the unmatched ones are: the known B-8 empty-stderr digest, two external allocator files that are deliberately not vendored, and two trace tools whose digests match the repository copies.
- Documentation hygiene reports no problems. The scan found no executables, personal home paths, personal mail addresses or credentials; its one credential match is a detection pattern in an audit script.
- `git diff --check` against the base reports a blank line at end of file in 19 snapshots of B's `support.h` (one under A's source-build bundle, 18 under B's results). They are byte-preserved evidence and were not edited. The integration's own edits pass.
- Five historical source references of A remain unverified, as recorded in the [publication audit](allocation-profiling/publication-audit.md).

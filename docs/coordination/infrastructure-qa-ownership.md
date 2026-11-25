# Infrastructure and QA ownership manifest

**Historical native record.** Plans and results below retain their original candidate scope. Native development is paused under [ADR 0005](../adr/0005-crossover-hybrid-direction.md); follow the [hybrid architecture](../architecture.md) for active priorities.

Non-overlapping file ownership for the infrastructure and engineering-quality
campaign. Active CPU/runtime, RDNA2 and native-application Gates 4–9 work stays
outside this manifest.

Rechecked against `origin/main` at the claim of issue #180. Update this file
when a path is claimed, released or closed.

## Hard exclusions (do not edit under this campaign)

- `crates/loader/`, `crates/import-gen/`, `crates/ffi/`, `crates/isa/`
- `crates/decoder/`, `crates/shader/`, `crates/ref/`, `crates/metal/`, `crates/texture/`
- `app/`, `fixtures/`, `tests/validation/`
- `scripts/test-gpu.sh` and existing CPU/GPU benchmark workload implementations
- Guest execution authorization and trusted-fixture policy

Shared Cargo configuration, production CI workflows and `CHANGELOG.md` require
an explicit per-change coordination note even when a QA issue mentions them.

## Active claims

| Issue | Holder | Paths | Notes |
| --- | --- | --- | --- |
| #180 | ranveersoni98 | `tests/agent_queue/`, `docs/coordination/` | Live smoke evidence; no production workflow edits |
| #277 | held until #180 releases coordination docs | `scripts/qa/`, `tests/qa/`, `scripts/release/`, `tests/release/`, `docs/testing/infrastructure/` | Maintainer-approved new paths; depends on #180 |

## Approved new paths (not yet exclusively held)

Maintainer-approved for the infrastructure/QA campaign once claimed without
overlapping an active issue:

| Path | Intended workstream |
| --- | --- |
| `scripts/qa/` | Advisory PR verification, test orchestration, env diagnostics |
| `tests/qa/` | Independent tests for `scripts/qa/` |
| `scripts/release/` | Native Mach-O, SBOM, checksum and release-evidence tools |
| `tests/release/` | Independent tests for `scripts/release/` |
| `docs/testing/infrastructure/` | Tooling docs; not product GPU/CPU protocol docs |

## Existing scripts to extend or wrap, not duplicate

| Existing path | Relation |
| --- | --- |
| `scripts/agent_queue/` | Owned by the merged queue (#165); #180 smoke-tests only |
| `scripts/audit/native.sh` | ARM64 and linkage gate; release tools may consume its outputs |
| `scripts/audit/claims.sh`, `dependencies.sh`, `status.py`, `test_inventory.py` | Prefer calling or summarizing; do not reimplement |
| `scripts/benchmark.sh` | Analysis tools consume results only; do not change workloads |

## Disposable remote testing

Live agent-queue Actions exercises run only in the maintainer-approved private
repository `ranveersoni98/xenora-agent-queue-smoke`. Do not enable blocking
production checks until that smoke evidence is recorded and reviewed.

## Phase II note

Issue #277 holds `scripts/qa/`, `tests/qa/`, `scripts/release/`, `tests/release/` and
`docs/testing/infrastructure/`. Ledger updates for Phase II commits are recorded in
`docs/coordination/infrastructure-qa-ledger.md` under the #180 coordination claim until
#180 closes, then move with the tooling owners.

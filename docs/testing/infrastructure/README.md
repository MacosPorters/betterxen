# Infrastructure QA tooling

**Scope:** the Phase II/III tooling names below belong to the earlier infrastructure campaign, not the active hybrid roadmap. Native CPU/RDNA2 candidate lists are read-only inventories, not authorized work queues. Use the [hybrid testing guide](../README.md) for current validation scope; reuse these helpers only where applicable.

Standalone helpers under `scripts/qa/` and `scripts/release/`. They do not replace
`scripts/audit/*`, `scripts/benchmark.sh` or `scripts/test-gpu.sh`. New checks are
advisory until reviewed.

## Suites

| Path | Role |
| --- | --- |
| `scripts/qa/exit_pipe.py` | Preserve command exit status |
| `scripts/qa/orchestrate.py` | Structured multi-suite runs and failure excerpts |
| `scripts/qa/pr_validate.py` | Advisory PR scope, DCO, artifact and evidence checks |
| `scripts/qa/bench_stats.py` | Median/percentile analysis of existing samples |
| `scripts/qa/regression.py` | Compare historical orchestration JSON reports |
| `scripts/qa/env_check.py` | Host, toolchain and branch diagnostics |
| `scripts/qa/claim_conflict.py` | Read-only file-scope conflict prediction |
| `scripts/release/macho.py` | ARM64 arch and forbidden-linkage checks |
| `scripts/release/bundle.py` | App bundle layout and codesign diagnosis |
| `scripts/release/manifest.py` | Checksums, manifests, license inventory |

## Tests

```sh
PYTHONPATH=scripts python3 -m unittest discover -s tests/qa
PYTHONPATH=scripts python3 -m unittest discover -s tests/release
```

## Phase II CLIs

```sh
PYTHONPATH=scripts python3 -m qa rdna2 inventory --compact
PYTHONPATH=scripts python3 -m qa rdna2 candidates --family SOP2 --status reuse-primitives --limit 50
PYTHONPATH=scripts python3 -m qa rdna2 plan --limit 20 --compact
PYTHONPATH=scripts python3 -m qa rdna2 manifests --status reuse-primitives --limit 10 --claims claims.json
PYTHONPATH=scripts python3 -m qa rdna2 reconcile --compact
PYTHONPATH=scripts python3 -m qa rdna2 discrepancies
PYTHONPATH=scripts python3 -m qa rdna2 diff --before before.json --after after.json --compact
PYTHONPATH=scripts python3 -m qa verify quick
PYTHONPATH=scripts python3 -m qa cpu-coverage --blockers
PYTHONPATH=scripts python3 -m qa cpu-inventory --compact
PYTHONPATH=scripts python3 -m qa bench-compare --baseline b.json --current c.json
PYTHONPATH=scripts python3 -m release readiness --revision HEAD --live --binary path --app path.app
```

RDNA2 planning is read-only over the AMD snapshot, semantic registry, decoder tables and
`docs/status.toml`. Candidates and manifests are never counted as implemented forms.
Family claims exclude those families; path-only claims block candidates whose default
permitted files overlap. Reconcile reports mismatches without adjusting planner counts.

## Phase III — compatibility intelligence

Checkpoint ledger: [phase3-compat-ledger.md](phase3-compat-ledger.md).

`cpu-inventory` consumes the x86 instruction registry (not heuristic opcode scans).
Declared forms are decodable only; native vs shared-semantic partitions come from
read-only `dbt.rs` analysis. Unsupported intentional rejections stay in a separate catalog.


# Agent queue live smoke test

Issue: #180  
Disposable repository: [`ranveersoni98/xenora-agent-queue-smoke`](https://github.com/ranveersoni98/xenora-agent-queue-smoke) (private)  
Production Xenora workflows are not modified by this exercise except where a
separate coordinated fix is recorded.

## Why a disposable repository

The queue unit suite does not call the GitHub API. Issue #180 requires Actions
evidence for simultaneous claims, refusal, release, stale-lease audit behaviour,
pending-run replay and exact head-SHA verification against a real repository.

## Harness notes

- Copies `scripts/agent_queue/`, the unit tests, `agent-queue.sh`,
  `agent-sync.sh` and the two agent workflows from Xenora.
- Uses `ubuntu-latest` runners in the disposable repo only. Queue decisions are
  standard-library Python; production Xenora keeps `ubuntu-24.04-arm`.
- Config upstream is `ranveersoni98/xenora-agent-queue-smoke`.

## Evidence log

| Scenario | Result | Evidence | Recorded |
| --- | --- | --- | --- |
| Claim grant on ready task | pass | issue #1 labels `agent:claimed,claimed-by:ranveersoni98`; run `45383910132` | 2026-11-24 |
| Overlap claim refusal | pass | issue #2 bot: file overlap with claimed task #1; run `45384365386` | 2026-11-24 |
| Disjoint second claim | pass | issue #3 claimed; run `45384367521` | 2026-11-24 |
| `/progress` renew | pass | issue #1 bot: Lease renewed; run `45384473061` | 2026-11-24 |
| `/release` | pass | issue #3 returned to `agent:ready`; bot release marker | 2026-11-24 |
| Claim without `agent:ready` | pass | issue #4 refused: not labelled agent:ready | 2026-11-24 |
| Comment-only apply crash | fixed | `ghio.apply_actions` skipped no-op `gh issue edit`; mirrored to smoke | 2026-11-24 |
| Audit `workflow_dispatch` | pass | run `45385817420` after `pull-requests: read` and `summary --now` argparse fix | 2026-11-24 |
| Exact `/verified` head SHA | deferred | needs second eligible non-author account on the smoke repo | 2026-11-24 |
| Full wall-clock stale lease | deferred | covered by unit tests with frozen clock (`lease_hours=168`) | 2026-11-24 |

## Production follow-ups

1. Coordinate adding `pull-requests: read` to the production audit job permissions
   (same defect as the smoke audit failure).
2. Add a second QA-eligible verifier before treating `/verified` as smoke-complete.

## Local unit suite

```sh
PYTHONPATH=scripts python3 -m unittest discover -s tests/agent_queue
```

# Infrastructure QA campaign ledger

**Historical native record.** Plans and results below retain their original candidate scope. Native development is paused under [ADR 0005](../adr/0005-crossover-hybrid-direction.md); follow the [hybrid architecture](../architecture.md) for active priorities.

Running record for the infrastructure and engineering-quality campaign.
Update after each meaningful commit batch. Do not invent passing tests.

## Baseline

- Branch: `main` synchronized with `origin/main` before each batch
- Hard exclusions: CPU/runtime crates, RDNA2 crates, `app/`, fixtures, GPU tests
- Approved paths: see `infrastructure-qa-ownership.md`

## Commits

| SHA | Summary | Tests |
| --- | --- | --- |
| `8410e3d` | test(qa): cover environment diagnostic helpers. | focused suite before commit |
| `48f44ca` | feat(qa): add operational python3 -m qa CLI. | focused suite before commit |
| `f56d3c9` | feat(release): add python3 -m release CLI entrypoints. | focused suite before commit |
| `22bafeb` | feat(qa): add read-only RDNA2 expansion planning over live inventories. | focused suite before commit |
| `c651a9d` | feat(qa): add unified verification profiles with stale evidence detection. | focused suite before commit |
| `f175a10` | feat(qa): scan CPU opcode and import coverage from existing sources. | focused suite before commit |
| `f400550` | feat(qa): compare recorded benchmarks with identity and noise guards. | focused suite before commit |
| `7a51764` | feat(qa): detect claim collisions across renames and deletions. | focused suite before commit |
| `050df30` | feat(release): build release-readiness reports with SBOM and evidence. | focused suite before commit |
| `63cf359` | test(qa): smoke-test qa and release CLIs against real commands. | focused suite before commit |
| `313430c` | docs(testing): document Phase II qa and release CLI workflows. | focused suite before commit |
| `0f7658b` | docs(changelog): record Phase II infrastructure automation CLIs. | focused suite before commit |

## Smoke repository

- Repo: `ranveersoni98/xenora-agent-queue-smoke`
- See `agent-queue-smoke.md` for Actions evidence

## Limitations

- Phase II deferred: exact `/verified` head-SHA smoke and wall-clock lease expiry still need a second eligible verifier / long wait.
- `qa verify cpu|gpu|app|full` invoke heavy existing commands; use `quick` for local iteration.
- CPU coverage statuses are heuristic where no authoritative opcode inventory file exists.

## Limitations

- Production agent PR checks remain advisory
- Independent `/verified` on smoke PRs needs a second eligible account
- New QA/release tools are local/advisory until review

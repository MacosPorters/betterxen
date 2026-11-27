# Methodology and reproduction

## Hosts

| | Workstream A | Workstream B |
| --- | --- | --- |
| Operator | Arths17 | atesahmet0 (Asufte) |
| Chip, RAM | M4, 24 GiB | M5, 16 GiB |
| macOS | 27.0 (26A428) | 27.2.1 (26B131) |
| CrossOver, D3DMetal | 26.3.0.39832, 3.0 | 26.3.0.39832, 3.0 |
| Wine engine | 11.0-8726-g2e2f5fca349 (not recorded in the first baseline) | same |
| Rosetta package | 1.0.0.0.1788432274 | 1.0.0.0.1793943009 |

## Method

- A frozen six-check baseline (`probe.cpp`, SHA-256 `33e90bfe…`) with five warmups and 40 timed submissions.
- Fresh guest processes in private copies of a CrossOver bottle, with the loaded D3DMetal libraries recorded.
- Expected values computed in the probes; omitted-copy and omitted-dispatch controls leave poison in place.
- Seeded repeat sessions for copy, compute and pooling work.
- Memory-pressure gates; stopped runs are kept and counted as incomplete.
- Reciprocal source and data review, with limited same-source reruns on the other host. Same-source reruns share any flaw in the source.

## Recheck without a GPU

```sh
git fetch origin docs/473-unified-memory-research-publication
git worktree add --detach /tmp/bx-report-001 origin/docs/473-unified-memory-research-publication
cd /tmp/bx-report-001/docs/research/publications/001-unified-memory
python3 scripts/recompute.py     # standard library; exits non-zero on a failed check
python3 scripts/figures.py       # needs matplotlib (drawn with 3.10.8)
```

`recompute.py` rewrites `data/results.json` from the raw logs. It reports, without failing, nine Workstream B output digests that do not match; see the [audit](evidence-audit.md#errors-and-corrections-found).

## Rerunning the experiments

GPU routes need Apple Silicon, a licensed CrossOver with D3DMetal, Rosetta 2, a win64 bottle and MinGW-w64. Do not run them under urgent memory pressure. None was rerun for this report.

| Scope | Instructions |
| --- | --- |
| Baseline | [README](../../d3dmetal-zero-copy/README.md) |
| Memory, pooling, textures, graphics | [Workstream A guide](../../d3dmetal-zero-copy/allocation-profiling/README.md) |
| Copy, compute, traces | [Workstream B guide](../../d3dmetal-zero-copy/copy-detection/README.md) |
| Fragmentation (CPU only) | [fragmentation](../../d3dmetal-zero-copy/allocation-profiling/fragmentation/README.md) |
| Allocator | [allocator evaluation](../../d3dmetal-zero-copy/allocation-profiling/allocator-evaluation/README.md) |
| Trace, finalization, budget replays | [final verification](../../d3dmetal-zero-copy/allocation-profiling/final-verification/README.md) |

## Reproduction appendix

| Item | Value |
| --- | --- |
| Integrated research | `f8810a48af384d13c594b9fba954200bfdb944f1` |
| Workstream A / B | `62c7702e507ee1cb7c7d72c62263f37a4fa5a8d8` / `4ba4926d2f491517c01b1e94fb8539184055dc22` |
| Baseline | `9ae6d1ee7092b6021aad075531f50ba60de4fd53` |
| `main` when audited | `f07a24c28cb6a1c0bd5f830494c98722feb5801d` |
| Inspected PR #486 head | `c13f9c76650edded41774528b50f40798244a6f3` |
| Copy sweep seeds / compute seeds | 473, 947 |
| Pooling and fragmentation seeds | 473, 474, 475 |
| Independent approval | [comment 7206445510](https://github.com/MacosPorters/betterxen/issues/473#issuecomment-7206445510) (integration only, not this report) |

## Licensing and provenance

The figures, scripts and text are original, under the repository's GPL-3.0-or-later. Colors follow the Okabe-Ito palette values. Figures embed glyph outlines of DejaVu Sans through matplotlib. No vendor binary, SDK, game asset or third-party image is included. AMD's allocator stays external, under its MIT license, as recorded in the [upstream registry](../../upstream-registry.md).

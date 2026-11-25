# Testing and acceptance

## Select checks by scope

Active hybrid work follows the [architecture guide](../architecture.md).
Preserved native tests remain regression checks, with their original synthetic
scope. Historical Phase 01/02 and G-number acceptance gates are not the current
hybrid phase criteria. The [roadmap](../roadmap.md) controls phase completion.

### Documentation-only changes

Run from the repository root:

```sh
python3 scripts/audit/docs_hygiene.py --require-yaml
python3 scripts/audit/status.py --check
python3 scripts/audit/semantic_coverage.py --check
scripts/audit/claims.sh
scripts/audit/dependencies.sh
git diff --check
```

Hygiene checks tracked relative Markdown file links, conflict markers and
workflow YAML; it does not validate anchors or external URLs. Status and coverage
checks detect drift in generated native tables. These are consistency checks,
not CPU/GPU execution or independent acceptance. API documentation can also be
compiled with `CARGO_BUILD_JOBS=2 cargo +1.100.0 doc --locked --workspace --no-deps`.

### Hybrid shader fixtures

On native ARM64 macOS with Rust 1.100.0 selected, `cargo test --locked -p victory-shader` runs CPU-side
compiler tests. Windows-only GPU tests require a Windows guest executable,
an externally installed CrossOver with a configured bottle and a D3D12-capable
backend. DXIL emission alone is not GPU verification.

The following command additionally requires a checkout containing the
`betterxen-hybrid` runner from [PR #384](https://github.com/MacosPorters/betterxen/pull/384)
and stall supervision from [PR #388](https://github.com/MacosPorters/betterxen/pull/388).
They are absent from the split shader/script stack's base. Install Rust 1.100.0
with `x86_64-pc-windows-gnullvm` and provide an unpacked llvm-mingw toolchain
(the recorded integration used release 20261117):

```sh
scripts/hybrid/run-shader-gpu-tests.sh /path/to/llvm-mingw betterxen-test fixtures
# Optional whole upstream suite, including the wave fixes now in this stack:
scripts/hybrid/run-shader-gpu-tests.sh /path/to/llvm-mingw betterxen-test all
```

`fixtures` filters on `betterxen`; the latest stack adds three fp64 tests to the
original four BetterXen tests. The script still expects the literal summary
`test result: ok. 4 passed`, so that expectation may reject a successful run of
the expanded selection. This documentation PR leaves the script unchanged.
Use `all` for the recorded whole-suite procedure and inspect output and skips. The script obtains the executable from Cargo build messages and
uses `debug/deps` as a fallback. It delegates supervision to the hybrid runner
and writes records under ignored `artifacts/hybrid-runs/`.

Record source revisions, host, CrossOver/bottle/backend, Rust and cross-toolchain
versions, commands, output and pass/fail/skip counts. Inspect whether each test
actually dispatched: some upstream tests return success after skipping when
D3D12, WARP or `dxil.dll` is unavailable. BetterXen's GPU tests fail without the
required hardware device. The [integration report](../research/victory-shader-integration.md)
preserves the original two failures and the included
[wave fixes](../research/shader-wave-semantics.md), which report 229 passed,
0 failed and 3 ignored on the recorded host. The device-width rejection test
is a CPU rejection check on that host, not a verified 64-lane GPU sum. No independent reproduction or
phase completion is claimed.

### Workspace and preserved native regressions

Use `cargo fmt --all -- --check`,
`cargo clippy --locked --workspace --all-targets -- -D warnings` and
`cargo test --locked --workspace` as appropriate to code changes.
`scripts/test-gpu.sh`, `scripts/demo.sh`, the independent semantic review,
fixture assembly, mutation and benchmark procedures below exercise the
preserved native path. They do not launch CrossOver or validate the DXIL path.
A successful native run does not substitute for hybrid workload readback.
Keep unavailable tools, excluded target modules and ignored tests visible.

## Preserved procedures and evidence

- [Migrated pull-request integration review](migration-pr-review.md) — Merge decisions, fixes, native verification and remaining scope limits.
- [AMD RDNA2 desktop GPU (gfx1030) form acceptance](shader-form-acceptance.md) — Rule and evidence required to promote a form from local results to `fixture verified`.
- [Test inventory comparison](test-inventory.md) — Detect missing or newly ignored Cargo tests across commits.
- [Persistent resource and translated rendering acceptance](x9-acceptance.md) — Native ARM64 results and unsupported cases.
- [Baseline acceptance](phase-02-baseline.md) and [Phase 01 assurance](phase01-assurance.md) — Candidate-specific evidence.
- [Independent reviews](review-log.md) — Review findings and acceptance records.
- [Mutation testing](mutation.md) and [benchmark protocol](benchmarks.md) — Sensitivity and timing requirements.
- [Infrastructure QA tooling](infrastructure/README.md) — Advisory orchestration, PR validation, benchmark analysis and release checks.

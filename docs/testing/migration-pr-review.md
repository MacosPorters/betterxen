# Migrated pull-request integration review

Reviewed on November 24, 2026 (America/Chicago). The initial inventory contained
23 open pull requests under `Arths17`, including the development-plan PR.
Twenty-two were merged after their updated heads passed the applicable GitHub
checks. Imported records retain their original author attribution and commits.

The maintainer authorized per-merge admin overrides for the review restriction.
The ARM64 build/test requirement, one-approval rule, code-owner review requirement
and enforcement for admins remain configured. Failed or pending checks were
resolved before merging. The conditional `verify` comment job is normally skipped;
that skip is not independent semantic verification.

## Results

All links below refer to the BetterXen repository.

| PR | Result | Integration work |
| --- | --- | --- |
| [#263](https://github.com/MacosPorters/betterxen/pull/263) | Pending | Pending: overlaps the contributor's live PR #290, excluded by the maintainer. |
| [#264](https://github.com/MacosPorters/betterxen/pull/264) | Merged; checks passed | Identification fixture runs in ordinary tests; benchmark-harness lint fixed. |
| [#265](https://github.com/MacosPorters/betterxen/pull/265) | Merged; checks passed | Bounded native texture staging; planner and Metal readback tests. |
| [#266](https://github.com/MacosPorters/betterxen/pull/266) | Merged; checks passed | Startup measurement harness; unit tests and correctness smoke run. |
| [#267](https://github.com/MacosPorters/betterxen/pull/267) | Merged; checks passed | Bounded flat draws and repeated-frame regressions. |
| [#268](https://github.com/MacosPorters/betterxen/pull/268) | Merged; checks passed | Windows x64 calling conventions; writable-home-slot regression and nonvolatile vector-register negative controls. |
| [#269](https://github.com/MacosPorters/betterxen/pull/269) | Merged; checks passed | Executable generations with source revalidation; original cache assertions preserved. |
| [#270](https://github.com/MacosPorters/betterxen/pull/270) | Merged; checks passed | ARM64 shift encoding and operand checks; draft marked ready. |
| [#271](https://github.com/MacosPorters/betterxen/pull/271) | Merged; checks passed | Floating-point unit initialization, combined decoder generation and preserved tests. |
| [#272](https://github.com/MacosPorters/betterxen/pull/272) | Merged; checks passed | Dynamic binary translation (DBT) report comparison and input validation. |
| [#273](https://github.com/MacosPorters/betterxen/pull/273) | Merged; checks passed | C startup service integration and registry regeneration. |
| [#274](https://github.com/MacosPorters/betterxen/pull/274) | Merged; checks passed | Instruction-length faults and precise retirement. |
| [#275](https://github.com/MacosPorters/betterxen/pull/275) | Merged; checks passed | Flag-transfer instructions; combined fixtures and supported-case expectations retained. |
| [#276](https://github.com/MacosPorters/betterxen/pull/276) | Merged; checks passed | Explicitly unsupported service stubs and combined registry. |
| [#277](https://github.com/MacosPorters/betterxen/pull/277) | Merged; checks passed | Guest arguments and environment with combined services. |
| [#278](https://github.com/MacosPorters/betterxen/pull/278) | Merged; checks passed | Packed texture footprints; CPU and native GPU checks. |
| [#279](https://github.com/MacosPorters/betterxen/pull/279) | Merged; checks passed | Loop instructions; regenerated tables and corrected description to 3,600 comparisons. |
| [#280](https://github.com/MacosPorters/betterxen/pull/280) | Merged; checks passed | Process termination and exit callbacks. |
| [#281](https://github.com/MacosPorters/betterxen/pull/281) | Merged; checks passed | Formatted output integrated with startup and arguments. |
| [#282](https://github.com/MacosPorters/betterxen/pull/282) | Merged; checks passed | File streams; combined registry and dependency readiness. |
| [#283](https://github.com/MacosPorters/betterxen/pull/283) | Merged; checks passed | Hosted C fixture; driver architecture checked before execution. |
| [#284](https://github.com/MacosPorters/betterxen/pull/284) | Merged; checks passed | Thread-local storage (TLS) template-copy integration and preserved changelog entries. |
| [#288](https://github.com/MacosPorters/betterxen/pull/288) | Merged; checks passed | Development phases, README and migration-check record. |

PR #263 remains open to avoid competing with the contributor's live PR #290.
PR #290 was excluded from branch edits, reviews and merge operations as requested.
Its filesystem security and integration acceptance are left to that workstream.

## Verification and evidence scope

The integrated runtime candidate was
`5366b9398668128fdbb0b0879840dfa072bfa352`. Its GitHub native Rust workflow passed,
including workspace formatting, strict Clippy, tests and the hosted C fixture.
Local native results below concern the reported Apple M4 host running macOS 27.0
(class A host observations); they establish no Xbox-hardware or commercial-game
execution result.

| Check | Observed result |
| --- | --- |
| `scripts/test-gpu.sh` | 124 passed, 0 failed; 2 manual timing tests ignored. Metal API and GPU validation enabled. |
| Packed block-compressed texture GPU target | 1 passed, 0 failed, 0 ignored, with both validation layers enabled. |
| `scripts/demo.sh` | All three original fixtures matched Metal, reference and known-answer outputs. |
| `scripts/audit/native.sh` | 110 executables/loadable dependencies checked with `file`, `lipo` and `otool`; ARM64 architecture and permitted linkage checks passed. |
| Status, claims and dependency audits | Passed. |

The cache fix also passed an isolated local workspace run at `44ac976`: 917
passed, 0 failed, 35 ignored, plus strict Clippy. Its three alternating CPU
benchmark pairs used native release executables and identical original inputs:
per-run warm medians were 87.334–88.333 microseconds with per-byte validation and
15.625–18.791 microseconds with generation validation. Machine idleness was not
controlled. These measurements do not close the formal performance gate; their
candidate scope and cold results are recorded in PR #269.

A supplemental benchmark test-harness command exposed a missing initialization
for the newly added cache-invalidation counter. The follow-up initializes that
fixture and runs the existing report, JSON, comparison and percentile tests via
an ordinary integration-test target, preserving their assertions. This lets
workspace CI detect future benchmark-test compilation failures. All 14 existing
tests passed through both the ordinary integration target and the documented
manual harness command; strict workspace Clippy passed as well.

Raw final native logs, cache benchmark JSON and the PR-state inventory are kept
under ignored `artifacts/pr-integration/` in the lead checkout. PR checks and merge
records remain linked from each GitHub record.

## Remaining acceptance

This is an integration and execution record. Independent semantic and security
acceptance remains separate from implementer-run checks and administrative merge
permission. No release gates were redefined or releases created. Commercial Xbox
game execution remains unsupported.

Active-block self-modifying code is tracked separately in
[issue #291](https://github.com/MacosPorters/betterxen/issues/291); native-code cache
reclamation remains [issue #95](https://github.com/MacosPorters/betterxen/issues/95).
The source fixture programs, desktop instruction evidence and native Mac results
retain their existing boundaries.

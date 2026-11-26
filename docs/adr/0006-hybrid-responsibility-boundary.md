# ADR 0006: BetterXen and CrossOver responsibilities

Status: **Proposed; not accepted.** Addresses [#351](https://github.com/MacosPorters/betterxen/issues/351). This draft neither completes Phase 1 nor authorizes Phase 2. [ADR 0005](0005-crossover-hybrid-direction.md) remains the accepted strategic direction.

## Context and evidence

CrossOver is the externally installed compatibility foundation. BetterXen needs reproducible experiments around it before deciding what additional compatibility work is justified. The evidence below is candidate-specific; open PRs are not merged capabilities.

- [Hybrid candidate #384 at e1b6a24](https://github.com/MacosPorters/betterxen/tree/e1b6a24a4f37360abb1f5c77bedbfd470cf1c720) supplies inspection, bounded workload execution, receipts, run records and offscreen interval analysis. Its original CPU and D3D11 fixtures exercise narrow Windows interfaces, not Xbox startup.
- [Interface review #350](https://github.com/MacosPorters/betterxen/issues/350#issuecomment-7177279618) records vendor-documented per-bottle graphics selection through the Mac UI. It found no supported automation setter in the reviewed documents. The candidate's backend environment override is an observed, version-specific interface. Parent environment inheritance can select DXVK while the candidate records no requested backend; that defect must be resolved before accepting default-backend comparisons.
- [Baseline #355](https://github.com/MacosPorters/betterxen/issues/355#issuecomment-7174389034) records local x86-64 CPU and offscreen D3D11 results on CrossOver 26.3 and rejection of the ARM64 Windows executables. Full independent baseline reproduction remains outstanding. Offscreen draw-and-wait intervals are not presented-frame times or game frame rates.
- [Independent build review #364](https://github.com/MacosPorters/betterxen/issues/364#issuecomment-7176752107) reproduced the eight fixture builds in fresh directories. Rust outputs required an additional linker flag to remove COFF timestamps. The external toolchain is universal, although its compiler ran as ARM64; its ARM64-only packaging criterion and bootstrap remain unresolved.
- [Licensing assessment #354](https://github.com/MacosPorters/betterxen/issues/354) and [registry review #363](https://github.com/MacosPorters/betterxen/issues/363) define separate component obligations and outstanding review. They do not grant permission to bundle proprietary components or clear individual files from unrelated runtimes.

These are recorded observations and research inputs, not an independent acceptance of the components that produced them. The merged [workload protocol](../testing/hybrid-workloads.md) defines the baseline evidence required.

## Proposed responsibility boundary

| Responsibility | Proposed owner and boundary |
| --- | --- |
| Install, activate and update CrossOver | User and CodeWeavers tooling. BetterXen records the installed version and location; it does not bundle CrossOver or bypass activation. |
| Windows API compatibility and selected graphics translation | CrossOver and its selected upstream components. BetterXen does not infer Xbox API compatibility from Windows fixture results. |
| Bottle creation and graphics configuration | Prefer documented CrossOver UI and supported interfaces. Record manual configuration. Keep observed command-line behavior explicitly version-scoped; do not edit proprietary internals to invent a configuration API. |
| BetterXen host tooling | Native ARM64 inspection, launch supervision, diagnostics and evidence collection, with explicit failures and bounded execution. Preserve source and runtime version receipts. |
| Workload correctness | Original or authorized workloads with independently derived expectations, complete readback where specified, negative controls and separate unavailable results. Reuse established compilers rather than add a compiler for these tests. |
| Benchmarking | Analyze validated measurements with the exact capture boundary, configuration, samples and overhead recorded. Optimize only a demonstrated bottleneck. |
| Compatibility gaps | First reproduce a specific failure and assess existing upstream support. Prefer a focused upstream fix or documented integration change. A replacement runtime, translator or graphics backend requires a separate justified decision. |

## Limits and excluded expansion

The preserved native decoder, ARM64 translator, runtime, shader compiler and Metal backend remain paused. This proposal does not schedule native work, promise game execution, or choose a console launch ABI.

[#394](https://github.com/MacosPorters/betterxen/pull/394) remains outside the accepted boundary pending the per-file provenance decision in [#399](https://github.com/MacosPorters/betterxen/issues/399). Passing synthetic tests does not resolve provenance. No policy exception, game-specific import or second compiler is authorized here.

CrossOver, D3DMetal, Rosetta 2 and Apple tools remain externally supplied under their respective terms. Component licenses and compatibility require their own review; this ADR is not legal advice.

## Conditions before acceptance

1. Review the architecture findings in [#348](https://github.com/MacosPorters/betterxen/issues/348), interface findings in #350, licensing assessment in #354 and registry review in #363. Name unresolved vendor and license questions rather than treating them as cleared.
2. Resolve backend environment inheritance, evidence persistence ([#398](https://github.com/MacosPorters/betterxen/issues/398)) and bottle-stop correctness ([#388](https://github.com/MacosPorters/betterxen/pull/388)) on the actual integration candidate; preserve review and runtime evidence separately.
3. Complete the toolchain criteria in #364 and the workload/readback criteria in [#365](https://github.com/MacosPorters/betterxen/issues/365). Texture and compute authoring is owned by [#400](https://github.com/MacosPorters/betterxen/issues/400) and [#401](https://github.com/MacosPorters/betterxen/issues/401).
4. Obtain the second-contributor baseline reproduction required by #355. Capture methods and overhead under [#145](https://github.com/MacosPorters/betterxen/issues/145) must retain their actual scope.
5. An independent reviewer checks the cited evidence and a maintainer explicitly accepts the responsibility boundary. Only then update the roadmap with the justified next-phase scope and acceptance record.

## Consequences and alternatives

This boundary keeps current work focused on reliable experiments using established compatibility infrastructure. It also leaves dependence on vendor interfaces, licenses and Rosetta availability visible. Automation may remain manual where a supported interface is unavailable.

Continuing native-first or importing another complete runtime would create a larger implementation and provenance burden without resolving the baseline gaps. Neither alternative is selected. A future proposal can reassess a specific missing capability when authorized evidence demonstrates the need.

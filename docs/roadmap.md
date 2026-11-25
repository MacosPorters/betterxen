# BetterXen roadmap

**This is the single authoritative active roadmap.** GitHub Issues hold the current phase's tasks, the open milestone names the current phase, and this file explains strategy, phase order, parked work and completion criteria. If another document disagrees, this one governs, except [ADR 0005](adr/0005-crossover-hybrid-direction.md) for the decision itself.

## Direction

BetterXen's active direction is a **CrossOver-based hybrid architecture for Apple Silicon**. CrossOver is the intended compatibility foundation and is an **externally installed dependency**, never bundled or redistributed. BetterXen's own work is the layer around it: integration, configuration, diagnostics, reproducible testing, benchmarking and measured optimization. The independent native implementation (x86-64 decoder, ARM64 translator, runtime, gfx1030 shader compiler, Metal backend) is **preserved but paused**; see the [native roadmap](research/native-roadmap.md) and the [native issue archive](research/native-issue-archive.md).

This is the intended architecture, **not a verified Xbox compatibility stack**. Nothing in this repository demonstrates that CrossOver, Rosetta 2 or D3DMetal run any Xbox title, and CrossOver is not assumed to provide Xbox runtime compatibility. All workloads must be independently authored, publicly licensed or otherwise authorized.

```
BetterXen: integration, configuration, diagnostics, benchmarking, optimization
        |
CrossOver (external): Wine-based runtime APIs | Rosetta 2 CPU translation | D3DMetal / DXMT graphics
        |
Apple Silicon: ARM64, Metal, unified memory
```

Priorities, in order: practical compatibility, functional integrated execution, rendering correctness, stability, performance, Apple Silicon-specific optimization, long-term independence. Reuse before reinvention; measure before optimizing; evidence before claims.

## Rolling planning

Only the current phase has a GitHub milestone and detailed issues. Later phases are described below and become milestones and issues only when the evidence from the current phase makes their requirements clear. Work that was specified early but depends on unverified assumptions is listed under [Parked ideas](#parked-ideas) with its closed issue, so it can be reopened rather than rewritten. Phase numbers are provisional. The earlier eight-phase plan (H0 to H7) is archived in [archive/hybrid-phases-h0-h7.md](archive/hybrid-phases-h0-h7.md).

## Engineering rules

[AGENTS.md](../AGENTS.md) (`CLAUDE.md` is a symlink to it) was rewritten to permit CrossOver, Wine-based infrastructure, Rosetta 2 and D3DMetal subject to licensing; ADR 0005 records the decision. The honesty, verification, provenance and clean-room rules are unchanged. Both are on the local branch `planning/hybrid-first-transition` and await maintainer review; nothing has been pushed.

## How progress is reported

Phase status is one of: Not started, Researching, In development, Blocked, In validation, Verified complete, Retired. A phase is Verified complete only with the evidence its completion criteria name. Progress is not derived from commits, lines of code, closed issues, instruction counts or passing isolated fixtures. No phase is currently Verified complete.

## Phase 1 — CrossOver Research, Feasibility and Baseline (active)

**Status:** Researching. **Milestone:** `Phase 1 - CrossOver Research, Feasibility and Baseline`.

**Objective:** Establish what the installed CrossOver version actually provides, which interfaces are supported, what may be reused or redistributed, and a reproducible baseline on authorized workloads.

**Completion criteria** (all required, each with recorded evidence):

1. CrossOver architecture, platform support, CPU translation and graphics backends documented from primary sources, with unknowns listed ([#348](https://github.com/MacosPorters/betterxen/issues/348)).
2. Supported integration interfaces separated from undocumented behavior ([#350](https://github.com/MacosPorters/betterxen/issues/350)).
3. Licensing and redistribution boundaries documented ([#354](https://github.com/MacosPorters/betterxen/issues/354)); the registry independently re-derived ([#363](https://github.com/MacosPorters/betterxen/issues/363)).
4. A reproducible way to build original Windows test programs ([#364](https://github.com/MacosPorters/betterxen/issues/364)) and original graphics workloads with self-verifying output ([#365](https://github.com/MacosPorters/betterxen/issues/365)).
5. Workload set, measurement protocol and host receipt defined ([#353](https://github.com/MacosPorters/betterxen/issues/353)) and a frame-time capture method ([#145](https://github.com/MacosPorters/betterxen/issues/145)).
6. A reproducible CrossOver environment and a recorded baseline of CPU, GPU, memory and stability, reproduced by a second contributor ([#355](https://github.com/MacosPorters/betterxen/issues/355)).
7. BetterXen's responsibilities versus CrossOver's recorded in an ADR a maintainer accepts ([#351](https://github.com/MacosPorters/betterxen/issues/351)).

**Open issues:** #145, #348, #350, #351, #353, #354, #355, #363, #364, #365, plus #346 (project board, needs the GitHub `project` token scope; does not block engineering).

**Known risks:** CrossOver is proprietary, so bundling or modifying may be prohibited; integration interfaces may be undocumented (see below); Xbox titles may not run at all.

### Critical path

1. Desk research needs nothing installed and can start now: #348, #350, #354, #364 (build only), #353.
2. #363 re-derives the registry independently of its author.
3. #365 needs #364 and #353.
4. #355 needs #364, #353 and the interface findings of #350.
5. #351 needs #348, #350, #354 and #355.

Hard dependencies are recorded as GitHub blocked-by relationships; everything else is preferred order.

### Verified and unverified (2026-11-25)

Details and sources are in [primary-source-verification-2026-11-25.md](research/primary-source-verification-2026-11-25.md). In short: CodeWeavers' public documentation names the backends Auto, D3DMetal, DXMT, DXVK and Wine, but **`CX_GRAPHICS_BACKEND` and `cxbottle.conf` are not documented there and must not be treated as a supported interface** until #350 resolves it. Apple's two September 2026 developer news items about Rosetta disagree with each other. D3DMetal's license text was not located; it stays user-supplied and unbundled.

## Provisional later phases (no milestone yet)

| Phase | Purpose | Starts when |
| --- | --- | --- |
| 2. Integration foundation | Only the tooling Phase 1 shows is missing: environment inspection, workload runner and run records, output-path policy | Phase 1 criteria 2 and 6 are met |
| 3. Compatibility validation | Run authorized workloads and upstream conformance tests, record reproducible failures | A runner exists and a baseline is recorded |
| 4. Graphics and performance | Backend comparison and measured bottleneck work; no improvement promised before measurement | Phase 3 shows what to measure |
| 5. Stability and maturity | Soak, memory pressure, thermals, version matrices, dependency maintenance, upstream contributions | Phase 4 results exist |

Independent native reassessment is separate and deferred: it needs an explicit maintainer decision.

## Parked ideas

Specified in detail on 2026-11-25, then closed as premature for Phase 1 with the text preserved. Reopen when the stated prerequisite is met. Parking is not rejection.

| Issue | Subject | Reopen when |
| --- | --- | --- |
| [#366](https://github.com/MacosPorters/betterxen/issues/366) | Read-only CrossOver inspection command | The #353 receipt script proves insufficient |
| [#367](https://github.com/MacosPorters/betterxen/issues/367) (absorbs #368) | Workload runner and run records | Phase 1 finds #350 supports scripted launches (Phase 2) |
| [#257](https://github.com/MacosPorters/betterxen/issues/257) | Output-path policy for exports and caches | A tool that writes exports exists (Phase 2) |
| [#369](https://github.com/MacosPorters/betterxen/issues/369) | Upstream Wine conformance subset in a bottle | Runner and baseline exist (Phase 3) |
| [#370](https://github.com/MacosPorters/betterxen/issues/370) | Backend comparison on equal workloads | Baseline and workloads exist (Phase 4) |
| [#371](https://github.com/MacosPorters/betterxen/issues/371), [#372](https://github.com/MacosPorters/betterxen/issues/372) | Shader-compile latency; Metal System Trace recipe | Baseline names a bottleneck (Phase 4) |
| [#373](https://github.com/MacosPorters/betterxen/issues/373), [#204](https://github.com/MacosPorters/betterxen/issues/204) | Soak and thermal drift; memory-pressure benchmark | Workloads and baseline exist (Phase 5) |
| [#374](https://github.com/MacosPorters/betterxen/issues/374) | Re-baseline on a Rosetta-free CrossOver build | Such a build ships (Phase 5); facts folded into #348 |
| #173, #174, #219 | Contributor-workflow administration | A concrete need appears |

## Native work

About 110 native issues were consolidated. All are closed with their text, comments and attribution intact except four with an assignee and an open pull request (#247, #291, #307, #313), which stay open until their authors decide. See the [native issue archive](research/native-issue-archive.md). Source code, tests, benchmarks and history are untouched.

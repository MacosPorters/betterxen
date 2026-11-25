# Detailed hybrid phases H0 to H7 (retired planning)

**Archived 2026-11-25; not the active plan.** These eight phases were defined before any research had been done. The active roadmap is [../roadmap.md](../roadmap.md): five provisional phases, with only Phase 1 active. The text below preserves the objectives, scope and exit conditions of the retired H2 to H7 milestones (and H0/H1) for reference when later phases are defined. Issue numbers in it are historical.

## Phases as originally written

### H0 — CrossOver Architecture Transition

**Status:** In development

**Objective:** Make CrossOver the single authoritative active compatibility foundation across the repository and GitHub planning surfaces.

**Scope:** README, architecture docs, ADR 0005, rewritten engineering rules, roadmap, upstream research registry, technical plans, labels, milestones, issue and PR templates, issue triage, project board, preservation of the native implementation.

**Dependencies:** None.

**Deliverables:** Updated README and docs; ADR 0005; docs/roadmap.md; deferred docs/research/native-roadmap.md; rebuilt issues and milestones; project board; preserved native code.

**Validation:** Repository link/terminology check; GitHub consistency check.

**Exit conditions:** Repository, documentation, issues, milestones and board consistently state the CrossOver-based direction. The project board requires a token with the project scope and is not yet configured.

**Known risks:** Project board cannot be configured without the GitHub project token scope (#346). The engineering rules conflict was resolved by rewriting AGENTS.md (#345).

### H1 — CrossOver Research and Baseline

**Status:** Not started

**Objective:** Establish what the installed CrossOver version actually provides and a reproducible baseline, without assuming it provides Xbox runtime compatibility.

**Scope:** CrossOver architecture, supported macOS versions, Apple Silicon support, relationship to upstream Wine, CPU translation mechanism, graphics backends (D3DMetal, DXMT and others), supported Direct3D versions, configuration interfaces, documented command-line interfaces, bottle management, diagnostics and profiling options, licensing and redistribution, supported integration mechanisms, long-term maintenance. Open-source and proprietary components distinguished.

**Dependencies:** Engineering rules rewritten (#345, done). CrossOver is an externally installed dependency until its license says otherwise; nothing is bundled.

**Deliverables:** CrossOver capability and integration-boundary report; licensing and dependency assessment; reproducible environment; baselines of CPU, GPU, memory and stability on authorized workloads.

**Validation:** Every claim cites vendor or upstream documentation, or a recorded observation on the installed version. Unknown stays unknown. Equivalent workloads, settings, hardware and software versions.

**Exit conditions:** Capabilities and integration boundaries are documented for the installed version; environment is reproducible by another contributor; baselines recorded; BetterXen vs CrossOver responsibilities proposed.

**Known risks:** CrossOver is proprietary: bundling, modifying or redistributing may be prohibited; integration interfaces may be undocumented; Xbox titles may not run at all.

### H2 — BetterXen Integration Foundation

**Status:** Not started

**Objective:** Build the BetterXen layer around CrossOver: integration boundaries, configuration, diagnostics, reproducible test workflows and host integration.

**Scope:** Environment and configuration management, diagnostics, test workflows, verified host integration, regression testing. Only what CrossOver does not already provide adequately.

**Dependencies:** H1.

**Deliverables:** Boundary document; configuration and environment management where justified; diagnostic infrastructure; reproducible test workflows; verified host integration; regression tests.

**Validation:** Independent contributor reproduces the workflows; no placeholder components.

**Exit conditions:** Integration foundation builds, tests and reproduces on a second setup.

**Known risks:** Reliance on undocumented or changing CrossOver behavior.

### H3 — Compatibility Validation

**Status:** Not started

**Objective:** Evaluate authorized workloads and document verified behavior and limitations.

**Scope:** Authorized workloads, documented limitations, runtime behavior, graphics correctness, input and interaction, reproducible failures, integrated validation criteria.

**Dependencies:** H2.

**Deliverables:** Validation criteria; per-workload results; reproducible failure reports; verified runtime, graphics and input results.

**Validation:** Independent verification of rendering and behavior. Startup or a screenshot alone is not proof.

**Exit conditions:** Selected workloads show verified integrated behavior under documented criteria; failures reproducible.

**Known risks:** Failures may originate in CrossOver or its components outside project control.

### H4 — Graphics and Performance Engineering

**Status:** Not started

**Objective:** Evaluate graphics backends and optimize only demonstrated bottlenecks.

**Scope:** Supported backends, CPU/GPU utilization, unified memory, frame-time consistency, synchronization, shader compilation and caching. Metrics: average FPS, 1% low, frame-time percentiles, CPU and GPU utilization, unified memory, memory pressure, translation overhead where measurable, shader compile latency, startup, thermals, long-duration stability. Reference machine: M4 Mac, 24 GB.

**Dependencies:** H3; measurement protocol.

**Deliverables:** Backend comparison; profiling reports; bottleneck list; before/after results; regression tests.

**Validation:** Equivalent workloads, rendering settings, hardware and versions; correctness and rendering equivalence verified.

**Exit conditions:** Improvements measurable, reproducible and without loss of correctness or stability. No improvement is promised before measurement.

**Known risks:** Many components are closed and not modifiable; single reference machine.

### H5 — Stability and Hardware Scaling

**Status:** Not started

**Objective:** Evaluate Apple Silicon configurations, memory pressure, thermals and long-duration stability.

**Scope:** Available Apple Silicon configurations, memory pressure, thermal behavior, long-duration tests, hardware documentation, regression coverage.

**Dependencies:** H3; hardware access.

**Deliverables:** Hardware compatibility documentation; memory-pressure and thermal reports; long-duration results; expanded regression coverage.

**Validation:** Each documented configuration backed by a test run on it.

**Exit conditions:** Tested configurations documented; untested hardware never claimed.

**Known risks:** Only one reference machine currently available.

### H6 — Hybrid Architecture Maturity

**Status:** Not started

**Objective:** Maintainable integration infrastructure and contributor workflows.

**Scope:** Contributor workflows, verified-capability documentation, reproducible performance results, complexity reduction, long-term dependency risk (CrossOver, Rosetta 2, D3DMetal).

**Dependencies:** H3 to H5 evidence.

**Deliverables:** Maintainable infrastructure; contributor documentation; verified capability list; reproducible results; dependency risk assessment.

**Validation:** Contributors other than the authors maintain the workflow.

**Exit conditions:** The architecture shows demonstrated value and the team can maintain it.

**Known risks:** Vendor decisions (CrossOver licensing, Rosetta lifetime) are outside project control.

### H7 — Future Native Reassessment (deferred)

**Status:** Not started

**Objective:** Deferred. Reassess independent native components only on an explicit maintainer decision or when one offers a demonstrated benefit to the active architecture.

**Scope:** See docs/research/native-roadmap.md.

**Dependencies:** Explicit maintainer decision.

**Deliverables:** Reassessment record.

**Validation:** Maintainer decision recorded in an ADR.

**Exit conditions:** A recorded decision to continue, resume specific native work, or archive components.

**Known risks:** Premature reinvention of mature infrastructure.


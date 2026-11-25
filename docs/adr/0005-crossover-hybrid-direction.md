# ADR 0005: CrossOver-based hybrid direction

Status: **Accepted (strategic direction).** Accepted by the maintainers' planning directive of 2026-11-25. The proposed technical architecture below is **not** accepted or verified; each integration still needs validation.

## Context

BetterXen was developed native-first: an independent x86-64 decoder, interpreter, ARM64 translator, runtime, shader compiler and Metal backend, verified only on synthetic programs and shaders. No Xbox title runs on it. The maintainers decided to focus all active engineering on a hybrid architecture and to pause native work.

## Accepted strategic direction

- CrossOver is the primary compatibility foundation, installed externally.
- Hybrid is the only active development direction. Native research is preserved (tag `native-first-final` marks the last `main` commit before this change; the code stays in the workspace) and paused; resuming needs an explicit maintainer decision.
- Reuse established infrastructure; build independent replacements only for demonstrated limitations.
- The engineering rules in `AGENTS.md` (and `CLAUDE.md`, a symlink) were rewritten accordingly, replacing the former prohibition on Wine, CrossOver, Rosetta and MoltenVK.

## Proposed technical architecture (not accepted)

BetterXen provides integration, configuration, environment management, diagnostics, reproducible testing, benchmarking and measured optimization around CrossOver. Within CrossOver, the intended layers are Wine-based runtime APIs, Rosetta 2 for x86-64 translation and D3DMetal or DXMT for graphics, on Apple Silicon (ARM64, Metal, unified memory). The exact responsibility boundary is decided in #351, at the end of Phase 1.

## Unverified integration assumptions

- That CrossOver, Rosetta 2, D3DMetal or DXMT run any authorized workload BetterXen needs. Assumed not until measured (#355).
- That CrossOver offers supported interfaces for bottle management, configuration, launching and diagnostics (#350).
- Rosetta 2's support lifetime and D3DMetal's license terms (#349, #354).
- That any of this provides Xbox runtime compatibility. It is assumed not to.

## Verified implementation capabilities

None for the hybrid architecture. Verified native-research capabilities (synthetic evidence only) are unchanged and listed in `docs/progress.md`.

## Planning consequence

Planning is progressive: Phase 1 (research, feasibility and baseline) is the only active phase; later phases are provisional and defined from its findings. Native work is paused indefinitely.

## Consequences

Positive: reuse of mature infrastructure. Negative: dependence on a proprietary vendor and on Rosetta 2 availability; limited ability to modify closed components; the former "No Wine, No CrossOver, No Rosetta" project identity is retired for active work. Anything derived from leaked or unauthorized material stays excluded, including projects built for such inputs (see the upstream registry).

## Alternatives considered

Continue native-first; maintain both tracks at 75/25; a generic Wine-based stack without CrossOver.

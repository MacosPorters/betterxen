# Hybrid documentation consistency audit

Audited 2026-11-25 for the maintainer's documentation-only alignment task.
Base: `59b6c4db5e2ceba5aecafc947d1f314c8fe0a895`, the head of split integration
[PR #395](https://github.com/MacosPorters/betterxen/pull/395), stacked on
[PR #394](https://github.com/MacosPorters/betterxen/pull/394). The documentation
branch is `docs/hybrid-architecture-alignment`. The base advanced during the audit
to include the wave fixes from #394; the documentation branch was rebased onto
that updated top before final checks. No new issue was requested or
created. This is a documentation consistency review, not independent technical
acceptance of either implementation PR.

## Coverage and disposition

The audit inventories the base's 95 tracked Markdown paths, including the
`CLAUDE.md` symlink, and checks architecture/development references in issue
forms, Cargo metadata, crate API comments and documentation registries. Ignored
experiments and generated local artifacts are outside the published documentation.

| Surface | Disposition |
| --- | --- |
| `AGENTS.md`, `CLAUDE.md`, `CONTRIBUTING.md`, PR template | Point agents and contributors to hybrid boundaries and appropriate checks; preserve the symlink, claim precedence, provenance rules and native pause. Explain direct maintainer assignments and stacked bases. |
| Root README and documentation index | Put hybrid responsibilities and split dependencies first; label retained native diagrams, build procedures and capability counts. |
| Architecture, interfaces and ADRs | Add the current integration guide. Retain native fixture contracts in ADRs 0003/0004 with explicit scope; retain ADR 0005's strategic decision and distinguish subsequent observations from architectural acceptance. |
| Roadmap, milestones and archived plans | Keep Phase 1 active and its exit criteria intact. Mark issue-state inventories as snapshots; preserve already-labelled archived plans and retired milestone data. Remove obsolete local-branch instructions. |
| Crate documentation | Add a workspace crate guide and shader-crate README. Inspect crate-level Rustdoc and manifest descriptions; retain API text and legacy names, with component scope explained in the guides. No Rust sources or manifests changed. |
| CPU, runtime and shader contracts | Label maintained native procedures and inventories as preserved research. Keep generated coverage content unchanged; its native scope is explained by the index, architecture and progress pages. |
| Research, upstream registry and reuse matrix | Distinguish initial discovery decisions from the shader-only snapshot already in the base. Preserve original pins, notices, uncertainty and the requirement for independent provenance review. No broader upstream adoption is authorized. |
| Testing, validation, benchmarks and campaign ledgers | Separate hybrid readback from native regressions and historical acceptance. Mark old next tasks and campaign ownership as historical; retain recorded counts and candidate scope. Explain the expanded BetterXen test selection versus the script’s old four-test expectation. |
| Legal/security/support and attribution | Clarify input policy versus demonstrated execution and the native threat-model boundary. Preserve security reporting, conduct rules, license texts, upstream notices and historical benchmark reports. |
| Workflow/issue forms and machine-readable evidence | Existing hybrid task forms already describe active scope. Leave workflows, claims/status data, retired milestone JSON, fixture manifests and generated evidence untouched. |

Searches covered native-first/native-only language, former bans on Wine,
CrossOver, Rosetta or Vulkan, phase and milestone references, active priorities,
next-task instructions, licensing/reuse assertions and testing guidance. Retained
old terminology is scoped historical evidence or a component identifier, not a
second active roadmap. No external vendor facts were revalidated by this audit;
new architecture statements refer to the checked-in source and recorded PR
observations, with their limits preserved.

## Validation

Commands are listed in the [testing guide](README.md). Repository hygiene,
generated status/semantic coverage, claims, dependency direction and whitespace
checks passed on the aligned tree: hygiene checked 699 tracked files with zero
problems, claims checked 22 records and dependency direction checked five
graphics crates. Generated tables matched and whitespace checks passed. These
checks do not execute workloads, certify upstream
provenance or close a phase gate.

`CARGO_BUILD_JOBS=2 cargo doc --locked --workspace --no-deps` initially stopped
because the default compiler was Rust 1.94.1, below the workspace requirement.
`CARGO_BUILD_JOBS=2 cargo +1.100.0 doc --locked --workspace --no-deps` completed.
It emitted 38 Rustdoc warnings in unchanged sources: 3 in `victory-shader`, 12 in
`xenora-loader`, 16 in `xenora-ref` and 7 in `xenora-isa` (private links,
unresolved bracketed expressions and an unclosed HTML tag). This is a successful
API-documentation build with warnings, not a warning-free build. Logs are under
ignored `artifacts/docs-alignment/`.

CPU, GPU, CrossOver workloads, mutation and performance tests were not rerun for
this documentation-only change. Existing results remain attributed to their
original candidates and hosts. Independent reproduction and acceptance remain
outstanding wherever the source records say so.

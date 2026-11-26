# Hybrid runtime audit and integration record (2026-11-25)

What was inspected, what the integration branch contains, what to do with the open shader and runner work, and what remains. Execution evidence is in [hybrid-execution.md](../hybrid-execution.md). Written by the author of the changes it describes, so it is not an independent review.

## Revisions

| Item | Revision |
| --- | --- |
| `main` | `d9333d2` |
| #384 hybrid runner | `e1b6a24` |
| #388 stall detection | `8e5856b` |
| #390 GPU wired-memory fields | `0427465` |
| #402 compute workload | `ce9c10a` |
| #394 shader import (excluded) | `d8c7a39` |
| #395 shader test runner (excluded) | `2713c9e` |
| skakwy/victory-mac and MacosPorters/victory-mac-new | both at `3ecbb8f96956` (one README commit after the previously reviewed `22b3f8e45a59`) |
| MacosPorters/VIctory-macOS | `d6fc0620f53a` |

## Integration branch

`engineering/unified-hybrid-runtime` = `main` + merges of #384, #388, #390 and #402, then the changes described here. Conflicts were in `CHANGELOG.md` only; both sides were kept. It cannot be merged to `main` as an independent change: it contains other contributors' unmerged commits. It becomes a small diff once #384, #388, #390 and #402 land.

Baseline before changes (merged base): format, Clippy, workspace tests (1059 passed, 0 failed, 35 ignored), release build, status, claims, dependency-direction and documentation checks passed. `scripts/assemble-fixture.sh --check` failed because Homebrew `llvm` is not installed; with `llvm@17` one assembled fixture differs. This is a toolchain mismatch present before any change.

Final code: format, Clippy, 1073 tests passed (0 failed, 35 ignored), release build and the same audits passed, with the same fixture failure. `scripts/test-gpu.sh` (native research GPU suite, on revision `e6d614b`; later commits touch no native code) reported 124 passed, 0 failed, 2 ignored.

## Open pull requests and merge order

| PR | State seen | Disposition |
| --- | --- | --- |
| #384 | conflicts with `main` (changelog) | Merge first after resolving. Independent reproduction still missing |
| #388 | clean on #384 | Merge after #384 |
| #390 | approved, clean | Merge after #384 |
| #402 | clean, workload not re-run here | Review and merge after #384 |
| #392 | closed 2026-11-26 as superseded | Nothing to do. Its fix is `ca2fe15` on #394, so it is **not** on `main` |
| #394 | draft, marked not approved | **Do not merge.** See below |
| #395 | draft, stacked on #394 | **Do not merge as is**; it only runs #394's tests |
| #397 | documentation, conflicts | Rewrite after the #394 decision; it describes the import |
| #403 | draft ADR | Independent of this branch |
| #404 | opened during this work, targets `main`, texture-sampling workload for #400 | Independent of this branch; not read or run here |

#398 and #399 are issues, not pull requests. Suggested order: #384, #388, #390, #402, then this branch.

### Why #394 is excluded

Its own provenance record and the independent review on [#399](https://github.com/MacosPorters/betterxen/issues/399) find title-driven behaviour in the imported files; the independent review recommends removing the import and keeping the original D3D12 probes. The engineering rules exclude game-specific code and source from projects built for leaked material. Contributor permission does not change that, and no maintainer policy decision is recorded. Consequence: the hybrid path has **no console shader translator**, and the wave-operation fixes have nowhere admissible to apply.

## Repository inventory and audit coverage

649 tracked files on `main`. Depth of inspection in this audit:

| Area | Depth | Classification |
| --- | --- | --- |
| `crates/hybrid` | read in full | Implemented; locally validated, not independently |
| `workloads/` | read: frame loop, build scripts, new workload; others by listing | Original fixtures |
| `scripts/hybrid`, CI workflows | scripts read or run; workflows read in part | the `native-rust` job runs on a hosted macOS runner; nothing seen there runs CrossOver |
| `docs/roadmap.md`, hybrid research records | read in part | Current |
| `crates/loader` (about 62,000 lines), `ffi`, `metal`, `shader`, `decoder`, `isa`, `ref`, `texture`, `import-gen`, `cli` | **not read line by line**: module headers, dependency graph, marker search, test run | Paused research; unit-tested; not on the hybrid path |
| `app/`, `experiments/x1`, `benchmarks`, `fuzz`, `tests/`, other `scripts/` | **listing only** | Not assessed |
| Remaining documentation | **not read** | Not assessed |
| `crates/victory-shader` | not on `main`; provenance records read, code not re-reviewed | Unmerged, not approved |

Dependency graph: `betterxen-hybrid` has no dependencies. The native crates depend only on each other plus `libc`, `sha2` and the `objc2` Metal bindings. `cargo audit` and `cargo deny` are not installed, so no advisory check was run.

Marker search over all Rust files: no `todo!` or `unimplemented!`, and no TODO, FIXME, XXX or HACK comments outside documentation. That does not show the code is complete. Ignored tests (35) state their reasons: 27 need a Metal device, 5 need an LLVM AMDGPU target and 3 are manual timing runs. A static count of `unsafe` sites (`ffi` 152, `metal` 33, `loader` 28) was taken; they were **not reviewed**.

## Reference repositories

Inspected at documentation level only (commit, licence, file tree, README, and the earlier component review), because the engineering rules limit projects built for leaked material to discovery use. Their functions and types were not read in this audit. All are GPL-3.0 at repository level; per-file provenance is otherwise unestablished. The VIctory-macOS README states that it runs a console build of a commercial game.

| Component (path) | Responsibility | BetterXen equivalent | Disposition |
| --- | --- | --- | --- |
| `crates/victory-loader`, `victory-pe` | Load a console executable | native `loader` (paused) | Exclude: serves a console image; contains title patches |
| `crates/victory-hle` | Console runtime services | none on the hybrid path; Wine serves Windows programs | Exclude: console and title behaviour |
| `crates/victory-gpu` | Console Direct3D to desktop Direct3D 12 | none | Exclude: title-driven |
| `crates/victory-shader` | GFX10 to DXIL | native `shader` (MSL, paused) | Exclude (#399). Any replacement is written from AMD's public ISA |
| `crates/victory-formats`, `victory-game`, `victory-pipeline`, `mac/scan` | Asset and title handling | none | Exclude: game-specific |
| `crates/victory-core`, `mac/run-victory.sh` | Supervision, memory guard, stall notes | `memory.rs`, stall detection in `run.rs` | Ideas already reimplemented in #384 and #388 and credited there |
| `mac/unixlib` | Memory alias and Metal swizzle | none | Exclude: acts on D3DMetal internals |
| `tools/macos/build-crossover-wine.sh`, `build-deps.sh` | Build Wine from CodeWeavers' published source | none | Candidate, tracked in #389; needs per-file review first |
| `tools/macos/gnullvm-cc.sh` | Cross-link wrapper | pinned llvm-mingw | Not needed |
| `crates/victory-shader/src/emit/f64.rs` | Integer emulation of fp64 | none | Deferred (#387); native work is paused |

[utmapp/d3dmetal-native](https://github.com/utmapp/d3dmetal-native) was reviewed later; see its [evaluation](d3dmetal-native-evaluation.md). It is not adopted.

Upstream infrastructure (Wine, DXVK, DXMT, VKD3D-Proton, MoltenVK) stays an external dependency delivered by CrossOver. Mesa, LLVM and DXC are not integrated and were not re-audited.

## Gaps found and changed here

| Gap | Effect | Change |
| --- | --- | --- |
| Inherited `CX_GRAPHICS_BACKEND` | Backend changed while the record said none | Removed from the child; recorded |
| Mistyped backend ignored by CrossOver | Silent fallback to default | Refused |
| No program check before launch | ARM64 or non-PE input failed late or obscurely | `pe.rs` |
| Process matching missed the Windows program | Recorded memory never included the workload | Matched by drive-letter command line |
| Exit 0 plus a marker counted as success | Partial initialization could pass | Gate lines and `--require-gates` |
| Non-UTF-8 byte emptied the log; unbounded output | Lost evidence; disk growth | Lossy decode; 64 MiB limit |
| `cannot execute` matched on success | Misclassification | Only on non-zero exit |
| No program presented a frame | Gates F to I had no evidence | `workloads/d3d11-integrated` |

## Not done

- No Direct3D 12 workload, no compiled-from-console shader path, no audio, networking, controller or typed-keyboard test.
- Texture formats beyond RGBA8 and D32, mip mapping, blending, MSAA, resize, full screen and device loss are untested.
- The launcher is a separate binary (`betterxen-hybrid`); it was not folded into the `betterxen` native-research CLI.
- Shader and pipeline caches are CrossOver's; BetterXen manages none.
- No thermal, GPU-memory, CPU-utilization attribution or multi-hour stability data.
- `README.md` and architecture documents were left to #397 to avoid overlapping edits.

## Blockers

| Kind | Blocker |
| --- | --- |
| Decision | Maintainer ruling on #394 / #399 |
| Review | No independent reproduction of #384, #388, #402 or this branch |
| Documentation of interfaces | CrossOver launch and backend interfaces are undocumented (#350) |
| Scope | Nothing in the repository runs console software under CrossOver; Wine provides Windows interfaces, not Xbox ones. No evidence suggests the current path would run it |

## Next steps

1. Decide #394 as #399 recommends, or record a policy change.
2. Land #384, #388, #390, #402; rebase this branch.
3. A second contributor runs `scripts/hybrid/accept-integrated.sh` (gate J).
4. Type keys on the Mac in `--interactive` mode and record the result (gate H).
5. Multi-hour run with GPU-memory observation (gate I).
6. Handle supervisor signals so Ctrl-C stops the bottle.

## Continuation checkpoint

Branch `engineering/unified-hybrid-runtime`; local worktree `artifacts/unified-hybrid-runtime/worktree`; acceptance records under `artifacts/acceptance/` (ignored by Git). Claims posted on #355 and #365. Resume from "Next steps"; the inventory rows marked listing only or not read are the unfinished audit.

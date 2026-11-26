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

Mergeability checked with `git merge-tree` on 2026-11-25: #384 against `main` conflicts in `CHANGELOG.md` only; #388, #390 and #402 merge cleanly onto #384; this branch, #403 and #404 merge cleanly with `main`; #397 conflicts with its base in `component-reuse-matrix.md` and `victory-shader-integration.md`.

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

**The audit is not complete.** [audit-ledger.tsv](audit-ledger.tsv) lists every tracked file with the depth it received: of 691 files, 23 were read, 11 read in part, 119 summarised from their header comment, size and public item count only, and 538 not read (including tests, fixtures, most documentation and scripts).

Reuse in the hybrid path, judged from headers and interfaces, so provisional:

| Existing component | Possible hybrid use | Judgement |
| --- | --- | --- |
| `loader` PE parser and import parser | Executable identification | `crates/hybrid/src/pe.rs` **duplicates** part of it. The loader's parser accepts AMD64 only and its crate depends on the Metal and shader crates. Extracting a shared dependency-free parser would remove the duplication |
| `loader` static probe, `import-gen` classification and demangling | "What does this program need" diagnostics | Candidate; not wired up |
| `ref` (CPU depth, blend, indexed, sampling references) | Expected images computed independently of the workload author | Best candidate: would reduce self-verification in graphics workloads |
| `texture` (BC2 to BC7 decoders, footprints) | Expected values for compressed-texture workloads | Candidate; compressed formats are untested on the hybrid path |
| `loader` translator, services, scheduler, `shader`, `decoder`, `isa`, `metal` | None: Wine, Rosetta and CrossOver backends fill these roles | Keep as paused research |
| `ffi` and `app/` | A graphical front end for `betterxen hybrid` | Possible later; no need shown |
| `experiments/x1`, `fuzz`, `benchmarks` | The PE fuzz target could also cover the new parser | Not done |

Dependency graph: `betterxen-hybrid` depends on `libc`; `betterxen` depends on it and on the native crates. Marker search over Rust files found no `todo!`, `unimplemented!`, TODO or FIXME. Ignored tests (35): 27 need a Metal device, 5 an LLVM AMDGPU target, 3 are manual timing runs.

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

## Later work in this branch

`betterxen hybrid` with environment isolation, signal cleanup and `stop`; a Direct3D 12 workload; keyboard and lifecycle checks; [security review](security-audit-2026-11-25.md); [shader import decision proposal](shader-import-decision.md); [reference projects](reference-projects-2026-11-25.md) and [PortWatch inventory](portwatch-inventory-2026-11-25.md); [d3dmetal-native evaluation](d3dmetal-native-evaluation.md).

## Not done

- No second-person or second-machine reproduction.
- No DXIL or shader model 6, audio, networking, controller, mouse, resize, full screen, device loss, MSAA, mip, blending or compressed-texture test on the hybrid path.
- Shader and pipeline caches are CrossOver's; BetterXen manages none.
- No GPU-memory or thermal data.
- `README.md` and architecture documents were left to #397.

## Blockers

| Kind | Blocker |
| --- | --- |
| Decision | #394 / #399 ([proposal](shader-import-decision.md)) |
| Review | No independent reproduction of #384, #388, #402 or this branch |
| Coordination | The local worktree on #384's branch holds uncommitted edits and an unpushed merge of `main`, so its conflict was not resolved from here |
| Upstream behaviour | D3DMetal D3D12 swapchain references; `vkd3d` stall under the `wined3d` and `dxvk` settings |
| Documentation of interfaces | CrossOver launch and backend interfaces are undocumented (#350) |
| Scope | Nothing here runs console software; Wine provides Windows interfaces, not Xbox ones |

## Next steps

1. Decide #394.
2. Whoever owns the uncommitted #384 work finishes or discards it; then land #384, #388, #390, #402 and rebase this branch.
3. A second contributor runs the acceptance script (gate J).
4. Use `crates/ref` for expected images; add compressed-texture and shader model 6 coverage.
5. Report the D3DMetal swapchain behaviour to CodeWeavers with the minimal program (needs a maintainer to send it).
6. Finish the unread parts of the audit ledger.

## Continuation checkpoint

Branch `engineering/unified-hybrid-runtime`; local worktree `artifacts/unified-hybrid-runtime/worktree`; acceptance records under `artifacts/acceptance/` (ignored by Git). Claims posted on #355 and #365; notes on #390 (co-author trailer) and #397 (alignment). Resume from "Next steps"; the inventory rows marked listing only or not read are the unfinished audit.

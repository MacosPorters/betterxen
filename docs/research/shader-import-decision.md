# Decision proposal: the victory-shader import (#394, #395, #399)

For the maintainer. Prepared 2026-11-25 from the provenance record in #394 (`d8c7a39`), the independent review on [#399](https://github.com/MacosPorters/betterxen/issues/399) and the pull-request reviews. It changes no branch and closes nothing. The author of this proposal also worked on the original import, so it is not an independent review. Not legal advice.

## The concerns, by kind

| Concern | Kind | State of evidence | Can engineering fix it? |
| --- | --- | --- | --- |
| "Game-specific, asset-specific and decryption-related code is never adopted"; handling written for one title is present (17 of 36 files mention it; the review dispositions all 36 upstream paths as exclude) | Repository policy | Recorded with file and line | No. Deleting the comments does not change how the code was developed |
| "Never copy ... source from emulator projects built for leaked or unauthorized material" | Repository policy | The upstream project states that purpose | No |
| Layouts "as observed in shipped titles", behaviour tuned to the title's programs | Third-party rights (possible) | Plausible, **not established**; no title binaries, assets or keys were found in the crate | Only by independent re-derivation |
| `src/gfx10/opcodes.rs` carries Valve's MIT notice (Mesa-derived names) mixed with later edits | Third-party licence | Notice present; compatible with GPL-3.0-or-later | Yes: take names from AMD or re-audit Mesa directly |
| Up to 26 authors on one file; permission relayed covers two maintainers' own lines | Provenance | `git blame`; other lines rest on the repository licence | Not for the origin question |
| Whether apparently generic code originated independently | Provenance | **Unknown** | Only by a clean-room rewrite |
| Static samplers dropped; wave f64 sum used native doubles; folded probe; test runner could run a stale binary | Technical correctness | Found in review and fixed on the branches | Done, but moot if the code is excluded |
| Balanced-tree f64 sum order, wave64 behaviour, console equivalence | Technical correctness | **Unverified**; no console hardware | No |

Technical quality is not the blocker. The policy exclusions would remain if every test passed.

## What can be kept independently

| Item | Origin | Check made |
| --- | --- | --- |
| `workloads/d3d12-wave-probe/` (7 files) | BetterXen | Rated admissible by the #399 review, which also built and ran them on a second machine. Rebuilt here from `d8c7a39` with `-ld3d12 -ldxgi -ldxguid`, with no other file from the branch; no reference to the imported crate. The documented command is missing `-ldxguid` |
| Probe findings: D3DMetal reports no double-precision shader support; shaders using `double` write nothing; wave width 32 | Observations | Reproduced by the #399 review on an M2 Pro |
| `fixtures/rdna2/*` and their manifest expectations | BetterXen, already on `main` | Unaffected |
| Idea of #395's runner: execute only the binary named by the build that just succeeded, with a stale-binary regression test | BetterXen script | Generic; no need until another cross-built test exists |
| `betterxen.rs`, `betterxen_tests.rs` | BetterXen, but written against imported types | Review says exclude; keep the intent as notes |

## Is a clean-room replacement needed?

**Not for the active direction.** Authorized Windows programs under CrossOver bring ordinary DXBC or DXIL shaders, which CrossOver's backends already translate; the D3D11 and D3D12 workloads confirm that path for shader model 5.0. A console-code translator has no input in the hybrid path.

If one is ever wanted: derive encodings from AMD's public RDNA 2 documents and DXIL from Microsoft's public specification; have it written by someone who has not read the imported source (people and agents who vendored or reviewed it should not); and start from the existing native `crates/shader`, which already follows those rules and is paused.

## Options

| Option | Effect | Cost |
| --- | --- | --- |
| **A. Exclude the import, keep the probes (recommended)** | New small PR with the seven probe files and the observations; #394 and #395 stay as closed-unmerged history or draft reference, branches kept; #397 is rewritten without the import; the registry and matrix record the decision | Small. No policy change |
| B. Change the policy | Edit the engineering rules to permit this class of code, with legal review, then decide file by file | Weakens the rule that protects contributors and the project; the origin of much of the code stays unknown |
| C. Leave undecided | #394, #395 and #397 stay blocked | Documentation keeps drifting |

## What to record if you choose A

> The victory-shader snapshot at `22b3f8e45a59` is not admitted (provenance review #399). It is a documentation-level reference only. The standalone BetterXen D3D12 probes are retained. No branch is deleted.

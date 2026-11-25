# Upstream provenance: victory-shader

| | |
| --- | --- |
| Source | <https://github.com/skakwy/victory-mac>, directory `crates/victory-shader/` (Cargo.toml and `src/`) |
| Repository head when copied | `22b3f8e45a59` (2026-11-25); the directory's last change is commit `559357432b41` |
| Licence | GPL-3.0 (repository `LICENSE`); workspace manifest declares `GPL-3.0-or-later`, the same as BetterXen. No file in the crate carries a different licence |
| Third-party notice kept in place | `src/gfx10/opcodes.rs`: opcode names generated from Mesa's `aco_opcodes.py` (tag mesa-22.3.0), MIT, Copyright (c) 2018 Valve Corporation; the notice is in the file header |
| Authorization | skakwy and the VIctory-macOS maintainer authorize reuse of their contributions (relayed by the BetterXen maintainer). Other authors' lines are used under the repository's GPL-3.0 |
| Authors (commits touching this directory, from `git shortlog`) | Marvis (33), skakwy (12), fxceless (11), r4. (6), Duff (4), 9hvn9yt2vk-netizen (3), tedin7 (3), DexNom (2), Luiz Neves (2), Saitken91 (2), others; some commits are credited to an AI assistant |
| What it is | Xbox Series shader containers (DXBC with Xbox additions) and precompiled GFX10 (RDNA 2) machine code translated to PC DXIL (SM 6.6): GFX10 decoder and SP3 printer, control-flow structurizer, IR, register spilling, wave emulation, fp64 by integer operations, an own LLVM 3.7 bitcode writer and container signing |
| Upstream commit-level history | Not carried; see the upstream repository. This directory is a snapshot, so later upstream fixes must be merged by comparing with the pinned commit |

## Modifications made by BetterXen

Changes to upstream files (19 lines, nothing functional) and files BetterXen added (marked `BetterXen addition`, GPL-3.0-or-later): `src/emit/betterxen.rs` (a public `translate_native` for raw compute programs, same stages as `translate`) and `src/emit/betterxen_tests.rs` (BetterXen gfx1030 fixtures through the recompiler, and on the GPU under CrossOver). Two lines in `src/emit/mod.rs` declare them. Validation record: `docs/research/victory-shader-integration.md`.

### Edits to upstream files

- `Cargo.toml` rewritten: workspace-inherited dependency versions replaced by explicit ones (`thiserror` 2.0.21, `zlib-rs` 0.6.8, `windows` 0.62.2, `windows-sys` 0.61.2), same features and lint allowances as upstream's workspace.
- `src/lib.rs`: `#[rustfmt::skip]` on each `mod` so the vendored files stay byte-comparable with upstream.
- `src/emit/mod.rs`, `src/emit/f64.rs`, `src/emit/lower.rs`: `#[allow(...)]` attributes only, so the crate passes `clippy -D warnings` here. They silence imports and variables used only by Windows-only tests, an unused trait method, a test constant that looks like pi, and two dead duplicate match arms in `lower.rs` that exist upstream (an earlier arm already handles `v_cvt_f32_f64` and `v_permlanex16_b32`).

## Not carried

The upstream repository's other crates (`victory-gpu`, `victory-loader`, `victory-hle`, ...), which are tied to one console title and its runtime, and `mac/`, `tools/`. The shader inputs the upstream project was built for are not included; BetterXen validates this crate only with its own synthetic fixtures (see `docs/research/` for the validation record).

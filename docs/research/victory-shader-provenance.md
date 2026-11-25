# victory-shader: per-file provenance and licence review

**Status: NOT APPROVED. A maintainer decision is required before PR #394 can merge.** This is the review the engineering rules require for VIctory-family code. It records facts; it does not grant approval, and its author is the person who proposed the import, so it is not an independent review.

Upstream: <https://github.com/skakwy/victory-mac>, revision `22b3f8e45a59f7d11fef7718988c58d0b3f1d0f9`. Method: every tracked file of `crates/victory-shader/` at that revision; authors from `git shortlog`; licence from the repository `LICENSE` and a search of each file for other notices; title references by searching for shader names of the form `module:CS_Name` and the words title, titles, game, games. The search finds references, not every dependence: a file without one may still have been developed against the same title.

## Findings

1. **Licence:** GPL-3.0, compatible with BetterXen's GPL-3.0-or-later. One third-party notice (Mesa-derived opcode names, MIT, Valve) in `src/gfx10/opcodes.rs`, kept. Some comments cite Mesa behaviour and LLVM definitions. skakwy and the VIctory-macOS maintainer authorized reuse of their own contributions (relayed by the maintainer); other authors' lines rest on the repository licence.
2. **The crate was developed against one commercial title's shaders.** 17 of 36 files refer to "the title" or "the game"; comments describe that title's programs, layouts and structures, and four places cite its shaders by name (`wave64_tests.rs`, `wave.rs`, `own_bits.rs`). The upstream project's stated purpose is to run that title's Xbox Series build; this registry and PortWatch record that build as leaked.
3. **What is not in the crate:** a search found no shader binaries, shader dumps, game assets, keys or decryption code. The references are in comments, test descriptions and handling of shader constructs.
4. **Earlier statements that were wrong:** the integration record and PR descriptions said the crate had nothing game-specific. That claim came from searching only for the game's and publisher's names. It is withdrawn.

## How this meets the engineering rules (`AGENTS.md`)

| Rule | Assessment |
| --- | --- |
| "Projects whose stated purpose depends on leaked or unauthorized builds ... are discovery references at documentation level; adopt nothing from them without a per-file provenance and license review recorded in the registry." | This file is that record. Recording it does not by itself approve adoption. |
| "Game-specific, asset-specific and decryption-related code is never adopted." | Handling written for and documented against one title's shaders is present in at least some files. Whether that makes a file "game-specific code" is the decision to make; it is a general GFX10 recompiler in structure, with title-driven behaviour inside it. |
| "Provenance first: exclude anything derived from leaked proprietary material or unauthorized software." | Layouts "as observed in shipped titles" (`native.rs`) and behaviour tuned to the title's programs are plausibly derived from that material. |
| "Never copy, port, translate or adapt source from emulator projects built for leaked or unauthorized material." | Read plainly, this applies to this import. Author permission does not change it. |

**Reviewer's recommendation to the maintainer:** under the rules as written, do not merge the import. Either decline it (keep the crate as a documentation-level reference; BetterXen's own fixtures, probes and findings stand on their own), or change the rule explicitly in `AGENTS.md` with the legal question considered, and then decide file by file. This is not legal advice.

## Per-file record

| File (under `crates/victory-shader/`) | Lines | Upstream authors (commits) | Licence and third-party notes | Shader names of a title | Lines saying title/game |
| --- | --- | --- | --- | --- | --- |
| `Cargo.toml` | 40 | Marvis (2), fxceless (1) | GPL-3.0 (repository licence) | - | - |
| `src/bvh.rs` | 696 | fxceless (2) | GPL-3.0 (repository licence) | - | 5 |
| `src/dxbc.rs` | 181 | Marvis (2) | GPL-3.0 (repository licence) | - | - |
| `src/dxil.rs` | 211 | Marvis (1) | GPL-3.0 (repository licence); cites LLVM | - | - |
| `src/emit/bitcode.rs` | 1240 | Marvis (3), fxceless (2), Lyrnc (1) +2 more | GPL-3.0 (repository licence); cites LLVM | - | - |
| `src/emit/bitcode/census.rs` | 174 | fxceless (1) | GPL-3.0 (repository licence) | - | - |
| `src/emit/bitcode/opt.rs` | 2140 | armin (1), fxceless (1) | GPL-3.0 (repository licence) | - | 1 |
| `src/emit/container.rs` | 419 | Duff (1), Marvis (1), fxceless (1) +1 more | GPL-3.0 (repository licence) | - | - |
| `src/emit/diff_tests.rs` | 1287 | Marvis (3), fxceless (1) | GPL-3.0 (repository licence) | - | 5 |
| `src/emit/f64.rs` | 409 | skakwy (1) | GPL-3.0 (repository licence) | - | - |
| `src/emit/host.rs` | 1472 | fxceless (3), Marvis (1), skakwy (1) | GPL-3.0 (repository licence) | - | - |
| `src/emit/intersect.rs` | 557 | fxceless (1) | GPL-3.0 (repository licence) | - | 12 |
| `src/emit/intersect_tests.rs` | 690 | fxceless (1) | GPL-3.0 (repository licence) | - | 15 |
| `src/emit/lower.rs` | 12268 | Marvis (21), skakwy (9), fxceless (6) +23 more | GPL-3.0 (repository licence); comment cites Mesa behaviour; cites LLVM | - | 18 |
| `src/emit/mod.rs` | 1755 | Marvis (7), fxceless (6), skakwy (4) +8 more | GPL-3.0 (repository licence) | - | 16 |
| `src/emit/own_bits.rs` | 914 | fxceless (1) | GPL-3.0 (repository licence) | 1 | 3 |
| `src/emit/rect.rs` | 484 | fxceless (2) | GPL-3.0 (repository licence) | - | 2 |
| `src/emit/sampler.rs` | 125 | Marvis (1) | GPL-3.0 (repository licence) | - | 1 |
| `src/emit/shader.rs` | 908 | fxceless (4), Claude (1), Duff (1) +4 more | GPL-3.0 (repository licence) | - | 8 |
| `src/emit/threading.rs` | 218 | fxceless (1) | GPL-3.0 (repository licence) | - | - |
| `src/emit/validator.rs` | 221 | Marvis (1), lameloball2813-ui (1) | GPL-3.0 (repository licence) | - | - |
| `src/emit/valu_tests.rs` | 1624 | Marvis (6), r4. (5), fxceless (4) +6 more | GPL-3.0 (repository licence) | - | - |
| `src/emit/waterfall_tests.rs` | 435 | r4. (3), Marvis (1), Noname0202 (1) +1 more | GPL-3.0 (repository licence) | - | - |
| `src/emit/wave.rs` | 1228 | fxceless (3), skakwy (2), AscilCH (1) +1 more | GPL-3.0 (repository licence) | 1 | - |
| `src/emit/wave64_tests.rs` | 532 | fxceless (1), triippywhale (1) | GPL-3.0 (repository licence) | 2 | - |
| `src/gfx10/fields.rs` | 815 | Marvis (1), fxceless (1) | GPL-3.0 (repository licence) | - | - |
| `src/gfx10/mod.rs` | 496 | Marvis (3), tedin7 (1) | GPL-3.0 (repository licence) | - | - |
| `src/gfx10/opcodes.rs` | 1333 | Duff (1), Marvis (1) | GPL-3.0 (repository licence); Mesa-derived, MIT (Valve), notice in header; cites LLVM | - | - |
| `src/gfx10/sp3.rs` | 784 | Marvis (1) | GPL-3.0 (repository licence) | - | - |
| `src/io.rs` | 826 | Marvis (1), fxceless (1), local (1) | GPL-3.0 (repository licence) | - | - |
| `src/ir.rs` | 1791 | fxceless (4), Marvis (2), Yaekai (1) +1 more | GPL-3.0 (repository licence) | - | 1 |
| `src/lib.rs` | 38 | Marvis (3), fxceless (1) | GPL-3.0 (repository licence); cites LLVM | - | - |
| `src/native.rs` | 1103 | Marvis (4), fxceless (1), local (1) | GPL-3.0 (repository licence) | - | 1 |
| `src/resource.rs` | 3848 | Claude (3), fxceless (3), Marvis (2) +2 more | GPL-3.0 (repository licence); cites LLVM | - | 10 |
| `src/spill.rs` | 380 | fxceless (2) | GPL-3.0 (repository licence) | - | 1 |
| `src/structure.rs` | 1899 | Marvis (2), fxceless (2), DexNom (1) +2 more | GPL-3.0 (repository licence) | - | - |

BetterXen's additions (`src/emit/betterxen.rs`, `src/emit/betterxen_tests.rs`, `Cargo.toml`, `UPSTREAM.md`) are original, GPL-3.0-or-later. Functional edits to upstream files are listed in `crates/victory-shader/UPSTREAM.md`.

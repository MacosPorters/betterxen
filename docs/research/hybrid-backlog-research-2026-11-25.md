# Hybrid backlog research record (2026-11-25)

Evidence class D: public documentation and repository metadata, plus search summaries; see [primary-source-verification-2026-11-25.md](primary-source-verification-2026-11-25.md) for the re-check. Nothing was installed, built or run. Not legal advice. Candidate registry rows are listed here; merge them into [upstream-registry.md](upstream-registry.md) once that file's pending commit has landed.

## Findings that created issues

| Finding | Source | Consequence |
| --- | --- | --- |
| CrossOver 27 reportedly requires Apple silicon and macOS Sonoma+, drops 32-bit bottles, and an ARM64 build without Rosetta 2 is planned; Apple keeps Rosetta fully through macOS 27, a subset after | CodeWeavers blog 2026-06-11 and Apple WWDC 2025 (search summaries; primary pages not retrieved) | [#374](https://github.com/MacosPorters/betterxen/issues/374), comment on #349 |
| Bottle backend is reportedly set by `CX_GRAPHICS_BACKEND` (d3dmetal, dxmt, dxvk, wined3d) in `cxbottle.conf`; no official documentation found | Third-party projects | Unverified; #350 must confirm before [#366](https://github.com/MacosPorters/betterxen/issues/366) relies on it |
| Cross-building anything for Wine needs mingw-w64 or llvm-mingw | DXMT `docs/DEVELOPMENT.md` | [#364](https://github.com/MacosPorters/betterxen/issues/364) |
| Wine and vkd3d-proton ship Direct3D conformance tests that can be built as standalone PE programs | `wine-mirror/wine` `dlls/d3d11/tests`, `vkd3d-proton` `tests/` | [#369](https://github.com/MacosPorters/betterxen/issues/369) |
| DXMT offers Metal-coordinated frame pacing and MetalFX upscaling; DXMT is D3D11/10 only | DXMT `docs/CUSTOMIZATION.md`, README | [#370](https://github.com/MacosPorters/betterxen/issues/370) |
| DXMT compiles shaders at runtime with an LLVM-based compiler | DXMT `src/airconv` | [#371](https://github.com/MacosPorters/betterxen/issues/371) (hypothesis to test) |
| Metal HUD frame data is logged with `MTL_HUD_ENABLED`; logging variable name differs between sources; frame data may be redacted in the unified log | Apple tech talk, community tools | #145, [#372](https://github.com/MacosPorters/betterxen/issues/372) |
| D3DMetal license reportedly limits use to evaluation and restricts redistribution | Secondary sources | #354 must read Apple's License.pdf; never bundle |

## Registry candidates

| Project | Pin / state observed | License as reported | Classification | Issue |
| --- | --- | --- | --- | --- |
| llvm-mingw / mingw-w64 | not pinned yet | to read per release | External dependency | #364 |
| Wine conformance tests | `wine-mirror/wine`, pushed 2026-11-24 | LGPL-2.1-or-later | Run as external programs, not copied | #369 |
| vkd3d-proton tests | pushed 2026-11-25 | LGPL-2.1 | Requires feasibility validation (D3D12 path unconfirmed) | #369 |
| DXMT | v0.80 (2026-04-23) | LGPL-2.1-or-later | Requires feasibility validation; external | #370, #371 |
| Apple Game Porting Toolkit / D3DMetal | not retrieved | Apple license, reportedly restrictive | Requires licensing clarification; user-supplied only | #354 |
| Whisky | archived 2025-05-11 | GPL-3.0 | Technical reference only | #366 |
| Gcenx macOS Wine builds, Sikarugir | Sikarugir reports no license | unknown | Technical reference only; no reuse | none |
| MetalHUDLogger and similar community tools | not reviewed | unknown | Technical reference only | #145 |
| FEX, QEMU, LLVM, MoltenVK, DXVK, Xenia | see upstream registry | see registry | FEX: not applicable on macOS. Others: no change from existing registry | none |
| VIctory family, PortWatch VI | see upstream registry | GPL-3.0 (repos) | Rejected for reuse: the tracker describes efforts built around a leaked Xbox Series build of a commercial game; PortWatch is used for discovery only | none |

Not investigated in depth: Xenia internals and QEMU, because neither runs Windows x86-64 programs on macOS ARM64 in a way relevant to the CrossOver architecture; this is a judgement, not a finding.

## Issues created

Existing hybrid issues #344 to #355, #363, #145, #204, #257, #303 were left to the other planning session. New: #364 to #374, and phase trackers #375 (H3), #376 (H4), #377 (H5). Dependencies are recorded as GitHub blocked-by and sub-issue relationships.

## Conditional work deliberately not filed

D3D12 workloads (after #349 confirms a path), input-injection tests, a compatibility matrix generator (after #368), upstream contributions to DXMT or Wine (after #369 and #370 produce concrete findings), and release engineering (H6).

# Component reuse matrix

Companion to the [upstream registry](upstream-registry.md); same evidence limits (class D, nothing run, not legal advice). "External" means installed or supplied by the user and invoked, never bundled or copied. GPL-3.0-or-later is BetterXen's license.

| Need | Candidate | Decision | Reason and condition |
| --- | --- | --- | --- |
| Windows runtime APIs | CrossOver (Wine-based) | **Adopt as external dependency** (strategic decision, ADR 0005) | Proprietary product; do not bundle or redistribute. Capabilities and interfaces unverified (#348, #350). |
| Windows runtime APIs | Upstream Wine | Reference; source may be consulted | LGPL 2.1. Copying source would need per-file review and a recorded pin. Not needed for the current plan. |
| x86-64 on ARM64 | Rosetta 2 | **Permitted where Apple supports it**; external | Apple component; support lifetime unknown (#349). BetterXen's own code stays ARM64. |
| x86-64 on ARM64 | FEX | **Not applicable now** | README targets ARM64 Linux. Re-check only if a macOS host path appears. |
| Direct3D 12 to Metal | D3DMetal | **Evaluate as external**, subject to license | Not in CodeWeavers' FOSS list; terms and availability to verify (#349, #354). Not redistributable until proven. |
| Direct3D 11/10 to Metal | DXMT | **Evaluate** as external binary or build | LGPL 2.1-or-later; usable as a separate process or library under LGPL terms; linking or copying needs legal review (#354). |
| Direct3D to Vulkan | DXVK | **Evaluate** only together with MoltenVK | zlib/libpng is permissive and GPL-compatible. Value on macOS unknown. |
| Direct3D 12 to Vulkan | VKD3D-Proton | **Evaluate**; blocked on a Vulkan 1.3 provider | Depends on MoltenVK feature coverage, unknown. |
| Vulkan on Metal | MoltenVK | **Evaluate** | Apache-2.0 (compatible with GPLv3). Subset of Vulkan; coverage vs consumers unknown. |
| Shader compiler / runtime | VIctory, VIctory-macOS | **Rejected for reuse** | Built for a specific commercial game's Xbox Series build whose provenance is not established as authorized. Ideas only. |
| Project discovery | PortWatch VI | Reference only | Editorial tracker; not engineering evidence; nothing imported. |
| x86-64 decode and ARM64 translation, runtime, gfx1030 compiler, Metal backend | BetterXen native implementation | **Preserved, paused** | Reuse only where hybrid needs it and it shows a demonstrated advantage; not forced into the architecture. |
| Test and diagnostics methodology | BetterXen existing audits, benchmark harness, claims registry | **Reuse** | Existing evidence classes, claims audit and benchmark gating apply to hybrid work (Phase 1 onward). |
| Configuration, diagnostics, benchmarking around CrossOver | BetterXen | **Build** (Phase 2, provisional) | Only after Phase 1 shows what CrossOver does not already provide (#351). |
| Host receipt, frame statistics, cross-toolchain probe | `crates/hybrid` | **Build** (original Rust, public system interfaces) | ossover and Whisky are Swift references; no code adapted. See [audit](upstream-integration-audit.md). |
| Metal HUD log parsing | ossover `MetalHUDLog` | **Reference only** | Log format is documented only by third-party observation; implement after a real trace is captured (#145). |
| Bottle and bundle internals (`cxbottle.conf`, SharedSupport layout) | ossover and others | **Do not depend** | Undocumented, derived from reverse engineering; #350 decides. |
| Game-specific XCL layers | MacBox, ScarlettMac | **Rejected** | GPL-2.0-only and game-specific. |

| Memory-pressure guard for workloads | skakwy/victory-mac `mac/run-victory.sh` | **Adapted** into `crates/hybrid/src/memory.rs` | Contributor-authored, GPL-3.0, permission granted; reimplemented in Rust. |
| Windows cross-linker wrapper | skakwy/victory-mac `tools/macos/gnullvm-cc.sh` | **Not used** | Failed with Homebrew `llvm@17`; llvm-mingw covers the need. |
| Windows cross toolchain | llvm-mingw 20261117 | **External dependency**, not vendored | Apache-2.0 with LLVM exceptions; checksum recorded in [hybrid-toolchain.md](hybrid-toolchain.md). |
| Zero-copy Metal swizzle, D3DMetal state fixes | skakwy/victory-mac `mac/unixlib`, `PROGRESS.md` | **Rejected** | Act on D3DMetal internals; Apple licence forbids reverse engineering. |

Open items: license text for every selected component must be read from the exact release used; DXMT, DXVK and Wine contributor and file-level notices are not reviewed.

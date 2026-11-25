# Component reuse matrix

## Split integration update

The original discovery assessment below is retained as a dated research snapshot. The split shader integration subsequently included only `crates/victory-shader` from `skakwy/victory-mac` at `22b3f8e45a59f7d11fef7718988c58d0b3f1d0f9`, plus BetterXen fixture additions. Its [provenance record](../../crates/victory-shader/UPSTREAM.md), [license record](../licenses.md) and [local integration results](victory-shader-integration.md) are the component-specific records, not an approval of the wider VIctory family. This documentation audit does not independently certify that review. Other upstream runtime, loader, game-specific and decryption components remain excluded.

| Component | Selected scope | Recorded license and limits |
| --- | --- | --- |
| `victory-shader` | Vendored GFX10-to-DXIL snapshot and original compute-fixture bridge | Upstream GPL record and retained MIT opcode-table notice; exact files, modifications and attribution in `UPSTREAM.md`. Independent provenance/acceptance review is not claimed here. |

## Original discovery assessment (2026-11-25)

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
| Shader compiler / runtime | VIctory, VIctory-macOS | **Initial rejection; later shader-only record above** | Built for a specific commercial game's Xbox Series build whose provenance is not established as authorized. Ideas only. |
| Project discovery | PortWatch VI | Reference only | Editorial tracker; not engineering evidence; nothing imported. |
| x86-64 decode and ARM64 translation, runtime, gfx1030 compiler, Metal backend | BetterXen native implementation | **Preserved, paused** | Reuse only where hybrid needs it and it shows a demonstrated advantage; not forced into the architecture. |
| Test and diagnostics methodology | BetterXen existing audits, benchmark harness, claims registry | **Reuse** | Existing evidence classes, claims audit and benchmark gating apply to hybrid work (Phase 1 onward). |
| Configuration, diagnostics, benchmarking around CrossOver | BetterXen | **Build** (Phase 2, provisional) | Only after Phase 1 shows what CrossOver does not already provide (#351). |

Open items: license text for every selected component must be read from the exact release used; DXMT, DXVK and Wine contributor and file-level notices are not reviewed.

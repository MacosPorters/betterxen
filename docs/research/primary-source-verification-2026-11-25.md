# Primary-source verification (2026-11-25)

Re-checks claims first taken from search summaries in [hybrid-backlog-research-2026-11-25.md](hybrid-backlog-research-2026-11-25.md). Evidence class D (public documentation read; nothing installed or run). Not legal advice. #363 still has to re-derive these independently.

| Claim | Result | Primary source read |
| --- | --- | --- |
| CrossOver offers the backends Auto, D3DMetal, DXMT, DXVK and Wine | **Confirmed.** Auto uses CodeWeavers' per-game database and wined3d if none is given; D3DMetal covers DirectX 11 and 12; DXMT is Metal-based Direct3D 11; DXVK is Vulkan-based Direct3D 10 and 11; DLSS via MetalFX applies to D3DMetal and DXMT only | support.codeweavers.com "Advanced Settings in CrossOver Mac 26" (modified 2026-02-10). That page is for CrossOver 26 and does not name MoltenVK |
| `CX_GRAPHICS_BACKEND` in `cxbottle.conf` is a supported interface | **Not confirmed.** Neither CodeWeavers page read mentions it or `cxbottle`, `cxstart` or `cxrun`. It appears only in third-party projects. Treat as undocumented; #350 decides | Same page and support.codeweavers.com "Advanced CrossOver Mac Configuration" |
| Documented configuration surfaces | The second page documents `defaults` keys in `~/Library/Preferences/com.codeweavers.CrossOver.plist` (BottleDir, ManagedBottleDirs) and bottle hooks run with `CX_ROOT`, `CX_BOTTLE` and `WINEPREFIX` set | Same |
| CrossOver needs Rosetta 2; ARM64 build without it planned; CrossOver 27 requirements | **Unverified.** The CodeWeavers blog returned HTTP 403 to automated fetches, and the documentation pages above do not cover it. Read it in a browser for #348 | CodeWeavers blog (blocked) |
| Rosetta after macOS 27 | **Sources conflict.** Apple developer news of 2026-09-01 says macOS 27 is the final release supporting Rosetta, with continued support for older unmaintained games that rely on Intel-based frameworks. News of 2026-09-09 says macOS 26 is the final release supporting Intel Macs and Rosetta, and 27 is Apple silicon only. Record both until Apple clarifies | developer.apple.com/news (ids w5ngl9k2, k1mtkt1k) |
| D3DMetal license | **Not located.** The `apple/game-porting-toolkit` repository is Apache-2.0 and never mentions D3DMetal; D3DMetal ships in the separate Game Porting Toolkit download whose license was not read. Secondary sources describe evaluation-only use and restricted redistribution. Stays user-supplied and unbundled | github.com/apple/game-porting-toolkit, LICENSE |
| DXMT license and scope | **Confirmed.** LGPL-2.1-or-later (copyright 2023-2026 Feifan He for CodeWeavers). README: Direct3D 11 and 10 on Metal using Wine. Wiki: macOS 14 or later, Apple silicon supported, Intel GPUs experimental and planned for removal once macOS 28 is released, Wine 8+ required, DXMT needs symbols from `winemac.so`. Cross builds need LLVM 15 and a mingw cross-compiler. Latest release v0.80 (2026-04-23) | github.com/3Shain/dxmt (LICENSE, README, wiki, docs/DEVELOPMENT.md) |
| Wine conformance tests exist as standalone Windows programs | **Confirmed in tree.** `dlls/d3d11/tests/Makefile.in` defines a `d3d11.dll` test target from `d3d11.c`. The Wine conformance-test documentation page loaded (HTTP 200) but its requirements were not read; build and run requirements remain unverified | wine-mirror/wine |

## Dependencies observed

DXMT lists Meson 1.3+, LLVM 15, Xcode 16+ with the Metal toolchain and a Windows cross-compiler as build requirements. The Rust workspace requires `rustc` 1.100 (`rust-version` in `Cargo.toml`).

## Rust toolchain

The default `stable` on the reference machine was rustc 1.94.1, which cannot build the workspace. rustc 1.100.0 is the current official stable (channel manifest dated 2026-11-12, release 2026-11-09) and was already installed as toolchain `1.100.0-aarch64-apple-darwin`, so nothing was installed. Run `rustup run 1.100.0 cargo ...` or update `stable`. With it, `cargo test --locked -p xenora-loader --test generated_runtime_integration --test runtime_x15` passed (3 and 35 tests, 1 ignored). These native tests are not needed for hybrid development; the full workspace suite was not run.

## Not verified

CrossOver's CPU-translation internals and ARM64 plans, CrossOver 27 requirements, D3DMetal license text, `cxbottle`/`cxstart` command-line syntax, Wine conformance-test requirements, and whether D3D12 is usable under any backend on the installed version. These belong to #348, #350 and #354.

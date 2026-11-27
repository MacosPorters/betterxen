# Parallel Track Architecture & Contributor Handoff Guide

BetterXen is organized so **5 to 8+ contributors** can develop the CrossOver-based Apple Silicon compatibility stack concurrently via Git and GitHub Issues without merge conflicts.

## Repository Layout

| Directory | Track | Language / Build | Purpose |
| --- | --- | --- | --- |
| `abi/` | Shared (frozen) | C headers + `.def` | Cross-DLL contracts (`bx_log.h`, `bx_config.h`, `bx_xmem_internal.h`, `bx_shader_bridge.h`) and canonical DLL export lists |
| `launcher/` | Track A | Rust (`aarch64-apple-darwin`) | Native ARM64 macOS CLI (`bx-cli`), PE32+ static probe (`bx-probe`), JSONL trace aggregator (`bx-telemetry`) |
| `shims/runtime_veh/` | Track A | C++20 (`mingw-w64`) | `bx_runtime_veh.dll`: `clwb` VEH handler, `adcx`/`adox` tripwire, Lockhart/Anaconda config, JSONL trace logger |
| `shims/win32_bridge/` | Track A | C++20 (`mingw-w64`) | PSM AppNotify stubs + `ext-ms-win-rtcore-ntuser-*` → `user32.dll` forwarders; not loaded for static imports under CrossOver ([ADR 0007](../adr/0007-api-set-shims-under-crossover.md)) |
| `shims/xmem/` | Track B | C++20 (`mingw-w64`) | `xmem.dll`: physical/virtual/scatter memory allocator, `__XMemAllocationHooks__`, 16 GB Lockhart cap |
| `shims/xcrdapi/` | Track B | C++20 (`mingw-w64`) | `XCRDAPI.dll`: DirectStorage async I/O factory, queues, and decompression |
| `shims/d3d12_xs/` | Track C1 | C++20 (`mingw-w64`) | `d3d12_xs.dll`: Xbox D3D12 COM proxy over Wine `d3d12.dll` (D3DMetal 3.0), `PresentX`, placed resources |
| `shims/xg_xs/` | Track C2 | C++20 (`mingw-w64`) | `xg_xs.dll`: RDNA 2 texture tile addressing & swizzle layout computer |
| `shims/ffx_bvh_hooks/` | Track C3 | C++20 (`mingw-w64`) | Optional PC FidelityFX/MetalFX upscaler shim & BVH4 rehydration → DXR live rebuild bridge |
| `shader-transpiler/` | Track D | Rust (`aarch64` / C ABI) | `DXBC`/`DXIL`/`ACCC`/`FSPX` shader classifier, RDNA 2 → DXIL/MSL transpiler (M2 gate), shader cache, `d3dmetal-native` harness |
| `shims/xgameruntime/` | Track E1 | C++20 (`mingw-w64`) | `xgameruntime.dll`: offline GDK runtime (`XTaskQueue`, `XAsync`, `XUser`, `XStore`, `XSystem`, `XPersistentLocalStorage`) |
| `shims/gameinput/`, `shims/audio/` | Track E2 | C++20 (`mingw-w64`) | `GameInput.dll` ordinal 1 adapter + `AcpHal.dll`, `xapu.dll`, `chad.dll` audio/DSP shims |
| `tests/conformance/` | Track F | C++20 (`mingw-w64`) | Clean-room synthetic PE32+ test executables verifying each shim under CrossOver/Wine |

*(The earlier native-first research workspace was retired from `main` per [ADR 0006](../adr/0006-retire-in-tree-native-workspace.md) and remains permanently accessible at Git tag `native-first-final`.)*

## Milestones (M0 → M6)

1. **M0 — Repo, ABI & Cross-Build:** One-command ARM64 + x86-64 PE32+ build, export verification, and `test_veh_clwb.exe` passing under CrossOver + Rosetta 2 (`ROSETTA_ADVERTISE_AVX=1`).
2. **M1 — Boot to `D3D12XboxCreateDevice`:** Zero missing imports; all 17 shim DLLs (16 mingw-w64 C++ shims plus the Rust `bx_shader_bridge.dll`) load cleanly; auto-logging COM proxy captures startup call traces.
3. **M2 — First Frame & Shader Format Gate:** Present real frames through D3DMetal 3.0 (`PresentX` → `IDXGISwapChain3::Present`) and classify runtime shaders (`DXIL` fast path vs. `ACCC`/`FSPX` RDNA 2 transpilation).
4. **M3 — Full Shaders, Streaming & Texture Correctness:** Async DirectStorage decompression, accurate RDNA 2 `xg_xs` tiling/deswizzling, full shader pipeline.
5. **M4 — 3D World, Ray Tracing & Input:** BVH4 rehydration → live D3DMetal 3.0 DXR build, FidelityFX/MetalFX upscaling shim, full `GameInput` controller/KB/M mapping.
6. **M5 — Real Audio, Saves & Stability:** Software audio/DSP decoding behind `AcpHal`/`xapu`/`chad`, `PersistentLocalStorage` saves, multi-hour soak testing.
7. **M6 — Shader Cache, 16 GB Optimization & Polish:** Persistent disk PSO/shader cache, Lockhart 16 GB unified memory tuning, Metal System Trace optimization, Swift launcher UI.

## Zero-Conflict Git & Issue Protocol

- **ABI-first:** Any cross-track header or `.def` change lands first in a small `[ABI]` PR touching only `abi/`.
- **Branch naming:** `track-<a..f>/<issue>-<slug>` off `main`, squash-merged daily.
- **Exclusive file scope:** Every issue lists the exact directories it may edit (using `.github/ISSUE_TEMPLATE/track-handoff.yml`). Never edit another track's folder in a feature PR.
- **Local verification:**
  ```sh
  cargo fmt --all -- --check
  cargo clippy --workspace --all-targets -- -D warnings
  cargo test --workspace
  scripts/build-shims.sh
  ```

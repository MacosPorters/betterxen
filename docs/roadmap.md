# BetterXen Roadmap

**This is the single authoritative active roadmap.** Strategic decisions are recorded in [ADR 0005](adr/0005-crossover-hybrid-direction.md) (CrossOver-based hybrid direction) and [ADR 0006](adr/0006-retire-in-tree-native-workspace.md) (retiring the paused native-first workspace to Git tag `native-first-final`). Track ownership and parallel workflow rules live in [docs/architecture/parallel-tracks.md](architecture/parallel-tracks.md); Wave 1 contributor issues live in [docs/issues/wave-1-backlog.md](issues/wave-1-backlog.md).

## Direction

BetterXen builds a modular **Xbox Series (`Scarlett`) compatibility layer for Apple Silicon macOS** around externally installed **CrossOver 26.3+**, **Rosetta 2** (`ROSETTA_ADVERTISE_AVX=1`), and **D3DMetal 3.0**:

```text
BetterXen (in-tree):
  launcher/ (ARM64 CLI, probe, telemetry) + abi/ (frozen contracts)
  shims/ (16 cross-compiled x86-64 PE32+ Xbox Game OS DLLs)
  shader-transpiler/ (DXBC / DXIL / ACCC / FSPX classifier & RDNA 2 pipeline)
        |
External Foundation (user-installed, never bundled):
  CrossOver 26.3 / Wine (Win32, CRT, XAudio2, WinHTTP) | Rosetta 2 | D3DMetal 3.0
        |
Apple Silicon: ARM64, Metal, unified memory
```

This is an active engineering stack, **not a verified commercial Xbox game compatibility layer**. Progress is measured strictly by reproducible synthetic conformance tests and recorded verification evidence — never by commit counts or unverified compatibility claims.

Priorities, in order: practical compatibility, correctness, stability, observability, Apple Silicon performance, and long-term maintainability.

---

## Completed Research Inputs (Carried Forward)

- **Licensing & redistribution boundaries:** [docs/research/crossover-licensing.md](research/crossover-licensing.md) and [docs/licenses.md](licenses.md) (#354). CrossOver, D3DMetal, and Rosetta 2 stay strictly external and unbundled.
- **Primary-source verification & upstream registry:** [docs/research/primary-source-verification-2026-11-25.md](research/primary-source-verification-2026-11-25.md), [docs/research/upstream-registry.md](research/upstream-registry.md), and [docs/research/component-reuse-matrix.md](research/component-reuse-matrix.md).
- **Hybrid measurement protocol:** [docs/testing/hybrid-workloads.md](testing/hybrid-workloads.md) (#353).

---

## Milestones (M0 → M6)

| Milestone | Objective | Exit Gate | Status |
| --- | --- | --- | --- |
| **M0 — Repo, ABI & Cross-Build** | Establish 6-track directory layout (`abi/`, `launcher/`, `shims/`, `shader-transpiler/`, `tests/conformance/`), frozen `.def`/header contracts, env-isolated `bx-cli`, strict PE Export Directory verifier, and live CrossOver + Rosetta 2 smoke tests (`test_veh_clwb.exe`, `test_shim_exports.exe`) | ARM64 Rust checks green; 16/16 PE32+ DLLs pass strict export & zero-MinGW-dep verification; both synthetic EXEs pass live under CrossOver + Rosetta 2 | **In validation** (merged, #406) |
| **M1 — Startup Shims to `D3D12XboxCreateDevice`** | Complete Wave 1 parallel issues across Tracks A–F: full `clwb` + crash register VEH, `xmem` core + physical/scatter page engine + Lockhart cap, `XCRDAPI` async file I/O MVP, `d3d12_xs` auto-logging COM proxy over Wine `d3d12.dll`, `xg_xs` linear-compatible layout MVP, `dxbc-accc` C ABI hook + `d3dmetal-native` evaluation, offline `xgameruntime`, non-blocking `AcpHal`/`xapu`/`chad` stubs, `GameInput` `@1`, and combined synthetic integration boot fixture | All 13 Wave 1 conformance binaries meet their expected result under CrossOver + Rosetta 2; the combined `test_integration_boot.exe` initializes every subsystem cleanly ([M1 exit gate](#m1-exit-gate)) | **In development** |
| **M2 — First Rendered Frame & Shader Format Gate** | Implement `d3d12_xs` swapchain presentation (`PresentX` → `IDXGISwapChain3::Present`), placed resources over `xmem`, and `dxbc-accc` shader inspection to establish whether target runtime shader corpora use **DXIL (Branch D1)** or **console-compiled `ACCC`/`FSPX` RDNA 2 ISA (Branch D2)** | Synthetic D3D12 `PresentX` workload renders and verifies pixel readback through D3DMetal 3.0; M2 Gate ADR selects Branch D1 or Branch D2 | Planned |
| **M3 — Full Shaders, Streaming & Texture Correctness** | DirectStorage decompression, accurate RDNA 2 (`gfx1030`) `xg_xs` tile-addressing & deswizzling, full `d3d12_xs` command-list/barrier/GPUVA fidelity, and Branch D1 DXIL normalization or Branch D2 `ACCC`/`FSPX` → DXIL/MSL transpiler | Synthetic textured/swizzled, compute, multi-queue, and compressed DirectStorage conformance suites pass | Planned |
| **M4 — 3D Rendering, DXR/BVH4 & Upscaling** | Bridge `XboxBvhBuilder` / `ID3D12StateObject` ray tracing to D3DMetal 3.0 DXR (intercepting incompatible AMD BVH4 rehydration → live rebuild), provide PC FidelityFX / MetalFX upscaling shim, and wire full `GameInput` controller/KB/M input | Synthetic DXR + upscaled 3D scene and scripted `GameInput` conformance tests pass | Planned |
| **M5 — Software Audio/DSP, Saves & Stability** | Replace `AcpHal`/`xapu`/`chad` stubs with software audio/DSP decoding + XAudio2_9/CoreAudio output, wire `PersistentLocalStorage`, and resolve concurrency/memory/GPU hang bugs under soak testing | Synthetic audio playback, save round-trip, and multi-hour soak benchmarks pass with bounded memory | Planned |
| **M6 — Shader Cache, 16 GB Optimization & Polish** | Persistent disk PSO + translated-shader cache, Lockhart 16 GB unified memory pressure tuning, Metal System Trace optimization, and optional Swift launcher UI | Reproducible frame-pacing and memory-pressure receipts under [docs/testing/hybrid-workloads.md](testing/hybrid-workloads.md) | Planned |

---

## M1 Exit Gate

Build with `scripts/build-shims.sh`, then run each binary under `build-shims/bin/` with
`bx-cli run --shims build-shims/bin --exe <binary>` (with `--bottle` or `--prefix`).

| Binary | Expected result |
| --- | --- |
| `test_audio_input.exe` | exit 0 |
| `test_d3d12_xs.exe` | exit 0 |
| `test_integration_boot.exe` | exit 0 with 0 failed and 0 pending stages |
| `test_shim_exports.exe` | exit 0 |
| `test_veh_clwb.exe` | exit 0 |
| `test_veh_unhandled.exe` | non-zero exit (it crashes on purpose) and one `unhandled_exception` record in the log |
| `test_win32_bridge.exe` | exit 0 |
| `test_xcrdapi.exe` | exit 0 |
| `test_xg_xs.exe` | exit 0 |
| `test_xgameruntime.exe` | exit 0, synthetic license active |
| `unentitled/test_xgameruntime.exe` | exit 0, synthetic license inactive |
| `test_xmem.exe` | exit 0 |
| `test_xmem_physical.exe` | exit 0 |

A boot stage reported as `pending` did not initialize, so it does not satisfy the gate even though
the program exits 0 without `--strict`. The gate is not met while any stage is pending (see #437
and #446) or until someone other than the implementer has reproduced the results.

---

## How Progress Is Reported

Milestone status is one of: `Planned`, `In development`, `In validation`, `Verified complete`, or `Blocked`. A milestone reaches `Verified complete` only after independent verification of its exit gate per [AGENTS.md](../AGENTS.md).

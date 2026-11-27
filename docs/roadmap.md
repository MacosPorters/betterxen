# BetterXen Roadmap

**This is the single authoritative active roadmap.** Strategic decisions are recorded in [ADR 0005](adr/0005-crossover-hybrid-direction.md) (CrossOver-based hybrid direction) and [ADR 0006](adr/0006-retire-in-tree-native-workspace.md) (retiring the paused native-first workspace to Git tag `native-first-final`). Track ownership and parallel workflow rules live in [docs/architecture/parallel-tracks.md](architecture/parallel-tracks.md); Wave 1 contributor issues live in [docs/issues/wave-1-backlog.md](issues/wave-1-backlog.md).

## Direction

BetterXen builds a modular **Xbox Series (`Scarlett`) compatibility layer for Apple Silicon macOS** around externally installed **CrossOver 26.3+**, **Rosetta 2** (`ROSETTA_ADVERTISE_AVX=1`), and **D3DMetal 3.0**:

```text
BetterXen (in-tree):
  launcher/ (ARM64 CLI, probe, telemetry) + abi/ (frozen contracts)
  shims/ (16 cross-compiled x86-64 PE32+ Xbox Game OS DLLs
          + the Rust-built bx_shader_bridge.dll)
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
- **Buffer ownership on CrossOver 26.3.0.39832 and D3DMetal 3.0 (#473).** On the tested M4 (24 GiB) and M5 (16 GiB), the two existing-memory heap imports returned `E_NOTIMPL`. A D3D12-owned persistent buffer mapping supported fence-separated CPU/GPU access and ordered virtual aliasing (BASE-2, BASE-4, A-8). The actual CPU address could miss 64 KiB alignment (A-6). Timing bounded the cost of a serialized copy within the measured window. Physical page identity, hidden copies, workload memory savings and speedups remain open. [Report 001](https://github.com/MacosPorters/betterxen/blob/0aaa81ab81309d64432d97d3005284c8b13cd285/docs/research/publications/001-unified-memory/technical-findings.md) · [Evidence register](https://github.com/MacosPorters/betterxen/blob/f8810a48af384d13c594b9fba954200bfdb944f1/docs/research/d3dmetal-zero-copy/allocation-profiling/consolidation-draft/evidence-register.md) · [Review corrections](https://github.com/MacosPorters/betterxen/discussions/493)

---

## Milestones (M0 → M6)

| Milestone | Objective | Exit Gate | Status |
| --- | --- | --- | --- |
| **M0 — Repo, ABI & Cross-Build** | Establish 6-track directory layout (`abi/`, `launcher/`, `shims/`, `shader-transpiler/`, `tests/conformance/`), frozen `.def`/header contracts, env-isolated `bx-cli`, strict PE Export Directory verifier, and live CrossOver + Rosetta 2 smoke tests (`test_veh_clwb.exe`, `test_shim_exports.exe`) | ARM64 Rust checks green; all 17 PE32+ DLLs (16 mingw-w64 C++ shims + the Rust `bx_shader_bridge.dll`) pass strict export & zero-MinGW-dep verification; both synthetic EXEs pass live under CrossOver + Rosetta 2 | **Verified complete (`#406`)** |
| **M1 — Startup Shims to `D3D12XboxCreateDevice`** | Complete Wave 1 parallel issues across Tracks A–F: full `clwb` + crash register VEH, `xmem` core + physical/scatter page engine + Lockhart cap, `XCRDAPI` async file I/O MVP, `d3d12_xs` auto-logging COM proxy over Wine `d3d12.dll`, `xg_xs` linear-compatible layout MVP, `dxbc-accc` C ABI hook + `d3dmetal-native` evaluation, offline `xgameruntime` + dynamic loader exports, non-blocking `AcpHal`/`xapu`/`chad` stubs, `GameInput` `@1`, supervised `bx-cli run`, ADR 0007, and combined synthetic integration boot fixture | All Wave 1 conformance binaries pass under CrossOver 26.3 + Rosetta 2; combined `test_integration_boot.exe` passes (19 pass, 2 ADR-0007 pending, 0 fail) | **Verified complete (Milestone #17, 16/16)** |
| **M2 — First Rendered Frame & Shader Format Gate** | Clear dynamic GDK startup (`system32` staging + public GDK COM `QueryApiImpl` dispatch), implement `d3d12_xs` swapchain presentation (`PresentX` → `IDXGISwapChain3::Present`), integrate the shipped xmem-configured D3D12 placed heap (#464) into the first-frame fixture (#469), bridge `dxbc-accc` into Windows guests (`#445`), measure D3DMetal GPU limits (`#453`), and establish whether target runtime shader corpora use **DXIL (Branch D1)** or **console-compiled `ACCC`/`FSPX` RDNA 2 ISA (Branch D2)**. Measured translator constraints: [D3DMetal GPU limits](research/d3dmetal-gpu-limits.md#measured-constraints) | Synthetic D3D12 `PresentX` workload renders and verifies pixel readback through D3DMetal 3.0; M2 Gate ADR selects Branch D1 or Branch D2 | **In development (Milestone #18)** |
| **M3 — Full Shaders, Streaming & Texture Correctness** | DirectStorage decompression, accurate RDNA 2 (`gfx1030`) `xg_xs` tile-addressing & deswizzling, full `d3d12_xs` command-list/barrier/GPUVA fidelity, and Branch D1 DXIL normalization or Branch D2 `ACCC`/`FSPX` → DXIL/MSL transpiler, within the [measured D3DMetal GPU limits](research/d3dmetal-gpu-limits.md#measured-constraints) (`WaveSize(64)` and SM 6.6 64-bit atomics not run; bounded lock emulation) | Synthetic textured/swizzled, compute, multi-queue, and compressed DirectStorage conformance suites pass | Planned |
| **M4 — 3D Rendering, DXR/BVH4 & Upscaling** | Bridge `XboxBvhBuilder` / `ID3D12StateObject` ray tracing to D3DMetal 3.0 DXR (intercepting incompatible AMD BVH4 rehydration → live rebuild), provide PC FidelityFX / MetalFX upscaling shim, and wire full `GameInput` controller/KB/M input | Synthetic DXR + upscaled 3D scene and scripted `GameInput` conformance tests pass | Planned |
| **M5 — Software Audio/DSP, Saves & Stability** | Replace `AcpHal`/`xapu`/`chad` stubs with software audio/DSP decoding + XAudio2_9/CoreAudio output, wire `PersistentLocalStorage`, and resolve concurrency/memory/GPU hang bugs under soak testing | Synthetic audio playback, save round-trip, and multi-hour soak benchmarks pass with bounded memory | Planned |
| **M6 — Shader Cache, 16 GB Optimization & Polish** | Persistent disk PSO + translated-shader cache, Lockhart 16 GB unified memory pressure tuning, Metal System Trace optimization, and optional Swift launcher UI | Reproducible frame-pacing and memory-pressure receipts under [docs/testing/hybrid-workloads.md](testing/hybrid-workloads.md) | Planned |

### Next product checkpoint: advance past startup

On a pinned launcher, shim set and backend, reproduce the startup stop in an authorized workload and record the exit status, stopping thread, module and offset. Correlate the requested DirectStorage factory IID and HRESULT (#483) and the virtual-memory configuration outcome (#464) with that thread. Distinguish a null configuration from an unrecognized one. Either warning can precede successful work on another thread. Use a synthetic regression to isolate the first failing contract, then repeat the same launch and demonstrate progress past the original stop. The following product checkpoint is a frame presented by the workload itself.

### M2 first-frame acceptance and parallel memory work

- **First frame (#465, #469).** For the synthetic placement requirement, accept an xmem region that configures the separate D3D12-owned WRITE_BACK heap shipped in #486. On a combined tree, #494 copied backbuffer pixels before the flip, counted one host present, then compared all 256 RGBA8 pixels through the D3D12 map after waiting. The original xmem page retained its sentinel. Require the fixture to build and run on the integrated presentation path, including #471/#534 fence-error recovery. Independently verify the trace, pixels and intentional pixel-mismatch control. The shader exit gate still requires a recorded D1/D2 decision in [ADR 0008](adr/0008-m2-shader-format-gate.md). [Fixture and receipts](https://github.com/MacosPorters/betterxen/pull/494)
- **Title budget (#550, #552).** Keep xmem and D3D12 allocation ownership. Give xmem atomic admission authority for their shared title budget through an ABI-first contract. Under a 512 MiB cap, 256 MiB of xmem followed by a 384 MiB heap was refused, while the reverse order admitted 640 MiB. Test both orders, concurrent admission and rollback on host failure. Count each backing allocation once across aliases. Check versioned placement, wrapper/device lifetime and ordinary D3D12 forwarding (#554, #572). Track B/C can address this defect alongside startup diagnosis. [Merged-base reproduction](https://github.com/MacosPorters/betterxen/pull/486#issuecomment-7209751182)
- **M3 resource handling.** Keep explicit paths for fixed addresses, existing contents, pre-device allocations, sections/scatter and opaque textures. Trial a mapped buffer only when a concrete consumer identifies the resource before pointer publication and its CPU alignment and lifetime can be met. For work BetterXen submits, verify completion on every participating queue before reuse. Keep #483 scoped to DirectStorage interfaces. [Eligibility analysis](https://github.com/MacosPorters/betterxen/blob/7e0b515131150fec6b5117c9498b6c0faf0ca382/docs/architecture/proposals/001-unified-memory/appendix/eligibility.md)
- **M6 optimization.** Compare the existing path and candidate allocator or bounded pool using matched workloads on a 16 GB host. Include cold setup, transfers, fragmentation, pending retirements, post-release process footprint and whole-frame time. Require a repeatable workload benefit before choosing a default.

---

## How Progress Is Reported

Milestone status is one of: `Planned`, `In development`, `In validation`, `Verified complete`, or `Blocked`. A milestone reaches `Verified complete` only after independent verification of its exit gate per [AGENTS.md](../AGENTS.md).

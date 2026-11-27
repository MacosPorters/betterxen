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
- **Unified-memory research (supporting, not a gate):** [Research Report #001](https://github.com/MacosPorters/betterxen/blob/0aaa81ab81309d64432d97d3005284c8b13cd285/docs/research/publications/001-unified-memory/README.md) (#473, discussion #493), with its [evidence register](https://github.com/MacosPorters/betterxen/blob/f8810a48af384d13c594b9fba954200bfdb944f1/docs/research/d3dmetal-zero-copy/allocation-profiling/consolidation-draft/evidence-register.md) and [evidence audit](https://github.com/MacosPorters/betterxen/blob/0aaa81ab81309d64432d97d3005284c8b13cd285/docs/research/publications/001-unified-memory/evidence-audit.md). The report lives on a research branch and is not merged into `main`.
  - **Tested configuration:** CrossOver 26.3.0.39832 and D3DMetal 3.0, on one Apple M4 (24 GiB) and one M5 (16 GiB), with original synthetic programs only.
  - **Review status:** 13 of 29 register entries are accepted by the other workstream within stated scopes. Most of those are same-source reruns, and the reviews on both sides were AI-assisted. The other 16 are unknown (11), failed gates (3) or unsupported (2).
  - **Not established:** physical page identity, absence of hidden copies, memory savings, any performance gain, and behaviour on other chips or versions.
  - **How it is used here:** see [Memory ownership and budget](#memory-ownership-and-budget-research-report-001).

---

## Milestones (M0 → M6)

| Milestone | Objective | Exit Gate | Status |
| --- | --- | --- | --- |
| **M0 — Repo, ABI & Cross-Build** | Establish 6-track directory layout (`abi/`, `launcher/`, `shims/`, `shader-transpiler/`, `tests/conformance/`), frozen `.def`/header contracts, env-isolated `bx-cli`, strict PE Export Directory verifier, and live CrossOver + Rosetta 2 smoke tests (`test_veh_clwb.exe`, `test_shim_exports.exe`) | ARM64 Rust checks green; 16/16 PE32+ DLLs pass strict export & zero-MinGW-dep verification; both synthetic EXEs pass live under CrossOver + Rosetta 2 | **Verified complete (`#406`)** |
| **M1 — Startup Shims to `D3D12XboxCreateDevice`** | Complete Wave 1 parallel issues across Tracks A–F: full `clwb` + crash register VEH, `xmem` core + physical/scatter page engine + Lockhart cap, `XCRDAPI` async file I/O MVP, `d3d12_xs` auto-logging COM proxy over Wine `d3d12.dll`, `xg_xs` linear-compatible layout MVP, `dxbc-accc` C ABI hook + `d3dmetal-native` evaluation, offline `xgameruntime` + dynamic loader exports, non-blocking `AcpHal`/`xapu`/`chad` stubs, `GameInput` `@1`, supervised `bx-cli run`, ADR 0007, and combined synthetic integration boot fixture | All Wave 1 conformance binaries pass under CrossOver 26.3 + Rosetta 2; combined `test_integration_boot.exe` passes (19 pass, 2 ADR-0007 pending, 0 fail) | **Verified complete (Milestone #17, 16/16)** |
| **M2 — First Rendered Frame & Shader Format Gate** | Clear dynamic GDK startup (`system32` staging + public GDK COM `QueryApiImpl` dispatch), implement `d3d12_xs` swapchain presentation (`PresentX` → `IDXGISwapChain3::Present`), placed resources over `xmem`, bridge `dxbc-accc` into Windows guests (`#445`), measure D3DMetal GPU limits (`#453`), and establish whether target runtime shader corpora use **DXIL (Branch D1)** or **console-compiled `ACCC`/`FSPX` RDNA 2 ISA (Branch D2)**. Measured translator constraints: [D3DMetal GPU limits](research/d3dmetal-gpu-limits.md#measured-constraints) | Synthetic D3D12 `PresentX` workload renders and verifies pixel readback through D3DMetal 3.0; M2 Gate ADR selects Branch D1 or Branch D2 | **In development (Milestone #18)** |
| **M3 — Full Shaders, Streaming & Texture Correctness** | DirectStorage decompression, accurate RDNA 2 (`gfx1030`) `xg_xs` tile-addressing & deswizzling, full `d3d12_xs` command-list/barrier/GPUVA fidelity, and Branch D1 DXIL normalization or Branch D2 `ACCC`/`FSPX` → DXIL/MSL transpiler, within the [measured D3DMetal GPU limits](research/d3dmetal-gpu-limits.md#measured-constraints) (`WaveSize(64)` and SM 6.6 64-bit atomics not run; bounded lock emulation) | Synthetic textured/swizzled, compute, multi-queue, and compressed DirectStorage conformance suites pass | Planned |
| **M4 — 3D Rendering, DXR/BVH4 & Upscaling** | Bridge `XboxBvhBuilder` / `ID3D12StateObject` ray tracing to D3DMetal 3.0 DXR (intercepting incompatible AMD BVH4 rehydration → live rebuild), provide PC FidelityFX / MetalFX upscaling shim, and wire full `GameInput` controller/KB/M input | Synthetic DXR + upscaled 3D scene and scripted `GameInput` conformance tests pass | Planned |
| **M5 — Software Audio/DSP, Saves & Stability** | Replace `AcpHal`/`xapu`/`chad` stubs with software audio/DSP decoding + XAudio2_9/CoreAudio output, wire `PersistentLocalStorage`, and resolve concurrency/memory/GPU hang bugs under soak testing | Synthetic audio playback, save round-trip, and multi-hour soak benchmarks pass with bounded memory | Planned |
| **M6 — Shader Cache, 16 GB Optimization & Polish** | Persistent disk PSO + translated-shader cache, Lockhart 16 GB unified memory pressure tuning, Metal System Trace optimization, and optional Swift launcher UI | Reproducible frame-pacing and memory-pressure receipts under [docs/testing/hybrid-workloads.md](testing/hybrid-workloads.md) | Planned |

### Memory ownership and budget (Research Report #001)

Checked against `main` at `b8724bf` on 2026-11-27. This section changes no exit gate. Register ids such as A-6 refer to the [evidence register](https://github.com/MacosPorters/betterxen/blob/f8810a48af384d13c594b9fba954200bfdb944f1/docs/research/d3dmetal-zero-copy/allocation-profiling/consolidation-draft/evidence-register.md). Items marked *proposed* have no issue yet and nothing is assigned.

**Shipped (M2).** #464 landed with PR #486:
- **Backing:** a configured `D3D12ConfigureVirtualMemoryX` region is backed by one D3D12-owned `CUSTOM` / `WRITE_BACK` / `L0` heap whose persistent map is published as `cpu_pointer`. It is not `xmem`-owned memory: both existing-memory import calls returned `E_NOTIMPL` on the tested stack (BASE-2).
- **Budget:** each heap created through `CreateHeap` is charged against the title cap until its wrapper's final release.
- **Placement:** `CreatePlacedResource` validates alignment and bounds on the backing heap, and resources it creates retain the heap wrapper.

**What the research supports.**
- **Candidate for eligible buffers:** D3D12-owned, persistently mapped buffers. Fenced CPU and GPU reads agreed (BASE-4), and aliased buffers inherited data (A-8).
- **Resource classes that keep explicit paths:** memory with existing contents, fixed or pre-device addresses, sections and scatter mappings (A-5, BASE-2), and textures, whose layout is opaque (A-9 to A-12; sampling, depth and stencil coverage is incomplete).
- **Four different claims, not to be merged:** fenced visibility (BASE-4), virtual aliasing (A-8), process footprint (A-1: released 256 MiB and 1 GiB allocations stayed charged, one complete M4 session, not shown to be a leak), and timing bounds (B-1: no copy as costly as an explicit full copy inside the timed window; a write effect is not supported).
- **CPU alignment:** mapped CPU pointers were not reliably 64 KiB aligned (A-6).
- **Placement validation:** the host accepted invalid placements (A-7), so the shim must validate before forwarding.

**Remaining correctness work, in order.** Items 2 to 4 can proceed in parallel with item 1, except where they edit `shims/d3d12_xs/vmem.cpp`.

| # | Work | Milestone | Where it stands |
| --- | --- | --- | --- |
| 1 | Combined, atomic budget admission. On `main`, `xmem` cannot see graphics heap charges and the two sides use separate locks. Source-derived, not run: a graphics heap of 6 units then a CPU allocation of 3 admits 9 against a cap of 8, and a stale-snapshot schedule admits 12 | M2 | ABI-first: #550 (PR #551), then #552 (PR #553), both open and unreviewed. A concurrent-admission and rollback regression is *proposed* |
| 2 | Wrapper, device and queue lifetimes and failure handling. `CreatePlacedResource1/2` forward without retaining the heap wrapper | M2 for the wrapper path, M3 for fenced retirement | #572 (unclaimed after a deferral). Fenced retirement of the backing map across region changes, queues and device loss is *proposed* for M3 (A-13 covers one disposable helper only) |
| 3 | Placement and CPU pointer alignment | M2 | Ordinary heaps: #554 (PR #555). `CreatePlacedResource1/2`: #572. Checking the published `cpu_pointer` against the alignment a caller needs is *proposed* |
| 4 | Research receipt finalization, redaction of host process listings in raw receipts, and the missing repetitions | M2 research | #473 (B-8 failed gate; A-1, A-12 and B-1 to B-4 incomplete). *Proposed* as scoped research tasks |

**Milestone placement.**
- **M2:** the work above is correctness for the shipped placed-resource path. #473 remains supporting research and does not block #465 or #469.
- **M3:** resource eligibility rules, fenced retirement, and per-class buffer and texture handling belong with full command-list, barrier and GPUVA fidelity. #483 (DirectStorage interfaces) stays separate: it is confined to `shims/xcrdapi/` and adds no dependency on this work. Storage choices need matched workload evidence first.
- **M6:** allocator selection (AMD's D3D12 Memory Allocator was only qualified for bounded `CUSTOM` buffer pools, A-4), pooling (A-2 unknown: suballocation calls were cheaper, the whole cycle barely changed), retention (A-1) and 16 GB memory-pressure tuning. No memory or performance benefit is promised until a matched workload on a 16 GB host shows one.

**Pending maintainer decisions.**
1. **M2 gate wording (#469, PR #494):** #469 asks for an "`xmem`-backed placed resource" and for pixels read back after `PresentX`. PR #494 reads pre-flip pixels from the D3D12 backing map and keeps a separate `xmem` sentinel. Either accept that as meeting the gate or reword #469.
2. **Budget authority and ABI (#550):** whether `xmem` owns the single ledger through a charge hook, and whether PR #551 may carry the `shims/xmem/` implementation in an `[ABI]` pull request.
3. **What the budget counts:** committed and reserved resources are forwarded uncharged today, and so are the D3D12 buffers `XCRDAPI.dll` creates.
4. **Eligibility:** whether D3D12-owned mapped buffers become the preferred path for eligible allocations, and which component decides.
5. **CPU alignment contract:** what a caller may assume about `cpu_pointer`.
6. **Allocator:** AMD's allocator or a small in-tree one, and the evidence that decides.

---

## How Progress Is Reported

Milestone status is one of: `Planned`, `In development`, `In validation`, `Verified complete`, or `Blocked`. A milestone reaches `Verified complete` only after independent verification of its exit gate per [AGENTS.md](../AGENTS.md).

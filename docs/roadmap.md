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
- **Unified-memory research (Research Report #001, 2026-11-27):** [discussion #493](https://github.com/MacosPorters/betterxen/discussions/493), publication branch `0aaa81ab`, integrated register/roadmap at `f8810a48`. Tested **CrossOver 26.3.0.39832 / Wine 11.0-8726-g2e2f5fca349 / D3DMetal 3.0** on an **M4 (24 GiB, macOS 27.0)** and an **M5 (16 GiB, macOS 27.2.1)**. This is **supporting research, not an M2 exit condition**, and #473 stays open. It establishes no memory saving and no performance gain; see the evidence-class caveats in §Research status below.

---

## Research status: Unified Memory (Research Report #001)

Supporting research for M2/M3/M6. **No milestone exit gate depends on it**, and nothing below authorizes a production, ABI or issue-ownership change.

**What the research supports** (register IDs are in the [evidence register](https://github.com/MacosPorters/betterxen/blob/f8810a48af384d13c594b9fba954200bfdb944f1/docs/research/d3dmetal-zero-copy/allocation-profiling/consolidation-draft/evidence-register.md)):

| Claim | Register | Status |
| --- | --- | --- |
| Fence-separated CPU/GPU copies through a D3D12-owned persistently mapped buffer are visible to both sides | BASE-4, A-8 | Accepted by the other workstream, within stated scope |
| Memory the CPU already owned cannot be imported as a D3D12 heap (both import calls returned `E_NOTIMPL`) | BASE-2 | Unsupported, version-scoped |
| Mapped CPU pointers are **not** reliably 64 KiB aligned; placement alignment ≠ CPU pointer alignment | A-6 | Accepted, within stated scope |
| An invalid resource placement could still return success (`A-7`: the check **failed**) | A-7 | **Failed** |
| Suballocation is far cheaper than heap creation; the whole allocate/free cycle barely changed | A-2, A-3 | Unknown / accepted |
| Plain CPU `MEM_WRITE_WATCH` reports exact 4 KiB pages; GPU-write tracking is unresolved | BASE-6, B-4, B-5 | Accepted / Unsupported |
| Physical page identity ("true zero-copy") is **not** established | X-1 | Unknown |

**What remains unknown.** Physical page identity, any absence of hidden copying, production memory savings, any FPS change, other chips and other CrossOver versions. Large released allocations stayed charged to the process (A-1) — **why** is not shown, and it is not shown to be a leak.

**Corrections carried from review** ([discussion #493](https://github.com/MacosPorters/betterxen/discussions/493), `devil8blade`). These bound what this roadmap may claim:

1. **Copy/compute sample counts.** The published 5,040 copy and 576 compute comparisons include 864 and 96 poison controls; the real sample counts are **4,176** and **480**.
2. **No write effect is supported.** The "46–279 µs" figure holds only for the WB 64 MiB case; across all 36 write/no-write pairs the range is **−238 to +228 µs**, the no-write baseline is bimodal between identical runs, and one 4-byte write produces a similar step. The data supports **no size-proportional copy in the timed window** — not a write effect.
3. **"Independently accepted" is narrower than it sounds.** Most of the 13 scoped acceptances are same-source reruns by the other workstream with AI agents deciding for both; A-13 rests on a *simulated* fence failure. Read them as **accepted by the other workstream within stated scope**.
4. **Provenance.** ~49 raw `receipt.json` files embed full process listings from both contributors' Macs in `load_before`; no usernames, emails or tokens were found, but they must be redacted before the publication merges. The unmatched Workstream B output digests are counted differently across the artifacts — five in the integration record, **eight** `xctrace-output.txt` digests in the audit (taken while Instruments was still saving), and **nine** in the methodology — so treat the exact count as unresolved; one stopped-run stderr hash also does not match, and five historical source references have no retained bytes.
5. **Stale pins in the publication.** It cites #486 at `c13f9c7` and `main` at `f07a24c2`. #486 **has merged** (merge commit `b8724bf`), so the budget counterexample below was re-derived against current `main` rather than carried forward.

**The one concrete defect the research identified, re-derived on `main`.** The combined title budget is **one-way**. `shims/d3d12_xs/vmem.cpp::try_charge` reads `bx_xmem_get_internal_stats` and subtracts CPU usage, but `shims/xmem/xmem.cpp::can_commit` sums only `heap_bytes + virtual_bytes + physical_bytes` and **never** sees `charged_private_bytes`. So:

- **xmem-first** (CPU 3 GiB, then graphics 6 GiB) is **rejected** correctly.
- **graphics-first** (graphics 6 GiB, then CPU 3 GiB) is **admitted**: `can_commit` sees `used = 0`, so 3 ≤ 8 − 0 passes. **9 GiB admitted against an 8 GiB cap.**

`abi/include/bx_xmem_internal.h` exposes **no** charge hook, so a coherent combined budget **requires an `abi/` addition first** (review point 6), not just a shim edit. This is the highest-priority correctness item the research surfaced, and it is **not** fixed on `origin/main` at `b8724bf` (as of 2026-11-27).

Two precisions on the numbers. The published figures are **abstract budget units**, not GiB: `budget-review.json` records `sequential {cap 8, graphics 6, cpu 3, combined 9, exceeded true}`, `reverse_order_control_rejects true`, and a `snapshot_interleaving {4,4,4,4 → 12 against cap 8}`. The figures above are that same arithmetic restated in GiB for readability. Its own `scope` field says **"source-derived logical counterexample only; no host memory allocation, shim invocation or production modification"** — so this is a **source-derived** defect, never observed in a run. It still needs backend regression tests, which is why the follow-up below names them.

## Milestones (M0 → M6)

| Milestone | Objective | Exit Gate | Status |
| --- | --- | --- | --- |
| **M0 — Repo, ABI & Cross-Build** | Establish 6-track directory layout (`abi/`, `launcher/`, `shims/`, `shader-transpiler/`, `tests/conformance/`), frozen `.def`/header contracts, env-isolated `bx-cli`, strict PE Export Directory verifier, and live CrossOver + Rosetta 2 smoke tests (`test_veh_clwb.exe`, `test_shim_exports.exe`) | ARM64 Rust checks green; all 17 PE32+ DLLs (16 mingw-w64 C++ shims + the Rust `bx_shader_bridge.dll`) pass strict export & zero-MinGW-dep verification; both synthetic EXEs pass live under CrossOver + Rosetta 2 | **Verified complete (`#406`)** |
| **M1 — Startup Shims to `D3D12XboxCreateDevice`** | Complete Wave 1 parallel issues across Tracks A–F: full `clwb` + crash register VEH, `xmem` core + physical/scatter page engine + Lockhart cap, `XCRDAPI` async file I/O MVP, `d3d12_xs` auto-logging COM proxy over Wine `d3d12.dll`, `xg_xs` linear-compatible layout MVP, `dxbc-accc` C ABI hook + `d3dmetal-native` evaluation, offline `xgameruntime` + dynamic loader exports, non-blocking `AcpHal`/`xapu`/`chad` stubs, `GameInput` `@1`, supervised `bx-cli run`, ADR 0007, and combined synthetic integration boot fixture | All Wave 1 conformance binaries pass under CrossOver 26.3 + Rosetta 2; combined `test_integration_boot.exe` passes (19 pass, 2 ADR-0007 pending, 0 fail) | **Verified complete (Milestone #17, 16/16)** |
| **M2 — First Rendered Frame & Shader Format Gate** | Clear dynamic GDK startup (`system32` staging + public GDK COM `QueryApiImpl` dispatch), implement `d3d12_xs` swapchain presentation (`PresentX` → `IDXGISwapChain3::Present`), placed resources over `xmem`, bridge `dxbc-accc` into Windows guests (`#445`), measure D3DMetal GPU limits (`#453`), and establish whether target runtime shader corpora use **DXIL (Branch D1)** or **console-compiled `ACCC`/`FSPX` RDNA 2 ISA (Branch D2)**. Measured translator constraints: [D3DMetal GPU limits](research/d3dmetal-gpu-limits.md#measured-constraints) | Synthetic D3D12 `PresentX` workload renders and verifies pixel readback through D3DMetal 3.0; M2 Gate ADR selects Branch D1 or Branch D2 | **In development (Milestone #18)** |
| **M3 — Full Shaders, Streaming & Texture Correctness** | DirectStorage decompression, accurate RDNA 2 (`gfx1030`) `xg_xs` tile-addressing & deswizzling, full `d3d12_xs` command-list/barrier/GPUVA fidelity, and Branch D1 DXIL normalization or Branch D2 `ACCC`/`FSPX` → DXIL/MSL transpiler, within the [measured D3DMetal GPU limits](research/d3dmetal-gpu-limits.md#measured-constraints) (`WaveSize(64)` and SM 6.6 64-bit atomics not run; bounded lock emulation) | Synthetic textured/swizzled, compute, multi-queue, and compressed DirectStorage conformance suites pass | Planned |
| **M4 — 3D Rendering, DXR/BVH4 & Upscaling** | Bridge `XboxBvhBuilder` / `ID3D12StateObject` ray tracing to D3DMetal 3.0 DXR (intercepting incompatible AMD BVH4 rehydration → live rebuild), provide PC FidelityFX / MetalFX upscaling shim, and wire full `GameInput` controller/KB/M input | Synthetic DXR + upscaled 3D scene and scripted `GameInput` conformance tests pass | Planned |
| **M5 — Software Audio/DSP, Saves & Stability** | Replace `AcpHal`/`xapu`/`chad` stubs with software audio/DSP decoding + XAudio2_9/CoreAudio output, wire `PersistentLocalStorage`, and resolve concurrency/memory/GPU hang bugs under soak testing | Synthetic audio playback, save round-trip, and multi-hour soak benchmarks pass with bounded memory | Planned |
| **M6 — Shader Cache, 16 GB Optimization & Polish** | Persistent disk PSO + translated-shader cache, Lockhart 16 GB unified memory pressure tuning, Metal System Trace optimization, and optional Swift launcher UI | Reproducible frame-pacing and memory-pressure receipts under [docs/testing/hybrid-workloads.md](testing/hybrid-workloads.md) | Planned |


**M2 gate notes (2026-11-27).** The first-frame gate and the shader-format gate are **unchanged**; no maintainer has approved a different gate.

- **First-frame gate:** PR #494 provides the end-to-end fixture but its target is **skipped** on a base that lacks #471's `PresentX` headers, so a passing base CI run does **not** exercise the gate. #471 is open with `CHANGES_REQUESTED`. #494 also reproduced two heap-budget failures in #486's placed-resource fixture on the combined tree.
- **#469 is still open by design.** #494's description asks a maintainer to decide whether its **separate D3D12 readback mapping** satisfies #469's `xmem` requirement, or whether the readback must come through the `xmem`-backed placed resource. That is a **contract decision, not a code gap** — surface it rather than assume either reading.
- **Shader-format gate:** ADR 0008 remains **Proposed** (Branch D1). Unchanged by this research.

**M6 allocator note (2026-11-27).** Allocator selection, pooling and 16 GB memory-pressure work are candidate M6 items, **not yet scheduled**. The research supports only that suballocation was cheaper than heap creation and that total free bytes can hide fragmentation (A-2, A-3); it does **not** support a memory or performance benefit. Any M6 claim needs **matched-workload production measurement** first.

**Coordination note (review point 6).** Two items the research's own roadmap did not state: the `` #486 `` lifetime defects were **heap-wrapper unwrapping** on `CreatePlacedResource1`/`CreatePlacedResource2` (both call `bx_unwrap(a0)` in `shims/d3d12_xs/proxy_generated.cpp`) and **device refcount lifetimes**, not fence lifetime alone; and any work editing `shims/d3d12_xs/CMakeLists.txt` overlaps **PR #471**, which is open with `CHANGES_REQUESTED`.

**M3 boundary.** Eligibility, placement and persistent-map changes are **M3 streaming/graphics candidates** and do not gate M2. #483 (DirectStorage) is **independent** — do not add it as a dependency of this work.

---

## How Progress Is Reported

Milestone status is one of: `Planned`, `In development`, `In validation`, `Verified complete`, or `Blocked`. A milestone reaches `Verified complete` only after independent verification of its exit gate per [AGENTS.md](../AGENTS.md).

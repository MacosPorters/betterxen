# ADR 0008: M2 Shader Format Gate — Branch D1 (DXIL normalization) for M3

Status: **Proposed** (2026-11-26, issue #466). The choice for M3 is the maintainers' to accept. The evidence below is from local Apple Silicon runs by the implementer and has not been independently reproduced.

## Context

The roadmap's M2 gate asks which shader path M3 builds:
- **Branch D1, DXIL normalization:** shaders reach D3DMetal as DXIL, and BetterXen rewrites only what D3DMetal cannot run.
- **Branch D2, `ACCC`/`FSPX` translation:** console-compiled RDNA 2 code in `ACCC` chunks is translated to DXIL or MSL.

The gate needs a classifier that guest pipeline creation actually reaches, plus measurements of what D3DMetal does with each form.

## What M2 built

- **Bridge (#445):** `bx_shader_bridge.dll` is the x86-64 Windows build of `dxbc-accc`, so guest code can call the inspector.
- **Gate (#466):** `d3d12_xs.dll`'s `CreateComputePipelineState` and `CreateGraphicsPipelineState` pass every stage through `bx_shader_inspect`. One `shader_gate` JSONL record per stage gives:
  - the container kind, stage, shader model and chunk sizes;
  - the `SFI0` feature flags (`D3D_SHADER_FEATURE_*` values from DirectX-Headers `d3dcommon.h`);
  - the `PSV0` wave lane range.
- **Decisions:**
  - **DXIL and dual containers:** go to the host as the bridge's prepared copy (`bx_shader_prepare_for_d3dmetal`).
  - **ACCC-only containers:** are refused with `E_NOTIMPL` before the host device is called.
  - **Legacy DXBC and unclassifiable input:** pass unchanged.
  - **A missing bridge:** is logged once (`shader_bridge_missing`). Shaders then pass unclassified, except that a container with an `ACCC` chunk and no `DXIL` chunk is still refused with `E_NOTIMPL`, found by a bounds-checked chunk scan, because D3DMetal crashes on it. The bridge is loaded only by full path from `d3d12_xs.dll`'s own directory.
- **Known D3DMetal gaps:** a stage that needs a wave wider than 32 lanes, or 64-bit atomics, also gets a `shader_d3dmetal_gap` record.

`tests/conformance/test_shader_gate` exercises all of this.

## Evidence

| Observation | Source | Host and class |
| --- | --- | --- |
| A real DXC 1.9.2609 SM 6.6 compute shader (`fill_cs66`) creates a PSO through `d3d12_xs`, runs, and all 256 output words match the CPU reference. | `test_shader_gate` | Apple M5 Pro, macOS 27.2.1, CrossOver 26.3 (`26.3.0.39832`) bottle with `CX_GRAPHICS_BACKEND=d3dmetal`; class A, this host only |
| The same shader, run outside Wine through `d3dmetal-native` against CrossOver's D3DMetal 3.0, matches 256/256. | [d3dmetal-harness](../../shader-transpiler/crates/d3dmetal-harness/README.md) (#417) | Same host; class A |
| An ACCC-only container is refused with `E_NOTIMPL` for compute and graphics, no host object is created, and the device stays usable. | `test_shader_gate` | Same bottle; class A |
| In a bare CrossOver 26.3 prefix (vkd3d), SM 6.6 DXIL PSOs fail with `E_FAIL`. vkd3d reports `Cannot load DXIL conversion library`. | `test_shader_gate`, same host | Class A; DXIL needs the D3DMetal backend under CrossOver |
| `[WaveSize(64)]` kernels and kernels with SM 6.6 64-bit atomics are created but never run on D3DMetal. | [D3DMetal GPU limits](../research/d3dmetal-gpu-limits.md#measured-constraints) (#453) | Apple M3 Pro, Wine 11.0 source build, D3DMetal 4.0b3; class A for that host |
| DXC records `[WaveSize(64)]` as `PSV0` lanes 64/64, and 64-bit atomics on typed resources as `SFI0` `0x400000`. 64-bit atomics on raw buffers set only `INT64_OPS` (`0x8000`), the same as other 64-bit integer use. | The #453 blobs, read by the gate and by a script | Class A |
| Public sources do not establish a console-native shader container layout. BetterXen's `FSPX` is a clean-room synthetic layout, not a description of a console compiler's format. | `CLAIM-XBOX-ISA-ABI` (class U) in [claims.toml](../claims.toml); `shader-transpiler/crates/dxbc-accc/src/fspx.rs` | Source review |
| A serialized console pipeline path exists for Series X\|S, with no public version-independent format. | `CLAIM-XBOX-ACO-SAMPLE` (class D) | Source review |

No measurement of a title's shader corpus exists or is planned. BetterXen runs only original synthetic programs, so the DXIL vs. ACCC mix of real titles is unknown. Wherever the gate runs, its `shader_gate` records count it.

## Decision

Branch **D1** is proposed for M3:

1. **Pass DXIL through the gate to D3DMetal.** This is the only path with measured, end-to-end execution evidence.
2. **Spend M3 shader work on the two measured gaps,** within the [D3DMetal GPU limits](../research/d3dmetal-gpu-limits.md#measured-constraints):
   - lower `WaveSize(64)` programs to 32-lane code;
   - lower SM 6.6 64-bit atomics to 32-bit operations or bounded lock emulation.

   The gate's `shader_d3dmetal_gap` records show which pipelines need this.
3. **Keep refusing ACCC-only containers** with `E_NOTIMPL` and a structured log.

Branch **D2** is deferred. It needs a documented, lawfully available description of console-compiled containers and their RDNA 2 code, and none exists (`CLAIM-XBOX-ISA-ABI`). A translator built against the synthetic `FSPX` layout would prove nothing about real inputs.

Revisit this ADR in any of these cases:
- an authorized public source for that format appears;
- gate telemetry from a lawful workload shows ACCC-only containers in practice;
- D3DMetal starts running the gap cases.

## Consequences

- **Pipeline creation:** `CreateComputePipelineState` and `CreateGraphicsPipelineState` now depend on `bx_shader_bridge.dll` for classification. Without it, they still work, refuse ACCC-only containers, and say so in the log.
- **Packaging:** `scripts/build-shims.sh` builds the bridge, links it with `abi/defs/bx_shader_bridge.def`, and stages it in `build-shims/bin` beside `d3d12_xs.dll` (#484).
- **Not covered by the gate yet:**
  - **Stream PSOs:** `ID3D12Device2::CreatePipelineState` stream descriptions are still forwarded unparsed.
  - **Raw-buffer 64-bit atomics:** these cannot be flagged from `SFI0`; the DXIL bitcode would have to be read.
  - **Telemetry summary:** `bx-cli trace` now summarizes the gate's records (#496): per-decision, per-kind and per-stage counts for `shader_gate`, the `shader_d3dmetal_gap` list with its wave-width and 64-bit-atomic flags, and `shader_bridge_missing`. The summary only reflects records a run actually wrote; it adds no live run and no new compatibility claim.
- **Wave size:** the gate logs constraints but changes no shader. A pipeline that D3DMetal creates but does not run still behaves that way.

## Not claimed

Nothing here describes any Xbox title's shaders, behaviour on Xbox hardware, or D3DMetal versions other than those named. Evidence class: local Apple Silicon runs and public-source review.

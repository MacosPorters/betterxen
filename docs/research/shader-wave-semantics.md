# Shader wave-operation failures under D3DMetal: investigation

Branch `research/shader-wave-semantics`, based on PR #391. Evidence class B (runs on one machine: Apple M4, 24 GiB, macOS 27.0, CrossOver 26.3.0.39832, default backend), not independently reproduced. The two failures are the ones recorded in `victory-shader-integration.md`. This note separates what is **established** from what is **a hypothesis**.

## Device capabilities (measured with BetterXen's own host, `workloads/d3d12-wave-probe/d3d12_run.c caps`)

| Field | Value |
| --- | --- |
| Adapter reported | "AMD Compatibility Mode", vendor 0x1002, 18,186 MB (D3DMetal's reported identity; the hardware is the M4) |
| Device created at feature level | 12_1 |
| Highest shader model | 6.6 |
| `WaveOps` | supported |
| `WaveLaneCountMin` / `Max` | 32 / 32 (waves are exactly 32 lanes; `TotalLaneCount` 1024) |
| `DoublePrecisionFloatShaderOps` | **0** (no double-precision shader operations) |
| `Int64ShaderOps` | supported |
| Min-precision support | 0x2 (16-bit) |

## Failure 2: `a_sum_of_halves_where_lanes_have_left_is_the_consoles_tree_bit_for_bit` (established)

**Cause: a wrong test precondition (a subgroup-size assumption), not a translation error.**

1. The test lowers the program with `lowered_on(.., 64)`, "for a host of 64-lane waves (AMD)" per its own comment, then dispatches it on the default device without asking that device's wave width.
2. The device's waves are exactly 32 lanes (table above). A program written for 64-lane waves reads lanes 32 to 63, which do not exist; the observed result was `0x0` against the expected tree `0x5914`.
3. It is the only test in that file that lowers for 64 lanes (line 238); the sibling `a_sum_of_halves_over_the_wave_is_the_consoles_tree_bit_for_bit` lowers for 32 (line 160) and passes on the same device.
4. Lowering this same program for a 32-lane host does not produce a wrong shader: the recompiler refuses it with `Unsupported: a wave sum of 16-bit floats where not every lane runs`. So the recompiler already behaves correctly; it states a limit instead of emitting something wrong.
5. The expected value `0x5914` is derived on the host by integer 16-bit float arithmetic in the test itself, independent of the recompiler.

**Correction made (test, not recompiler):** `Host::wave_lane_count_max()` (new, from `D3D12_OPTIONS1`), and the test now checks the device first. On a device narrower than 64 lanes it asserts the refusal above and returns, printing why; on a 64-lane device it runs the original hardware comparison unchanged. After the change the test passes on this device, and what it verifies there is "the recompiler refuses a 64-lane-only program for a 32-lane host", not "the sum is correct on the GPU". The correct-sum claim stays **unverified on 32-lane hardware** and has not been run on a 64-lane device.

## Failure 1: `whole_wave_f64_sum_and_cvt_f32_f64` (hypothesis, not yet proven)

Observed: a Wave32 program with 4 launched threads, each contributing 1.25, should store 5.0 (`0x40a00000` as f32); it stores `0x00000000`. The program is Wave32 on a 32-lane device, so **the wave-width hypothesis from the PR does not apply to this test**.

What is established from the source and the capability record:
- The recompiler emits native `double` DXIL in exactly one place: `active_f64_sum` (`makeDouble`, `waveActiveOp.f64` with opcode 119, `splitDouble`; sets the shader's "doubles" flag). Every other fp64 instruction uses upstream's integer emulation (`emit/f64.rs`), which upstream states exists because "the host (D3DMetal, Metal) has no fp64"; those tests pass here.
- The device reports `DoublePrecisionFloatShaderOps = 0`.
- D3DMetal nevertheless created the pipeline and ran it, returning zero rather than failing.

Hypothesis consistent with all of that: native double operations in DXIL are not honoured by D3DMetal on this hardware, and the one native-double site in the recompiler is therefore wrong for this backend; the recompiler should use its own integer f64 add for the cross-lane reduction as it does everywhere else. **Not yet established**, because every run so far went through the recompiler itself. Independent evidence needs a native-double shader from a different compiler (`hlsl/sum_f64_4.hlsl`, `hlsl/add_f64_4.hlsl`: a double sum across lanes and a plain double add, to separate "doubles" from "wave operations on doubles"), plus `hlsl/sum_f32_4.hlsl` and `hlsl/lanes4.hlsl` as controls. These need Microsoft's DXC (open source, GitHub release v1.9.2609, `dxc_2026_09_29.zip`, SHA-256 `ad31b1fc…a7e7f1`, matching the release digest) run under CrossOver to compile them. That step has not been done: the permission system blocked running the downloaded compiler and it was not retried. The sources are committed and marked not compiled or run.

No recompiler change is proposed for failure 1 until that evidence exists.

## Reproduce
```sh
scripts/hybrid/run-shader-gpu-tests.sh <llvm-mingw-dir> betterxen-test all   # expect: 1 failure (whole_wave_f64_sum_and_cvt_f32_f64)
# capabilities (needs the d3d12_run.exe built from workloads/d3d12-wave-probe/d3d12_run.c with llvm-mingw):
#   x86_64-w64-mingw32-clang -O2 -o d3d12_run.exe d3d12_run.c -ld3d12 -ldxgi -ldxguid
#   CrossOver bin/wine --bottle <bottle> ./d3d12_run.exe caps
```

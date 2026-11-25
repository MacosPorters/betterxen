# Shader wave-operation failures under D3DMetal: investigation

Part of the shader integration PR (#394). It began as the separate investigation PR #392, whose content is carried here. Evidence class B (runs on one machine: Apple M4, 24 GiB, macOS 27.0, CrossOver 26.3.0.39832, default backend), not independently reproduced. The two failures are the ones recorded in `victory-shader-integration.md`. This note separates what is **established** from what is **a hypothesis**.

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

## Failure 1: `whole_wave_f64_sum_and_cvt_f32_f64` (established: unsupported double-precision operations in D3DMetal; fixed in the recompiler)

Observed: a Wave32 program with 4 launched threads, each contributing 1.25, should store 5.0; it stored `0x00000000`. It is a Wave32 program on a 32-lane device, so wave width was never the cause.

**Independent evidence** (Microsoft's DXC compiled original HLSL, BetterXen's own host dispatched it; expected values derived by hand before running; DXC v1.9.2609 digest-verified and run under CrossOver 26.3):

| Probe (`workloads/d3d12-wave-probe/hlsl/`) | Expected | Observed |
| --- | --- | --- |
| `lanes4`: wave size and lane index, 4 threads | `0x2000`..`0x2003` (wave of 32) | exactly that |
| `sum_f32_4`: `WaveActiveSum(1.25f)`, 4 threads | `0x40a00000` in every lane | exactly that |
| `add_f32_4`: control, run-time load, float add, store | `0x40000000`, `0` per thread | exactly that |
| `add_f64_4`: a `double` add of run-time operands (DXIL contains `fadd fast double`) | low `0x00000000`, high `0x40000000` | **every dword still `0xDEADBEEF`: the shader wrote nothing** |
| `sum_f64_4`: `WaveActiveSum` over a run-time `double` (DXIL `waveActiveOp.f64`) | low `0x00000000`, high `0x40140000` | **every dword still `0xDEADBEEF`: the shader wrote nothing** |

An earlier version of `add_f64_4` used constant operands: the compiler folded the addition to `splitDouble(2.0)` with no `fadd` (reviewer finding by atesahmet0, confirmed with `dxc -dumpbin`), so it did not show a double *addition* failing. The probes above take their operands from the buffer at run time and were rerun; the result is the same.

So wave operations, wave width and partial waves are fine on this device. Any shader that uses a `double` runs without effect: pipeline creation succeeds, the dispatch completes, and nothing is stored. That matches the capability record (`DoublePrecisionFloatShaderOps = 0`; Metal has no fp64). Upstream's test harness zero-fills its output buffer, which is why it saw `0` instead of the sentinel. **Conclusion:** the failure comes from unsupported double-precision operations in the backend, combined with the recompiler emitting a native `double` in one place (`active_f64_sum`) while using integer emulation everywhere else. It is not a mistranslation of the wave operation and not a wave-semantics problem.

**Fix (recompiler, `active_f64_sum`):** the existing integer fp64 add (`add_f64`, checked upstream against the CPU's doubles) is reused to build the console's sum, which is the balanced tree over the lanes in their order. Each lane adds its partner run's partial sum, read through `waveReadLaneAt` from the first lane of that run that is present (found with a ballot, as `half_sum_present` already does for 16-bit sums), `+0.0` where none is, which is what a lane outside EXEC adds on the console. No `double` type is emitted and the doubles feature flag is no longer set. The same code covers the console wave64 on two host waves of 32 (an integer add for the last step).

**Validation** (M4, CrossOver 26.3, default backend; one machine, not independently reproduced):
- Whole suite under CrossOver: **232 passed, 0 failed, 3 ignored** (229 before the review's sampler tests) (was 224 passed, 2 failed). macOS: 179 + new tests pass (`cargo test -p victory-shader`).
- New test `f64_wave_sums_match_the_cpu_tree_bit_for_bit`: 24 cases (1, 4, 20 and 32 launched threads; six values including ones that round: 0.1, 123456.789, a value one ulp above 1.0, -3.75e10, 1e-300), comparing all 64 bits with a CPU model of the balanced tree using the CPU's own f64 additions and `+0.0` for lanes that are not launched. All 24 match. The model is independent of the recompiler.
- Negative controls: against the **original** lowering this GPU test fails (`got 0x0000000000000000, want 0x3ff4000000000000`) and the new `the_f64_wave_sum_is_lowered_without_native_doubles` test (the container's feature word must declare wave operations but not doubles) fails too; both pass with the fix.

**Limits:** (1) the tree order is what the recompiler's recognizer claims the console program computes (`wave::buddies`); this was not independently simulated from the DPP instructions, so bit-for-bit fidelity to the console for rounding values is only as good as that recognition. (2) The cost of the integer version was not measured and no performance claim is made. (3) Other fp64 wave operations (products, min/max) were not examined; only this one site emitted native doubles. (4) On a device with real fp64 the old native operation would work and is no longer used. (5) DXC is an external tool (University of Illinois/NCSA licence plus MIT components per its licence files), used only to compile probes; nothing from it is in the repository.

## Reproduce
```sh
scripts/hybrid/run-shader-gpu-tests.sh <llvm-mingw-dir> betterxen-test all   # the runner script is in PR #395; or run the cross-built test executable with wine directly   # expect: 0 failures with the fix in this branch (the vendored original has 2)
# probes: compile hlsl/*.hlsl with DXC under CrossOver, then d3d12_run.exe run <file>.dxil 1 <dwords>
# capabilities (needs the d3d12_run.exe built from workloads/d3d12-wave-probe/d3d12_run.c with llvm-mingw):
#   x86_64-w64-mingw32-clang -O2 -o d3d12_run.exe d3d12_run.c -ld3d12 -ldxgi -ldxguid
#   CrossOver bin/wine --bottle <bottle> ./d3d12_run.exe caps
```

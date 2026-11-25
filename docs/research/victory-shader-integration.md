# victory-shader integration (GFX10 to DXIL recompiler)

Evidence: classes noted per row. Everything ran on one machine (Apple M4, 24 GiB, macOS 27.0, CrossOver 26.3.0.39832, bottle `betterxen-test`, default backend) and has **not been independently reproduced**. No game, console title or leaked material was used.

## What was integrated

`crates/victory-shader` is a snapshot of `crates/victory-shader/` from skakwy's [victory-mac](https://github.com/skakwy/victory-mac) (head `22b3f8e45a59`). It translates GFX10 (RDNA 2) machine code to PC DXIL, shader model 6.6, in pure Rust: a GFX10 decoder with an SP3 printer, control-flow structurizer, register spilling, wave emulation, fp64 by integer operations, its own LLVM 3.7 bitcode writer and DXIL container writer. About 43,500 lines against 4,022 in BetterXen's native `xenora-shader`. The native RDNA-to-Metal compiler is untouched and still paused; the two are different targets (DXIL for D3D12 under CrossOver versus MSL) and nothing was replaced. Provenance, authors, licences and the 19 changed lines are in [`crates/victory-shader/UPSTREAM.md`](../../crates/victory-shader/UPSTREAM.md). Licensing: GPL-3.0 upstream, GPL-3.0-or-later here, one MIT Mesa-derived table with its notice kept.

Left out on purpose: the upstream repository's loader, HLE, GPU runtime and game-specific code (one console title's runtime) and their provenance questions; the console shader containers the crate can also parse were not exercised, because no authorized container exists.

## How it connects to BetterXen

- `emit::translate_native` (BetterXen addition, `emit/betterxen.rs`): the crate only accepted shaders inside console containers; this entry point takes a compute program as machine code plus root layout, which is what BetterXen has.
- `emit/betterxen_tests.rs`: BetterXen's own synthetic gfx1030 Wave32 fixtures (`fixtures/rdna2/*.bin`, original, assembled with LLVM's `llvm-mc`) are wrapped in a short descriptor-table prologue, translated to DXIL, and compared with the expected words in `fixtures/manifest.toml`. Those expectations were derived by hand in the manifest and are checked by BetterXen's independent reference; none comes from the recompiler. Six of the eleven manifest fixtures have input and expected words (add, float-exact, bitwise-branch, or, shifts, ashr); the five graphics-stage fixtures (`x9-*`) have none and are not bridged.
- Running the cross-built Windows test executable under CrossOver is done by `scripts/hybrid/run-shader-gpu-tests.sh` through `betterxen-hybrid run`. Those live in separate pull requests (the hybrid runner, and the script on top of it); the GPU results below were produced with them.

## Results

| Check | Result | Notes |
| --- | --- | --- |
| Native build, aarch64-apple-darwin, Rust 1.100.0 | builds; fmt, `clippy -D warnings` clean | upstream also builds for Windows targets |
| Upstream tests that run on macOS (CPU-side: decode, structurize, lower, emit) | 177 passed, 2 ignored | The Windows-only GPU modules are not compiled here |
| BetterXen fixtures to DXIL on macOS | 6 of 6 translate to a DXBC container | `translate_native` |
| BetterXen fixtures run on the GPU: D3D12 under CrossOver 26.3 (D3DMetal) | **6 of 6 output words match the manifest** | via `betterxen-hybrid run`, 2.3 to 2.6 s including bottle start; the GPU is a D3D12 device with shader model 6.6 as upstream's host layer requires |
| Negative control | a flipped expected word is rejected; the output is not the zeroed buffer | `a_wrong_expectation_is_detected` |
| Same GPU tests on DXMT, DXVK and wined3d (no D3D12) | fail with "no SM 6.6 D3D12 hardware device: not verified" | confirms they cannot pass by skipping; upstream's own tests print "skipped" instead |
| Whole upstream suite, vendored unchanged, under CrossOver (default backend), 229 tests | 224 passed, **2 failed**, 3 ignored | the two failures are explained in `shader-wave-semantics.md` |
| Whole suite with the fixes in this PR, under CrossOver | **232 tests: 229 passed, 0 failed, 3 ignored** | the 3 additional tests are BetterXen's f64 tests; see below |
| Upstream suite on a backend without D3D12 (DXMT) | 201 passed, 25 failed, 3 ignored | see the classification below |

The two failures are `whole_wave_f64_sum_and_cvt_f32_f64` (a wave-wide fp64 sum returned 0, expected `0x40a00000`) and `a_sum_of_halves_where_lanes_have_left_is_the_consoles_tree_bit_for_bit` (64 lanes: got 0, expected `0x5914`). Both are cross-lane wave reductions of the console's 64-lane waves. Hypothesis, **not verified**: the host's waves are 32 lanes wide (Apple GPU) and D3DMetal does not honour the required wave size or wave operation semantics. The cause, and whether upstream passes them on a Windows GPU, were not investigated.

## Which tests actually executed on the GPU

The harness reports a test as passed whether or not it touched the GPU; many return early when no device is available. Each test was therefore run on the default backend (D3D12 via D3DMetal) and on DXMT (no D3D12), with libtest JSON output including each test's own printed output (`--format json --show-output`). A test counts as GPU-dependent if its outcome changes without D3D12 or it prints "no SM 6.6 hardware device; skipped" there. Final content of this PR (232 tests: 225 upstream, 7 BetterXen):

| Group | Tests |
| --- | --- |
| Executed on the D3D12 hardware device and passed | **31** (26 of them fail without D3D12; 5 pass only by skipping there) |
| Executed on the D3D12 hardware device and failed | **0** |
| Not GPU-dependent: CPU-side (decode, structurize, lower, emit, container) or ignored | 201, including 3 ignored |
| Total | 232 |

For the vendored upstream code before the fixes in this PR the same method gave 229 tests: 32 GPU-dependent, 30 passed on the device and 2 failed (they "passed" without D3D12 by skipping, which hid the failures), see `shader-wave-semantics.md`. Three of the 31 are BetterXen's GPU tests (fixtures, negative control, bit-exact f64 sums). Counts are by name from the JSON events and are exact for this binary and machine. "WARP" skips (the software adapter, which Wine does not provide) and "no `dxil.dll`" skips are not GPU runs and are not counted as such.

## Limits

- Microsoft's `dxil.dll` is not present, so upstream's DXIL validation is skipped in six tests. DXIL is accepted unsigned by D3DMetal under Wine (upstream's code states this; the passing runs agree).
- Many upstream tests also run on WARP (a software adapter) and print "no SM 6.6 warp device; skipped" because Wine has none. Those skips are expected and are not counted as GPU runs.
- Only compute fixtures with one input and one output buffer are bridged. Graphics stages, textures, samplers, wave64 and LDS programs are exercised only by upstream's own tests.
- No performance was measured: a six-fixture dispatch says nothing about speed or about real shaders.
- Correctness is shown for these original fixtures, not for console shaders; the crate's real inputs (console shader containers) were not used.
- Upstream is a snapshot; later upstream fixes must be merged by hand against the pinned commit.

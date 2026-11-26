# `d3dmetal-harness` (Track D, M1-D2)

An opt-in harness that runs a user-supplied `D3DMetal.framework` **outside CrossOver/Wine**, through [`utmapp/d3dmetal-native`](https://github.com/utmapp/d3dmetal-native). It lets a D3D12 failure seen under CrossOver be attributed to D3DMetal or to Wine. BetterXen never bundles, patches or redistributes D3DMetal.

## Why there are two binaries

`D3DMetal.framework` ships as **x86_64 only**. The CrossOver 26.3 copy (D3DMetal 3.0) has a single x86_64 slice, and d3dmetal-native's build refuses any other target. BetterXen's host tools must be arm64-only, so D3DMetal cannot be loaded into them. The work is therefore split:

| Part | Arch | Role |
| --- | --- | --- |
| `bx-d3dmetal-harness` (`src/`) | arm64 | Finds the framework and checks both binaries' architectures. Runs the probe with a cleared environment (`HOME`, `TMPDIR`, `DMN_LOG` and a fixed `PATH` only) and a timeout that also bounds output collection. Judges the readback against values it computes itself (`src/kernel.rs`). |
| `bx-d3dmetal-probe` (`probe/`) | x86_64, under Rosetta 2 | Links d3dmetal-native, creates a D3D12 device, runs the SM 6.6 DXIL kernel below and prints every word it reads back. It never decides correctness. |

The probe fills the output buffer with a sentinel (`0xA5A5A5A5`) first. A readback that is still all sentinel means the kernel never ran, which is reported separately from wrong values.

## Running it

Default builds and `cargo test --workspace` need neither D3DMetal nor the probe. The live test is `#[ignore]`d, and when run explicitly it fails if its prerequisites are missing.

```sh
# 1. Build d3dmetal-native at the evaluated revision (needs meson, ninja, Xcode clang).
git clone https://github.com/utmapp/d3dmetal-native.git /path/to/d3dmetal-native
git -C /path/to/d3dmetal-native checkout af1ee1710711fadff41dead87960d0f1336c1047
(cd /path/to/d3dmetal-native && meson setup build --cross-file build-macos-x86_64.txt && meson compile -C build)

# 2. Build the probe (writes target/d3dmetal-harness/bx-d3dmetal-probe).
shader-transpiler/crates/d3dmetal-harness/probe/build-probe.sh /path/to/d3dmetal-native

# 3. Run. The framework comes from --framework, then $D3DMETAL_FRAMEWORK_PATH, then CrossOver.
cargo run --locked -p d3dmetal-harness -- status
cargo run --locked -p d3dmetal-harness -- run --probe target/d3dmetal-harness/bx-d3dmetal-probe
BX_D3DMETAL_PROBE=target/d3dmetal-harness/bx-d3dmetal-probe \
  cargo test --locked -p d3dmetal-harness --test live_d3dmetal -- --ignored
```

`run` exits with 0 on PASS, 1 on FAIL or error, 2 on a usage error and 77 on SKIP. SKIP means only that no framework was found. An explicit `--framework` or `$D3DMETAL_FRAMEWORK_PATH` that does not resolve is an error. It never falls back silently to another copy. These are also failures, not skips:

- a framework that d3dmetal-native cannot initialize;
- a fence that reports device removal (`UINT64_MAX`), reported with `GetDeviceRemovedReason()`;
- probe output still open at the deadline, for example because a descendant inherited its pipes.

## Synthetic kernel and provenance

`probe/shaders/fill_cs66.hlsl` is original BetterXen source. Thread `i` of a 4 × 64 dispatch writes `((i·0x9E3779B1) ^ ((i·0x9E3779B1) >> 15)) + 0x01234567`, using 32-bit wrapping arithmetic. The unit tests check this function against reference values computed separately in Python.

`probe/shaders/fill_cs66.h` is the compiler output, committed so that building the probe needs no shader compiler:

| Field | Value |
| --- | --- |
| Compiler | DXC `1.9.2609.5 (01b62ad47)`, Microsoft's official release [`v1.9.2609`](https://github.com/microsoft/DirectXShaderCompiler/releases/tag/v1.9.2609) |
| Release package | `dxc_2026_09_29.zip`, SHA-256 `ad31b1fc8443175d204f77a611fdb3ef2ec42759bdc2f1167368de24a4a7e7f1` |
| How it ran | `bin/x64/dxc.exe` under CrossOver 26.3 Wine. DXC has no macOS release, and nothing from the package is committed. |
| Command | `dxc -T cs_6_6 -E main -Vn g_fill_cs66 -Fh fill_cs66.h fill_cs66.hlsl` |
| Output | A 3,092-byte DXBC container with a validator-signed DXIL part (shader model 6.6, compute), SHA-256 `7df11b637ebc7598d14bf916bc908bb3fe7b9034ea49ad42006366dba317d0dd`. It uses the SM 6.6 `createHandleFromBinding`/`annotateHandle` operations. |

A unit test checks the embedded bytes with `dxbc-accc`, independently of DXC's own listing: it confirms a DXIL container for a compute shader at model 6.6.

## Recorded evaluation (2026-11-26)

These are local runs by the implementer on one host. They have not been reproduced independently, and they are not Xbox hardware results or a general statement about D3DMetal.

- **Host:** Apple M5 Pro, macOS 27.2.1, Rosetta 2. **Framework:** D3DMetal 3.0 bundled with CrossOver 26.3 (thin x86_64; SHA-256 of the binary starts with `05a7beaed449`). **d3dmetal-native:** `af1ee17`, built with Meson 1.12.1 and Xcode clang.
- **`bx-d3dmetal-harness run`:** all 16 D3D12 steps succeeded, the device reported shader model 6.6, and the result was `PASS: 256/256 words match`. The ignored live test also passed.
- **d3dmetal-native's own suite on the same framework:** `dmn-smoke` and `dmn-device-test` passed. `d3d12-atomic64-test` created its cs_6_6 pipeline but read back zeros for every 64-bit atomic result. That is d3dmetal-native's test, not this harness. It is recorded as an observation to follow up, not a diagnosed D3DMetal defect.

## Limits

- One kernel shape: one UAV bound as a root descriptor, with no descriptor heaps, textures or graphics pipeline.
- Rosetta 2 is required. Apple's statements on Rosetta's lifetime after macOS 27 are recorded in [the primary-source verification](../../../docs/research/primary-source-verification-2026-11-25.md).
- d3dmetal-native also ships `scripts/patch_d3dmetal_imports.py`, which rewrites the D3DMetal binary for Mac App Store builds. BetterXen does not use it: modifying a proprietary component is outside project rules, and an unpatched framework works for this harness.

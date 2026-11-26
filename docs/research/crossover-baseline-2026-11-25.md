# CrossOver baseline on the M4 reference machine (2026-11-25)

Evidence class: observed runs on one machine by one person (class B in the sense of [claims.toml](../claims.toml)); **not independently reproduced**, so #355 and #353 stay open. Every result is for original BetterXen workloads, never for a game. Nothing here shows Xbox compatibility.

## Environment

| Item | Value (source) |
| --- | --- |
| Machine | Apple M4, 10 cores (4 performance, 6 efficiency), 24 GiB, macOS 27.0 build 26A428 (`betterxen-hybrid host`) |
| CrossOver | `/Applications/CrossOver.app`, 26.3, build 26.3.0.39832, bundle id `com.codeweavers.CrossOver`, main executable universal (x86_64, arm64) (`betterxen-hybrid crossover`). Licence or trial state not inspected; runs worked |
| Wine host binaries | `wineloader` and `wineserver` are **x86_64 only** (`lipo -archs`), so CrossOver's Wine runs under Rosetta 2 on this Mac. `arch -x86_64 /usr/bin/true` executes |
| Bottle | `betterxen-test`, template `win11_64`, created in 18.7 s with `cxbottle --bottle betterxen-test --create --template win11_64` |
| Launch interface | `bin/wine --bottle B program` and `bin/wineserver -k`. `cxbottle --help` and `cxstart --help` exist in the bundle. **None of these command-line interfaces is documented by CodeWeavers for macOS**: the [Mac user guide](https://support.codeweavers.com/en_US/crossover-mac-user-guide) and [advanced configuration page](https://support.codeweavers.com/advanced-crossover-mac-configuration) cover the GUI and hook variables only. They are used here as observed vendor behaviour of this build |
| Backend selection | `CX_GRAPHICS_BACKEND=<name>` in the environment. Undocumented (see #350). An unknown value (`bogus`) was **accepted silently** and gave the same result as the default |
| Toolchain | llvm-mingw 20261117, Rust 1.100.0; see [hybrid-toolchain.md](hybrid-toolchain.md) |
| Host noise | CrossOver's antivirus helper prints `LibClamAV Error` and `Can't open file or directory` because Homebrew's ClamAV has no database in `/opt/homebrew/var/lib/clamav`. Harmless to results; the run record reports it as a diagnostic |

## Workloads executed

Run with `betterxen-hybrid run --bottle betterxen-test --expect PASS -- <exe>`, default backend.

| Workload | x86-64 result | ARM64 result |
| --- | --- | --- |
| C self-check (CRC-32, four threads) | passed, exit 0, 1.4 s | `cannot_execute` |
| Rust self-check (threads, TLS) | passed, exit 0, 0.6 s | `cannot_execute` |
| D3D11 clear and readback | passed, exit 0, 1.1 s | `cannot_execute` |
| D3D11 indexed triangle, shaders, frame loop | passed, exit 0, 0.8 s | `cannot_execute` |

CrossOver 26.3 here **rejects Windows ARM64 executables** with `winewrapper.exe:error: cannot execute`. The ARM64 builds exist and have the correct PE type, but cannot be run in this environment. Process memory sampled while running was about 325 MB (CrossOver's persistent processes, identical across workloads, so it says nothing about the workloads themselves); minimum system free memory stayed at 64 to 68 %.

Negative controls (#365): `d3d11-clear --negative-control` and `d3d11-frameloop --negative-control` expect a deliberately wrong colour and exit 1; the normal runs exit 0.

### Defects found in BetterXen's own workloads, not in CrossOver

1. **Clear test required exact bytes.** The first run read green as 127 where the program expected 128 (0.5 × 255 = 127.5, an exact tie). Re-derived from the primary source ([Direct3D data conversion rules](https://learn.microsoft.com/en-us/windows/win32/direct3d10/d3d10-graphics-programming-guide-resources-data-conversion)): the ideal conversion adds 0.5 and drops the fraction (128), but implementations are permitted a tolerance of 0.6 unit-last-place on the integer side (`D3D12_FLOAT32_TO_INTEGER_TOLERANCE_IN_ULP (0.6f)` in the mingw-w64 header; the D3D11 constant is described by the same page). 127 is 0.5 from 127.5 and is within tolerance. The workload now checks that tolerance and prints how many pixels differ from the ideal. This was a correction to match the specification, not a loosening to get a pass: the negative control still fails.
2. **Triangle was culled.** The first frame-loop run left the centre black. The index order was counter-clockwise, which Direct3D 11 culls by default. Fixed to clockwise; the centre then read (51, 102, 204, 255) exactly.

## Graphics backend comparison

`betterxen-hybrid bench --backends default,d3dmetal,dxmt,dxvk,wined3d --repeat 5`, 2000 frames per run, workload `d3d11-frameloop`: a 640x360 offscreen RGBA8 target, one indexed triangle with a vertex and a pixel shader, an event query waited on every frame. **No window and no presentation.** All 25 runs passed. Medians across five runs:

| Backend (detected from workload output) | Adapter reported | Median frame | p95 | p99 | Frame rate | Startup + run, median |
| --- | --- | --- | --- | --- | --- | --- |
| default | AMD Compatibility Mode | 0.281 ms | 0.448 ms | 0.929 ms | 3153 | 1.42 s |
| d3dmetal | AMD Compatibility Mode | 0.282 ms | 0.558 ms | 1.805 ms | 2898 | 1.44 s |
| dxmt | Apple M4 | 0.158 ms | 0.239 ms | 0.382 ms | 5896 | 1.13 s |
| dxvk (via MoltenVK, "cxaddon-1.10.3") | Apple M4 | 0.150 ms | 0.263 ms | 0.478 ms | 5835 | 1.13 s |
| wined3d | Apple M4 | 0.150 ms | 0.256 ms | 0.419 ms | 5788 | 1.13 s |

Observations, with their limits:

- **The default behaved like D3DMetal** (same adapter identity, same timings) for a bottle made from `win11_64` and an unknown executable. CodeWeavers' Advanced Settings page, as recorded in [primary-source-verification](primary-source-verification-2026-11-25.md), says Auto falls back to wined3d without a per-game database entry. The observation disagrees with that reading; why was not investigated.
- D3DMetal's median per-frame submit-and-wait time was about 1.8 times that of the other three, with a larger tail. This is the cost of a trivial draw plus synchronisation, **not rendering throughput**, and shows nothing about real scenes. It identifies a candidate for investigation, not a bottleneck.
- Sustained run, 200,000 frames: D3DMetal 64.1 s, median 0.283 ms, p99 0.987 ms, p99.9 2.07 ms, maximum 20.3 ms; DXMT 29.6 s, median 0.135 ms, p99 0.339 ms, p99.9 0.48 ms, maximum 7.4 ms. No failure, no hang, and free memory did not trend downward (minimum 64 % and 66 %).
- Not measured: GPU utilization (no unprivileged documented source on this host), memory use of the workload itself, thermal behaviour under load (pmset reported no thermal, performance or CPU-power warning before and after; the workload loads the GPU too lightly for that to mean anything), presentation and frame pacing, texture sampling, depth testing, and Direct3D 12.

## What this does and does not establish

Established on this machine and build: CrossOver 26.3 runs original x86-64 console, threaded, Direct3D 11 clear and triangle programs under four graphics backends with correct pixels; it does not run ARM64 Windows programs; BetterXen can detect, supervise, classify and record such runs. Not established: anything about Direct3D 12, real-scene performance, other macOS or CrossOver versions, other machines, or any game. A second contributor still has to reproduce this.

## Saving new baseline evidence

The runner now writes `run-<nanoseconds>-<pid>-<sequence>.json` and
`bench-<nanoseconds>-<pid>-<sequence>.json` when `--out` is supplied. Exclusive
creation preserves existing reports, including older `bench.json` files. The
selected path is printed to stderr; stdout retains the workload classification
and report JSON. A directory, creation or write error returns a nonzero CLI
status even if the workload itself passed. These persistence changes do not
revalidate or replace the historical results above.

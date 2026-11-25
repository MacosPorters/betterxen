# Hybrid workloads and measurement protocol

This is the Phase 1 protocol for [issue #353](https://github.com/MacosPorters/betterxen/issues/353).
It defines workload selection, measurement boundaries and the evidence a contributor
must retain for the [CrossOver baseline task](https://github.com/MacosPorters/betterxen/issues/355).
It adds no runner and reports no new Windows or GPU execution results.

## Implementation checkpoint

The commands below refer to [PR #384](https://github.com/MacosPorters/betterxen/pull/384),
reviewed at commit `566272082a74d8b45fcec37edd00f4f6703df3ab` on 2026-11-25.
Its `betterxen-hybrid` command, build script and Windows workloads are not on
`main` at this document's base revision. Use a separate checkout of that exact
candidate to review the recipe; after it merges, record the actual merged SHA
and recheck the command options. A missing command is an unavailable prerequisite.
Do not substitute the preserved native benchmark suite for a CrossOver run.

The implementation links in this document are pinned to that candidate. They
describe source inspection, not independent acceptance of the author's recorded
M4 results. Those results remain in the candidate's
[baseline record](https://github.com/MacosPorters/betterxen/blob/566272082a74d8b45fcec37edd00f4f6703df3ab/docs/research/crossover-baseline-2026-11-25.md).
Use the [roadmap](../roadmap.md) for phase completion and
[benchmark protocol](benchmarks.md) for the existing native benchmark's separate scope.

## Selected workloads and provenance

All four initial workloads are original BetterXen source. Each source below has
an `SPDX-License-Identifier: GPL-3.0-or-later` header at the pinned revision.
Use the repository [license](../../LICENSE); no external test source, shader,
texture, game binary or asset is added by this selection. The C programs use
external mingw-w64 headers; the Rust program uses the Rust standard library.
Those build dependencies retain their own terms and are not relicensed by the
workload headers. The build's exact dependency review belongs to
[#364](https://github.com/MacosPorters/betterxen/issues/364) and
[#354](https://github.com/MacosPorters/betterxen/issues/354).

| Workload and source at the candidate | Purpose and expected result | Limits |
| --- | --- | --- |
| [C self-check](https://github.com/MacosPorters/betterxen/blob/566272082a74d8b45fcec37edd00f4f6703df3ab/workloads/win-selfcheck-c/selfcheck.c) | CRC-32 of `123456789` is `0xCBF43926`; four threads increment a shared counter to 400,000; stdout ends in `PASS`, exit 0. | CPU/runtime smoke check; no GPU work. The thread wait result is not checked separately. |
| [Rust self-check](https://github.com/MacosPorters/betterxen/blob/566272082a74d8b45fcec37edd00f4f6703df3ab/workloads/win-selfcheck-rust/src/main.rs) | Four thread-local counters give 1,000, 2,000, 3,000 and 4,000; their total is 10,000; the main thread's counter stays zero; `PASS`, exit 0. | One small threads/TLS case, not general Wine conformance. TLS means thread-local storage. |
| [Direct3D 11 clear](https://github.com/MacosPorters/betterxen/blob/566272082a74d8b45fcec37edd00f4f6703df3ab/workloads/d3d11-clear/d3d11_clear.c) | Clear a 64×64 RGBA8 target, read all 4,096 pixels, `PASS`, exit 0. Each channel must be within 0.6 of `(0.25, 0.5, 0.75, 1) × 255`; ideal bytes are `(64,128,191,255)`. | The specified conversion tolerance permits green 127 or 128. No shader, texture sampling or presentation. |
| [Direct3D 11 indexed triangle/frame loop](https://github.com/MacosPorters/betterxen/blob/566272082a74d8b45fcec37edd00f4f6703df3ab/workloads/d3d11-frameloop/frameloop.c) | A 640×360 target, one indexed triangle, vertex/pixel shaders and an event-query wait per iteration. First-frame readback checks corner `(0,0,0,255)` and centre `(51,102,204,255)` within the source's tolerance; `PASS frames=N`, exit 0. | Only two pixels of the first frame are checked. HLSL is compiled at runtime with `D3DCompile`, a deviation from #365's requested build-time compilation. No window, swapchain or presented frames. |

For both graphics programs, `--negative-control` deliberately changes the
expected colour: require a mismatch and exit 1, not a missing-device result.
Exit 2 with `NO DEVICE` is unsupported/unavailable graphics, never a pass; exit 3
is an API failure. Re-derive pixel expectations from geometry and Microsoft's
[data conversion rules](https://learn.microsoft.com/en-us/windows/win32/direct3d10/d3d10-graphics-programming-guide-resources-data-conversion)
before accepting the output. Do not change tolerances after seeing a failure.

The x86-64 Windows PE builds are the initial execution set. ARM64 Windows builds
are a separate admission experiment: record what the installed runtime does,
including refusal, without treating it as x86-64 success or assuming every
CrossOver release behaves alike. PE denotes Portable Executable, the Windows
executable format; it is guest input, not the host executable architecture.

Texture sampling and compute workloads are still required by
[#365](https://github.com/MacosPorters/betterxen/issues/365). Depth, presentation,
Direct3D 12 and upstream Wine conformance runs are not established by these four
programs. External test candidates need an exact revision and per-file license
review before admission; none is selected here.

## Preparation and correctness run

Use an Apple Silicon Mac and record its actual chip and memory. The initial
reference is an M4 with 24 GiB; another configuration is another observation,
not a replacement for that host's acceptance. CrossOver and any required Rosetta
installation must already be supplied by the contributor under their own terms.
Nothing in this protocol installs, bundles or activates proprietary software.

1. Record the full candidate SHA, dirty state, Rust/compiler versions, build
   flags and target. Use native ARM64 host tools; retain `file`, `lipo -archs`
   and `otool -L` evidence for the tools and executable actually used. A universal
   tool's ARM64 slice and its Windows output target are different facts.
2. Verify the llvm-mingw archive version and checksum against the pinned
   [toolchain note](https://github.com/MacosPorters/betterxen/blob/566272082a74d8b45fcec37edd00f4f6703df3ab/docs/research/hybrid-toolchain.md).
   At this checkpoint the build script accepts an unpacked directory and does
   not enforce the pin. Rebuild twice, retain output hashes and report any
   directory-dependent differences; do not claim reproducibility from one build.
3. Create a disposable, dedicated bottle through CrossOver's UI. Record its
   template, configuration and name locally. Keep unrelated programs out of it:
   stopping the runtime can stop every process in that bottle.
4. Keep raw evidence under ignored `artifacts/`. Use a new directory per attempt;
   the candidate overwrites `bench.json` and names individual run JSON by seconds.
   Record setup, correctness and timing as separate attempts.

In the checkout containing the candidate implementation, with `LLVM_MINGW_DIR`
set to the already verified toolchain directory:

```sh
test -f crates/hybrid/Cargo.toml || exit 1
: "${LLVM_MINGW_DIR:?Set the verified llvm-mingw directory}"
export CARGO_BUILD_JOBS=2
cargo +1.100.0 build --locked --release --target aarch64-apple-darwin -p betterxen-hybrid
B=target/aarch64-apple-darwin/release/betterxen-hybrid
OUT=artifacts/hybrid-protocol/correctness-01
mkdir -p "$OUT"
git rev-parse HEAD > "$OUT/candidate.txt"
git status --porcelain > "$OUT/dirty.txt"
"$B" receipt > "$OUT/receipt.json"
scripts/hybrid/build-windows-workloads.sh "$LLVM_MINGW_DIR"
shasum -a 256 artifacts/hybrid-workloads/x86_64/*.exe > "$OUT/workload-sha256.txt"

for p in selfcheck-c selfcheck-rust d3d11-clear d3d11-frameloop; do
  "$B" run --bottle betterxen-protocol --timeout 60 --expect PASS \
    --out "$OUT/$p" -- "$PWD/artifacts/hybrid-workloads/x86_64/$p.exe" \
    > "$OUT/$p.json" || exit 1
done

"$B" run --bottle betterxen-protocol --timeout 60 \
  --out "$OUT/clear-negative" -- \
  "$PWD/artifacts/hybrid-workloads/x86_64/d3d11-clear.exe" --negative-control \
  > "$OUT/clear-negative.json"
```

Create the bottle `betterxen-protocol` before this recipe. For the last command,
require runner exit 1 **and** JSON `classification: failed_exit_1`, workload
`exit_code: 1` and the pixel-mismatch diagnostic. Also run the triangle negative
control with the same option. A runner error, timeout or `no_graphics_device`
does not satisfy a negative control. The positive runs need their expected
checks, not just a zero supervisor exit or the substring `PASS`.

At the pinned checkpoint, #384's stop path can leave a workload running because
it selects the bottle incorrectly for `wineserver -k`. The correction is in
[PR #388](https://github.com/MacosPorters/betterxen/pull/388); inspect the actual
revision under test rather than assuming the correction merged. After any timeout
or low-memory stop, stop the dedicated bottle in CrossOver and confirm the workload
is gone before another run. A classification alone is not proof of cleanup.

## Receipt and run-record sources

`receipt` emits `betterxen-hybrid-receipt/0` with `host`, `crossover` and
`windows_cross_compilers`. A `run` record (`betterxen-hybrid-run/0`) does **not**
embed that host receipt at the pinned revision; keep the sibling `receipt.json`
with each attempt. `bench` embeds `host` once. Supplement these records with
candidate/workload hashes, build options, source commands and measurement settings;
they are not all emitted by the current tool. Do not invent an implemented schema.

| Fact | Source at this checkpoint | Interpretation and missing data |
| --- | --- | --- |
| macOS version/build | `sw_vers -productVersion`; `sysctl -n kern.osversion` | Retain both; a marketing version alone is insufficient. |
| Chip, memory and CPUs | `sysctl -n machdep.cpu.brand_string`, `hw.memsize`, `hw.logicalcpu`, `hw.perflevel0.physicalcpu`, `hw.perflevel1.physicalcpu` | Bytes for memory; failed or absent keys are unknown. Preserve raw perflevel values; their P/E labels are the collector's interpretation. |
| Collector architecture and Rosetta execution probe | `receipt_binary_arch` comes from Rust's build target; `arch -x86_64 /usr/bin/true` supplies `rosetta_executes_x86_64` | The latter deliberately executes a system binary's Intel slice. Failure does not distinguish absence from disabled/unusable translation; it does not establish the Wine process's architecture. |
| GPU and Metal description | `system_profiler SPDisplaysDataType -json`, first matching `sppci_model`, `sppci_cores`, `spdisplays_mtlgpufamilysupport` | Inventory strings, not runtime `MTLDevice` capability queries or execution proof. Capability queries not collected are unknown. |
| CrossOver build and bundle architecture | `Contents/Info.plist` via `plutil`; `lipo -archs` on `Contents/MacOS/<CFBundleExecutable>` | Record the selected installation; the GUI executable's slices do not establish `wineloader`/`wineserver` architecture. Multiple installations need explicit reconciliation with the runner's first-discovered selection. |
| Bottle search/preferences | `defaults read com.codeweavers.CrossOver BottleDir` and `ManagedBottleDirs`; candidate locations and `mdfind` | Unset/unreadable preferences are not proof of no bottles or no installation. |
| Windows compiler identity | Resolved compiler path, `--version`, `-dumpmachine`; retain archive hash separately | Inspect native executables before running them. The receipt's probe does not establish the archive pin or inspect every helper. |
| Requested/detected backend | CLI selection and `CX_GRAPHICS_BACKEND`; stdout/stderr banners or adapter-name heuristic in `run.rs` | The variable is undocumented for macOS in the reviewed sources (#350). Preserve `unknown` and `inferred` labels; `default` is not proof of a particular backend. |
| Run outcome and lifetime | Child status/output; supervisor monotonic clock from spawn through completion/stop | Require correctness diagnostics and expected exit as well as classification. `duration_ms` includes startup and workload time, not isolated startup latency. |

Field definitions come from the candidate's
[host collector](https://github.com/MacosPorters/betterxen/blob/566272082a74d8b45fcec37edd00f4f6703df3ab/crates/hybrid/src/host.rs),
[CrossOver discovery](https://github.com/MacosPorters/betterxen/blob/566272082a74d8b45fcec37edd00f4f6703df3ab/crates/hybrid/src/crossover.rs),
[CLI](https://github.com/MacosPorters/betterxen/blob/566272082a74d8b45fcec37edd00f4f6703df3ab/crates/hybrid/src/main.rs)
and [runner](https://github.com/MacosPorters/betterxen/blob/566272082a74d8b45fcec37edd00f4f6703df3ab/crates/hybrid/src/run.rs).
This describes their implementation, not a guarantee that every probe works on
every macOS version. Preserve JSON `null` and explain absent fields as unknown;
never replace them with zero or a nearby machine's values.

## Metric definitions and collection limits

| Metric | Definition / collection | Required qualification |
| --- | --- | --- |
| Average rate and frame-time percentiles | Positive finite intervals `t[i]` in milliseconds; rate = `1000 × n / sum(t)`. Sort intervals; percentile p is rank `ceil(p × n / 100)`, one-based. Report n, p50, p95, p99, p99.9 and maximum. | The hybrid analyzer uses nearest rank; the preserved native startup benchmark uses interpolation. Do not combine them. Report sample count; a small sample's p99.9 can be its maximum. |
| 1% and 0.1% lows | `1000/p99_ms` and `1000/p99.9_ms`, respectively. | These are reciprocal percentiles, not averages of the slowest 1% or 0.1%. Keep that definition in reports. |
| Stutters | Count intervals strictly greater than twice p50. | A protocol convention, not a universal perceptual threshold. Intervals equal to twice p50 do not count. |
| CPU | Sample `/bin/ps -axo rss=,pcpu=,command=`; sum matched CrossOver/bottle processes and average the samples. | Percent of one core, can exceed 100%. Path matching is heuristic and can include unrelated CrossOver processes. This is not CPU time divided by the run's wall time. Zero samples means unavailable, not zero usage. |
| Unified memory / memory pressure | `memory_pressure -Q` free-percentage line; retain starting and minimum observed percent plus poll interval. `ps` provides sampled peak summed resident set size (RSS, KB). | The free-percentage text format is observed, not a stable documented API. System free percentage and process RSS are not total unified/GPU allocation. RSS can double-count shared pages; failed `ps` currently also yields zeros, so do not accept those as measured zero. |
| GPU utilization | Not collected by the candidate. | Unknown; do not infer it from FPS, RSS or elapsed time. A future collector must name its source and overhead. |
| Startup and shader compilation | Current `duration_ms` covers the whole supervised process. D3D11 HLSL compilation occurs before the loop. | Separate spawn-to-first-result and compile durations are not instrumented. Mark them uncollected; a first run is not necessarily a cold-cache run. |
| Thermals | `pmset -g therm` before/after; retain raw warnings and power-source/idle notes (`pmset -g batt`, `uptime`). | No warning is not a temperature reading, proof of no throttling or a sustained-load result. |
| Long-duration stability | Predeclare wall-time/iteration budget, timeout, sampling interval and stop conditions; retain every failure, memory trend and repeat. | The tiny triangle is not representative load. Its only pixel readback is at the beginning, so successful exit does not establish correctness of all later frames. |

The frame loop uses
[QueryPerformanceCounter](https://learn.microsoft.com/en-us/windows/win32/api/profileapi/nf-profileapi-queryperformancecounter)
around submission and a
[GetData event-query wait](https://learn.microsoft.com/en-us/windows/win32/api/d3d11/nf-d3d11-id3d11devicecontext-getdata).
It measures CPU-observed offscreen iteration intervals, including wait, loop and
logging overhead. It does not measure GPU timestamp duration or displayed FPS.
The first interval starts before first submission; after the first readback the
clock is reset. Preserve that boundary when describing the series. The busy wait
also affects CPU and thermal observations.

The runner computes statistics from `frame_ms` lines but deletes its full raw
stdout/stderr files, retaining only tails without the frame lines in JSON.
Consequently its summary alone cannot be independently reanalyzed. Full trace
retention, presented-frame capture per backend and capture-overhead measurement
remain [#145](https://github.com/MacosPorters/betterxen/issues/145) work. Until they
exist, label these runs exploratory offscreen summaries. Do not present them as
validated frame-pacing or capture-overhead results.

## Repetition and analysis check

Run correctness and both graphics negative controls for each backend before
using its timing. Use release builds, AC power, an otherwise idle machine and
one GPU job at a time. Record validation variables and toolchain settings;
keep correctness with validation enabled separate from timing with it disabled.
Reuse the [native protocol's](benchmarks.md#protocol) environment discipline.

Declare iteration count (for example 2,000), two untimed warm-up processes and
five measured processes per backend before starting. Record the exact backend
order and reverse/rotate it for a repeated comparison. Do not silently discard
failed attempts, outliers, the first interval or additional warm-ups. The
candidate does not implement within-process warm-up exclusion; report the full
series it actually summarizes. Keep first-run and warmed observations separate
without claiming to have cleared any driver cache.

After correctness passes, a measured block on the candidate can use:

```sh
OUT=artifacts/hybrid-protocol/timing-01
mkdir -p "$OUT"
MTL_DEBUG_LAYER=0 MTL_SHADER_VALIDATION=0 "$B" bench \
  --bottle betterxen-protocol --backends d3dmetal,dxmt --repeat 5 \
  --timeout 60 --min-start-free 50 --min-free 15 --expect PASS --out "$OUT" \
  -- "$PWD/artifacts/hybrid-workloads/x86_64/d3d11-frameloop.exe" 2000 \
  > "$OUT/bench-stdout.json"
```

Run the declared warm-ups separately before that block. These backend names are
candidates, not promises of availability; retain refusals and unknown identity.
Report each process's statistics and the median/range across processes. Do not
pool intervals across processes or call a median of p99 values the pooled p99.
Do not subtract an assumed capture overhead; it has not been measured.

Check the analyzer independently with 100 synthetic intervals: 98 at 10 ms,
one at 20 ms and one at 100 ms. Their sum is 1,100 ms, mean 11 ms and rate
`1000/11` per second. Nearest ranks 50, 95, 99 and 100 give p50=10, p95=10,
p99=20 and p99.9=100 ms. The lows are 50 and 10 per second; exactly one
interval exceeds twice the median. In the implementation checkout:

```sh
mkdir -p artifacts/hybrid-protocol
python3 -c 'print("\n".join(["10"] * 98 + ["20", "100"]))' \
  > artifacts/hybrid-protocol/known-intervals.txt
"$B" frames artifacts/hybrid-protocol/known-intervals.txt
```

This arithmetic is a check of analysis conventions, not Windows/GPU evidence or
a new performance measurement. An independent reviewer must re-derive it rather
than use the analyzer to generate its own expected answers.

## Evidence and independent acceptance

Attach the candidate identity, workload hashes, receipt, settings, command exit
statuses, output checks and run records to each local attempt. Classify CPU,
graphics readback, runtime admission and timing separately. Record `passed`,
failed, unavailable, unsupported and not-run outcomes explicitly. Missing
tooling, a skipped test or a timeout cannot establish GPU support.

Keep source review (class D), local original-fixture execution and independent
reproduction distinct under the repository's [claims registry](../claims.toml)
and [review records](review-log.md). Do not promote one machine's observation to
M1–M5 compatibility, Xbox-hardware verification or game compatibility. A second
contributor records their own candidate, hardware, commands and disagreements;
the implementer cannot independently accept their result.

Before posting evidence, review a separate redacted copy: remove personal paths,
email addresses, unrelated process data and private bottle/program names, while
retaining versions, units, outcomes and hashes needed to understand the run.
Use repository-relative paths and a pseudonym. Raw receipts can contain local
compiler/application paths and stdout tails; this protocol never uploads them.

# Benchmark protocol

The benchmark harness measures each fixture in its manifest that passes its
correctness test in the same run. It never reports a number for a shader that
failed or was skipped.

## Corpus

`benchmarks/manifest.toml` lists each fixture with its stage, source, binary path,
declared input and hand-derived expected output, and the word counts to time. The
manifest uses a documented subset of TOML (`[[fixture]]` tables with string and
integer-array values) parsed by the harness, so no external TOML dependency is
pulled into the workspace.

A fixture is added to the corpus by adding a table. If it passes correctness it is
benchmarked automatically; there is no separate opt-in list.

## Correctness gate

Before any timing, the harness requires all of the following for each fixture:

1. The Rust pipeline (`xenora-shader::compile`) accepts the binary.
2. The independent reference interpreter agrees with the hand-derived `expected`
   values on the declared input.
3. The native GPU produces the expected output for the smallest declared size.

If any step fails the fixture is marked `excluded` with a reason, listed in the
coverage table, and not timed. A nonempty exclusion set makes the harness exit
nonzero. Measurements are included only after all sizes for a fixture
pass. A debug smoke run using a temporary manifest with sizes 4 and
4097 for the add fixture verified that rejection at 4097 produces an
empty timing list for that fixture while the other two fixtures retain
all five sizes. The smoke run is a correctness check for reporting.

## What is measured

Per fixture and per word count, over `--reps` measured repetitions after two
warm-up repetitions:

| Field | Source | Meaning |
| --- | --- | --- |
| `compile` | host monotonic clock | Rust decode, lift, verify and MSL generation |
| `library` | host monotonic clock | `newLibraryWithSource` Metal compilation |
| `pipeline` | host monotonic clock | compute pipeline state creation |
| `gpu` | `GPUEndTime - GPUStartTime` | command-buffer GPU execution time |
| `host` | host monotonic clock | wall time of one compile-and-dispatch call |

`gpu_zero_fraction` records the share of samples whose GPU timestamp delta was
zero, which happens for very short kernels. A high value means the GPU timing is
at timer resolution and should not be read as a precise duration.

Statistics reported are median, minimum, maximum and the first and third quartiles.
The report never uses best-of.

## Protocol

- Release builds only for published numbers; the metadata records the profile.
- The machine must be otherwise idle, on AC power, with the thermal state noted.
- Validation is off for timing runs. The metadata records the `MTL_DEBUG_LAYER`
  and `MTL_SHADER_VALIDATION` values in effect so a validated run is never confused
  with a timing run.
- The fanless M5 MacBook Air throttles under sustained load; a single run does not
  characterize sustained performance.
- No comparison to Xbox hardware, other emulators or other platforms is made.

## Output

`benchmarks/results/<hardware>/<run-id>/` contains `metadata.json`, `results.json`
and `report.md`. Only small text reports are committed. Raw logs stay under the
ignored `artifacts/` directory.

## Reproduction

```sh
cargo build --release -p xenora-benchmarks
MTL_DEBUG_LAYER=0 MTL_SHADER_VALIDATION=0 ./target/release/xenora-bench --reps 7
```

## Limitations

The corpus currently covers three synthetic gfx1030 compute fixtures under the
[synthetic memory ABI](../adr/0003-demo-memory-model.md). It does not measure
graphics stages, guest containers, Wave64 or divergence. Translation-versus-native
baseline ratios are not reported because no hand-written native MSL baseline exists
yet for these fixtures.

## Exploratory AC run

Run `1795352394` used the release harness built at `cc0daa7` on Apple M5,
with AC power confirmed before and after, 30 seconds between the build and
timing, two warmups, 21 measured repetitions per size and Metal validation
off. Neither thermal nor performance warnings were recorded by
`pmset -g therm`. All three declared translated fixtures passed their
correctness checks. The recorded 1-, 5- and 15-minute load averages were
8.12, 8.68 and 6.31, so an idle environment was not established. G10 remains open for the full correctness corpus,
individual compiler stages and cold/warm timing, tool and capability metadata,
and noise-based variability reporting. See the
[generated report](../../benchmarks/results/apple-m5/run-1795352394/report.md).

## Comparing runs

`scripts/compare-benchmarks.py` compares two structured run directories
(`metadata.json` and `results.json`) only when their revision-independent
configuration matches (`device`, `host_arch`, `os_version`, `os_build`,
`rustc`, `xcode`, `profile`, `api_validation`, `shader_validation`, `power`,
`thermal`, `reps`, and `warmup`). Only `run_id`, `timestamp`, and `commit` may
differ. If any revision-independent field differs or is missing, the tool exits
nonzero (`2`) and lists each mismatch instead of computing misleading deltas.

When configuration matches, the tool reports per-fixture, per-size medians,
interquartile ranges (`q3 - q1`), min-max spans, relative deltas, missing or
excluded fixtures/sizes/metrics, and factual uncertainty flags (`iqr_overlap`,
`low_sample_count`, `gpu_timer_zero_samples`, `zero_baseline_median`). It
intentionally applies no arbitrary pass/fail speedup threshold.

```sh
python3 scripts/compare-benchmarks.py <baseline-dir> <candidate-dir> [--format text|json]
python3 scripts/compare-benchmarks.py --self-test
```

# Extended Remill evaluation

**Decision: retain BetterXen's current CPU path. The tested Remill adapter fails
floating-point and instruction-budget requirements needed before adoption.**
This is a local research result from November 25, 2026, not independent acceptance
or a verdict on every possible Remill integration. The original native prototype
remains usable; its 32 checks also passed inside this extended run.

## Candidate and evidence

- BetterXen worktree: `b2654e93a42e74e29570183da1f31bb8cdc6f721`, with local files
  restricted to the experiment, documentation index and changelog.
- Remill: `66c488c9911fd9b368e4d0c9647d74239f8bba8b`, unchanged upstream source.
- Apple M4, native ARM64, macOS 27.0 (26A428), LLVM/Clang 17.0.6 and Rust 1.100.0.
- Native tool/object/archive/linkage receipts cover 59 inputs. Guest x86-64
  ELF objects are assembler data and were never executed as host binaries.
- Evidence class A applies to the observed execution on this particular host.
  Expected CPU behavior is spec-derived, not compared with physical x86 or Xbox.
  Independent reviewers have not accepted these results.

`fixtures.s` contains seven additional original programs. LLVM assembles them,
and the driver compares the extracted bytes against the separately declared
fixture bytes before lifting. Remill's SSE/AVX probes use `amd64_avx`; the bridge
keeps the same explicitly sized state layout. The production CPU registry,
CPUID and AVX contracts are unchanged. The related design scope remains #299.

The extended executable exits **1**, and the driver records **failed**. It does
not turn mismatches into successful tests. There are **48 additional semantic
observations, with eight mismatches**, all in the Remill floating-point route.
Explicit VEX rejection checks on the existing engines are rejection evidence,
not AVX support. The runaway-budget failure is recorded separately.

## Floating-point findings

Nine `ADDSS xmm0, xmm1` cases check binary32 result bits, preservation of the
other vector lanes and source register, guest MXCSR, and the execution exit.
Both existing BetterXen engines match all nine expectations; Remill matches the
exact `1 + 2` case and fails the other eight through this adapter.

Expected values were specified as literal constants before the lifted code ran.
The midpoint cases use `1 + 2^-24` or `-1 + -2^-24`. Positive round-up must choose
`3f800001`; negative round-down must choose `bf800001`. Inexact results set the
precision flag, and enabling denormals-are-zeros changes a subnormal input to
zero. MXCSR was initialized in Remill's `state.x87.fxsave64` model for each case.
The host rounding mode was checked as round-to-nearest.

| Case | Expected low 32 bits / MXCSR | Remill low 32 bits / MXCSR | Comparison |
| --- | --- | --- | --- |
| exact | `40400000` / `1f80` | `40400000` / `1f80` | match |
| tie-nearest | `3f800000` / `1fa0` | `3f800000` / `1f80` | mismatch |
| tie-down | `3f800000` / `3fa0` | `3f800000` / `3f80` | mismatch |
| tie-up | `3f800001` / `5fa0` | `3f800000` / `5f80` | mismatch |
| tie-zero | `3f800000` / `7fa0` | `3f800000` / `7f80` | mismatch |
| negative-down | `bf800001` / `3fa0` | `bf800000` / `3f80` | mismatch |
| denormal | `00000001` / `1f82` | `00000001` / `1f80` | mismatch |
| denormal-daz | `00000000` / `1fc0` | `00000001` / `1fc0` | mismatch |
| precision-unmasked | `3f800000` / `0fa0` | `3f800000` / `0f80` | mismatch |

The unmasked-precision case must also stop with the guest SIMD exception, leaving
the destination unchanged. BetterXen reports that exit; the Remill trace returns
normally. The sticky-flag failures matter even when result bits happen to agree.

The primary reference is [Intel SDM Volume 1, December 2024](https://cdrdv2-public.intel.com/843827/253665-sdm-vol-1-dec-24.pdf),
sections 10.2.3.1–10.2.3.4 and 11.5.2.6, plus Volume 2's ADDSS entry. The generated
`addss.ll` contains a plain LLVM `fadd` and no guest MXCSR update. That supports
the inference that this route needs additional guest floating-point modeling;
merely lowering the current lifted IR to ARM64 is insufficient. This experiment
did not test other floating-point instructions, NaNs, flush-to-zero or x87.

## Vector and execution findings

- `PADDD xmm0, xmm1` matches four independently calculated wrapping integer lanes
  in all three paths, preserves the source, and leaves MXCSR unchanged.
- Remill's legacy PADD preserves the tested YMM upper half; `VPADDD ymm0, ymm0,
  ymm1` matches eight lanes; VEX.128 `VPXOR xmm0, xmm0, xmm0` zeroes the tested full
  YMM destination. These are three narrow Remill state checks, not AVX2 coverage.
- The existing interpreter and DBT explicitly reject both VEX fixtures without
  changing the checked XMM state. No production AVX support was added.
- A backward `INC/DEC/JNE` loop terminates with the hand-derived result and flags
  for counts 1, 3 and 7 in all three paths.
- For the same loop starting at count zero, both existing engines return their
  32-instruction budget exit. The Remill route does not yield a guest budget
  exit; the supervisor terminates its process group after one second. A process
  timeout does not satisfy the guest instruction-budget contract.

## Exploratory measurements

**The desktop was active. These are diagnostic toy-workload measurements, not
an idle-machine benchmark campaign, acceptance gate G10 or a general speedup.**
The earlier exploratory receipt is retained rather than selecting a favorable
run. No production optimization was made.

Both workloads start RAX and RCX at one, execute 256 additions, store the final
value and return. `register256` stores once; `memory256` stores after every add.
The same guest bytes are used by every path. All timed repetitions check RAX
257, the stored value, six arithmetic flags, stack movement and memory sentinels.
Each path performs one untimed correctness/warmup run, then nine batches of
200 repetitions, rotating engine order between batches. Warm DBT samples assert
zero new translations and positive cache hits.

Values below are microseconds per guest invocation: median, with minimum and
maximum batch averages in parentheses. They describe the final recorded run.

| Workload | Interpreter | Warm DBT | Cold DBT | Remill AOT |
| --- | ---: | ---: | ---: | ---: |
| register256 | 4.531 (3.907–11.857) | 0.552 (0.481–0.977) | 19.482 (17.350–44.555) | 0.959 (0.824–1.774) |
| memory256 | 8.310 (8.057–14.108) | 2.716 (2.639–4.356) | 47.383 (45.075–51.644) | 1.862 (1.796–3.760) |

Cold DBT creates an engine and translates each invocation. Remill is compiled
before these samples; its lift/lower process durations are separate receipt
entries, so the cold-DBT column must not be compared as equal preparation work.
The bridge uses release mode, fat LTO and one codegen unit, matching the project's
release profile; Remill objects use LLVM `llc -O2`, and the harness uses C++ `-O2`.
The interpreter and DBT also execute a terminal sentinel HLT after RET; the Remill
harness validates the return sentinel directly.

These timings include resetting CPU state, checked-memory access and validation.
State sizes, return plumbing and per-instruction budget checks differ, and Remill
still calls predicate-annotation helpers. They are not isolated instruction
throughput or equally featured production runtimes. Background activity and
these adapter costs limit any performance conclusion. Warm DBT is faster on
this register trace; Remill is faster on this store-heavy trace in the recorded
run. There is no demonstrated overall performance winner.

Remill object text is 48,288 bytes for `register256`
and 58,488 for `memory256`; warmed DBT reports
6,396 and 38,852 emitted native bytes respectively. These are code-size counters
for different glue/dispatch strategies, not full process memory or cache-use
measurements. Full-workload memory consumption remains unmeasured.

## Reproduce and next decision

```sh
python3 experiments/remill/evaluate.py --root /path/to/ignored/artifacts/remill-evaluation --extended
```

This command currently returns nonzero because the semantic and budget gates
fail. It still records the separately correctness-gated integer measurements.
`receipt-extended.json` contains command exit statuses, source hashes, assembled
bytes, observations, raw timing samples, summaries and the executable hash.
`extended-prototype-run.log`, `extended-benchmark.log` and
`extended-runaway-budget.log` retain the actual outcomes. The first extended
receipt and the intermediate exit-mapping diagnostic receipt are also retained.
The latter exposed a harness status-mapping omission; a distinct illegal-opcode
status fixed the reporting without altering either existing CPU engine or the
rejection expectations.

Before considering production use, implement and independently review a guest
floating-point contract and instruction-budget/dispatch adapter. Preserve the
known failing inputs as regressions. Then expand the workload corpus and run
matched, idle-host performance/memory measurements with equivalent guarantees.
No engine replacement, guest API change, game execution, GPU verification or
independent sign-off follows from these results. This review branch publishes
the isolated experiment sources and reports; raw artifacts remain local.

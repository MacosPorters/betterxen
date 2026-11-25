# CPU test runtime, shards and coverage map

This page records what the CPU test gate costs, describes
`scripts/cpu-tests.sh`, which splits that gate into deterministic shards, and
maps which x86-64 and loader behavior the tests verify. The shards are a
convenience for local iteration and parallel runners. They do not replace any
gate: `cargo test --locked --workspace` stays the full CPU gate, and
`scripts/test-gpu.sh` stays the GPU gate.

## What counts as a CPU test

A CPU test is any test target that `cargo test --locked --workspace` builds with
default features. Targets that need a Metal device are either behind the
`gpu-tests` feature (`xenora-metal`, `xenora-loader`, `xenora-ffi`) or marked
`#[ignore = "requires a native Metal device ..."]` (for example
`tests/validation/known_answers.rs`), so the default run does not execute them.
`scripts/test-gpu.sh` runs them. The selection uses Cargo's own target list, so
it never decides by file name which tests are CPU tests.

One exception: two `xenora-ffi` library tests
(`test_resource_management_lifecycle_and_validation` and
`test_deferred_resource_retirement_waits_for_in_flight`) call
`xenora_session_create_texture`. When a Metal device is available, that
function creates one and uploads a texture; when none is available, it
continues without one. So on a Mac these default tests do touch Metal, and they
are not GPU verification. The selection keeps them because the full gate runs
them.

## Usage

```sh
scripts/cpu-tests.sh --list         # every default-feature test target, sorted
scripts/cpu-tests.sh --shard 2/3    # run shard 2 of 3
scripts/cpu-tests.sh --check 3      # show that 3 shards cover every target once
```

Targets are sorted by package, kind and name. Shard K of N takes each target
whose zero-based index i satisfies i mod N = K - 1. Shard 1 also runs the
workspace doctests. A new target joins a shard without any edit to the script.
The script exits 1 when any selected target fails or when Cargo cannot build the
workspace. Partial target lists never run. It exits 2 on bad arguments.
It uses at most four Cargo build jobs unless `CARGO_BUILD_JOBS` says otherwise.

## Measurements

Host: Apple M4 (10 cores), macOS on ARM64, `cargo 1.100.0`, target
`aarch64-apple-darwin`, debug test profile, main at `d7f7d58`. All runs used a
warm build unless noted. These are single-host samples, not benchmarks.

Full gate, `cargo test --locked --workspace`, three runs: 40.47 s, 39.99 s and
40.12 s wall time. It ran 82 test targets plus doctests (92 `test result` lines):
681 passed, 0 failed, 34 ignored.

Running each target alone through `cargo test -p <package> <selector>` adds up
to 79.5 s, because every invocation pays Cargo start-up. The five most expensive
targets, one sample each:

| Target | Wall time | Passed / ignored |
| --- | --- | --- |
| `xenora-loader --test runtime_services` | 19.13 s | 23 / 0 |
| `xenora-loader --test cpu_known_answers` | 8.18 s | 42 / 5 |
| `xenora-loader --test probe_delay_imports` | 3.53 s | 8 / 0 |
| `xenora-loader --test probe_truncation` | 3.32 s | 1 / 0 |
| `xenora-ffi --lib` | 2.16 s | 18 / 0 |

`runtime_services` alone is about a quarter of the summed cost. Any shard that
contains it takes at least about 19 s. The next step for a faster gate is
profiling that target, not finer sharding.

Shard runs, one sample each, sequential on the same host:

| Selection | Wall time per shard | Passed | Failed | Ignored | Result lines |
| --- | --- | --- | --- | --- | --- |
| 2 shards | 37.98 s, 4.55 s | 360 + 321 = 681 | 0 | 21 + 13 = 34 | 51 + 41 = 92 |
| 3 shards | 6.54 s, 29.55 s, 6.27 s | 160 + 282 + 239 = 681 | 0 | 9 + 12 + 13 = 34 | 38 + 27 + 27 = 92 |

For both shard counts, the union of the shards matches the full gate exactly
in passed, failed, ignored and result-line counts. Round-robin by name does
not balance cost: with one dominant target, parallel shards finish no sooner
than the shard that holds it. Running the shards one after another costs more
than the full gate, because each target pays Cargo start-up.

## Verification of the script

- `--check N` for N = 1 to 4 reports that all 82 targets are covered once.
- `--check 3` fails when the selection expression is changed to skip a
  residue or to repeat a target.
- `--list` exits 1 with an error when a crate fails to compile, instead of
  printing a shortened list.
- Bad arguments (`--shard 0/2`, `--shard 3/2`, `--check x`) exit 2.

## Limitations

- Timings come from one machine and few samples. Re-measure before using them
  for decisions.
- Shard membership changes when targets are added or renamed. Shards from two
  different commits are not comparable.
- The experiment workspace `experiments/x1` is outside the selection. CI runs
  it separately.

## CPU and loader coverage map

This map inventories what the CPU and loader tests verify, per operation
family, with the exact evidence commands and counts behind every number. It is
documentation of verified state, not a compatibility claim, and it contains no
percentages: every figure below is a count that a recorded command produced on
one machine. The behavioral rules behind the map (how forms are declared, what
"implemented" means, which forms are rejected on purpose) are documented in
[docs/cpu/instruction-registry.md](../cpu/instruction-registry.md) and are not
repeated here except where the map adds evidence.

### Method and revision

All commands ran on Apple M5, macOS 26.6.2, native arm64, Rust
`1.100.0-aarch64-apple-darwin` (the revision CI pins), `CARGO_BUILD_JOBS=2`,
on the branch holding this change, based on upstream `main` at
`67b8cff` (2026-11-25).
Recorded evidence commands:

```sh
python3 crates/loader/generate_x86.py validate   # 213 operations, 403 forms, 498 opcode slots
python3 crates/loader/generate_x86.py --check    # x86_table.rs is fresh
PYTHONPATH=scripts python3 -m qa cpu-inventory   # advisory partitions, cross-checked below
cargo test --locked --workspace                  # 116 result lines: 1015 passed, 0 failed, 34 ignored
```

The registry numbers are the generator's own count, not a hand count. The
advisory `qa cpu-inventory` output was cross-checked against the code before
use, and one stale row is called out under "Intentionally rejected forms".
The default run recorded 34 ignored tests.

| Count | Category | Files |
| --- | --- | --- |
| 26 | Native Metal correctness | Five in `crates/loader/tests/` (`dbt_render.rs`, `graphics_d3d12.rs`, `graphics_textures.rs`, `runtime_x15.rs`), seven in `tests/validation/known_answers.rs`, and fourteen across `tests/validation/tests/` (`operand_domains.rs`, `sop1_unary.rs`, `sop2_arith.rs`, `sop2_stage_c.rs`, `sopc_stage_d.rs`, `sopk.rs`, `vop1_integer.rs`, `vop2_integer.rs`, `vop2_vcc.rs`, `vop3_compare.rs`, `vop3_integer.rs`, `vopc_f_t.rs`, `vopc_wide.rs`). |
| 5 | LLVM AMDGPU oracle | Two in `crates/decoder/tests/gfx1030.rs` and one each in `crates/isa/tests/extension_word_negative.rs`, `meta_corpus.rs` and `meta_llvm.rs`. |
| 3 | Manual CPU timing | Warm guest memory in `crates/loader/src/memory.rs`, relocation in `crates/loader/src/reloc.rs` and TLS templates in `crates/loader/src/tls.rs`. |

The two manual GPU timing tests in `crates/metal/tests/native_compute.rs`
are outside these totals. Cargo requires the `gpu-tests` feature to build
that target.

### Declared instruction inventory

`crates/loader/x86/instructions.toml` declares the decodable x86-64 surface.
`generate_x86.py validate` reports **213 operations, 403 forms, 498 opcode
slots** at this revision. By opcode map the slots split 235 one-byte and 263
`0F`; by mandatory prefix 352 none, 107 `66`, 16 `F2`, 23 `F3`; by operand size
96 byte (`b`), 139 variable 16/32/64 (`v`), 160 XMM 16-byte (`x`), 58 no
operand (`n`), 22 32/64-only (`y`), 23 64-bit (`q`).

A declared form is decodable, not implemented. A form counts as supported only
when it has semantics and fixture evidence. The fixture half is enforced:
`every_declared_form_is_executed_by_a_fixture` in
`crates/loader/tests/x86_integer.rs` decodes every line of the three assembly
corpora (`tests/x86/integer.txt`, `vector.txt`, `float.txt`) and fails if any
declared form is never executed. It lists the operations that are covered
elsewhere instead (control flow and exits, `LDMXCSR`, and the
`MOVS`/`STOS`/`CLD`/`STD` string family, which need their own memory layout and
are covered form by form in `tests/cpu_rep.rs` and the loop tests).

### Engine path per operation

Of the 213 declared operations, **93 have at least one ARM64-lowered form**.
Some other forms of those operations still use the shared interpreter helper.
**119 operations use the helper for every translated form**, and **one**,
`SYSCALL`, is decoded but refused by `is_translatable` in the DBT. Helper
calls count toward `DbtMetrics::interpreter_fallbacks`. The conditional gates
in `is_native` in `crates/loader/src/dbt.rs` apply at form level.

- `ADD`, `SUB`, `CMP`, `AND`, `OR`, `XOR`, `TEST`, `INC`, `DEC`, `NEG`, `NOT`
  and `CMOV` are native only at 32- and 64-bit width; 8- and 16-bit forms use
  the shared semantics.
- `ADC` and `SBB` are native only for register and immediate operands; a memory
  destination stays on the helper path.
- `BT` is native only in the `0F A3` register-index form; the `0F BA /4`
  immediate-index form can read a memory operand and uses the helper.
- `BSF` and `BSR` are native at 16/32/64-bit width.
- `POP` is native only in the opcode-register form.

`tests/x86_integer.rs` asserts the split both ways for every fixture: native
forms must never call the helper, and helper forms always must. The interpreter
runs all 213 operations through the shared semantics.

### Test evidence by artifact

Counts are from the full gate run recorded above (target name, tests run,
passed/ignored). Expectation sources are named so independence can be judged.

| Artifact (under `crates/loader/tests/`) | Expectations derived from | Run result |
| --- | --- | --- |
| `x86_integer.rs` + `x86/integer.txt` (1230 fixture lines) | LLVM-assembled encodings; byte-level references written from the SDM/APM operation text, independent of the production semantics | 16 passed, 0 failed |
| `x86_vector.rs` + `x86/vector.txt` (374 lines) | same method, SSE2 packed forms | 9 passed |
| `x86_float.rs` + `x86/float.txt` (172), `x86/float_vectors.txt` (6824), `x86/float_reference.py` | exact-fraction reference computed by script, following SDM Vol. 1 floating-point rules | 9 passed |
| `cpu_known_answers.rs` (50 tests, 838 named case lines) | Intel SDM and AMD APM sections cited per case; both engines run each case | 50 passed, 0 ignored |
| `movsxd.rs` | SDM operation text; modular arithmetic in the test; LLVM-assembled encodings | 9 passed |
| `x86_atomic.rs` | SDM LOCK/XADD/CMPXCHG rules; both compare outcomes, unaligned operands, faults | 5 passed |
| `x86_carry_chain.rs` | widening `u128`/`i128` reference arithmetic on 256-bit chains | 8 passed |
| `x86_prefix_length.rs` | SDM prefix and length rules | 8 passed |
| `cpu_loop.rs` | a loop-model reference over every count, match point and budget | 3 passed |
| `cpu_rep.rs` | a byte-array model over every MOVS/STOS form, direction and count, plus step accounting and faults | 7 passed |
| `cpu_features.rs` | CPUID feature-bit pairing per the SDM; each reported bit requires the feature's fixtures, each commonly-tested unreported bit requires a refused witness | 4 passed |
| `cpu_differential.rs` | interpreter and DBT must agree on shared fixtures | 5 passed |
| `arm64_encodings.rs` | LLVM encodings for the assembler's emitted ARM64 | 3 passed |
| `memory_page_boundaries.rs`, `memory_validation.rs` | mapping/permission/boundary contracts | 8 + 9 passed |
| `generated_conformance.rs`, `bootstrap.rs`, `gate_a1.rs`, `gate_a2.rs`, `indirect_control.rs`, `bswap.rs`, `dbt_render.rs`, `process_exit.rs` | per-file documented contracts (generated conformance vectors, startup gates, control flow) | all passed except one Metal-gated ignore in `dbt_render.rs` |

Count named known-answer rows, including both byte-encoded and decoded cases,
with the following command. This excludes section directives, format comments
and Rust match arms.

```sh
rg -c '^[A-Za-z0-9_]+:.*=>' crates/loader/tests/cpu_known_answers.rs
```

The cache revalidation change in [PR #269](https://github.com/MacosPorters/betterxen/pull/269)
restored `dbt_executes_modified_code_after_a_jump`. It passed in this run,
and `cpu_known_answers.rs` reported 50 passes and zero ignores.

### Operations covered by model tests instead of the fixture corpus

The mnemonic-level fixture scan behind the advisory
`PYTHONPATH=scripts python3 -m qa cpu-inventory` reports no assembly-text
fixture for eight operations: `cld`, `std`, `jrcxz`, `loop`, `loope`,
`loopne`, `movs`, `stos`. That is by design, not a gap: those operations need
their own memory layout or counter model and are covered form by form in
`tests/cpu_rep.rs` (MOVS/STOS/REP, the direction flag set by `STD` and cleared
by `CLD`) and `tests/cpu_loop.rs` (LOOP/LOOPE/LOOPNE, JRCXZ on zero RCX). The
declared-form enforcement test also exempts `LDMXCSR`, `Jcc`, `JMP`,
`CALL`, `RET`, `HLT` and `SYSCALL`. It checks the remaining forms against the
corpora.

### Intentionally rejected forms

The rejection list with its scope rules (LOCK limited to the memory forms of
XADD and CMPXCHG, every x87 instruction except FNINIT,
LODS/CMPS/SCAS/REPE/REPNE, MMX, RCP/RSQRT approximations, SSE3 and later,
VEX, address-size overrides, 16-bit stack/branch forms, 16-bit BSWAP and
MOVSXD, bit tests of memory by register index, CMPXCHG8B/16B, and the DBT
refusing raw SYSCALL) is maintained in
[docs/cpu/instruction-registry.md](../cpu/instruction-registry.md) and is
tested by refusal cases inside the files above; this map does not duplicate it.

One advisory-tool discrepancy is recorded for its owner: the hand-written
`unsupported_catalog` in `scripts/qa/cpu_inventory.py` lists "RCL/RCR/SHLD/
SHRD", "string instructions" and "atomics/fences", which is stale against the
code and tests at this revision — RCL/RCR/SHLD/SHRD are declared registry
forms with implemented semantics (`x86_exec.rs`), 9 named `rcl*` cases and 8 named `shld*` or `shrd*` cases using the rule above,
and 72 SHLD/SHRD assembly fixture lines; MOVS/STOS and the
LOCK forms of XADD/CMPXCHG and the fences are supported within the documented
scope. The catalog is advisory only and does not feed any gate.

### Map limitations

- Every count is pinned to the revision, host and single run named above.
- The fixture evidence is mnemonic-level where it comes from the assembly
  corpora: it proves every declared form executes somewhere, not that every
  encoding variant of a form has its own expected-value case. The known-answer
  files carry the per-case expectations; their case lines are counted, not
  audited one by one here.
- The native/shared partition is read from `is_native` at this revision and
  asserted by the fixtures; it is not a promise for other revisions.
- Nothing here was compared with x86 hardware, no compatibility percentage is
  implied, and this is local documentation evidence, not independent
  acceptance.

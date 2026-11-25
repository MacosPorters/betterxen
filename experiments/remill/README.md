# Remill native ARM64 experiment

Issue [#324](https://github.com/MacosPorters/betterxen/issues/324). This is an
isolated evaluation, with no production CPU or workspace dependency changes.
It uses original fixture bytes and harness code. Remill and its dependencies
remain external under ignored `artifacts/`; their notices are retained there.

## Decision and local result

**Keep the existing translator. The extended Remill adapter fails tested
floating-point and instruction-budget requirements.** See the
[extended evaluation](evaluation-2.md) for the failing cases, vector/control
results and exploratory measurements.
Remill built natively and the small lifted integer workload executed correctly
on this host. This establishes a usable experiment, not a production backend.
Broader instruction semantics, representative performance and runtime
integration remain unestablished.
Commercial Xbox execution remains unsupported.

The experiment also passed Rust formatting, strict Clippy for the standalone
bridge and its targets, C++ compilation with `-Wall -Wextra -Werror`, Python
syntax checks, local documentation-link checks and `git diff --check`. Full
workspace/GPU suites, fuzzing, mutation campaigns and the production benchmark
suite were not run. The extended report records separate toy-workload timings.

The initial November 25, 2026 prototype on Apple M4/macOS 27.0 (26A428) completed:

| Check | Local result |
| --- | --- |
| Dependencies and Remill | Native ARM64 builds succeeded with LLVM 17.0.6; upstream build warnings remain in the logs. Upstream tests were disabled; the result below comes from the original experiment harness. |
| Original AMD64 fixtures | Five traces lifted to LLVM IR, then compiled to ARM64 Mach-O objects with LLVM. Guest x86 bytes were never executed as host code. |
| Arithmetic | Six ADD/store/return cases passed through Remill, BetterXen's interpreter and its ARM64 dynamic binary translator (DBT). All three matched hand-derived RAX, carry/parity/auxiliary-carry/zero/sign/overflow flags, stored data and stack movement. |
| Checked memory | Four cases passed through all three paths: unmapped read, unmapped write, read-only write and an eight-byte write crossing the end of a mapping. Each stopped at guest RIP `0x1000`, preserved the checked registers and flags, and left both data words and the read-only sentinel unchanged. |
| Unsupported control | Remill's UD2 and indirect jump traces stopped explicitly in the harness. This does not implement guest exception delivery or indirect dispatch. |
| Wrong-answer control | Changing the first expected sum makes the harness fail with exit 1, as required. |
| Aggregate | 32 case/path checks passed: `(6 arithmetic + 4 memory) × 3 engines + 2 Remill control rejections`. |
| Native audit | `file`, `lipo -archs` and `otool -L` receipts cover the native tools, linked non-system dylibs, dependency archives, generated objects and final executable. macOS shared-cache libraries are identified separately. |

Host execution evidence above is **class A: direct local observation on this
Apple M4**. It is not Xbox hardware evidence, independent R1/R2 acceptance,
an M1–M5 compatibility result or a performance result. No GPU tests were run;
this is a CPU experiment. The author of this harness does not certify it.

The default Rust 1.94.1 build was refused by BetterXen's Rust 1.100 requirement;
an exploratory `--ignore-rust-version` attempt also failed on `atomic_try_update`.
The successful run uses the separately installed native Rust 1.100.0, without
changing BetterXen's requirement or implementation. Both failures remain logged.

## What the prototype does

`evaluate.py` checks the pinned Remill source, audits native inputs, optionally
builds dependencies/Remill, lifts five explicit byte strings and lowers the
resulting IR to ARM64. Only entry symbol names and x86 code-generation attributes
are adjusted; instruction bodies are retained. The original IR and adjusted IR
are both kept for inspection. LLVM receives the native host target explicitly.

`prototype.cpp` supplies the checked 64-bit memory intrinsics, a validated return
sentinel and explicit rejection hooks. Its flag annotation functions return the
predicate already computed by the lifted semantics. Unimplemented intrinsic
symbols cause a link failure. No generic no-op fallback is supplied.

`bridge/` is a separate, non-publishable Rust crate. It calls the real
`xenora_loader::memory::GuestMemory` methods and runs the same bytes through
BetterXen's existing interpreter and DBT. It is not a second memory mock. Its C
ABI accepts trusted harness pointers only, and is not a public guest interface.
The C++ error escape happens after a Rust memory call has returned; it does not
unwind through a Rust frame. This single-threaded harness cannot be used as a
general runtime error adapter.

The arithmetic program is `add rax, rcx; mov [rbx], rax; ret`. The six input pairs
cover an ordinary sum, unsigned wraparound, positive signed overflow, two
negative operands overflowing to zero, an auxiliary carry and zero. Expected
values are literal, hand-derived constants in the harness, independent of the
three execution paths. The Intel instruction reference is the semantic source:
[Intel Software Developer's Manuals](https://www.intel.com/content/www/us/en/developer/articles/technical/intel-sdm.html),
Volume 2, ADD/MOV/RET/UD2. These expectations still require independent review.

## Reproduce

Use native ARM64 macOS, Rust 1.100.0, CMake, Ninja, Git, Python 3 and native
Homebrew LLVM 17. Keep all source/build products beneath an ignored artifact
directory. The dependency builds can download a substantial Ghidra source tree.
The driver bounds each build to 1,800 seconds and limits parallel builds to two
jobs. It does not install system packages or change shell configuration.

From this worktree's root:

```sh
TASK_ROOT="$PWD/artifacts/remill-evaluation"
mkdir -p "$TASK_ROOT"
git clone https://github.com/lifting-bits/remill.git "$TASK_ROOT/upstream"
git -C "$TASK_ROOT/upstream" checkout --detach 66c488c9911fd9b368e4d0c9647d74239f8bba8b
python3 experiments/remill/evaluate.py --root "$TASK_ROOT" --build
```

For an already built artifact directory, omit `--build`. The `--llvm` argument
can select another installation of LLVM 17. Commands, exit statuses, native
architecture/linkage checks, source hashes and the actual harness output go to
`receipt.json` and individual logs in that directory. A failed stage leaves a
failed receipt and raises an error; absent prerequisites never count as a pass.

Raw receipts and generated files remain in the ignored artifact directory
selected with `--root`. The full build-driver receipt is `receipt-build.json`;
`receipt.json` records the initial prototype and `receipt-extended.json` records
the failing extended evaluation. This review branch includes source, reproduction
instructions and scoped findings; it does not include upstream build trees,
executables, raw gate logs or machine-specific paths. Recorded results are local
evidence, not independent acceptance.

## Adapter design

[Proposed correctness adapters](adapter-design.md) specify exact numerical-helper
commit rules, per-instruction dispatch, a versioned result layout and regression
gates. This is a design deliverable; the adapters are not implemented and the
existing extended gate still fails.

## Requirements before adoption

1. **State and faults:** define conversions for all CPU state, precise memory
   fault metadata, partial/restartable instructions, exceptions and budget exits.
   This harness checks only a small integer state slice and four memory faults.
2. **Execution:** supply bounded block dispatch, indirect calls/returns, service
   interception, cache invalidation, code lifetime and scheduler transitions.
   Ahead-of-time compilation of these five traces establishes none of those.
3. **Instruction coverage:** independently test the actual SSE/AVX forms needed
   by admitted workloads. Validate rounding, MXCSR, NaNs and x87 behavior against
   BetterXen's documented contract; advertised lifting coverage is insufficient.
4. **Memory ordering:** derive atomic/barrier semantics and fault atomicity for
   ARM64 explicitly. No atomics, concurrency or host TSO assumptions were tested.
5. **Measurements:** compare matched workloads, cold compilation, warm execution,
   memory use and code size with the current DBT. The harness recreates engines
   for correctness cases; its elapsed time is not a meaningful speed comparison.
6. **Review and licensing:** obtain independent R1/R2 review and a complete
   file/dependency attribution audit for the precise integration. See
   [the preliminary license record](licenses.md). No release artifact is cleared.

Primary upstream design references:
[Remill README](https://github.com/lifting-bits/remill/blob/66c488c9911fd9b368e4d0c9647d74239f8bba8b/README.md),
[architecture and memory intrinsics](https://github.com/lifting-bits/remill/blob/66c488c9911fd9b368e4d0c9647d74239f8bba8b/docs/DESIGN.md),
[runtime intrinsic declarations](https://github.com/lifting-bits/remill/blob/66c488c9911fd9b368e4d0c9647d74239f8bba8b/include/remill/Arch/Runtime/Intrinsics.h).

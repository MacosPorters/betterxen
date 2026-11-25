# Phase 01 engineering checkpoint

**Historical native record.** Plans and results below retain their original candidate scope. Native development is paused under [ADR 0005](../adr/0005-crossover-hybrid-direction.md); follow the [hybrid architecture](../architecture.md) for active priorities.

**NOT RELEASE-READY.** The evidence below was recorded on 2026-11-22 against the integrated local working tree based on `9ab5bf9`. The release brief requires evidence against one final candidate commit. Every formal gate therefore remains **NOT MET**. A passing local check is identified separately from outstanding work.

| Gate | Local evidence and remaining requirement |
|---|---|
| G1 | `scripts/audit/native.sh` passed. The host reported arm64 with translation off. All 11 Cargo executables or loadable dependencies checked were arm64 and their linkage contained no Vulkan or MoltenVK. Repeat against the candidate and include all candidate artifacts. Log `artifacts/phase01/native-audit.log`. |
| G2 | `cargo fmt --all -- --check`, `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` and `cargo test --locked --workspace` passed. 24 tests passed, with the LLVM oracle opted out of the default run and passed separately. No production C++ exists. Candidate rerun required. |
| G3 | `scripts/test-gpu.sh` passed 12 tests with zero failures or skips on Apple M5. The runtime reported Metal API and GPU validation enabled. Includes original offscreen vertex and fragment MSL with 48 checked RGBA8 pixels. Candidate rerun required. Log `artifacts/phase01/native-gpu-final.log`. |
| G4 | `scripts/demo.sh` passed all three original fixtures with both validation layers on. Exact readbacks are below. Confirm the command from the final candidate checkout. Log `artifacts/phase01/demo-final.log`. |
| G5 | R2 passed four independent CPU tests across widened integer known answers, both SCC outcomes, malformed branch rejection and exact FP32 boundaries. Full independent reference-interpreter assurance and candidate certification remain open. |
| G6 | Automated form coverage is complete. LLVM 23.1.6 and both Xenora decoders matched 303 instructions across all 15 supported forms. Candidate-commit and independent reviewer sign-off remain open. See the [coverage matrix](../shaders/instruction-coverage.md#automated-three-way-comparison). |
| G7 | `cargo mutants --workspace --list --json` restricted to the decoder, compiler and reference sources enumerated 373 source mutants. The source, semantic and oracle campaigns and survivor triage remain pending. No kill rate is available. |
| G8 | `scripts/audit/claims.sh` passed for 15 entries (A=4, B=2, D=6, E=2, U=1). The lint checks registered references and verification state. The audit of all hardware claims in prose remains open. |
| G9 | All three CPU-only targets completed the required 600 seconds serially with no crash, timeout, OOM or leak report: `instruction_stream` 161,115,729 runs, `fixture` 98,204,690 runs and `reference` 176,482,975 runs, 435,803,394 executions in total. Logs are under ignored `artifacts/g9/`. Candidate rerun and R2 sign-off remain. |
| G10 | `scripts/benchmark.sh` builds a release harness that gates every timed fixture on the Rust pipeline, the independent reference and a native GPU correctness check before timing, then reports compile, library, pipeline, GPU and host medians with quartiles. A reps=7 release run with validation off benchmarked all three fixtures at 4 to 4096 words on Apple M5 (`run-1795338391`); the GPU zero-sample fraction was 0%. The host was on battery power, so per protocol this run is **not** release evidence. An idle AC-power rerun remains. |
| G11 | Pinned runtime and fuzz inventories and applicable repository notices are present. The upstream objc2 notice raises an unresolved SDK-derived binding redistribution question. Final independent license approval remains open. |
| G12 | Independent initial history and local output inspection found no matching private key or token patterns. A previously unignored standalone validation build folder is now ignored. Full candidate and history audits remain open. |
| G13 | README and machine-readable status reflect the three supported synthetic fixtures and the native suite. Final independent consistency review remains open. |
| G14 | The CPU-boundary ADR exists and `scripts/audit/dependencies.sh` passed across five graphics crates. Candidate rerun required. Log `artifacts/phase01/dependencies.log`. |
| G15 | Both reviewers completed two checkpoint passes. Final candidate review and closure of all release findings remain open. |

## Native execution evidence

`scripts/demo.sh` ran at 13:36:03 local time on Apple M5 using native Rust 1.100.0, Xcode 27.2 and macOS 27.3. Both Metal validation banners appeared. The script exited successfully and printed disassembly, IR, MSL and comparisons. For each row the Metal readback, independent CPU result and hand-derived answers were equal.

| Fixture | Exact output words |
|---|---|
| `add` | `[305419896, 305419897, 305419895, 0]` |
| `bitwise-branch` | `[305419896, 305419897, 315315847, 318723824]` |
| `float-exact` | `[1091305472, 1091305472, 1092616192, 1092616192]` |

These fixtures use an explicit synthetic binding ABI, statically resolved forward scalar control flow and the verified exact positive normal FP32 domain. Console containers, guest graphics stages and execution-mask divergence remain unsupported. See [fixture hashes and derivations](../../fixtures/manifest.toml), [supported instruction forms](../shaders/instruction-coverage.md) and [subnormal risks](../risk-register.md).

Eight agents performed the requested research, implementation and review assignments in waves. The [R1 review log](review-log.md) and [R2 validation report](../../tests/validation/README.md) retain findings and commands. Runtime and fuzz lockfiles passed RustSec advisory checks in this session. The hosted workflow was created and has yet to run. The 89/92 historical laboratory count is tracked separately in the [lab intake](../research/host-lab.md).

## Assurance sprint, 2026-11-22

The published baseline `96cff7c66e55f1e4693ace19577c207795258d5c` passed its [Native Rust workflow](https://github.com/ranveersoni98/Xenora/actions/runs/45001680455). The local results above remain historical checkpoint evidence.

**G6 remains open.** The LLVM oracle also compares the high-VGPR sequence covered by the independent reference decoder test. This adds three instructions to the LLVM comparison across four streams. The reference decoder still needs automated form-wide comparison of decoded operands, instruction lengths and offsets. Production and reference semantics are unchanged.

**G11 remains open.** The locked runtime dependency declarations are MIT-compatible selections. The three pinned upstream objc2 licensing explanations match the repository copy byte for byte. The full upstream MIT text is now included with its revision. The sufficiency of copyright notices for locked versions and the Apple SDK terms for a linked distributable remain unresolved. See the [runtime redistribution review](../licenses.md#runtime-redistribution-review).

Sprint verification passed `cargo fmt --all -- --check`, all-features workspace Clippy with warnings denied, and 17 tests across decoder, reference and semantic review. The opt-in LLVM test passed separately with 37 instruction comparisons. Claims and dependency-direction checks passed. Both lockfiles passed `cargo audit --no-fetch` against the local advisory database. Local LLVM output is in ignored `artifacts/licensing/llvm.log`.


## Three-way decoder assurance, 2026-11-22

G6 now has automated three-way coverage for the supported gfx1030 subset. The
LLVM-enabled run passed 8 tests, including 303 successful instruction
comparisons across all 15 forms, 3 malformed LLVM probes, and 6 LLVM-decodable
cases rejected by both Xenora decoders. The reference decoder previously
rejected non-fixture MUBUF operands at decode time. It now reports the full
supported MUBUF encoding while retaining the original synthetic ABI guard
in execution. It still has no production decoder or ISA dependency.

Formatting, all-features Clippy, the 24 default workspace tests, fixture
reproduction, claims and dependency checks, 12 native tests, and all three
validated GPU demos passed. The two LLVM tests excluded from the default
workspace run passed in the explicit LLVM run. Logs are under ignored
`artifacts/g6/`. G11 and the other open release requirements are unchanged.

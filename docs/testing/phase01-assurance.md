# Phase 01 assurance checkpoint

**Historical native record.** Plans and results below retain their original candidate scope. Native development is paused under [ADR 0005](../adr/0005-crossover-hybrid-direction.md); follow the [hybrid architecture](../architecture.md) for active priorities.

**NOT RELEASE-READY.** The full source mutation campaign and native
technical evidence in this checkpoint ran from the clean
`aff46b0904a0182a6f0543125e4491ab67270f64` candidate on 2026-11-22. The
exploratory AC benchmark ran at `cc0daa79e63c468e748f0333bd1314224752893a`.
Later changes have separate evidence. Nothing was published from this sprint.

The test machine was a native Apple M5 MacBook Air with macOS 27.3
(26C5047e), Rust 1.100.0, Xcode 27.2 (27B52), and Homebrew LLVM 23.1.6.
Commands, effective environment and exit statuses are in ignored
`artifacts/assurance-aff46b0/{commands,environment,exits}.log`. PASS means the
listed gate has candidate evidence, and FAIL or BLOCKED means it remains
unmet.

| Gate | Status | Evidence and remaining work |
| --- | --- | --- |
| G1 | PASS | `scripts/audit/native.sh` exited 0. Host was ARM64 with translation off. All 15 checked workspace executables or loadable dependencies were ARM64 with no Vulkan or MoltenVK linkage. The release benchmark and dependency graph were checked separately in `additional-linkage.log`. |
| G2 | PASS | Formatting, all-target and all-feature Clippy with warnings denied, and locked workspace tests exited 0. 46 tests passed and two LLVM tests ran separately under G6. |
| G3 | PASS | `scripts/test-gpu.sh` exited 0. 12 Metal tests passed with zero skips and both validation layers active on Apple M5, including 48 offscreen RGBA8 pixels. |
| G4 | PASS | `scripts/demo.sh` exited 0. All three original synthetic gfx1030 fixtures returned the hand-derived outputs and matched the CPU reference. |
| G5 | BLOCKED | The candidate's five validation tests passed. R2 identified and corrected defects in an isolated second-interpreter draft, which passed 10 Rust tests against the candidate crates. Integration and renewed oracle sensitivity checks remain open. |
| G6 | PASS | Both Homebrew LLVM tests passed. The two Xenora decoders and LLVM matched 303 instructions from all 15 supported forms, with three malformed and six out-of-subset probes. R2 signed off on this exact candidate and scope. |
| G7 | FAIL | Full source score 359/361 (99.45%) after R2 review. All 13 raw misses were equivalent and the two non-progress outcomes remain in the denominator. All 12 native semantic mutations were killed. Seven of 44 mutations survived the independent oracle-only campaign. See [mutation testing](mutation.md). |
| G8 | BLOCKED | Candidate claims lint exited 0 with 18 entries. Follow-up claims and generated status checks passed with 20 entries. Complete the independent prose coverage audit on the final candidate. |
| G9 | BLOCKED | Historical logs show 435,803,394 executions across three 601-second campaigns. The verifier change invalidated fixture fuzz coverage. The refreshed candidate fixture campaign was started at 18:44 IST. |
| G10 | BLOCKED | The historical battery and new exploratory AC evidence are preserved. The AC run covered the three declared translated fixtures. Idleness, complete protocol and corpus coverage remain unresolved. See [benchmark protocol](benchmarks.md). |
| G11 | BLOCKED | The updated inventory records six active dependencies. Both lockfiles passed the local advisory scan. Pinned-binding attribution and SDK-derived compiled redistribution rights remain open. See [licensing](../licenses.md#runtime-redistribution-review). |
| G12 | BLOCKED | Strict Git integrity and scans of all 19 reachable commits found no matching private paths, credentials or tracked artifacts. Expected local directories are ignored. Missing contribution attestations remain a policy finding. See `hygiene.log` for exact results. |
| G13 | BLOCKED | README, interfaces and status describe the tested scope. The status generation and consistency check were added after the source candidate. Independent confirmation of the integrated checkout remains. |
| G14 | PASS | `scripts/audit/dependencies.sh` exited 0, checking five graphics crates. The [CPU boundary ADR](../adr/0004-cpu-boundary.md) records the direction and guest address translation contract. |
| G15 | BLOCKED | R1 independently reviewed documentation, licensing, AC benchmark provenance and semantic campaign safety. R2 independently reviewed the source campaign, LLVM evidence and reference assurance. The oracle, benchmark, licensing and final integrated review findings remain unresolved. |

## Commands

```sh
export CARGO_BUILD_JOBS=2
export LLVM_BIN="$(brew --prefix llvm)/bin"
export PATH="$LLVM_BIN:$PATH"
scripts/bootstrap.sh
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo test --locked --workspace
scripts/assemble-fixture.sh --check
cargo test --locked -p xenora-decoder --test gfx1030 -- --ignored --nocapture
scripts/test-gpu.sh
scripts/demo.sh
cargo run --locked -p xenora-semantic-review --example semantic-mutation --features native-mutation
scripts/audit/native.sh
scripts/audit/claims.sh
python3 scripts/audit/status.py --check
scripts/audit/dependencies.sh
cargo audit --no-fetch
cargo audit --no-fetch --file fuzz/Cargo.lock
```

The native readbacks agree with the independent reference and hand-derived
words for all three unmodified shaders.

| Fixture | Readback words |
| --- | --- |
| Add | `[305419896, 305419897, 305419895, 0]` |
| Bitwise and branch | `[305419896, 305419897, 315315847, 318723824]` |
| Exact FP32 | `[1091305472, 1091305472, 1092616192, 1092616192]` |

The semantic mutation suite separately executed 12 deliberately wrong
shaders and required a completed output mismatch after a matching unmodified
baseline. Compile errors, crashes, validation failures and timeouts cannot
count as semantic kills. Game execution, proprietary console containers,
full guest execution-mask divergence and a guest CPU runtime are unsupported.

## Follow-up evidence at 5c47900

The affected campaign finished with exit 0 after 601 seconds and
244,422,168 executions. No crash, hang or sanitizer failure was recorded.
It used native nightly Rust and cargo-fuzz 0.13.2. The transcript SHA-256
is `ea81caf9495b57fc617d6470d3cac0488e59d1d727f16bbe8eda470987333a28`. It ran at aff46b0 with the same fuzz inputs as 5c47900.

### Integrated candidate checks

All commands in `artifacts/assurance-5c47900/commands.log` completed with
exit 0. This includes 54 workspace tests, the separate LLVM oracle, 12
validated GPU tests, three end-to-end demos, 12 semantic mutant kills,
native and dependency audits, and claims and generated status checks.
The oracle-only campaign killed 44/44 in 23 seconds. Each outcome has
a named Rust test failure. All seven prior oracle survivors were killed.
This closes the observed oracle test gaps. The full source score stays
attached to aff46b0 and G7 was open at this checkpoint. The later [integrated campaign](mutation.md#integrated-source-campaign-at-4b9a4ce) records its closure.

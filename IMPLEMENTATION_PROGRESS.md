# Maintainer build implementation checkpoint

## Merge of main f7194d0 and launcher fix (2026-11-26)

- Merged the M1 work from `main`; `abi/`, `shims/`, `shader-transpiler/` match `main`. The earlier
  raise of the minimum Rust version to 1.100 was dropped.
- Fixed (jelly013's report on #425): `bin/wine` discards `WINEDLLOVERRIDES`; now passed with
  `--dll`. Bottle settings also replaced `--backend` (old runner ran vkd3d when asked for
  d3dmetal); now passed with `--env`.
- Rerun: Rust checks, shim build, all `test_*.exe` fixtures in bottle and prefix mode.
- Not rerun: `scripts/hybrid/accept-integrated.sh` (about 12 minutes), including on-screen checks.
- #440 (open) changes `bx-cli` trace output and may conflict. #408 overlaps this launcher work.

## Latest state (resumed by Claude Code, 2026-11-26)

Read this section first; the sections below are the earlier session's record.

- Branch `engineering/maintainer-build-readiness`, worktree `../betterxen-maintainer-hybrid`.
- Commits: `b9469aa` migration (Codex's uncommitted work, backed up under ignored `artifacts/recovery/`), `a9d7bc0` merge of `main` `a51271d`, `1be97f2` acceptance fixes, then the crash-dialog, D3D12 soak and testing-guide commit.
- Verified: fmt, clippy, 68 tests, 16 shim DLLs / 102 exports, both shim fixtures; D3D12 and D3D11 workloads, intended-gate negative controls, lifetime, SM6, keyboard, lifecycle 20/20 and independent image oracles on `1be97f2`; two 20,000-frame D3D12 soaks (known shutdown only) and three D3D11 reruns.
- Findings: D3DMetal swapchain starts with two refs (vkd3d: one); vkd3d path spends 2000 ms per `Present` after ~437 frames; crash hang was Wine's crash dialog (fixed).
- Open: on-screen checks failed because the window was on another macOS Space; one unexplained D3D11 soak stall; one unexplained keyboard-negative failure; independent maintainer reproduction not done.
- Pushed; draft PR https://github.com/MacosPorters/betterxen/pull/425 (CI: both checks passed on `c1e9581`). #405 left open and unmodified.
- Clean clone `../betterxen-maintainer-verify` of `c1e9581` with new llvm-mingw/DXC under `../betterxen-maintainer-deps`: all checks passed except five on-screen captures (screen locked mid-run). Records in that clone's ignored `artifacts/`.
- Private bundle built from `53c4380` (in the clean clone's ignored `artifacts/packages/`); its unpacked `test.sh` gave the same results: all passed except the five on-screen captures (locked screen) and the known D3D12 shutdown.
- Dedicated bottles created by this work: `betterxen-maintainer-407`, `-clean`, `-test`, `-bundle`. The user's `betterxen-test` bottle was not used.
- Next: (1) with the Mac unlocked and a desktop in front, rerun the suite for the on-screen checks; (2) independent maintainer reproduction; (3) D3D11 soak stall if it recurs.

## Current state

- Active branch: `engineering/maintainer-build-readiness`.
- Active worktree: `../betterxen-maintainer-hybrid`, relative to the original checkout; locate it with `git worktree list`.
- Base: merged #406, `f2c42ba1101d345ef45d34cc7ef5f36dee5b33a9`; reviewed head `30e551a161c39337dbe706d48540254f5c982067`.
- #405 was reopened at the maintainer's explicit request. It remains at old-layout head `b42133add913297690dbade1941916f4ce0b666a` in `MacosPorters/betterxen`.
- New issue #407 records the requested integration scope and is claimed. No ABI, shim implementation or new-root runtime source has been changed.
- Maintainer clarified: adapt the candidate to the new architecture merged in #406; preserve #405 separately. No other migration source was identified. D3D12 is primary; D3D11 remains regression/control coverage.

## Preserved implementation and commits

The donor worktree `../betterxen-maintainer-build-readiness` is on `checkpoint/maintainer-readiness-before-406`. It retains all earlier files and ignored evidence. No retired native workspace has been merged into this active branch.

- `a676e4e`: initial signed-off inspection/recovery checkpoint.
- `ff5606a9cb2b4fb0f42ab5dd04f2c098731bc617`: signed-off preservation of the complete pre-transition draft (runtime checks were pending at commit time).
- `2645c42`: bounded output processing and explicit session unlock, with local tests and Clippy subsequently passing.
- Original #384 dirty worktree and unpushed merge remain untouched. Its three-file patch is saved under donor `artifacts/readiness/upstream-uncommitted.patch`; useful frame-query/pin changes were incorporated, duplicate fixes were already in #405.
- Original #405 source/worktree is unchanged; no source branch was overwritten or merged to main. Nothing from this task has been pushed yet.

Donor implementation includes scoped bottle shutdown (no global basename kills), lock/stale-session recovery, isolated environment, output/error reporting, pinned dependency installers, build/private-package scripts, first/final frame capture, BC1 checks, independent image digest oracle, standalone SM6 numerical checks and an original COM lifetime witness. Four new/updated C probes compile as x86-64 PE32+. They are not all graphically verified yet. No excluded console shader compiler was imported.

## Actual verification

### Merged #406 baseline, in this active worktree

Source is `f2c42ba`, before migration edits:

- `cargo fmt --all -- --check`: passed.
- `cargo clippy --locked --workspace --all-targets -- -D warnings`: passed.
- `cargo test --locked --workspace`: 7 passed, 0 failed/ignored.
- ARM64 release `bx-cli` build: passed; `file` and `lipo -archs` identify arm64 only.
- `BX_BUILD_JOBS=2 scripts/build-shims.sh`: passed; 16 PE32+ shim DLLs, 102 exact exports, zero extras/forbidden MinGW runtime imports.
- Both live synthetic fixtures passed in a new dedicated prefix under `artifacts/readiness/m0-prefix`: `test_veh_clwb` reported traps 0 -> 2 and 8192 MB limit / 8192 allocated bytes; `test_shim_exports` resolved 25 checks. `bx-cli trace` reported 4 events, 2 CLWB traps, 0 ADX faults.
- These are narrow fixture results, not Xbox execution or full shim correctness. The CLWB fixture prints the trap count without asserting it, and its two emitted CLWB instructions both lack a REX prefix. FMA is present in disassembly; general AVX2 integer behavior is not separately established.
- Logs: active `artifacts/readiness/m0-*.log`, fixture `.stdout`/`.stderr`, `m0-trace.txt`, disassembly extracts. Frozen `abi/` remains unchanged.

### Donor hybrid code

- At `ff5606a`: 60 tests passed, 0 failed/ignored; full suite 72.96 s, dominated by an output-flood log-processing case.
- Bounded-read change: targeted flood test passed in 1.64 s.
- Subsequent full rerun: 59 passed, 1 failed (session-lock release race); failure log retained.
- Explicit unlock fix: 60 passed, 0 failed/ignored, 5.06 s; strict package Clippy passed. No test was weakened.
- ARM64 release `betterxen-hybrid` built; four original probes built with pinned llvm-mingw (D3D11, D3D12, lifetime, wave host).
- Dedicated new bottle `betterxen-maintainer-407` created; donor doctor checks all passed on CrossOver 26.3 (26.3.0.39832), with 55% free memory against unchanged 50% threshold.
- Lifetime session 15925 completed: create-only, presentation, QI, window-destruction order and resize each retained 12 private-data witnesses after 12 cycles, despite no application command submissions. Detach control passed; deliberately retained application references failed as expected. Backend was requested as D3DMetal but the minimal probe prints no adapter identity, so actual backend attribution still needs direct evidence.

### Original protected two-hour soak

Completed without intervention. This was observed completion of the developer-started run, not an independent rerun:

- 844,497 presented frames; 1,414 verified frames; zero pixel mismatches.
- Resource churn, scripted input, bounded working-set and device health gates passed.
- Shutdown failed with `device_refs_after_release=2`; classification `failed_exit_1`. It is not an all-gates pass.
- Duration 7202.736 s; peak sampled workload RSS 97,616 KB. No commercial-game performance inference.
- Summary: active ignored `artifacts/readiness/original-soak-summary.json`; original raw logs stay in the unchanged #405 worktree's `artifacts/soak/`.

## Research and provenance already done

- Reused the 84-entry inventory and audit ledger. All 23 specifically requested upstreams were reachable; public head metadata and selected source review are preserved in donor `docs/research/maintainer-*` and ignored `artifacts/readiness/upstream/`.
- Selected Wine DXGI, VKD3D-Proton, DXMT and NativeX source paths were inspected. No reference implementation was adopted. Wine's 2000 ms presentation-semaphore wait is a hypothesis to test against installed CrossOver, not a confirmed root cause.
- Official CrossOver 26.3 FOSS archive is downloaded in donor `artifacts/readiness/`; targeted extraction/review remains.
- #394/#395 remain excluded from the candidate. Only seven separately reviewed original BetterXen wave-probe files were retained, with bounds/cleanup/build corrections. DXC is checksum-pinned and external; no compiler/proprietary binary is bundled.
- No helper agents were spawned. Maintainer authorization covers incremental signed-off commits, pushing the candidate and a draft PR; no merge/public visibility change is authorized.

## Remaining work, in priority order

1. Adapt to merged #406 as confirmed; preserve #405 independently. Use a single bx-cli execution engine and keep the frozen ABI intact.
2. Finish lifetime runs, D3D12 stall reproduction and graphical/SM6/BC1/first-frame/lifecycle verification of useful donor behavior. Retain failures and independently derived expectations.
3. If carrying #405 functionality into #406 as previously requested, migrate deliberately under `launcher/`, route one `bx-cli` execution engine through the strongest supervisor, preserve probe/shader/trace and prefix/shim/profile/log behavior, keep `abi/` byte-identical, and move authorized workloads to conformance directories with per-test CMake. No old root workspace/native crates or native CI restoration.
4. Retarget reproducible install/build/package/acceptance scripts and extend active hybrid CI. Pin/document any libc dependency with notices and source; do not import excluded shader code.
5. Clean-checkout build and acceptance using newly installed verified dependencies; create private checksummed artifact and `docs/maintainer-testing.md` containing commands actually tested. No artifact is ready yet.
6. Update changelog/index/checkpoint; commit verified increments; push and open/update a draft PR. Independent maintainer reproduction remains a separate unpassed gate.

## Resume exactly

Run `git worktree list`, then inspect status in both worktrees above. Read any latest user clarification before migration. Donor code to retain is on `checkpoint/maintainer-readiness-before-406`; its newest verified source is after `2645c42`. Do not blindly cherry-pick the old workspace. Lifetime session 15925 completed; inspect its JSON logs before rerunning. New-root M0 checks and original inventory need not be repeated absent changes. Never use the original `betterxen-test` bottle. Continue with a dedicated test prefix/bottle, at most four Cargo/CMake jobs total and one GPU workload at a time.

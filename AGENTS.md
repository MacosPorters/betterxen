# BetterXen Engineering & Agent Rules

These rules apply to every human contributor and coding agent (`CLAUDE.md` is a symlink to this file). Strategic decisions are recorded in `docs/adr/0005-crossover-hybrid-direction.md` and `docs/adr/0006-retire-in-tree-native-workspace.md`; the single authoritative roadmap is `docs/roadmap.md`.

---

## 0. Contributor & Agent Task Dispatch (Onboarding & Priority Order)

Whenever a user points their agent at BetterXen to contribute, get started, or pick work (even with a short prompt like *"help me contribute"* or *"what should we work on?"*), walk through preflight and then select work in the **strict 3-tier priority order** below:

1. **Preflight host & tools:**
   - Check `uname -m` (`arm64`), `git`, `gh`, `cargo`, `cmake`, `x86_64-w64-mingw32-g++`, and `python3`.
   - If `mingw-w64`, `cmake`, or `gh` is missing on macOS, ask permission before running `brew install mingw-w64 cmake gh`.
   - Run `gh auth status` (if not logged in, pause and ask the user to run `gh auth login`).
   - Run `cargo build --locked -p bx-cli && target/debug/bx-cli doctor` to report Apple Silicon, Rosetta 2, CrossOver, and toolchain readiness.
   - Verify whether the user has push access or needs a fork (`gh repo fork MacosPorters/betterxen --remote`), and sync with `origin/main`.
2. **Select work in strict priority order:**
   - **Priority 1 (Highest) — Handle your own claimed PRs with requested changes or review feedback:**
     ```sh
     gh pr list --repo MacosPorters/betterxen --author "@me" --state open --json number,title,headRefName,reviewDecision,reviews,statusCheckRollup,mergeStateStatus
     ```
     Inspect comments and reviews (`gh pr view <pr> --repo MacosPorters/betterxen --comments` and `gh api repos/MacosPorters/betterxen/pulls/<pr>/comments`). If any of the user's open/claimed PRs has `CHANGES_REQUESTED`, unresolved reviewer feedback, failing CI, or merge conflicts, **address that PR first before starting new work**: check out the PR branch, resolve every review item, re-run Section 8 checks, push the DCO-signed update (with the user's authorization, per Section 7), and summarize how each comment was addressed.
   - **Priority 2 — Pick an unclaimed open issue:**
     If Priority 1 is clear, query open issues (preferring `status: ready` in the active milestone, then other actionable open issues):
     ```sh
     gh issue list --repo MacosPorters/betterxen --state open --label "status: ready" --json number,title,labels,assignees
     gh issue list --repo MacosPorters/betterxen --state open --json number,title,labels,assignees
     ```
     Inspect comments and open PRs on candidate issues to filter out any issue with an active claim (< 7 days old) or open PR. Present the unclaimed options in plain English (recommending 1–2 good matches) and confirm with the user before coding.
   - **Priority 3 — Review & independently verify open PRs needing review:**
     When Priority 1 is clear and no unclaimed open issues remain (or the user asks to help unblock the queue), query open non-draft PRs by other contributors that still need review/verification:
     ```sh
     gh pr list --repo MacosPorters/betterxen --state open --json number,title,author,headRefName,reviewDecision,isDraft
     ```
     Fetch a candidate PR (`reviewDecision` empty or `REVIEW_REQUIRED`) into an isolated worktree, independently verify its contract and implementation per Section 5, run the Section 8 checks locally (including live CrossOver / D3DMetal runs where available), and draft an honest evidence-backed review for user approval before posting.
3. **Claim & isolate (for Priority 2 issue work):**
   - Post `Claiming — branch: track-<a..f>/<issue>-<slug>` on the chosen issue and create that branch (or `git worktree`) off `origin/main`.
4. **Implement strictly in scope:**
   - Read the full issue body (`gh issue view <issue> --repo MacosPorters/betterxen`) and the relevant contracts in `abi/defs/` and `abi/include/`.
   - Edit **only** files inside the issue's **Exclusive file scope** (plus `CHANGELOG.md`). Add new synthetic C++ conformance tests under `tests/conformance/test_<name>/CMakeLists.txt` + `main.cpp` (auto-discovered by CMake).
5. **Verify locally (including hosts without CrossOver):**
   - Run every command in Section 8 and fix all warnings/failures before stopping.
   - **No CrossOver installed?** The contributor can still run all ARM64 Rust checks, `scripts/build-shims.sh` (cross-compiling the 16 mingw-w64 C++ shim DLLs, building the Rust `bx_shader_bridge.dll` with `cargo`, and verifying the exact exports of all 17 PE32+ DLLs), and `python3 scripts/audit/docs_hygiene.py`. Report honestly in the PR that live CrossOver execution was not run locally and needs verification on a CrossOver host—never report an unrun check as passed.
6. **Review & submit PR:**
   - Present a concise summary of changes and exact verification outputs, and ask explicit user permission before running `git commit -s` (DCO sign-off, zero AI trailers), pushing, and opening a PR (`Closes #<issue>`).
   - After opening the PR, watch CI (`gh pr checks --watch`) until both `Hybrid build and shim checks` and `Repository hygiene` pass.

---

## 1. Direction & Architecture

- **Hybrid-first on Apple Silicon:** CrossOver 26.3+ (externally installed), Wine runtime APIs, Rosetta 2 (`ROSETTA_ADVERTISE_AVX=1`), and D3DMetal 3.0 (subject to license terms) form the external compatibility foundation. DXMT, DXVK, VKD3D-Proton, MoltenVK, and `utmapp/d3dmetal-native` may be evaluated and used where license-compatible and technically justified.
- **BetterXen's in-tree scope:**
  - `abi/` — Frozen cross-DLL C headers (`abi/include/*.h`) and canonical `.def` export tables (`abi/defs/*.def`).
  - `launcher/` — Native **ARM64** macOS Rust CLI (`bx-cli`), PE32+ import classifier (`bx-probe`), and JSONL trace aggregator (`bx-telemetry`).
  - `shims/` — Cross-compiled **x86-64 Windows PE32+** Xbox Game OS shim DLLs (C++20 via `x86_64-w64-mingw32-g++` + CMake, statically linked against GCC/C++/winpthread so Wine bottles need zero extra MinGW runtime DLLs).
  - `shader-transpiler/` — Rust shader container classifier (`dxbc-accc`) and RDNA 2 (`gfx1030`) transpiler pipeline (expanded on demand per the M2 shader-format gate).
  - `tests/conformance/` — Clean-room synthetic x86-64 PE32+ test binaries (auto-discovered via `test_*/CMakeLists.txt`).
- **Retired native-first code:** The earlier native-first workspace was retired from `main` per ADR 0006 and remains preserved permanently at Git tag `native-first-final`. Do not open native-only issues; if hybrid work requires an independent component (such as RDNA 2 decoding), file a focused track issue.
- **No unverified compatibility assumptions:** This is an active engineering foundation, not a verified Xbox game compatibility stack. Do not assume CrossOver, Rosetta 2, or D3DMetal provide Xbox runtime behavior without measurement.

---

## 2. Parallel Track Ownership (5–8+ Concurrent Contributors / Agents)

Every feature issue belongs to **one track** and defines an **exclusive file scope**. Stay strictly inside your issue's file scope to avoid merge conflicts across concurrent contributors and agents:

| Track | Exclusive Directories | Responsibility |
| --- | --- | --- |
| **ABI Gate** | `abi/` | Frozen headers & `.def` files (small `[ABI]` PRs merged first) |
| **Track A** | `launcher/`, `shims/runtime_veh/`, `shims/win32_bridge/`, `scripts/build-shims.sh` | ARM64 CLI/runner (with `env_clear()` isolation), `clwb` VEH trap, ADX tripwire, JSONL telemetry, Win32/PSM forwarders |
| **Track B** | `shims/xmem/`, `shims/xcrdapi/` | `xmem.dll` (virtual/physical/scatter allocator, Lockhart 8–10 GB budget) & `XCRDAPI.dll` (DirectStorage) |
| **Track C** | `shims/d3d12_xs/`, `shims/xg_xs/`, `shims/ffx_bvh_hooks/` | Xbox D3D12 COM proxy → D3DMetal 3.0, `PresentX`, RDNA 2 texture tiling (`XG`), DXR/BVH4 & FidelityFX hooks |
| **Track D** | `shader-transpiler/` | `DXBC`/`DXIL`/`ACCC`/`FSPX` parser, RDNA 2 → DXIL/MSL transpiler, shader cache, `d3dmetal-native` harness |
| **Track E** | `shims/xgameruntime/`, `shims/gameinput/`, `shims/audio/` | Offline GDK runtime, `GameInput.dll` (`@1`), `AcpHal.dll`, `xapu.dll`, `chad.dll` |
| **Track F** | `tests/conformance/`, `docs/` | Synthetic PE32+ conformance tests, integration fixture, benchmarks, documentation |

### Cross-Track `[ABI]` Rule
- For issue-claimed contributor work, stay inside your issue's track directory; if your track needs a new cross-DLL helper or export, open a small `[ABI]` PR touching only `abi/` first so provider and consumer tracks can implement in parallel.
- Maintainer-directed compatibility and startup-unblocking tasks may touch the required shim and `tests/conformance/` directories across tracks without a pre-existing issue claim. They must not overlap another contributor's active claim or open PR without that contributor's or a maintainer's recorded agreement, any new cross-DLL helper or export still goes through an `abi/`-only `[ABI]` PR first, and independent review (Section 5) still applies.
- When adding a new conformance test, create `tests/conformance/test_<name>/CMakeLists.txt` + `main.cpp` — CMake auto-discovers `test_*` directories without editing `tests/conformance/CMakeLists.txt`.

---

## 3. Native Apple Silicon Host & Runtime Isolation

- **ARM64 host binaries:** BetterXen's own host executables (`bx-cli`, etc.) must be native `arm64`-only (`aarch64-apple-darwin`); verify with `file` and `lipo -archs`. Only guest shim DLLs under `shims/` and conformance EXEs under `tests/conformance/` target x86-64 Windows PE32+.
- **Apple Silicon generations:** Support M1 through M5 and future generations via runtime capability detection; do not hardcode single-generation GPU or RAM assumptions (default to Lockhart memory limits on 16 GiB hosts).
- **Child environment isolation:** `bx-cli run` must isolate the child CrossOver/Wine process (`env_clear()` + minimal explicit allowlist; `--inherit-env` opt-in only) so host credentials and stray variables never leak into guest processes.
- **Build concurrency:** Use at most four Cargo/CMake build jobs across simultaneous local builds, with one GPU workload at a time.

---

## 4. Dependencies, Licensing & Provenance

- Before integrating or copying any third-party code: verify the **per-file** license (not just repository metadata), confirm compatibility with `GPL-3.0-or-later` and every selected component, preserve copyright notices, record the upstream repository and exact commit/version, document modifications, and record the entry in `docs/research/upstream-registry.md` and `docs/research/component-reuse-matrix.md`. If licensing is uncertain, do not use it until resolved.
- Never bundle, modify, or redistribute proprietary components (CrossOver binaries, D3DMetal, Rosetta 2, Apple tools) unless their license explicitly allows it. Never bypass licensing or activation requirements of any dependency.
- **Provenance first:** Exclude anything (code, data, documentation, or observations) derived from leaked proprietary material or unauthorized software. A project's license being GPL does not clear material it was built from or for. Projects whose stated purpose depends on leaked or unauthorized builds (including VIctory, PE-Vulkan, KyTx, AnyPS5, and similar efforts) are never adopted as code, shader, asset, or tooling sources without a recorded per-file provenance and license audit. Game-specific patches, asset-specific workarounds, and decryption-related code are never adopted; general Xbox Game OS, Direct3D, DirectStorage, and runtime ABI contracts established from project-authorized builds that meet every Section 6 condition are in-scope compatibility work.

---

## 5. Honesty, Evidence & Independent Verification

- **Never fabricate verification:** Never report a test as passed unless it ran and passed on this revision. Never count a skipped test or unavailable tool as a pass. Never invent benchmark results, API names, encodings, hardware capabilities, or compatibility claims.
- **No silent fallbacks:** Do not hide errors behind silent no-ops. Every unimplemented shim export must call `BX_STUB_LOG(dll, sym)` and every unimplemented COM vtable slot must call `BX_VTABLE_STUB_LOG(iface, slot, method)` so `bx-cli trace` surfaces exact blockers. Do not weaken or delete tests to make them pass.
- **Evidence classes & hardware claims:** Classify evidence per `docs/claims.toml`. Never claim "verified on Xbox hardware" — BetterXen has no Xbox hardware access; agreement with a public specification or Apple Silicon run is not Xbox hardware verification.
- **Independence of verification:** The agent or contributor who writes a component does not independently certify it. Independent reviewers re-derive expectations from primary sources and are not told the implementer's conclusions as ground truth. The same faulty code must never produce both the expected and actual values in a test.
- **Milestone completion:** A milestone is complete only when every exit condition has recorded evidence — never from commit counts, lines of code, or isolated stubs.

---

## 6. Compatibility Evidence & Legal Boundaries

- **Authorized compatibility evidence:** Contributors and agents may use publicly accessible documentation, publicly licensed software, synthetic workloads, and **project-authorized target builds** (via local call-site inspection and runtime diagnostics) to establish undocumented Xbox OS, graphics, memory, storage, and runtime ABI contracts (struct layouts, flags, COM IIDs, vtable slot orders, and expected return/error semantics). Separate direct observations from inferences and verify every implemented contract with a synthetic conformance test under `tests/conformance/`. Such a test shows that a shim matches the recorded inference, not that the inference is correct.
- **Conditions for target builds:** A build is project-authorized only when a maintainer has approved it for compatibility research **and all** of the following hold. Approval is an additional condition, never a substitute: it cannot grant access or license rights that the project does not hold.
  - The person inspecting it obtained it lawfully.
  - Its license terms and applicable law permit the inspection performed.
  - It is not leaked, stolen, confidential, or otherwise distributed without authorization, and neither are the tools used on it.
  - Inspecting it requires no decryption and no circumvention of security, DRM, licensing, or activation controls.

  If any condition is unknown, do not use the build. State in the PR which contracts come from target-build observation, without naming the title or build.
- **Never obtain or use** leaked executables or SDKs, confidential Xbox documentation, restricted console tools, firmware, encryption keys, unauthorized assets or source code, or unauthorized game builds and shader collections. Maintainer approval does not change this.
- **Repository & clean-tree boundary:** Never commit or paste proprietary executables, SDKs, confidential vendor documentation, restricted console tools, firmware, encryption keys, unauthorized assets or source code, proprietary shaders, raw caller memory dumps, executable contents/addresses, private traces, proprietary file names or hashes, personal paths, or secrets into the repository, its Git history, commit messages, issues, or PR descriptions.
- **Legal boundary (`docs/legal.md`):** BetterXen does not circumvent console security or DRM and does not load encrypted or protected content. Validate technical compatibility and safety of user-provided files; do not ask for ownership receipts, accounts, or acquisition provenance as part of file loading.

---

## 7. Claiming, Git Safety & Changelog

1. **Check before coding (open backlog issues):** When picking an unclaimed backlog issue, read every comment and check open PRs so you never claim over an active claim; reply `Claiming — branch: track-<a..f>/<issue>-<slug>` (stale after 7 days without commits/PR). Direct maintainer-assigned tasks do not require a prior issue claim, but still check open issues and PRs first and never take over another contributor's active claim.
2. **Isolate with branches or worktrees:**
   ```sh
   git worktree add ../wt-<issue> -b track-<a..f>/<issue>-<slug> origin/main
   ```
3. **Link & sign off:** Reference any related issue (`Closes #<issue>` when applicable) in the PR description and sign off every commit (`git commit -s`, DCO).
4. **Git safety:** Commit, push, and open PRs only when requested or authorized by the user; never change repository visibility without explicit instruction. Do not add AI attribution or `Co-authored-by` trailers to commits. Preserve the configured Git author and committer identity.
5. **Changelog & docs index:** Read `CHANGELOG.md` before starting and update `[Unreleased]` for every meaningful addition, change, fix, removal, or security update. Update `docs/README.md` whenever documentation is added, removed, or reorganized.

---

## 8. Verification Commands (Run Before Every Handoff / PR)

```sh
# 1. ARM64 Rust workspace (launcher + shader-transpiler)
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace
cargo build --locked -p bx-cli && lipo -archs target/debug/bx-cli   # must print: arm64

# 2. Cross-compile the 16 x86-64 PE32+ mingw-w64 shim DLLs, build the Rust bx_shader_bridge.dll, and
#    build the auto-discovered conformance EXEs
#    (strictly verifies all 17 .edata Export Directories: names, ordinals, forwarders, 0 extra exports, 0 MinGW runtime DLL deps)
scripts/build-shims.sh

# 3. Live synthetic PE32+ conformance tests inside CrossOver 26.3 + Rosetta 2
target/debug/bx-cli run --prefix /tmp/bx-test --shims build-shims/bin --log /tmp/bx-test.jsonl --exe build-shims/bin/test_veh_clwb.exe
target/debug/bx-cli run --prefix /tmp/bx-test --shims build-shims/bin --log /tmp/bx-test.jsonl --exe build-shims/bin/test_shim_exports.exe
target/debug/bx-cli trace /tmp/bx-test.jsonl

# 4. Repository Markdown link & hygiene check
python3 scripts/audit/docs_hygiene.py
```

---

## 9. AI Agent Efficiency & Resource Management

These rules apply to every coding agent, whatever its model, vendor or plan. Saving resources never justifies skipping necessary verification, fabricating or assuming results, weakening a security, legal or provenance safeguard, or bypassing independent review. Where this section and Sections 4–8 appear to conflict, Sections 4–8 win.

- **Spend in proportion:** Keep token consumption, API usage, reasoning effort and compute cost as low as correctness allows.
- **Reuse what is still valid:** Reuse conversation context, repository knowledge, review findings and verification evidence. Evidence is valid only for the revision and environment it was recorded on; re-run whatever a later change could affect.
- **Read narrowly:** Inspect relevant files and incremental diffs instead of rescanning the repository. Batch independent tool calls and GitHub API requests where practical.
- **Delegate sparingly:** Avoid unnecessary subagents, duplicate investigations and overlapping responsibilities. Independent review (Section 5) is not duplicate work.
- **Reasoning effort:** Default to low for routine tasks and for helper subagents; raise it only when technical complexity justifies it or the user asks.
- **Verify without redundancy:** Use targeted checks while iterating and for small changes. This does not replace Section 8 or required CI. Do not repeat builds, tests or CI runs when nothing relevant has changed, and do not poll workflows repeatedly.
- **Communicate concisely:** Report outcomes, blockers and what is needed next; skip lengthy status reports and repeated summaries.
- **Blocked work:** Do not re-review an unchanged PR that is waiting on an unresolved external blocker.
- **Finish authorized work:** Carry an authorized task through to completion, including a merge that is authorized and meets every required approval and check, instead of stopping at intermediate reports. This grants no permission beyond Section 7.
- **Escalate real uncertainty:** Raise genuine doubt about ABI compatibility, memory safety, concurrency, security, GPU synchronization, licensing or architecture with a maintainer rather than guessing.

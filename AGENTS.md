# BetterXen Engineering & Agent Rules

These rules apply to every human contributor and coding agent (`CLAUDE.md` is a symlink to this file). Strategic decisions are recorded in `docs/adr/0005-crossover-hybrid-direction.md` and `docs/adr/0006-retire-in-tree-native-workspace.md`; the single authoritative roadmap is `docs/roadmap.md`.

---

## 0. First-Time Contributor Onboarding

If the user asks to contribute, get started, or pick a task (even with a short prompt like *"help me contribute"*), walk them through this exact end-to-end flow:

1. **Preflight host & tools:**
   - Check `uname -m` (`arm64`), `git`, `gh`, `cargo`, `cmake`, `x86_64-w64-mingw32-g++`, and `python3`.
   - If `mingw-w64`, `cmake`, or `gh` is missing on macOS, ask permission before running `brew install mingw-w64 cmake gh`.
   - Run `gh auth status` (if not logged in, pause and ask the user to run `gh auth login`).
   - Run `cargo build --locked -p bx-cli && target/debug/bx-cli doctor` to report Apple Silicon, Rosetta 2, CrossOver, and toolchain readiness.
   - Verify whether the user has push access or needs a fork (`gh repo fork MacosPorters/betterxen --remote`), and sync with `origin/main`.
2. **Discover & pick an unclaimed issue:**
   - Query ready issues in the active milestone:
     ```sh
     gh issue list --repo MacosPorters/betterxen --state open --label "status: ready" --json number,title,labels,assignees
     ```
   - Inspect comments and open PRs on candidate issues to filter out any issue that already has an active claim (< 7 days old).
   - Present the unclaimed options in plain English (recommending 1–2 good starting issues matching the user's background) and wait for the user to choose one before coding.
3. **Claim & isolate:**
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
- Never edit another track's directory inside a feature PR.
- If your track needs a new cross-DLL helper or export, open a small `[ABI]` PR touching only `abi/` first. Once merged, both provider and consumer tracks implement against the frozen contract in parallel.
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
- **Provenance first:** Exclude anything derived from leaked proprietary material or unauthorized software. A project's license being GPL does not clear material it was built from or for. Projects whose stated purpose depends on leaked or unauthorized builds (including VIctory, PE-Vulkan, KyTx, AnyPS5, and similar efforts) are never adopted as code, shader, asset, or tooling sources without a recorded per-file provenance and license audit. Game-specific, asset-specific, and decryption-related code is never adopted.

---

## 5. Honesty, Evidence & Independent Verification

- **Never fabricate verification:** Never report a test as passed unless it ran and passed on this revision. Never count a skipped test or unavailable tool as a pass. Never invent benchmark results, API names, encodings, hardware capabilities, or compatibility claims.
- **No silent fallbacks:** Do not hide errors behind silent no-ops. Every unimplemented shim export must call `BX_STUB_LOG(dll, sym)` and every unimplemented COM vtable slot must call `BX_VTABLE_STUB_LOG(iface, slot, method)` so `bx-cli trace` surfaces exact blockers. Do not weaken or delete tests to make them pass.
- **Evidence classes & hardware claims:** Classify evidence per `docs/claims.toml`. Never claim "verified on Xbox hardware" — BetterXen has no Xbox hardware access; agreement with a public specification or Apple Silicon run is not Xbox hardware verification.
- **Independence of verification:** The agent or contributor who writes a component does not independently certify it. Independent reviewers re-derive expectations from primary sources and are not told the implementer's conclusions as ground truth. The same faulty code must never produce both the expected and actual values in a test.
- **Milestone completion:** A milestone is complete only when every exit condition has recorded evidence — never from commit counts, lines of code, or isolated stubs.

---

## 6. Legal & Clean-Room Rules

- Use only publicly accessible documentation, publicly licensed software, and authorized synthetic workloads. Never obtain, paste, or commit leaked executables or SDKs, confidential Xbox documentation, restricted console tools, firmware, encryption keys, unauthorized assets or source code, or proprietary game builds and shader collections.
- **Legal boundary (`docs/legal.md`):** BetterXen does not circumvent console security or DRM and does not load encrypted or protected content. Validate technical compatibility and safety of user-provided files; do not ask for ownership receipts, accounts, or acquisition provenance as part of file loading.
- Never place restricted material, personal paths, or secrets in the repository or its Git history.

---

## 7. Claiming, Git Safety & Changelog

1. **Check before coding:** Read every comment on the issue and check open PRs. Never claim over an active claim.
2. **Claim in plain text:** Reply `Claiming — branch: track-<a..f>/<issue>-<slug>` on the issue (earliest claim wins; stale after 7 days without commits/PR).
3. **Isolate with branches or worktrees:**
   ```sh
   git worktree add ../wt-<issue> -b track-<a..f>/<issue>-<slug> origin/main
   ```
4. **Link & sign off:** Put `Closes #<issue>` in the PR description and sign off every commit (`git commit -s`, DCO).
5. **Git safety:** Never commit, push, or change remote visibility without explicit user authorization. Do not add AI attribution or `Co-authored-by` trailers to commits. Preserve the configured Git author and committer identity.
6. **Changelog & docs index:** Read `CHANGELOG.md` before starting and update `[Unreleased]` for every meaningful addition, change, fix, removal, or security update. Update `docs/README.md` whenever documentation is added, removed, or reorganized.
7. **Subagent reasoning:** When spawning helper subagents in automated workflows, keep subagent reasoning effort low unless the user explicitly asks otherwise.

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

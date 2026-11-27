# Contributing to BetterXen

BetterXen is a modular, CrossOver-based Xbox Series (`Scarlett`) compatibility layer for Apple Silicon macOS. Contributions are organized into **6 parallel tracks** so 5–8+ human contributors and coding agents can work simultaneously without merge conflicts.

## Active Work & Track Ownership

The single authoritative plan is [docs/roadmap.md](docs/roadmap.md) (Milestones M0–M6), supported by [docs/architecture/parallel-tracks.md](docs/architecture/parallel-tracks.md) and the ready-to-claim Wave 1 issues in [docs/issues/wave-1-backlog.md](docs/issues/wave-1-backlog.md). The earlier native-first workspace was retired from `main` per [ADR 0006](docs/adr/0006-retire-in-tree-native-workspace.md) and remains preserved permanently at Git tag `native-first-final`.

1. Pick an open unclaimed issue on your track (A–F) using `.github/ISSUE_TEMPLATE/track-handoff.yml`.
2. Check all comments and open PRs, then claim in plain text (`Claiming — branch: track-<a..f>/<issue>-<slug>`). A claim with no commits or PR after 7 days is stale and may be taken over with a reply saying so.
3. Work in a dedicated branch or `git worktree` and stay strictly inside the issue's **exclusive file scope**. Maintainer-directed tasks have a narrow exception ([AGENTS.md](AGENTS.md) §2).
4. If your track needs a new cross-DLL helper or export, open a small `[ABI]` PR touching only `abi/` first. Once merged, both tracks implement in parallel.
5. Add new synthetic PE32+ conformance binaries under `tests/conformance/test_<name>/CMakeLists.txt` + `main.cpp` (auto-discovered by CMake — no shared-file edits needed).

## Development & Verification

Requirements on Apple Silicon macOS: Rust 1.85+ (`aarch64-apple-darwin`), `mingw-w64` & `cmake` (`brew install mingw-w64 cmake`), Python 3.11+, Rosetta 2, and CrossOver 26.3+ (externally installed, never bundled).

Run these checks before opening a PR:

```sh
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace
cargo build --locked -p bx-cli && lipo -archs target/debug/bx-cli
scripts/build-shims.sh
target/debug/bx-cli run --prefix /tmp/bx-test --shims build-shims/bin --log /tmp/bx-test.jsonl --exe build-shims/bin/test_veh_clwb.exe
target/debug/bx-cli run --prefix /tmp/bx-test --shims build-shims/bin --log /tmp/bx-test.jsonl --exe build-shims/bin/test_shim_exports.exe
python3 scripts/audit/docs_hygiene.py
```

Report exact commands, host hardware/macOS/CrossOver versions, and pass/fail/skip counts. Never report a skipped test or unavailable tool as a pass.

## Clean-Room, Licensing & Provenance Policy

- Use publicly accessible specifications, publicly licensed software, independently authored synthetic workloads, and project-authorized target builds only under every condition in [AGENTS.md](AGENTS.md) §6 (maintainer approval alone is not enough). Never obtain or use leaked or unauthorized material, and never submit leaked executables or SDKs, confidential Xbox documentation, firmware, keys, proprietary game binaries, or unauthorized shader dumps.
- Under BetterXen's clean-room policy, anyone who has accessed leaked or confidential Xbox materials must not contribute. Do not post details of restricted material in issues or PRs.
- Before incorporating third-party code, verify **per-file** license compatibility with `GPL-3.0-or-later`, record provenance in [docs/research/upstream-registry.md](docs/research/upstream-registry.md) and [docs/research/component-reuse-matrix.md](docs/research/component-reuse-matrix.md), preserve copyright notices, and update [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md). Public visibility or a GPL license does not clear code derived from leaked material.
- Never bundle or redistribute proprietary components (CrossOver, D3DMetal, Rosetta 2).
- Never claim Xbox hardware verification (BetterXen has no Xbox hardware access) or independent certification of your own work; classify evidence per [docs/claims.toml](docs/claims.toml).

## Pull Requests & DCO Sign-Off

- Branch from `main`, keep changes focused inside your track scope, link the issue with `Closes #<number>`, update [CHANGELOG.md](CHANGELOG.md) under `[Unreleased]`, and update [docs/README.md](docs/README.md) when adding or reorganizing docs.
- Disclose material AI assistance and personally review generated changes. Never include secrets, personal paths, or credentials in patches or logs.
- Sign off every commit under the [Developer Certificate of Origin](https://developercertificate.org/):

```sh
git commit -s -m "Describe the change"
```

See [AGENTS.md](AGENTS.md), [docs/legal.md](docs/legal.md), [SECURITY.md](SECURITY.md), and [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md).

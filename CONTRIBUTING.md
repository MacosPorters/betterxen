# Contributing to BetterXen

BetterXen is a modular, CrossOver-based Xbox Series (`Scarlett`) compatibility layer for Apple Silicon macOS. Contributions are organized into **6 parallel tracks** so 5–8+ human contributors and coding agents can work simultaneously without merge conflicts.

## Active Work & Track Ownership

The single authoritative plan is [docs/roadmap.md](docs/roadmap.md) (Milestones M0–M6), supported by [docs/architecture/parallel-tracks.md](docs/architecture/parallel-tracks.md) and the ready-to-claim Wave 1 issues in [docs/issues/wave-1-backlog.md](docs/issues/wave-1-backlog.md). The earlier native-first workspace was retired from `main` per [ADR 0006](docs/adr/0006-retire-in-tree-native-workspace.md) and remains preserved permanently at Git tag `native-first-final`.

1. Finishing and reviewing existing PRs comes before new work ([AGENTS.md](AGENTS.md) §0.3). For new work, pick an open unclaimed issue on your track (A–F); new handoff issues use `.github/ISSUE_TEMPLATE/track-handoff.yml`.
2. Check all comments and open PRs, then claim in plain text (`Claiming — branch: track-<a..f>/<issue>-<slug>`). A claim with no commits or PR after 7 days is stale and may be taken over with a reply saying so.
3. Work in a dedicated branch or `git worktree` and stay strictly inside the issue's **exclusive file scope**. Maintainer-directed tasks have a narrow exception ([AGENTS.md](AGENTS.md) §2).
4. If your track needs a new cross-DLL helper or export, open a small `[ABI]` PR touching only `abi/` first. Once merged, both tracks implement in parallel.
5. Add new synthetic PE32+ conformance binaries under `tests/conformance/test_<name>/CMakeLists.txt` + `main.cpp` (auto-discovered by CMake — no shared-file edits needed).

## Development & Verification

Requirements on Apple Silicon macOS: Rust 1.85+ (`aarch64-apple-darwin`), `mingw-w64` & `cmake` (`brew install mingw-w64 cmake`), Python 3.11+, Rosetta 2, and CrossOver 26.3+ (externally installed, never bundled).

Before opening a PR, run every command in [AGENTS.md](AGENTS.md) §8. It uses a fresh Wine prefix and log directory per run, so parallel work cannot collide.

Report exact commands, host hardware/macOS/CrossOver versions, and pass/fail/skip counts. Never report a skipped test or unavailable tool as a pass.

## Evidence, Authorization, Licensing & Provenance

[docs/legal.md](docs/legal.md) is authoritative for authorization and confidentiality; [AGENTS.md](AGENTS.md) §4 and §6 give the working rules.

- Use public specifications, publicly licensed software and independently authored synthetic workloads. Target builds and other restricted material may be used only within an active authorization record that grants the specific activity. Affiliation, possession or maintainer approval alone is not authorization.
- Employment by the original developer, publisher or platform vendor is not required for contribution. Authorized external collaborators, contractors, independent developers, maintainers and coding agents may participate within the scope of an established authorization record. Do not demand proof of employment, corporate email or repeated identity verification when that authorization is already established; follow the existing scoped process in [docs/legal.md](docs/legal.md).
- Never obtain or use leaked or unauthorized material, circumvent DRM, security or activation, or submit proprietary binaries, SDKs, header or binary text, confidential documentation, firmware, keys, shader dumps or records. Do not post details of restricted material in issues or PRs.
- Do not send restricted material, or data derived from it, to an external AI service unless the record grants it.
- Confidential access under an authorization does not by itself exclude you from contributing; your confidentiality obligations still apply. Anyone who has accessed leaked or unauthorized Xbox material must not contribute ([docs/legal.md](docs/legal.md#contributor-exposure-and-separation)).
- You sign no attestations beyond the DCO.
- Before incorporating third-party code, verify **per-file** license compatibility with `GPL-3.0-or-later`, record provenance in [docs/research/upstream-registry.md](docs/research/upstream-registry.md) and [docs/research/component-reuse-matrix.md](docs/research/component-reuse-matrix.md), preserve copyright notices, and update [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md). Public visibility or a GPL license does not clear code derived from leaked material.
- Never bundle or redistribute proprietary components (CrossOver, D3DMetal, Rosetta 2).
- Never claim Xbox hardware verification (BetterXen has no Xbox hardware access) or independent certification of your own work; classify evidence per [docs/claims.toml](docs/claims.toml).

## Pull Requests & DCO Sign-Off

- Branch from `main`, keep changes focused inside your track scope, link the issue with `Closes #<number>`, update [CHANGELOG.md](CHANGELOG.md) under `[Unreleased]`, and update [docs/README.md](docs/README.md) when adding or reorganizing docs.
- Label contracts that come from observation or inference, include a synthetic conformance test for each, and cite the authorization record ID if restricted material was involved ([AGENTS.md](AGENTS.md) §6).
- Disclose material AI assistance in the PR description (no AI trailers on commits) and personally review generated changes. Never include secrets, personal paths, or credentials in patches or logs.
- Sign off every commit under the [Developer Certificate of Origin](https://developercertificate.org/):

```sh
git commit -s -m "Describe the change"
```

## Review and Merging

- A PR needs at least one current approval, passing required checks and resolved blocking conversations. Any later push that changes the PR dismisses earlier approvals.
- You do not merge your own PR. A maintainer other than the original author merges it.
- Please leave **Allow edits by maintainers** enabled on fork PRs (GitHub does not offer it for organization-owned forks). Maintainers may then resolve conflicts and push fixes to your branch, and may still merge it.
- A maintainer may re-approve after their own routine fix (conflicts, changelog, minor docs). Substantive changes need approval from someone who did not write them.

The full rule, including what GitHub does not enforce, is in [AGENTS.md](AGENTS.md) §7, "Merge eligibility".

See [AGENTS.md](AGENTS.md), [docs/legal.md](docs/legal.md), [SECURITY.md](SECURITY.md), and [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md).

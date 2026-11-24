# System-runtime agent handoff

**PR review checkpoint:** 2026-11-24T23:40Z (UTC) — no new human feedback on #312 or #316 since 20:01Z
**Owner identity on commits:** Adeeb Rahman `<therealabeeb@icloud.com>` (GitHub: `soggyramennoodle`)  
**Do not add** `Co-authored-by: Cursor` or any AI attribution trailers (`AGENTS.md`).

This file is the pick-up brief for the next agent continuing the system-runtime lane. Read it before editing.

## Resume here

1. Start at `docs/coordination/system-runtime-handoff.md` in `/Volumes/SSD/Xenora`.
2. Check [PR #312](https://github.com/ranveersoni98/Xenora/pull/312) and [PR #316](https://github.com/ranveersoni98/Xenora/pull/316) for conversation comments, review submissions, and inline review comments newer than the PR review checkpoint above. Address any new actionable feedback immediately.
3. If there is no new actionable feedback, continue the remaining work in `/Users/therealabeeb/Downloads/gta6 prompt 1 6.2 sol low.txt`. The SSD copy is `artifacts/system-runtime/implementation-brief.txt` and the work log is `artifacts/system-runtime/ledger.md`.
4. While actively working, recheck both PRs every 10 minutes and sync upstream `main` every 10–15 minutes. Resume prompt work after addressing feedback.

---

## One-line status

Active branch `feat/system-runtime-expansion` is at local tip **`c45e11d`**. The last fetched fork tip is **`c789dc6`**. Review-fix tip for elforeign is **`8a677bc`**. PR **#312** is open with **CHANGES_REQUESTED** (blocking items addressed; awaiting re-review). PR **#316** still **CHANGES_REQUESTED** (fuzz harness fix already replied; awaiting maintainer re-review). Keep expanding runtime **unless** new PR feedback appears — then switch to feedback first.

---

## Test environment note

The SSD is **exFAT** (no hard links, no POSIX permissions). Do **not** set `XENORA_TEST_TMP` to an SSD path: 9 host-file tests fail there for filesystem reasons. Leave it unset (defaults to `/tmp`, APFS).

## Workspace and remotes

| Item | Value |
| --- | --- |
| Workspace | `/Volumes/SSD/Xenora` |
| Fork remote | `fork` → `https://github.com/soggyramennoodle/Xenora.git` |
| Upstream remote | `origin` → `https://github.com/ranveersoni98/Xenora.git` |
| Active branch | `feat/system-runtime-expansion` (tracks `fork/feat/system-runtime-expansion`) |
| Local branch tip | `b0b6733` — Share file position and timestamp policy across duplicate handles |
| Last fetched fork tip | `c789dc6` |
| Last fetched upstream `main` | `3b47c21` (merged into both PR branches 2026-11-24) |
| Toolchain | `CARGO_HOME=/Volumes/SSD/xenora-toolchain/cargo` `RUSTUP_HOME=/Volumes/SSD/xenora-toolchain/rustup` put `…/cargo/bin` on `PATH` |
| Prompt / brief | `/Users/therealabeeb/Downloads/gta6 prompt 1 6.2 sol low.txt` (ownership of Win32/CRT/runtime lane — **not finished**) |

`origin` is upstream; push feature work to **`fork`**, not `origin`, unless the user says otherwise.

---

## Standing goals (from human)

1. Own system-runtime expansion from the prompt (CRT, Win32 services, FS, net, platform) on `feat/system-runtime-expansion`.
2. Sync with upstream `main` every ~10–15 minutes when actively implementing.
3. **PR feedback has priority:** if new human review comments land on open PRs (#312, #316, …), stop feature work and address them first.
4. If there is **no** new feedback on a tick, **continue expanding** the runtime.
5. Do not push/commit without user authorization (user **did** authorize push/PR for the CRT expansion and review fixes).
6. Never claim Xbox/game execution. Never invent passing tests. Follow `AGENTS.md` (ARM64 native, DCO `Signed-off-by`, no AI co-author trailers).
7. **Not our claim:** guest thread scheduler — [issue #323](https://github.com/ranveersoni98/Xenora/issues/323) claimed by `jelly013`. Do not take it.

Suggested reply if asked about #323: *Not us — we’re on system-runtime CRT/FS/Winsock/XAsync. Go ahead and open the PR for #323.*

---

## Open pull requests

### [#312](https://github.com/ranveersoni98/Xenora/pull/312) — primary vehicle

- **Title:** System runtime: host mounts, CRT, XAsync and Winsock loopback  
- **Head:** `soggyramennoodle:feat/system-runtime-expansion` → `main`  
- **State:** OPEN, **CHANGES_REQUESTED**  
- **Reviews:**
  - `ranveersoni98` — earlier FS security CHANGES_REQUESTED (hardenings done; human audit still pending).
  - `elforeign` — 2026-11-24T17:28:55Z CHANGES_REQUESTED (see below). **Reply posted** at tip `8a677bc` addressing blockers.

#### elforeign blocking items — status

| # | Request | Status |
| --- | --- | --- |
| 1 | Remove `Co-authored-by: Cursor` trailers from history | **Done** — `filter-branch` msg-filter; **0** trailers on `origin/main..HEAD`; force-pushed with lease to `8a677bc` |
| 2 | Dead Linux `AT_EMPTY_PATH` unlink + false TOCTOU claims | **Done** — path removed; docs/module/changelog corrected to name-based remove only |
| 3 | Name-based remove race / nlink post-check | **Done** — documented plainly (in-root only; host writers trusted; file nlink fail-closed; dirs no post-check) |

#### elforeign non-blocking (not done yet)

- ASCII-only case folding follow-up verified locally in `c45e11d` on SSD-backed APFS. The commit is unpushed.
- Per-mount quota implemented locally: `HostMount::with_quota(bytes, entries)` (net growth, `ERROR_DISK_FULL`/`ENOSPC`), guest test `mount_quota_bounds_net_bytes_and_entries_added_by_the_guest`.
- Scope: branch still overlaps #316 history; once #316 merges, #312 diff should shrink toward FS+later CRT — or split if maintainer asks.

#### Local verification after unlink fix

```sh
export CARGO_HOME=/Volumes/SSD/xenora-toolchain/cargo RUSTUP_HOME=/Volumes/SSD/xenora-toolchain/rustup
export PATH="/Volumes/SSD/xenora-toolchain/cargo/bin:$PATH"
cargo test -p xenora-loader --test runtime_host_files   # 10 passed
cargo clippy -p xenora-loader --lib -- -D warnings
```

### [#316](https://github.com/ranveersoni98/Xenora/pull/316) — fuzz / bench / demangle

- **Head:** `feat/runtime-fuzz-bench`  
- **State:** OPEN, **CHANGES_REQUESTED** (`ranveersoni98`)  
- Blocking harness issues were addressed in a prior reply (deterministic mem fill, coverage asserts, corrected fuzz evidence, `XMemAlloc` fail-closed). **Awaiting re-review.** No new human comments since that reply at handoff time.  
- Worktree used earlier: `/Volumes/SSD/xenora-x19-baseline` (may still exist).

---

## What this branch already contains (high level)

Relative to upstream `main`, the branch includes (among merges):

- Host-backed guest drives (`host_files.rs`, `runtime_host_files.rs`) + security hardenings.
- Large CRT surface: strings, ctype, locale, sprintf/snprintf, FILE streams (in-memory + **mount-backed fopen**), env get/set/`_putenv`, calendar time, `rand`/`srand`, `_stricmp` family, `atoi` family.
- XAsync immediate + manual `XTaskQueue` work/completion ports.
- Process-local Winsock UDP loopback + `sendto`/`recvfrom`.
- Synthetic `GetModuleFileNameA`/`W`.
- Some fuzz/bench/demangle history that also lives conceptually under #316.

Recent tip commits (after trailer rewrite; hashes changed):

- `680f572` — Expand CRT I/O, env, time, rand and Winsock UDP helpers  
- `550b2ce` — Merge upstream main `d7f7d58`  
- `8a677bc` — Drop dead `AT_EMPTY_PATH` unlink and document name-based remove limits  

---

## Prompt 1 — honesty about completion

The download prompt is a **multi-phase ownership track**, not a finished checklist. Substantial subsets exist; **not done:** full C++ runtime, guest threads (#323 elsewhere), broad metadata→conformance productization, full Win32 surface, independent human security acceptance, mutation/perf campaigns, etc.

---

## Background automation

A shell loop (terminal ~`217814`) echoes `AGENT_LOOP_TICK_pr_feedback` every 10 minutes while running. On each tick:

1. `git fetch origin main` and note `HEAD..origin/main`.
2. Check #312 and #316 issue comments + reviews for **new human** feedback.
3. If new feedback → address it first (code + PR reply).
4. If none → continue feature work; optionally merge/sync `main` when needed.
5. Briefly tell the user the tick result.

Restart the loop if the terminal died:

```sh
cd /Volumes/SSD/Xenora
while true; do
  sleep 600
  echo 'AGENT_LOOP_TICK_pr_feedback {"prompt":"Check GitHub PRs #312 and #316 for new human review comments. If new feedback is not addressed, stop feature work and address it first. Fetch origin/main and sync if needed."}'
done
```

GitHub PR subscribe MCP returned 403 earlier; the local loop is the reliable path.

---

## Next actions (priority order)

1. **Check both PRs** using the timestamp under Resume here.
2. On new #312/#316 comments → implement / reply immediately.
3. If quiet → continue CRT/runtime gaps that **do not** collide with #323 (no `CreateThread` scheduler). Sensible candidates:
   - `GetTempFileNameW` is verified on synthetic and mounted drives. Continue remaining filesystem compatibility and verification from the brief. The host-mount quota suggestion remains open.
   - `strtok_s` and `wcstok_s` are implemented and exercised through translated guest imports. Both are partial because they return `EINVAL` without invoking the UCRT invalid-parameter handler.
   - Keep docs/`CHANGELOG` honest; no Xbox claims.
4. Sync `main` when new upstream commits appear; resolve changelog conflicts by **keeping both** sides.
5. Prefer updating **#312** for work that depends on host mounts; a cherry-pick onto bare `main` fails (stdio/host_files not on `main` alone).
6. DCO on every commit: `Signed-off-by: Adeeb Rahman <therealabeeb@icloud.com>`.
7. Do **not** force-push again unless a review requires another history rewrite; prefer normal commits.

---

## Key files

| Area | Paths |
| --- | --- |
| Host mounts | `crates/loader/src/services/host_files.rs`, `files.rs`, `tests/runtime_host_files.rs` |
| CRT | `crt.rs`, `crt_stdio.rs`, `crt_env.rs`, `crt_time.rs`, `crt_ctype.rs`, `crt_locale.rs`, `crt_callbacks.rs` |
| Winsock | `winsock.rs` |
| Modules | `modules.rs` |
| XAsync / queues | `gdk_async.rs`, `gdk_queue.rs` |
| Registration | `crates/loader/src/services.rs` |
| Contracts | `docs/interfaces.md`, `CHANGELOG.md` |
| Rules | `AGENTS.md`, `CONTRIBUTING.md`, `docs/legal.md` |

---

## Commands cheat sheet

```sh
export CARGO_HOME=/Volumes/SSD/xenora-toolchain/cargo
export RUSTUP_HOME=/Volumes/SSD/xenora-toolchain/rustup
export PATH="/Volumes/SSD/xenora-toolchain/cargo/bin:$PATH"
cd /Volumes/SSD/Xenora

git fetch origin main
git log --oneline HEAD..origin/main

cargo test -p xenora-loader --lib crt_stdio::tests
cargo test -p xenora-loader --test runtime_host_files
cargo clippy -p xenora-loader --lib -- -D warnings
cargo fmt -p xenora-loader -- --check

git push -u fork HEAD   # after user auth; feature branch only
gh pr view 312 --repo ranveersoni98/Xenora
gh pr view 316 --repo ranveersoni98/Xenora
```

---

## Agent store copy

A mirror of this handoff may also live in the Cursor agent store for this chat:

`…/AgentStores/cursor_agent_stores/d3fb0682-0141-4df7-9bf0-e84a271beeea/files/HANDOFF-system-runtime.md`

Prefer the in-repo path above once pulled; keep them in sync if you edit either.

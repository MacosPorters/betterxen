# System-runtime agent handoff

**Updated:** 2026-11-24T20:01Z (UTC)  
**Owner identity on commits:** Adeeb Rahman `<therealabeeb@icloud.com>` (GitHub: `soggyramennoodle`)  
**Do not add** `Co-authored-by: Cursor` or any AI attribution trailers (`AGENTS.md`).

This file is the pick-up brief for the next agent continuing the system-runtime lane. Read it before editing.

---

## One-line status

Active branch `feat/system-runtime-expansion` is pushed to tip **`36f9b1c`** (handoff doc). Review-fix tip for elforeign is **`8a677bc`**. PR **#312** is open with **CHANGES_REQUESTED** (blocking items addressed; awaiting re-review). PR **#316** still **CHANGES_REQUESTED** (fuzz harness fix already replied; awaiting maintainer re-review). Keep expanding runtime **unless** new PR feedback appears — then switch to feedback first.

---

## Workspace and remotes

| Item | Value |
| --- | --- |
| Workspace | `/Volumes/SSD/Xenora` |
| Fork remote | `fork` → `https://github.com/soggyramennoodle/Xenora.git` |
| Upstream remote | `origin` → `https://github.com/ranveersoni98/Xenora.git` |
| Active branch | `feat/system-runtime-expansion` (tracks `fork/feat/system-runtime-expansion`) |
| Tip (handoff) | `36f9b1c` — *Document system-runtime handoff for the next agent.* (review-fix parent: `8a677bc`) |
| Upstream `main` at handoff | `d7f7d58` (merged into the branch) |
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

- ASCII-only case folding: add test + clearer error for non-ASCII case variants on APFS.
- Consider per-mount quota (total bytes / file count) beyond per-file size cap.
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

1. **Wait / poll** for elforeign or maintainer re-review on #312 after `8a677bc`.
2. On new #312/#316 comments → implement / reply immediately.
3. If quiet → continue CRT/runtime gaps that **do not** collide with #323 (no `CreateThread` scheduler). Sensible candidates:
   - elforeign non-blocking: ASCII case test + clearer EEXIST mapping on mounts.
   - `GetCurrentDirectory` / `SetCurrentDirectory` synthetic `C:\` (if still missing).
   - More CRT (`strtok` is stubbed in import-gen — only if a real bounded design exists; don’t ship a fake).
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

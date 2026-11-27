# Merge order and CHANGELOG conflicts

Why merging the open `track-*` branches produces conflicts, and how to resolve
them. This is a process note for maintainers and contributors landing parallel
PRs (#529). It changes no code and no convention.

## Measured result (VERIFIED)

A local run by the implementer on 2026-11-27. Class A for this host and this
branch set only; not independently reproduced. It will change as branches merge.

Starting from `origin/main` `b8724bf`, merging the 14 open `track-*` branches
from the `EternaPeptix` fork in the sequence below produced **9 conflicts out of
14**. Every conflict included `CHANGELOG.md`. Eight of the nine conflicted in
`CHANGELOG.md` alone; one (`track-e/509-acphal-shape-contexts`) also conflicted
in `tests/conformance/test_audio_input/main.cpp`.

```sh
git checkout -B verify/merge origin/main   # b8724bf
for b in <the 14 branches, in order>; do
  git merge --no-edit "fork/$b" || { echo "CONFLICT $b"; git merge --abort; }
done
```

Observed output (conflicting merges only; the other five merged cleanly):

```
CONFLICT track-c/505-xg-sw64kb-s            unmerged: CHANGELOG.md
CONFLICT track-a/507-bx-probe-hardening     unmerged: CHANGELOG.md
CONFLICT track-e/509-acphal-shape-contexts  unmerged: CHANGELOG.md tests/conformance/test_audio_input/main.cpp
CONFLICT track-a/511-veh-adx-tripwire       unmerged: CHANGELOG.md
CONFLICT track-e/515-delayed-callback       unmerged: CHANGELOG.md
CONFLICT track-b/517-xmem-virtualalloc-arity unmerged: CHANGELOG.md
CONFLICT track-a/519-psm-context-roundtrip  unmerged: CHANGELOG.md
CONFLICT track-d/521-chunk-lookup-api       unmerged: CHANGELOG.md
CONFLICT track-f/523-placed-resource-x-gap  unmerged: CHANGELOG.md
```

`git diff --name-only --diff-filter=U` during each conflict confirmed the
unmerged paths listed above. No source file under `shims/`, `launcher/`, or
`shader-transpiler/` was ever unmerged.

**Limitation:** one host, one day, one specific set of 14 branches in one
order. The count and the conflicting set change as PRs land or are rebased.

## The mechanism

[AGENTS.md](../../AGENTS.md) §7.6 requires a `CHANGELOG.md` `[Unreleased]`
entry for every meaningful change, and the house convention is to **prepend**
the new bullet at the top of its section (`### Added`, `### Fixed`, …). Every
PR therefore edits the same line region at the top of the same section. Any two
PRs that touch the same section conflict on that region regardless of how
unrelated their code is, because Git sees two different insertions at the same
anchor line.

This is **not a defect in any PR** and not a bug in Git. It is the price of the
required prepend-at-top convention: the changelog is a shared append point that
the track-ownership model (§2) cannot partition, because every track writes to
the same file.

## Resolution recipe

The conflict is almost always two bullets that both want to be first. Keep
**both**. Order within the section does not matter for correctness.

```sh
git checkout main && git pull
git merge --no-ff <branch>            # conflict in CHANGELOG.md
# Edit CHANGELOG.md: remove the <<<<<<< / ======= / >>>>>>> markers
# and keep BOTH bullets (the incoming one and the existing one).
git add CHANGELOG.md && git merge --continue
python3 scripts/audit/docs_hygiene.py   # re-run after EACH merge
```

If the merge also conflicted in a source or test file (as
`track-e/509-acphal-shape-contexts` did), resolve that file on its merits and
re-run the relevant conformance test before continuing.

That one is a **different** cause, worth naming so it is not mistaken for the
changelog pattern: `track-e/509-acphal-shape-contexts` and
`track-e/500-gameinput-dispatcher` both edit
`tests/conformance/test_audio_input/main.cpp` — 509 adds the PSM shape-context
checks, 500 adds the dispatcher checks. Two PRs on one test file conflict on
their own, exactly as two PRs on one source file would. The track model (§2)
partitions directories, not files, so sibling PRs inside one track can still
collide. Reproduced here: merging `500` then `509` left **both**
`CHANGELOG.md` and `tests/conformance/test_audio_input/main.cpp` unmerged
(**VERIFIED**). Re-run
`docs_hygiene.py` after every merge, not just at the end, so a broken Markdown
link is attributed to the merge that introduced it.

## Alternatives (maintainer decisions — not changed here)

These are options to reduce the conflict rate. **Changing the changelog
convention is a maintainer call**; this note does not make that change.

- **Merge in an order that minimises overlap.** Land PRs touching disjoint
  changelog sections back to back, and rebase the rest, so fewer PRs race for
  the same prepend anchor.
- **Designate a fixed append point.** Append new bullets at the *bottom* of a
  section, or under a dated sub-heading, so insertions land at distinct anchors
  and Git can auto-merge them. This trades the "newest first" reading order for
  a lower conflict rate.
- **Squash-merge and rewrite `[Unreleased]` once.** Merge PRs without their
  changelog bullets and have a maintainer write the `[Unreleased]` section in a
  single pass before a release.

## Evidence class

Per [docs/claims.toml](../claims.toml): the merge-conflict counts and the
unmerged-path lists are **class A** — a local Apple Silicon run by the
implementer on the named `origin/main` revision and branch set, reproducible
with the commands above, not independently reproduced, and not an Xbox hardware
result. The mechanism explanation is a description of Git's text-merge behavior
combined with the §7.6 convention, not a separate measurement.

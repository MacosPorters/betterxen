# Native research roadmap

**Deferred — not part of the active development roadmap, and paused indefinitely.** The active direction is the [CrossOver-based hybrid roadmap](../roadmap.md). Native work resumes only after an explicit maintainer decision, with no date. Native tasks needed by hybrid work are filed as focused hybrid issues instead.

## Existing implementation (preserved in the workspace)

Rust crates under `crates/` and `app/`, tests under `tests/`, fixtures, fuzz targets and benchmarks. The last `main` commit before the hybrid transition is tagged `native-first-final`. Contributor history is untouched.

## Verified achievements (synthetic evidence only)

Per [docs/progress.md](../progress.md) and the [status registry](../status.toml): translated gfx1030 shaders executing on native Metal on Apple M5 for synthetic fixtures, bounded x86-64 guest execution, an ARM64 dynamic binary translator for supported instruction subsets, a deterministic scheduler running guest threads serially on one host thread. No Xbox title runs. These are the original evidence classes, not new verification.

## Known limitations

See the archived [native-first plan](../archive/development-phases-native-first.md) and [README current limitations](../../README.md#current-limitations): serial guest threading, partial runtime services, partial RDNA2 coverage, unknown console launch ABI.

## Previously planned work

The native-first phases 1 to 6 (trustworthy baseline, hosted C application, threaded behavior, translated rendering, integrated interactive application, measured bottlenecks) and the X1 to X25 capability map, kept in the archive. The issue backlog was closed as not planned and is indexed, with preserved findings, in the [native issue archive](native-issue-archive.md).

## Potential future architecture

Independent CPU translation and Metal components that could replace or complement CrossOver, Rosetta 2 or D3DMetal where a demonstrated limitation or availability risk justifies it.

## Conditions for resuming (maintainer decision, not automatic)

Hybrid reaches verified maturity; reliable baselines exist; a native component solves a demonstrated hybrid limitation; a dependency imposes an unavoidable constraint or becomes unavailable; or resources allow it without slowing hybrid work.

# Executive summary

> **DRAFT — FOR MAINTAINER DISCUSSION ONLY — NOT APPROVED FOR IMPLEMENTATION**

Source baseline: `main` at `cd945ba`. Open pull requests are described as of 2026-11-26 and are not part of `main`.

## The problem

BetterXen has two memory systems that do not know about each other.

- `xmem.dll` allocates CPU memory and enforces a title budget (8 GiB by default).
- D3D12 allocations go from `d3d12_xs.dll` straight to the host. On `main` they are not counted, tracked or given a lifetime.

Research [#473](https://github.com/MacosPorters/betterxen/issues/473) found that on the tested M4 and M5, mapped buffers that D3D12 itself allocates are usable by CPU and GPU with a fence between them, and that memory the CPU already owns cannot be imported. It did **not** show physical zero-copy, memory savings or a speed-up.

## The recommendation (proposal, not a decision)

**Architecture C: a hybrid resource provider, built correctness-first, with today's behaviour as the default.**

1. **First fix accounting and lifetime**, whatever happens to ownership: one atomic admission authority, every graphics allocation path counted, and a charge released only when the object is really destroyed.
2. **Then add a provider boundary** that records why each allocation took its path, without changing where anything is allocated.
3. **Only then try D3D12-owned mapped buffers**, behind an opt-in, for a narrowly defined eligible class.
4. **Pooling and transfer optimization come after measurement**, not before.

## The most important thing this audit adds

**Nothing currently tells BetterXen which allocations are eligible.** A D3D12-owned buffer must be chosen *before* the guest receives its pointer. `xmem` hands out pointers before D3D12 is involved, and it discards the high attribute bits that might have described intent, whose console meaning is not publicly established. PR #486's opt-in is a BetterXen-defined block that no existing title would send.

So Stage 2 has no real-world trigger today. Whether to build it at all depends on the answer to [decision D1](maintainer-decisions.md#d1-what-makes-an-allocation-eligible). Stages 0 and 1 are worth doing either way.

## New observations made for this draft

All are single-host, author-run checks on one Apple M4 (macOS 27.0, CrossOver 26.3, D3DMetal), not independently reproduced.

| Observation | Why it matters | Record |
| --- | --- | --- |
| At #486 `bd546d8`, 640 MiB was live under a 512 MiB cap (heap first, then `xmem`) | A statistics snapshot is not an admission authority | [review](https://github.com/MacosPorters/betterxen/pull/486#pullrequestreview-6082636773) |
| At the same head, releasing a heap credited its budget while a placed resource kept it alive | The wrapper's last `Release` is the wrong release point | same |
| At the same head, `CreateHeap1` and committed resources were not charged | Admission must cover every creation path | same |
| D3DMetal released a `SetPrivateDataInterface` object only on real destruction; `heap->Release()` returned 0 while a placed resource still held the heap | A candidate release signal that needs no resource wrapping | [probe](validation-strategy.md#appendix-destruction-signal-probe) |
| With #486 and #471 applied, the M2 fixture passes, but its `xmem` page holds none of the frame | "Configured region" is not `xmem`-backed memory | [review](https://github.com/MacosPorters/betterxen/pull/494#pullrequestreview-6082650696) |

## Documents

| Document | Contents |
| --- | --- |
| [Current architecture](current-architecture.md) | What `main` does today, with source locations |
| [Alternatives](architecture-alternatives.md) | A, B and C compared |
| [Proposed memory architecture](proposed-memory-architecture.md) | Budget authority, providers, pooling, diagnostics |
| [Resource lifecycle](resource-lifecycle.md) | Synchronization and retirement rules |
| [ABI impact](abi-impact-analysis.md) | What would change in `abi/`, and what can avoid it |
| [Cross-track dependencies](cross-track-dependencies.md) | Tracks A–F and open work |
| [Migration proposal](migration-proposal.md) | Stages 0–6 |
| [Validation strategy](validation-strategy.md) | Gates, fixtures, measurements |
| [Risk register](risk-register.md) | Causes, detection, mitigation |
| [Maintainer decisions](maintainer-decisions.md) | Open questions and proposed decision records |

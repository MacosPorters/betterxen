# Executive summary

> **DRAFT — FOR MAINTAINER DISCUSSION ONLY — NOT APPROVED FOR IMPLEMENTATION**

Source baseline: `main` at `cd945ba`. Open pull requests are described as of 2026-11-26 and are not part of `main`.

## The problem

BetterXen has two memory systems that do not know about each other.

- `xmem.dll` allocates CPU memory and enforces a title budget (8 GiB by default).
- D3D12 allocations go from `d3d12_xs.dll` straight to the host. On `main` they are not counted, tracked or given a lifetime.

Research [#473](https://github.com/MacosPorters/betterxen/issues/473) found that on the tested M4 and M5, mapped buffers that D3D12 itself allocates are usable by CPU and GPU with a fence between them, and that memory the CPU already owns cannot be imported. It did **not** show physical zero-copy, memory savings or a speed-up.

## The recommendation (proposal, not a decision)

**Commit only to correctness and visibility now. Keep a hybrid provider as the target shape, but do not build it yet.**

1. **Stage 0a, observe:** count graphics allocations and detect real destruction. No ABI change, no behaviour change.
2. **Stage 0b, enforce:** one atomic admission authority, behind a switch, after 0a shows what would be rejected.
3. **Stage 1:** record a written provider contract and reason-coded diagnostics, then **stop**.
4. A mapped-buffer provider (Stage 2 onwards) waits for a reliable [eligibility mechanism](eligibility.md) and a measurement that justifies it.

## Eligibility in one paragraph

Microsoft's public samples mark graphics memory at allocation time, and public Windows headers give values for the `PAGE_GRAPHICS_*` protection bits. That is a candidate signal. It is not yet usable: BetterXen's `XMemVirtualAlloc` appears not to read the argument that carries it, the method that consumes the address has no public ABI here, and textures placed at that address cannot be backed by a host buffer. **The first revision of this draft wrongly said no signal existed.**

## Most urgent item for maintainers

The public samples call `XMemVirtualAlloc` with five arguments; BetterXen declares four. See [the discrepancy](eligibility.md#a-discrepancy-on-main-that-needs-maintainer-attention). It concerns `main` today, independently of this proposal.

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
| [Meeting briefing](meeting-briefing.md) | One-page agenda |
| [Eligibility investigation](eligibility.md) | Public signals and why they are not enough |
| [Self-audit](self-audit.md) | Errors found in the first revision; case against |
| [Evidence record](evidence.md) | Probe sources, patches, commands, environment |

# Architectural alternatives

> **DRAFT — FOR MAINTAINER DISCUSSION ONLY — NOT APPROVED FOR IMPLEMENTATION**

The research recommends *investigating* inverted ownership. It does not establish that it is better, so it is compared here with two other options. Assessments are qualitative; no scores or performance numbers are implied.

## The options

**A — Keep existing ownership, fix correctness.** `xmem` and D3D12 stay separate. Count and track graphics allocations. BetterXen performs no transfer between the two today; only `xcrdapi` staging copies could be optimized.

**B — Fully inverted ownership for eligible buffers.** For eligible allocations D3D12 creates the memory; `xmem` hands out the mapped pointer.

**C — Hybrid resource provider.** A selection step chooses per allocation among existing CPU memory, D3D12 mapped buffers, pooled graphics allocations, GPU-optimized resources and explicit transfer. Today's behaviour is the default.

## Evidence that bounds all three

| Finding | Source |
| --- | --- |
| `OpenExistingHeapFromAddress` / `FromFileMapping` return `E_NOTIMPL` | [findings §1](https://github.com/MacosPorters/betterxen/blob/0aaa81ab81309d64432d97d3005284c8b13cd285/docs/research/publications/001-unified-memory/technical-findings.md#1-buffer-memory) |
| Mapped CPU pointers are not reliably 64 KiB aligned | same |
| Textures have opaque layouts | [§2](https://github.com/MacosPorters/betterxen/blob/0aaa81ab81309d64432d97d3005284c8b13cd285/docs/research/publications/001-unified-memory/technical-findings.md#2-graphics-resources) |
| Physical sharing and absence of internal copies are unknown | [§4](https://github.com/MacosPorters/betterxen/blob/0aaa81ab81309d64432d97d3005284c8b13cd285/docs/research/publications/001-unified-memory/technical-findings.md#4-copying-and-coherence) |
| Large released graphics allocations stay in the process footprint | [§3](https://github.com/MacosPorters/betterxen/blob/0aaa81ab81309d64432d97d3005284c8b13cd285/docs/research/publications/001-unified-memory/technical-findings.md#3-allocation-pooling-and-lifetime) |
| Requested addresses, reserve/commit, sections, scatter views and pre-device allocations cannot be expressed by `Map` | [ownership matrix](https://github.com/MacosPorters/betterxen/blob/f8810a48af384d13c594b9fba954200bfdb944f1/docs/research/d3dmetal-zero-copy/allocation-profiling/inverted-ownership.md) |

## Decision matrix

| Criterion | A: keep ownership | B: inverted ownership | C: hybrid provider |
| --- | --- | --- | --- |
| Compatibility | Highest: no allocation semantics change | Weakest: several `xmem` contracts cannot be met, so B needs fallbacks and becomes C | High if the default path is unchanged and fallback is explicit |
| ABI stability | Only the budget fix needs `abi/` | Needs a cross-DLL provider contract | Same budget change, plus a provider contract at Stage 2 |
| Pointer stability | Unchanged | Pointer lifetime tied to a D3D12 resource and device | Decided per allocation before exposure; never migrated |
| Memory consumption | Two copies remain where the guest needs CPU and GPU views | Could remove a copy for eligible data; unmeasured, and retention may offset it | As B for the eligible subset only |
| Transfer overhead | Reducible, not removable | Fewer explicit copies for eligible buffers; internal copies unknown | Mixed; must be measured per class |
| Synchronization | Existing explicit boundaries | Highest: guest CPU writes can race the GPU with no copy boundary | High, limited to opted-in allocations |
| Implementation complexity | Low | High | Highest total, but divisible into stages |
| Migration risk | Low | High | Medium if each stage can be turned off |
| Maintainability | Good | One path in theory, two in practice | Two or more paths; needs a strict boundary and diagnostics |
| Observability | Easy to count copies | Hard: physical behaviour is opaque | Good if provider choice is recorded |
| Performance qualification | Straightforward before/after | Needs production-like measurement that does not yet exist | Same, but old and new can run side by side |
| Existing contributor work | No conflict | Overlaps #486 and changes `xmem` semantics | Can adopt #486's backing path as the first, opt-in provider |

## Recommendation (needs maintainer approval)

**Do A now. Treat C as the shape to adopt if a second provider is ever justified.** Do not build the boundary in code before then.

Reasons:

1. B's unsupported cases force a fallback path, so B is not actually simpler than C.
2. No evidence yet shows a memory or performance benefit, so the default must not change.
3. [Eligibility](eligibility.md) has only a candidate signal and no usable consumer today, so work stops after Stage 1 and has still delivered A.

A fourth route, **translation by copy** (keep `xmem` memory, create host resources, copy at synchronization points), is inside C as "explicit transfer". It is the only route that works for textures placed at a console address without memory import. Where its synchronization points would be is unknown and is probably the largest performance risk.

What would change this recommendation: a lawful public contract for the consuming API, plus a matched measurement showing a mapped path beats copying.

# Workstream A research report

BetterXen [#473](https://github.com/MacosPorters/betterxen/issues/473), 2026-11-26. This is local, original synthetic research on Apple M4 with 24 GiB. Independent acceptance remains pending. The objective is a defensible memory architecture, not proof of zero-copy. No production shim, ABI, main-branch file or original baseline probe was changed.

## Findings that affect the architecture

A D3D12-owned mapped buffer is a viable candidate for selected graphics allocations in the observed configuration. It is not a replacement for arbitrary xmem virtual/physical/scatter semantics. New Map-pointer receipts are not always 64 KiB aligned, even when heap placement is. Valid overlapping buffer placements exhibited the expected application-level data inheritance, but this does not identify physical pages or wrap an arbitrary existing allocation.

Process footprint depends on allocation size, prior runtime charging and retirement scope. Small allocations can use already charged regions; larger graphics allocations can remain process-accounted after resources, command objects and the device are released. RSS and footprint accounting can differ sharply under compression/swap. Neither a near-1x slope nor SM=SHM establishes unique physical backing or the absence of internal copies.

Pooling reduces the measured heap-creation component in a matched synthetic trace, but has a separately measured cold setup cost, reserved slack and noisy whole-cycle differences. It has not demonstrated a general application-level performance or memory improvement. The candidate architecture should not assume pooling is beneficial before workload-specific measurements and independent review.

Valid texture/mip/mixed-placement and RT/depth clear/alias readbacks matched in two new sessions. The overall texture matrix nevertheless failed its negative-rejection gate: the runtime accepted misaligned and out-of-bounds creation calls that were never GPU-used. Caller-side validation remains necessary. Invalid-call acceptance is not by itself proof of a runtime conformance defect because the public contract imposes preconditions without promising every rejection HRESULT.

## Deliverables and evidence

| Deliverable | Current evidence and status |
| --- | --- |
| Physical memory analysis | [Footprint findings](footprint-findings.md): historical two-repeat profile, corrected retirement checkpoints, controls, compression/pressure limits and interrupted attempt. Raw tool outputs are append-only. |
| Inverted ownership | [Requirement matrix](inverted-ownership.md): requested addresses, CPU alignment, attributes, reservation/decommit, aliases/scatter, pre-device allocations, budget and lifetime. Distinguishes in-tree implementation from unverified console semantics. |
| Allocation/pooling comparison | [Three corrected matched pairs](pooling-findings.md), raw API samples/distributions and separately timed cold heap setup. No stable production improvement claimed. |
| Fragmentation/retirement | Deterministic sizes/seeds, guard/interior GPU checks, logical free-span/waste/slack records, coalescing and a real gate-delayed retirement case. Multi-queue and arbitrary workloads remain open. |
| Texture/resource correctness | [Version-scoped matrix](../gpu-correctness/textures-and-resources/findings.md): three formats/three mips/WB/WC/DEFAULT, disjoint guards, RT/depth aliases and valid buffer inheritance. Failed negatives retained. |
| Shared versus GPU-optimized storage | Conditional guidance below; matched bandwidth and actual internal Metal storage modes remain unmeasured, requiring B's contribution. |
| Optional allocator reuse | [Exact-pin, per-file assessment](allocator-assessment.md) of AMD D3D12 Memory Allocator. No source adopted or runtime qualification claimed. |
| Independent review of B | [Pending](review-handoff.md): M5 comments reviewed as attributed reports; pinned B implementation/raw handoff and critical reproduction not yet available. |
| Recommended architecture | Candidate ownership split and explicit fallbacks below; no production adoption decision. |
| Limitations and fallbacks | [Inventory/gaps](inventory/README.md), ownership matrix, pressure interruptions, negative outcomes and unresolved mechanisms remain explicit. |
| #464 handoff | [Narrow evidence note](handoff-464.md), including current PR #486 source boundary at a pinned revision. No implementation changes or certification. |

## Candidate memory architecture

Keep a CPU-owned family for pre-device allocations, requested virtual addresses, reservations/decommit, section-backed physical tags/aliases/scatter views and unsupported attributes. Where graphics must consume those bytes, preserve an explicit, logged transfer path rather than silently relocating guest pointers or claiming unsupported wrapping.

Evaluate a D3D12-owned mapped-buffer family only for allocations whose CPU address, alignment, ownership and lifetime requirements it actually satisfies. Maintain resource ownership through every live pointer and last GPU use. Biased byte suballocation can be investigated for alignment, but does not automatically satisfy separately placed-resource offsets or CPU/GPU alias contracts. Retain explicit fallback mechanisms for incompatible rows in the matrix.

Use one logical budget with clearly separated live payload, reserved slabs, alignment waste, pending retirement and cached storage. Track host headroom/pressure and observed runtime retention separately. Logical release must not be presented as immediate physical reclamation, and a slice plus its slab must not be double charged. Bound pooling/caching rather than growing until pressure forces eviction.

Separate texture storage from raw mapped-buffer ownership. The tested CPU-subresource and DEFAULT upload/readback paths are usable candidates for their observed formats. For CPU-frequent buffers/textures, choose a valid CPU-visible path by measured access requirements; for GPU-owned resources evaluate DEFAULT with explicit transfers where needed. The A data cannot rank WB, WC and DEFAULT bandwidth or reveal D3DMetal's Metal storage mode. Combine correctness/accounting with B's measured copy/coherence/bandwidth tradeoffs before choosing by resource class.

Reuse established allocation infrastructure if it satisfies demonstrated requirements. The optional AMD allocator assessment is a starting point; production integration requires its own pinned build/runtime comparison, provenance/registry work and approval. The original trace free list is not a proposed production replacement.

## Environment and methodological limits

Recorded host: Apple M4, 25769803776 bytes unified RAM, macOS 27.0 build 26A428; CrossOver 26.3.0.39832 (cxoffice-26.3.0rc2), Wine engine wine-11.0-8726-g2e2f5fca349; loaded D3DMetal 3.0, SourceVersion 32047000000000; Rosetta receipt 1.0.0.0.1788432274. Tool receipts include native ARM64 GCC 16.2.0/MinGW-w64 14.0.0_3, Apple clang 17.0.0, SDK 26.2 and Python 3.14.3. Host page size is 16384 bytes. Own observer and CLI architecture/linkage checks are retained.

The host was not idle and had existing swap. A large sweep was interrupted when the stronger kernel pressure signal reported urgent; it remains incomplete. The bounded continuation completed six smaller/control processes and stopped automatically before proceeding with the WB 256 MiB graphics case when urgent pressure recurred. Large corrected repeats remain open. After the requester freed memory and authorized bounded continuation, runs explicitly used the warning-pressure profile, with urgent/critical stops and a conservative 23 GB host-headroom ceiling. No optional 2 GiB extension was run. The ceiling is a guard, not a target or a claim to control other applications. Three readings at one pause are correlated snapshots, not independent repetitions. Repeated runs on the same host do not establish cross-generation acceptance.

Hardware observations are local class A within their workloads; public API contracts class B; allocator project-source assessment class D; physical mechanisms, untested configurations and console compatibility class U. No Xbox hardware or commercial game execution was tested. Shared baseline sources and historical failures are preserved. No result is independently accepted merely because the author wrote both a probe and this report.

## Review and publication dependency

Exact executed sources, hashes, commands, raw data and review questions are in the append-only bundles and [handoff](review-handoff.md). B must independently review and reproduce A's critical findings; A must inspect and reproduce B's actual pinned implementation. Those steps remain open. New source publication also requires the requester's separate authorization to commit/push. No commit or push has occurred, and this report does not certify #464 or recommend adoption on the strength of synthetic success alone.

## Local validation

[Validation logs and receipts](../results/2026-11-26-apple-m4-arths17-a-verification/verification.json) record formatting, Clippy, 66 passed Rust tests with one existing ignored test, all 16 strict PE32+ shim/export checks, ARM64 CLI format/linkage and the two unchanged baseline CPU conformance programs (test_veh_clwb and test_shim_exports). Documentation hygiene passed with a temporary index containing candidate files, without changing the real index. Archive/source hash checks and Python/C++ formatting/compilation checks are retained. These engineering checks do not independently accept the new research or turn the texture negative failures into passes.

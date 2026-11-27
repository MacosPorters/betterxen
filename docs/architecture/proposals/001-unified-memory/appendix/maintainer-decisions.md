# Decisions requiring maintainers

> **DRAFT — FOR MAINTAINER DISCUSSION ONLY — NOT APPROVED FOR IMPLEMENTATION**

*Appendix. Deferred to the pre-M3 discussion; the [main page](../README.md) governs.*

Nothing below is decided. The proposed records are **not** registered ADRs and have no ADR numbers.

## Open decisions

### D1. What makes an allocation eligible?

A mapped D3D12 buffer must be chosen before the guest sees a pointer. [Candidates](eligibility.md#possible-signals): `PAGE_GRAPHICS_*` bits (public values, not read today); attribute words (no public values); a BetterXen opt-in (no existing title); or no mapped provider. **Draft view:** none is usable end to end; stop after Stage 1.

### D2. Is the recommended direction right?

C entered through A, versus A alone. **Draft view:** do A now; hold C as a target shape only.

### D3. What happens to #486's budgeting?

#486 has merged with a one-way, order-dependent cap. Known defects are listed on the [main page](../README.md#known-defects-in-merged-m2-code). Open: whether the later authority replaces it.

### D4. Authority location and contract

New `xmem` exports (draft view) or the shadow-commit interim. Token or byte release? Is an additive export a `BX_ABI_VERSION` change?

### D5. Release signal

Private-data sentinel, resource wrapping, or both. **Draft view:** qualify the sentinel on a second generation and with GPU work in flight before relying on it.

### D6. Who tracks GPU use?

**Draft view:** the guest, as in D3D12. BetterXen retires only what it submits.

### D7. Allocator

Vendor AMD's allocator or write a small one. What evidence decides it?

### D8. What is counted?

Swap-chain buffers, shared handles, driver-internal objects, descriptor heaps.

### D9. M2 exit gate

Does a configured D3D12 heap that is not `xmem` memory satisfy "`xmem`-backed placed resources" in #464 and #469? PR #494 leaves #469 open for this decision.

### D10. Qualification

What measurement counts as a benefit, on which hosts, and who runs it?

### D11. Correcting the record

#486's description no longer claims shared or copy-free memory (checked 2026-11-27). The earlier "true zero-copy" comment on #464 and the fixes requested for the research publication are for their authors.

### D12. The `XMemVirtualAlloc` parameter list

Public samples pass five arguments; `main` declares four. Who verifies it, from which lawful source, and what happens to the existing tests? **Draft view:** decide this first.

### D13. Roadmap placement

**Maintainer direction (Asufte, relayed 2026-11-27):** finish M2 under the existing roadmap; revise the roadmap together immediately before M3. Open: which of the [candidate wording](roadmap-integration.md#candidate-roadmap-wording) to take up then.

## Proposed decision records

### Proposed record P1 — One budget authority

- **Status:** Proposed. Not accepted.
- **Context:** Two ledgers cannot enforce one cap.
- **Decision (proposed):** `xmem` is the only admission authority; graphics code reserves through it.
- **Consequences:** An `[ABI]` change; every creation path must call it.
- **Alternatives:** Shadow commit; authority in `runtime_veh`; separate graphics budget.

### Proposed record P2 — Default behaviour does not change without evidence

- **Status:** Proposed. Not accepted.
- **Decision (proposed):** New providers are opt-in until an independent, matched comparison supports enabling them.
- **Consequences:** Two paths are maintained for a while.

### Proposed record P3 — Exposed pointers never move

- **Status:** Proposed. Not accepted.
- **Decision (proposed):** Provider choice happens before exposure. No transparent migration or defragmentation of guest-visible allocations.
- **Consequences:** Some fragmentation is permanent.

### Proposed record P4 — No zero-copy claim without a discriminating measurement

- **Status:** Proposed. Not accepted.
- **Decision (proposed):** Documentation and pull requests say "CPU- and GPU-visible with a fence", not "shared" or "zero-copy".

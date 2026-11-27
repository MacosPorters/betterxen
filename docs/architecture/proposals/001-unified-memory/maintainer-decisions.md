# Decisions requiring maintainers

> **DRAFT — FOR MAINTAINER DISCUSSION ONLY — NOT APPROVED FOR IMPLEMENTATION**

Nothing below is decided. The proposed records are **not** registered ADRs and have no ADR numbers.

## Open decisions

### D1. What makes an allocation eligible?

A mapped D3D12 buffer must be chosen before the guest sees a pointer. Candidates: (a) a public, established allocation attribute — none is currently established; (b) an explicit BetterXen opt-in, which no existing title uses; (c) interception at a D3D12-level API where the title already asks the graphics runtime for memory; (d) no mapped provider. **Draft view:** do not start Stage 2 until one of these is concrete.

### D2. Is the recommended direction right?

C entered through A, versus A alone. **Draft view:** C, with a stop after Stage 1 if D1 has no answer.

### D3. What happens to #486?

(a) Merge after its blocking findings, accepting a known order-dependent cap until Stage 0; (b) merge without charging; (c) hold charging for the authority. **Draft view:** its owner and the maintainers decide; this draft only supplies the evidence. Whatever is chosen, document the cap limitation.

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

Does a configured D3D12 heap that is not `xmem` memory satisfy #469's "`xmem` placed resources"?

### D10. Qualification

What measurement counts as a benefit, on which hosts, and who runs it?

### D11. Correcting the record

Whether "true shared memory, no extra copy" in #486 and the "true zero-copy" comment on #464 should be corrected by their authors.

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

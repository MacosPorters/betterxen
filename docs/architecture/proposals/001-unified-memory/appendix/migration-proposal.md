# Migration proposal

> **DRAFT — FOR MAINTAINER DISCUSSION ONLY — NOT APPROVED FOR IMPLEMENTATION**

*Appendix. Deferred to the pre-M3 discussion; the [main page](../README.md) governs.*

Stages are conceptual. They carry no dates and no owners, and each needs separate authorization. Every stage must be able to ship alone and be turned off or reverted.

```mermaid
flowchart TB
  S0A["0a Observe<br/>no ABI, no behaviour change"] --> G{Evidence that a cap<br/>should be enforced?}
  ABI[ABI: charge exports] --> S0B
  G -->|yes| S0B["0b Enforce<br/>behind a switch"]
  G -->|no| S1
  S0B --> S1["1 Contract and diagnostics"]
  S1 --> D1{Reliable eligibility<br/>and consumer ABI?}
  D1 -->|no| STOP[Stop here]
  D1 -->|yes| S2["2 Opt-in mapped provider"]
  S2 --> S3["3 Pooling"]
  S2 --> S4["4 Transfers"]
  S3 --> S5["5 Qualification"]
  S4 --> S5
  S5 --> S6["6 Adopt, revise or reject"]
  W["Open PR 486"] -.input.-> S0A
```

## Stages mapped to milestones

No stage is part of M2. See [the mapping](roadmap-integration.md#possible-placement).

## Stage 0a — Observe

- Count graphics allocations that pass through the proxy; log which paths cannot be seen.
- Detect real destruction (candidate: private-data sentinel).
- Log what a combined cap **would** have refused. Refuse nothing.

## Stage 0b — Enforce

Only if 0a shows a reason to.

- One admission authority (`[ABI]` change), covering every path the proxy can see.
- Off by default until reviewed; a switch that `bx-cli` passes through.
- Release charges on confirmed destruction.
- Validate offset, alignment and span at any placement boundary BetterXen owns, without a global 64 KiB rule.
- Make cleanup failures visible; no reuse on unproven completion.

## Stage 1 — Contract and diagnostics

- A written compatibility and fallback contract, and reason codes in the trace.
- Diagnostics from [the design](proposed-memory-architecture.md#5-diagnostics).
- **No allocation moves, and no provider interface is coded until a second provider exists.** The work stops here unless eligibility is solved.

## Stage 2 — Experimental mapped-buffer provider

- Off by default. Buffers only. One explicit eligibility signal.
- Existing path remains and is what runs when the switch is off.
- Pointer alignment checked at run time.

## Stage 3 — Pooling and reuse

- Evaluate the pinned AMD allocator behind the same provider interface.
- Block reservation charged once; slices not charged.
- Defragmentation off for exposed pointers.

## Stage 4 — Storage and transfer optimization

- Per-class storage choices backed by matched measurements.
- Staging-object reuse in `xcrdapi` (today each staged request creates its own queue and fence).
- Optional write-watch for CPU-owned sources only; GPU write tracking stays unknown.

Staging-object reuse does not depend on Stage 2 and can be proposed on its own if measurement shows it matters.

## Stage 5 — Production qualification

Memory pressure, multi-queue, failure injection, long runs, representative synthetic graphics workloads, several chip generations.

## Conditions for abandoning the provider architecture

See [the checklist](../pre-m3-checklist.md#abandon-the-provider-idea-if).

## Stage 6 — Adoption decision

Compare with the existing implementation on independent results. Enable, revise or remove. Removing is an acceptable outcome.

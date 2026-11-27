# Pre-M3 decision checklist

> **DRAFT — FOR MAINTAINER DISCUSSION ONLY — NOT APPROVED FOR IMPLEMENTATION**

For the discussion after M2 is verified and before M3 starts. Nothing here is decided. Drafted views are the author's, for challenge.

## Before the meeting

- [ ] M2 exit conditions independently verified and recorded
- [ ] Facts rechecked against the then-current `main`; this draft will be stale
- [ ] Research publication fixes and the process-listing redaction handled by its authors ([discussion 493](https://github.com/MacosPorters/betterxen/discussions/493))

## Decide

- [ ] **Parameter list.** Is `XMemVirtualAlloc`'s four-argument declaration wrong? ([detail](appendix/eligibility.md))
- [ ] **Eligibility.** Is there positive proof that an allocation is safe to route to another provider? Draft view: not yet
- [ ] **Observe or enforce.** Draft view: observe first; enforce behind a switch only with a reason
- [ ] **Budget meaning.** Does the cap count live bytes or retained footprint?
- [ ] **Authority and ABI.** New `xmem` reserve, commit and cancel exports?
- [ ] **Release signal.** Private-data sentinel, wrapping, or both? Merged #486 uses private data to retain heaps
- [ ] **Pointer lifetime.** Must a guest pointer stay valid until `XMemFree`?
- [ ] **Go or no-go numbers** for each stage
- [ ] **Roadmap.** Adopt any of the [candidate wording](appendix/roadmap-integration.md#candidate-roadmap-wording)?
- [ ] **Abandon.** Agree what would end the provider idea

## Research recommendations to reconsider then

| Recommendation | Why it needs another look |
| --- | --- |
| D3D12-owned mapped buffers for eligible allocations | Eligibility unresolved; textures are placed at the same console addresses |
| One combined budget authority | #486 has merged a one-way version; compare against it |
| Evaluate AMD's allocator | Only worthwhile if there is a provider |
| Placement validation and lifetime rules | Partly delivered by #486; list what remains |
| Selective copying and write-watch | CPU-owned sources only; unmeasured benefit |
| Storage choice by workload | Needs matched measurements |
| Multi-queue lifetime | Not qualified by the research; M3's gate already covers multi-queue |

## Abandon the provider idea if

No lawful contract for the consuming API; most graphics-marked allocations are textures; pointer alignment cannot be met; no matched benefit over keeping `xmem` memory and copying; footprint gets worse; unfixable compatibility regressions; or two paths cost more than they save.

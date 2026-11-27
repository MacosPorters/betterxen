# Architecture meeting briefing

> **DRAFT — FOR MAINTAINER DISCUSSION ONLY — NOT APPROVED FOR IMPLEMENTATION**

## Recommendation

Do correctness and visibility now. Keep a hybrid resource provider as the target shape, **and do not build it yet**.

**Why hybrid, if anything is built:** full inverted ownership cannot express requested addresses, reserve and commit, sections, scatter views, textures or pre-device allocations, so it needs fallbacks and becomes a hybrid anyway.

## Strongest arguments against

1. BetterXen makes no duplicate copy today and no title runs: the problem may be hypothetical.
2. Textures and placed-at-address resources need translation either way, so the mapped path may cover little.
3. Enforcing a graphics cap can only add failures while console-equivalent sizes are unknown.
4. The cap can be bypassed through host objects, which weakens the case for an ABI change.
5. Two paths cost more than one, and the second has no caller.

## Five decisions to make

| # | Question | Draft view |
| --- | --- | --- |
| 1 | Is `XMemVirtualAlloc`'s parameter list wrong ([detail](eligibility.md#a-discrepancy-on-main-that-needs-maintainer-attention))? | Verify first |
| 2 | Is any [eligibility](eligibility.md) signal good enough to build on? | Not yet |
| 3 | Observe first, or enforce a combined cap now? | Observe first |
| 4 | What happens to #486's charging? | Owner decides; at least document the limits and credit on real destruction |
| 5 | Release signal: private-data sentinel or wrapping? | Sentinel, after a second host and in-flight tests |

All twelve: [maintainer decisions](maintainer-decisions.md).

## Dependencies on open work (as of 2026-11-27 02:40 UTC)

| Item | State | Matters because |
| --- | --- | --- |
| #464 (`devil8blade`) | Claimed | Its scope is the first graphics accounting; this proposal does not widen or take it |
| #486 | New head `7056671`: drops the two extra `AddRef` calls and adds a stale-publication fix; CI passes. Not re-run here. `try_charge`, `release_charge` and `host_private_heap` are unchanged, so by source the order-dependent cap and early credit remain | Input to Stage 0a |
| #469 (`atesahmet0`) | Claimed | Wording says `xmem` placed resources |
| #494 | New head `6a1451b`: clarifies frame readback; CI fails at CMake configure of the new fixture. Not re-run here. At `c64534f` it passed with #486 and #471 while the `xmem` view stayed empty | Maintainers decide whether that meets the gate |

**Shader development can proceed independently.** The only contact is textual (`proxy.cpp`, `CMakeLists.txt`).

## Correctness foundations required

Real-destruction release; every visible creation path counted; no admission from a snapshot; no reuse on unproven completion; unseen paths stated; the parameter question resolved.

## Proposed order

0a observe → 0b enforce (only with a reason, behind a switch) → 1 contract and diagnostics → **stop** → 2–6 only with eligibility, a consumer ABI and a measured benefit. See [migration](migration-proposal.md).

## When to abandon it

- No lawful public contract for the consuming API can be obtained.
- Most graphics-marked allocations turn out to be textures.
- Required CPU alignment cannot be met from mapped buffers.
- A matched measurement shows no benefit over keeping `xmem` memory and copying.
- Retained driver memory makes the footprint worse.
- A supported CrossOver or D3DMetal version breaks mapping or destruction behaviour.
- Maintaining two paths costs more than the measured gain.

In each case keep the Stage 0a and 0b accounting and lifetime work and drop the provider.

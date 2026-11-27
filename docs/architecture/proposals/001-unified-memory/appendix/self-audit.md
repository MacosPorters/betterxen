# Self-audit of this proposal

> **DRAFT — FOR MAINTAINER DISCUSSION ONLY — NOT APPROVED FOR IMPLEMENTATION**

*Appendix. Deferred to the pre-M3 discussion; the [main page](../README.md) governs.*

The author challenged the first revision (`e478ef9`) against source and evidence. **This is self-review, not independent review.** Corrections are applied in the other documents.

## Errors and weaknesses found

| # | First revision said | Problem | Correction |
| --- | --- | --- | --- |
| 1 | Nothing identifies eligible allocations; attribute meaning not public | **Wrong in part.** Public samples and headers give a candidate signal | [Eligibility investigation](eligibility.md) |
| 2 | (not noticed) | `XMemVirtualAlloc` may have the wrong parameter list on `main` | Recorded in the investigation; not changed |
| 3 | Admission covers every D3D12 creation path | **Not achievable by proxy interception.** Resources are host objects, so `resource->GetDevice` returns the host device; `xcrdapi` creates staging resources that way ([`xcrdapi.cpp:330`](../../../../../shims/xcrdapi/xcrdapi.cpp#L330)); a guest can also call `d3d12.dll` directly (source reading, not tested) | The cap is best-effort unless more objects are wrapped |
| 4 | Stage 0 adds enforcement; new providers must not change default behaviour | **Contradiction.** On `main` graphics allocations never fail on the title cap, so enforcement is itself a behaviour change and can cause new failures | Stage 0 split into observe (0a) and enforce (0b) |
| 5 | An ABI change is needed first | Not for observing: `d3d12_xs` can count its own objects | ABI only at 0b |
| 6 | An unsatisfiable explicit request fails | The selection diagram sent it to the default path | Diagram separates preference from requirement |
| 7 | One state diagram for all resources | BetterXen cannot see GPU use of guest-owned objects | Diagram limited to BetterXen-owned objects |
| 8 | Authority has retire and migration operations | Overengineering: nothing proposed needs them | Reduced to reserve, commit, rollback, release |
| 9 | Stage 1 builds a selection function whose only result is "default" | An abstraction with one implementation | Stage 1 is a written contract and reason-coded diagnostics; code boundary only with a second provider |
| 10 | Option A improves existing transfers | BetterXen performs no transfer between `xmem` and D3D12; only `xcrdapi` copies | A restated as "keep ownership, fix correctness" |
| 11 | Sentinel behaviour rests on one probe | It also matches the [public contract](https://learn.microsoft.com/en-us/windows/win32/api/d3d12/nf-d3d12-id3d12object-setprivatedatainterface): released when the object is destroyed or the same GUID is overwritten. Timing and thread are unspecified | Cited; risks added |
| 12 | Rollback is "revert" | Enforcement needs a runtime switch; `bx-cli` clears the environment, so a new variable must be passed explicitly | Added to 0b |
| 13 | Performance risks are generic | Concrete: #486 calls `bx_xmem_get_internal_stats`, which walks every tracked virtual commit, on each `CreateHeap`; an authority would share `xmem`'s lock | Added to risks |

## The strongest case against the recommendation

1. **The problem may be hypothetical.** BetterXen makes no duplicate copy today. No title runs, so nothing shows that memory or transfers are a limit.
2. **The hybrid keeps every hard case.** Textures, address-based placement and alignment all still need translation, so a mapped path may cover little.
3. **Enforcing a graphics cap can only reduce compatibility** until console-equivalent sizes are known, and they are not.
4. **A cap that can be bypassed is a weak guarantee** to buy with an ABI change.
5. **Two paths cost more than one**, and the second has no eligible caller.

These do not remove the value of observing accounting and lifetime. They do argue against committing to anything past Stage 1.

## Traceability

| Recommendation | Rests on | Strength |
| --- | --- | --- |
| Observe graphics allocations and their destruction | `main` source: nothing counted or tracked | Source |
| Do not credit on wrapper release | #486 reproduction | One host, author-run |
| Never enforce from a snapshot | #486 reproduction; research source analysis | One host, author-run |
| Sentinel as release signal | Public contract plus probe | Contract; one host |
| Never move exposed pointers | `Map` takes no address; existing-memory import returns `E_NOTIMPL` | Contract; research, M4 and M5 |
| Textures need their own path | Opaque layouts | Contract; research |
| Mapped provider only opt-in, only if eligibility is solved | Eligibility investigation | Judgement |
| `xmem` as authority | It already holds the lock and limit | Judgement |
| Evaluate AMD's allocator before writing one | Research allocator trial | Bounded; M4, M5 partly |

## Conflicts with contributor work

`#486` (`devil8blade`) already charges heaps and wraps them. `#471` and `#486` both edit `d3d12_xs/CMakeLists.txt` and `proxy.cpp`. A signature change in `xmem` would touch Track B, `test_xmem` and `test_integration_boot`; nobody has claimed it. Nothing here takes over claimed work.

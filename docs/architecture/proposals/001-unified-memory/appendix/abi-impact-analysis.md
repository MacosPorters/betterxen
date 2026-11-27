# ABI impact analysis

> **DRAFT — FOR MAINTAINER DISCUSSION ONLY — NOT APPROVED FOR IMPLEMENTATION**

*Appendix. Deferred to the pre-M3 discussion; the [main page](../README.md) governs.*

No frozen file is changed by this proposal. Anything below would need its own `[ABI]` pull request.

**Observing needs no ABI change.** `d3d12_xs` can count the objects it sees. A change is needed only to enforce one cap.

## Can admission be unified without an ABI change?

| Option | How | Verdict |
| --- | --- | --- |
| Snapshot then private add (#486) | `bx_xmem_get_internal_stats` plus a `d3d12_xs` counter | **Not sufficient:** one-way visibility; reproduced exceeding the cap |
| Shadow commit | `d3d12_xs` calls the existing `XMemVirtualAlloc` with `MEM_COMMIT` and `PAGE_NOACCESS` for the same size, and `VirtualFree`s it on release | Atomic in both orders with **no ABI change**. Costs address space and commit charge, makes `xmem`'s statistics mix real and shadow memory, and lengthens the `VirtualQuery` reconciliation walk. Untested |
| `XMemAllocatePhysicalPages` as a token | Existing export | Rejected: creates real file-mapping sections and rounds to 64 KiB or 2 MiB |
| New reservation exports | See below | **Recommended**: explicit and observable |

The shadow commit is a genuine fallback if the maintainers want the correctness fix before an `[ABI]` change. It should not become permanent.

## What needs a contract change

| Concern | Needs `abi/` change? | Without it |
| --- | --- | --- |
| Lifetime notifications | **No** | A private-data sentinel stays inside `d3d12_xs` |
| Resource ownership metadata | No while it stays inside `d3d12_xs` | Private tables |
| Allocation diagnostics (0a) | No | Existing log call; Track A parses |
| Combined-budget reserve, commit, rollback | **Yes** for a durable design (M3, 0b) | Interim shadow commit, untested |
| Enforcement switch | Yes: a name in `bx_config.h`, or a guest environment variable read directly as `xcrdapi` does, which `bx-cli` passes only with `--inherit-env` | — |
| Provider eligibility | Unknown until eligibility is resolved | Do not design it yet |

All changes are unapproved.

## Illustrative additive contract (not agreed)

```c
/* bx_xmem_internal.h, illustrative only */
typedef struct bx_xmem_charge_stats {
    uint64_t struct_size;
    uint64_t external_reserved_bytes;
    uint64_t external_pending_bytes;
    uint64_t peak_total_bytes;
} bx_xmem_charge_stats_t;

int  bx_xmem_try_charge(uint64_t bytes, uint32_t charge_class); /* 1 admitted, 0 refused */
void bx_xmem_release_charge(uint64_t bytes, uint32_t charge_class);
void bx_xmem_get_charge_stats(bx_xmem_charge_stats_t* out);
```

Design points for the `[ABI]` discussion:

- Add functions and a **new** struct. Do not extend `bx_xmem_stats_t`: `XMemGetWorkingSetStatistics` writes it into caller buffers.
- `try_charge` must share `can_commit`'s lock.
- Byte-count release can underflow on caller bugs; token-based release costs a table but catches them.
- "Pending" can be a class, not a separate call.
- Additive exports do not change existing signatures. `abi/README.md` requires a `BX_ABI_VERSION` bump for breaking changes; maintainers should confirm this is not one.
- `abi/defs/xmem.def` gains entries, so the strict export verifier and export fixtures change with it.

## Other possible contract changes

| Need | File | Stage |
| --- | --- | --- |
| Provider on/off switch | `bx_config.h` environment name, and the launcher's passed variables | 2 |
| Memory-attribute capture in region info | `bx_xmem_region_info_t` (breaking layout change: add a new query instead) | 2, only if [D1](maintainer-decisions.md#d1-what-makes-an-allocation-eligible) needs it |
| `d3d12_xs` asking `xmem` to adopt a mapped range | New `xmem` export | 2 |
| Typed configuration block for `D3D12ConfigureVirtualMemoryX` | The console layout is not publicly established; #486's `BXVM` block is BetterXen-defined and lives outside `abi/` | Decide with D1 |

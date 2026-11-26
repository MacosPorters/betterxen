# Evidence handoff for placed-resource integration

This narrow handoff informs [#464](https://github.com/MacosPorters/betterxen/issues/464) and [PR #486](https://github.com/MacosPorters/betterxen/pull/486) without changing or certifying production code. The PR's `vmem.h` and `vmem.cpp` were read at `6308fdc0462d14f1f0b732e21dbc45329fb034c9`; this is boundary inspection, not a full independent implementation review. Its reported CrossOver 26.2/vkd3d runs are not the CrossOver 26.3/D3DMetal 3.0 environment used here.

| Integration question | Evidence-supported handoff |
| --- | --- |
| Can caller-owned xmem memory be wrapped? | Both existing-memory heap opens returned E_NOTIMPL in published version-scoped baseline observations. Keep this explicit; no bridge has been demonstrated. |
| What does mapped ownership provide? | A D3D12-owned WB/L0 buffer exposes a persistent CPU pointer and supports observed synchronized buffer access/alias inheritance. This is a candidate allocation provider, not evidence that arbitrary fixed/pre-device/scatter mappings are satisfied. |
| Can heap alignment be used as CPU alignment? | No. A2 mapped-pointer receipts include 16384 and 49152 modulo 65536 despite 64 KiB heap/resource alignment. Validate the actual requested CPU contract separately. |
| Does the configured block preserve an old address? | The inspected PR publishes a newly mapped cpu_pointer. That is not proof of an alias at existing base_va or preservation of its previous allocation contents/attributes. Distinguish the BetterXen-defined synthetic configuration from a verified console ABI. |
| May successful creation validate placement? | The M4 runtime accepted misaligned/out-of-bounds creation calls that were never GPU-used. Validate allocation-info alignment and bounds before forwarding. The PR's create_placed has explicit checks; no claim is made that this audit independently certifies all edge cases. |
| What do texture results cover? | Correct mixed-heap mip copies and RT/depth clear/alias readbacks in the tested configurations. Opaque texture layout, raw guest-pointer aliasing, sampling, console tiling and other formats remain separate. |
| What is released from the budget? | Logical heap/slice accounting and observed host/driver retention are separate quantities. Resource, command and device release checkpoints can retain graphics charges. Avoid calling that a proven leak or immediate physical reclamation. |
| Does pooling justify adoption? | It reduces the allocation-call component in this synthetic trace, with cold setup/slack costs and small/noisy cycle differences. It is not a demonstrated production speedup or memory reduction. |
| What may be claimed about copies? | Synchronized access, same-address alias behavior, footprint slopes and region labels do not establish physical page identity or universal absence of copies. The inspected PR's stronger “true shared”/“no extra copy” language is not established by #473. |

The maintainer's #464 claim remains respected. No forced ADR/ABI sequence, new dependency/blocking gate, production patch or approval of PR #486 is implied. Independent acceptance of A and actual source/raw review of B are pending. The underlying research is limited to original synthetic workloads on stated versions, with no Xbox hardware or game-execution claim.

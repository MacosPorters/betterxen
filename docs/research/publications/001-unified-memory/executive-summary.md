# Executive summary

## One minute

- Apple Silicon has one physical memory pool shared by CPU and GPU.
- That does not mean software stops copying. A translation layer can still keep a second buffer, stage data or convert layouts.
- We tested the D3D12 interfaces that CrossOver translates to Metal through D3DMetal.
- Buffers allocated by D3D12 and left mapped were visible to both sides, with a fence (a "GPU is done" signal) between them, on an M4 and an M5.
- Memory the CPU had already allocated could not be handed to D3D12: both import calls returned "not implemented".
- So the promising direction is the reverse: let D3D12 own eligible buffers and give the CPU a pointer into them.
- Reusing memory from a pool made some allocation calls much cheaper in a synthetic test. No overall speedup was shown.
- Textures and several allocation types need separate paths.
- Before any optimization: the memory budget must be counted correctly, pointer alignment checked and resource lifetimes enforced.

**Why it could matter (potential, not measured):** less duplicated memory, fewer transfers, less allocation churn, better behavior on 16 GB Macs, and possibly better performance and stability.

**What is not shown:** physical zero-copy, any memory saving, any FPS change, or that a game runs.

## Five minutes

| Question | Answer | Strength |
| --- | --- | --- |
| Can D3D12 wrap existing CPU memory? | No on the tested versions (`E_NOTIMPL`) | Reproduced on M4 and M5; version-specific |
| Does a persistently mapped buffer work across CPU and GPU? | Yes, with fences | Accepted within scope |
| Does a small copy cost as much as copying the whole buffer? | No. About 0.18 to 0.40 ms against 2.09 to 2.36 ms for a 64 MiB buffer (median, M5) | One workstream's runs; rules out only that specific cost |
| Is the mapped pointer aligned to 64 KiB? | Not reliably | Accepted |
| Does a successful create call mean the placement was valid? | No. Invalid placements returned success | The rejection gate failed on both hosts |
| Does pooling help? | Allocation calls, yes. Whole workload, not demonstrated | Author-tested |
| Is memory returned after release? | Large graphics allocations stayed charged to the process | One complete session; not shown to be a leak |
| Is the budget counted correctly in the proposed code? | No, in unmerged PR #486: it can exceed the cap | Source-derived; not run |

13 of 29 register entries have independent acceptance, each within a narrow scope. 11 are unknown, 3 failed and 2 are unsupported. "Independent" means the other contributor's side; the work and the reviews were largely done by AI agents under the contributors' direction.

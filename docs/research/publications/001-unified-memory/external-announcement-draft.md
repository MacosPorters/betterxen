# Draft: notes for Apple Silicon porting developers

**Draft only. Not published. Sharing it outside the repository needs separate authorization from the maintainers and the contributors involved.**

---

## What we saw when testing D3D12 memory behavior on Apple Silicon

We ran small original D3D12 programs under CrossOver 26.3 with D3DMetal 3.0 on an M4 (24 GiB, macOS 27.0) and an M5 (16 GiB, macOS 27.2.1). These are observations about that stack, not about Metal or Apple hardware in general.

**Specific to the tested versions**
- Wrapping existing memory (`OpenExistingHeapFromAddress`, `OpenExistingHeapFromFileMapping`) returned `E_NOTIMPL`.
- `ARCHITECTURE1` reported `UMA=0` and cache-coherent UMA `0`. Do not infer the physical topology from it.
- Custom write-back and write-combined L0 heaps could be created and persistently mapped; fenced CPU/GPU copies matched.
- Mapped CPU pointers were not reliably 64 KiB aligned, although GPU addresses and heap placement were.
- Some misaligned or out-of-bounds placed-buffer creations returned success. Validate on your side.
- D3D12 timestamp resolves did not write their results.
- Large released graphics allocations stayed charged to the process footprint. We have not shown this to be a leak.

**General engineering principles, which we relied on and did not discover**
- Unified memory does not by itself remove copies in a translation layer.
- A usable mapped pointer does not prove two sides share physical pages.
- Flat latency does not prove there is no copy.
- Reuse and free only after the fence for the last GPU use.
- Pool statistics, process footprint and physical residency are different measurements.
- Check what an API actually returns instead of assuming alignment or support.

**What we did not show:** physical zero-copy, memory savings, speedups, game compatibility, behavior on M1 to M3, other CrossOver or D3DMetal versions, MoltenVK, DXVK or VKD3D-Proton, or other translation layers.

Most of the work was done by AI agents directed by human contributors. If you can reproduce or contradict any of this on another configuration, we would like to know.

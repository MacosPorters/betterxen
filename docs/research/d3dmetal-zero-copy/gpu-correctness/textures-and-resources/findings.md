# Texture, placement and alias observations

Local author results on Apple M4, CrossOver 26.3.0.39832, Wine engine 11.0-8726-g2e2f5fca349 and loaded D3DMetal 3.0. Two fresh processes used changing input datasets. [Raw matrix records](../../results/2026-11-26-apple-m4-arths17-a22-texture-matrix/receipt.json) include source/PE hashes and backend receipts. Independent review/reproduction remains pending.

| Configuration | Exact observed data | Scope |
| --- | --- | --- |
| UNKNOWN RGBA8, 65x37, three mips, disjoint buffer on same heap | WB/WC/DEFAULT each matched 12500/12500 texel bytes and 16384/16384 buffer guard words per session | WB/WC use per-mip CPU subresource writes; DEFAULT uses GPU upload; all use GPU readback |
| UNKNOWN R8_UNORM, same dimensions/mips/mixed placement | Each heap type matched 3125/3125 texel bytes and all guards in each session | Padded 256-byte row pitches, no padding treated as pixels |
| UNKNOWN R32_UINT, same dimensions/mips/mixed placement | Each heap type matched 12500/12500 texel bytes and all guards in each session | Bit-exact integer texture copies, not sampling |
| Overlapping RGBA8 render targets, WB/DEFAULT | Both aliases matched 2405/2405 pixels after distinct endpoint-color clears in each session | Explicit alias barrier and post-activation clear, then copy/readback |
| Overlapping D32_FLOAT depth resources, WB/DEFAULT | Both aliases matched 2405/2405 words for exactly representable 0.5/0.25 clears in each session | Depth clear/copy only; no stencil or rasterized depth comparison |
| Buffer placement at offset 1 | S_OK in both sessions | Caller-invalid alignment; never GPU-used |
| 64 KiB buffer at offset 65536 in 64 KiB heap | S_OK in both sessions | Caller-invalid bounds; never GPU-used |
| Texture on buffer-only heap | E_INVALIDARG in both sessions | Negative creation observed rejected |

All valid positive cases above matched, but each overall matrix process exited 1 because two negative creation calls unexpectedly succeeded. No failures were removed or reclassified. The first smoke process stopped at the first such acceptance and is [preserved separately](../../results/2026-11-26-apple-m4-arths17-a22-texture-first-failure/receipt.json). The later source logs all negative outcomes before reporting the same failing gate.

Interpret this carefully: [CreatePlacedResource](https://learn.microsoft.com/en-us/windows/win32/api/d3d12/nf-d3d12-id3d12device-createplacedresource) imposes valid offset/alignment/bounds preconditions on callers. It does not promise a specific rejection HRESULT for every invalid call without validation tooling. Therefore these observations show that S_OK is insufficient as an application validation boundary; they do not independently prove a runtime conformance bug or permission to use invalid placements. The unchanged failed negative-rejection gate is explicit in the report.

## Buffer inheritance follow-up

The invalid acceptances prompted a separate valid test of whether correct overlapping placements preserve the public buffer-alias contract. [Microsoft's memory-aliasing/data-inheritance documentation](https://learn.microsoft.com/en-us/windows/win32/direct3d12/memory-aliasing-and-data-inheritance) describes inheritance for defined layouts and compatible resource states. This fixture uses the advanced model and explicit A-to-B/B-to-A alias barriers.

[Two fresh processes](../../results/2026-11-26-apple-m4-arths17-a22-buffer-inheritance/receipt.json) each performed three changing GPU-copy/alias/readback rounds. Two 64 KiB buffers at heap offset 0 returned identical CPU Map pointers and GPU virtual addresses; a disjoint buffer at offset 65536 returned corresponding +65536 addresses. Every source and inherited buffer matched 16384/16384 words in all rounds, and all disjoint guard words matched. No CPU alias dereference is used to manufacture the GPU result. Both processes exited 0.

This establishes the observed application-level buffer alias behavior for this configuration. It neither identifies physical pages nor supplies arbitrary requested CPU addresses, independent section handles or xmem scatter aliases. B must independently review the advanced-model expectation and barriers before acceptance. The RT/depth fixture reinitializes each alias; it does not test texture data inheritance.

## Storage choices and untested cases

CPU-visible and DEFAULT texture paths both produced correct copies/clears in this matrix. No matched bandwidth, sampling/rendering performance, physical texture-footprint comparison or internal Metal storage-mode identification was performed. B's bandwidth/copy/coherence evidence is still required for storage recommendations. A successful buffer map is not a raw pointer to UNKNOWN-layout texels; Map is called with null ppData for CPU texture access.

The historical valid ROW_MAJOR shared-cross-adapter one-mip control remains in the evidence inventory; no identical repeat was needed here. Ordinary ROW_MAJOR CPU-visible heaps, arbitrary format reinterpretation, compressed/planar formats, arrays/3D, MSAA, stencil, sampled filtering and console tiling remain unsupported by this evidence. Do not treat not-run configurations as successes or generalize these synthetic readbacks to Xbox workloads.

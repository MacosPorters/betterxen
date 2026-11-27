# Allocation eligibility investigation

> **DRAFT — FOR MAINTAINER DISCUSSION ONLY — NOT APPROVED FOR IMPLEMENTATION**

*Appendix. Deferred to the pre-M3 discussion; the [main page](../README.md) governs.*

Question: can BetterXen know, from public contracts and existing code, that an allocation will be used by the GPU, before the guest receives its pointer?

**Answer: there is a candidate signal, but no reliable end-to-end mechanism yet.** The migration should stop at the provider-abstraction stage.

## Sources used and not used

| Source | Status |
| --- | --- |
| Microsoft [Xbox-GDK-Samples](https://github.com/microsoft/Xbox-GDK-Samples/tree/b1e77b6ac10f95b07296025fda171ca2ddc42dbe) (MIT, commit `b1e77b6`) | Read as static observations; nothing copied, built or run |
| Public Windows headers (`winnt.h` as shipped by mingw-w64 14.0.0) | Read |
| Microsoft Learn GDK reference pages for `XALLOC_ATTRIBUTES`, `XMemVirtualAlloc`, `XMemAllocatePhysicalPages`, `XMemAlloc`, D3D12 `XMemAlloc` object types | **Access restricted** to an NDA developer programme. Not read; no other route tried |
| Third-party projects that reverse-engineer retail game executables (surfaced by a web search) | **Not opened or used** (AGENTS.md §4). Nothing here derives from them |

## What the public samples show

| Observation | Where |
| --- | --- |
| `XMemVirtualAlloc` is called with **five** arguments: address, size, allocation type, an attribute such as `XMEM_GRAPHICS`, then page protection | [TransientAllocator.cpp#L39](https://github.com/microsoft/Xbox-GDK-Samples/blob/b1e77b6ac10f95b07296025fda171ca2ddc42dbe/Samples/Graphics/AdvancedESRAM/TransientAllocator.cpp#L39), [PipelinedPostprocess.cpp#L832](https://github.com/microsoft/Xbox-GDK-Samples/blob/b1e77b6ac10f95b07296025fda171ca2ddc42dbe/Samples/Graphics/PipelinedPostprocess/PipelinedPostprocess.cpp#L832), [DirectStorageTextureShuffling.cpp#L599](https://github.com/microsoft/Xbox-GDK-Samples/blob/b1e77b6ac10f95b07296025fda171ca2ddc42dbe/Samples/Graphics/DirectStorageTextureShuffling/DirectStorageTextureShuffling.cpp#L599) |
| The protection argument includes `PAGE_GRAPHICS_READWRITE` | same |
| `XMemAlloc` attributes can be built with a graphics memory type (`XALLOC_MEMTYPE_GRAPHICS_*`) | [DirectXTexXboxD3D12X.cpp#L30](https://github.com/microsoft/Xbox-GDK-Samples/blob/b1e77b6ac10f95b07296025fda171ca2ddc42dbe/Kits/DirectXTex/DirectXTexXboxD3D12X.cpp#L30), [FBC_GPU.cpp#L48](https://github.com/microsoft/Xbox-GDK-Samples/blob/b1e77b6ac10f95b07296025fda171ca2ddc42dbe/Samples/Graphics/FastBlockCompress/FBC_GPU.cpp#L48) |
| The returned address is then passed to `CreatePlacedResourceX`, for buffers **and textures** | [DirectStorageTextureShuffling.cpp#L610](https://github.com/microsoft/Xbox-GDK-Samples/blob/b1e77b6ac10f95b07296025fda171ca2ddc42dbe/Samples/Graphics/DirectStorageTextureShuffling/DirectStorageTextureShuffling.cpp#L610) |

So the console programming model, as far as these samples show it, decides GPU use **at allocation time** and later places resources **at that address**.

## What the public headers give

`winnt.h` defines `PAGE_GRAPHICS_NOACCESS` `0x0800`, `READONLY` `0x1000`, `READWRITE` `0x2000`, further execute variants up to `0x10000`, `COHERENT` `0x20000` and `NOCACHE` `0x40000`. No public value was found for `XMEM_GRAPHICS` or `XALLOC_MEMTYPE_*`.

## Possible signals

| Signal | Value public? | BetterXen sees it today? | Covers |
| --- | --- | --- | --- |
| `PAGE_GRAPHICS_*` bits in the protection argument | Yes | **No** (see below) | `XMemVirtualAlloc` calls that set them |
| `XMEM_GRAPHICS` attribute argument | No | Receives it where it expects the protection | Unknown |
| `XALLOC_MEMTYPE_GRAPHICS_*` inside `XMemAlloc` attributes | No (encoding unknown) | Receives the word, cannot decode it | Unknown |
| BetterXen-defined opt-in | Ours | Yes | No existing title |

## A discrepancy on `main` that needs maintainer attention

BetterXen declares `XMemVirtualAlloc(address, size, alloc_type, protect)` ([`xmem.cpp:180`](../../../../../shims/xmem/xmem.cpp#L180)). The three official call sites pass five arguments. If their declaration matches those calls, on x86-64:

- BetterXen reads the **attribute** argument as the page protection;
- the real protection, including the graphics bits, is passed on the stack and ignored;
- the existing fallback to `PAGE_READWRITE` may hide this when the attribute is not a single protection bit.

The comment at [`xmem.cpp:184`](../../../../../shims/xmem/xmem.cpp#L184) and `test_xmem` assume a value such as `0x21` arrives in argument 4; the origin of that value is not recorded in the file.

**Limits:** this is inferred from call sites. The declaration itself is in restricted documentation or SDK headers and was not read. It was not tested with any real title. This proposal changes nothing; it is for the maintainers to assess (Track B).

## Why this is not yet reliable

1. The argument carrying the public bits is not read today.
2. The consumer, `CreatePlacedResourceX`, has no public ABI here (no declaration in public `d3d12.h`, no known vtable slot), so BetterXen cannot implement or observe it.
3. Textures are placed at the same addresses. A host buffer cannot back an opaque host texture, so those need translation whatever the allocation path is.
4. A mapped D3D12 pointer is not reliably 64 KiB aligned, while the samples request 64 KiB pages.
5. Reserve-then-map forms (`MEM_RESERVE` with `XMEM_MAPPABLE`) cannot be expressed by `Map`.
6. Whether every GPU-used allocation sets any of these signals is unknown.

## What would change the answer

- A lawful, public source for the declaration and the consumer's ABI.
- A synthetic design, agreed by maintainers, showing how a placed-at-address texture would be translated and when its copies would happen.
- A matched measurement showing a mapped path beats keeping `xmem` memory and copying.

Until then: build nothing that depends on eligibility.

# Resource, placement and lifetime contract

Workstream A's public-contract derivation for [#473](https://github.com/MacosPorters/betterxen/issues/473), read 2026-11-26. Class B describes API contracts, not console semantics or implementation certification. B's independent synchronization review is requested and pending. A-local helper code is isolated; it is not an agreed replacement for shared helpers.

## Ownership and addressing

[CreatePlacedResource](https://learn.microsoft.com/en-us/windows/win32/api/d3d12/nf-d3d12-id3d12device-createplacedresource) takes a heap-relative offset, with resource allocation alignment and bounds. It accepts no requested CPU virtual address. [GetResourceAllocationInfo](https://learn.microsoft.com/en-us/windows/win32/api/d3d12/nf-d3d12-id3d12device-getresourceallocationinfo%28uint_uint_constd3d12_resource_desc%29) gives adapter/resource allocation size and alignment; these are not CPU Map-pointer guarantees. Ordinary buffers round their allocation size to 64 KiB; texture requirements depend on the adapter.

[Map](https://learn.microsoft.com/en-us/windows/win32/api/d3d12/nf-d3d12-id3d12resource-map) supports persistent mapping until the resource's final reference is released. It does not promise normal pointer capabilities or a fixed address across unnested maps. The allocation owner must retain the resource and heap for any live guest pointer; callers cannot free or decommit an interior mapped pointer using ordinary VirtualFree. A 64 KiB heap offset and a 64 KiB-aligned CPU pointer are different conditions.

## Placement, aliases and initialization

For a simple alias model, activate the resource with an aliasing barrier, invalidate overlapping resources and initialize render/depth resources after activation. Full clear or full-subresource copies satisfy the required initialization. An alias switch has no implicit data-inheritance assumption. A's texture alias fixture uses two render/depth resources at the same heap offset in one command list, clears each after its own activation and reads each before the next activation. Disjoint buffer/texture cases retain and verify a separate buffer guard.

The API also has an advanced model: an ExecuteCommandLists boundary can provide alias-ordering guarantees between separate invocations. This does not excuse concurrent overlapping access or eliminate required texture initialization. A's fixtures explicitly issue aliasing barriers to keep their intended use inspectable. [CreatePlacedResource remarks](https://learn.microsoft.com/en-us/windows/win32/api/d3d12/nf-d3d12-id3d12device-createplacedresource).

## Retirement and synchronization handoff

CPU writes finish before GPU submission. CPU reads, overwrites, destruction and allocator reuse occur only after completion of the last queue fence using that range. A's allocation trace holds a real queued copy behind a short CPU-signaled gate, observes its retirement value still pending, withholds the allocation, releases the gate and waits with a timeout before validating/releasing. This tests the fixture's delayed-retirement policy; it does not exhaust every multi-queue lifetime case. No unsafe free or data race is deliberately executed.

[CreateHeap](https://learn.microsoft.com/en-us/windows/win32/api/d3d12/nf-d3d12-id3d12device-createheap) requires the GPU to stop using the heap before its final reference is released; placed resources hold heap references. Process-accounted memory after resource release alone cannot distinguish application references, runtime caches and driver retention. The corrected accounting fixture records resource release, disposal of all command objects/output/pipeline, a settled retirement checkpoint and device release separately. A high remaining footprint is not by itself a leak.

B owns common mapping/synchronization/tracking contracts and implementation. Review requested: CPU store completion, transition states, fence error paths, alias activation, reset/retirement ordering and queue-gate cleanup. A owns resource lifetime scenarios and observed accounting, not general CPU/GPU atomic interoperability. No concurrent CPU/GPU polling test is included.

## Texture copies and storage

[GetCopyableFootprints](https://learn.microsoft.com/en-us/windows/win32/api/d3d12/nf-d3d12-id3d12device-getcopyablefootprints) returns copy-buffer offsets, padded row pitches and unpadded row bytes. These describe transfer layouts, not opaque texture backing. Fixtures test offsets in 512-byte units, row pitches in 256-byte units and exact pixel bytes without interpreting padding as texels. CPU texture writes occur in COMMON; GPU reads use COPY_SOURCE and readback buffers stay COPY_DEST.

[Texture layouts](https://learn.microsoft.com/en-us/windows/win32/api/d3d12/ne-d3d12-d3d12_texture_layout) distinguish UNKNOWN from restricted cross-adapter ROW_MAJOR. Ordinary opaque textures do not expose a conventional texel pointer through Map; requesting a null output pointer is not an experiment proving pointer-return failure. The historical valid ROW_MAJOR control uses a shared cross-adapter DEFAULT heap and ALLOW_CROSS_ADAPTER, one mip/slice, no depth/MSAA. Ordinary UPLOAD/READBACK textures are excluded by the [heap-type contract](https://learn.microsoft.com/en-us/windows/win32/api/d3d12/ne-d3d12-d3d12_heap_type).

Apple's [storage-mode documentation](https://developer.apple.com/documentation/metal/choosing-a-resource-storage-mode-for-apple-gpus) distinguishes CPU/GPU-accessible shared resources from GPU-only private resources even on unified-memory hardware. That is a Metal contract; it does not establish which Metal mode an opaque D3DMetal D3D12 heap uses. DEFAULT versus CUSTOM is a D3D12 test comparison, not a measurement of the underlying Metal storage mode.

## Buffer data inheritance scope

The separate buffer fixture uses overlapping placed buffers with fully defined ROW_MAJOR layouts and compatible buffer states, an explicit alias barrier between each GPU write/read use, and full source initialization. It alternates the writer and verifies inherited data plus a disjoint guard. The expectation is derived from [Memory Aliasing and Data Inheritance](https://learn.microsoft.com/en-us/windows/win32/direct3d12/memory-aliasing-and-data-inheritance), rather than from the simple model's no-inheritance assumption. Its passing observations remain subject to B's independent source/contract review.

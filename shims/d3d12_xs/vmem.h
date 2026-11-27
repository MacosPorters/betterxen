#ifndef BETTERXEN_D3D12_XS_VMEM_H
#define BETTERXEN_D3D12_XS_VMEM_H

#include <stddef.h>
#include <stdint.h>
#include <windows.h>

/*
 * BetterXen-defined parameter block for D3D12ConfigureVirtualMemoryX.
 *
 * The export is PVOID and the frozen ABI has no size argument (abi/ is out of
 * scope). The first 8 bytes are therefore the minimum dereference: magic then
 * the struct size. Magic is 'BXVM' (0x4D565842). struct_size must equal
 * sizeof(BxD3d12VirtualMemoryConfig). Any other pointer, including null, is not
 * this struct: the call logs the unimplemented-API stub and returns S_OK. It
 * does not read past those 8 bytes and it does not write.
 *
 * A block whose magic and size match is then checked in full. version is 1.
 * flags may be 0 or BX_D3D12_VMEM_FLAG_BACKING_HEAP. Any other flag bit, or a
 * non-zero reserved word, is not this struct either (logged S_OK, no write).
 * A recognised block with a bad size or a base_va that bx_xmem_query_region
 * does not resolve still returns E_INVALIDARG and is not written.
 *
 * base_va is 0, or an address bx_xmem_query_region resolves. It is not a GPU pointer.
 * size_bytes is 0 or a 64 KiB multiple that does not exceed the active title limit.
 *
 * The call copies what it needs. The caller's pointer is not kept. cpu_pointer
 * is written only before this call returns: the persistent Map address of the
 * WRITE_BACK buffer if that backing heap already exists, otherwise 0. CreateHeap
 * does not write the block. A caller that needs the address after the backing
 * heap has been created calls D3D12ConfigureVirtualMemoryX again with the same
 * region (same base_va and size). That second call does not reset or recreate
 * the backing heap. It writes the current map into the new block. A nonzero
 * cpu_pointer is valid only while that backing buffer remains cached. Changing
 * the region or replacing the cache for another host device unmaps it, even if
 * the caller retains the heap. Callers must synchronize those operations with
 * access through the pointer and obtain a fresh pointer after replacement.
 *
 * CreateHeap returns that backing heap only when all of the following hold:
 * the block opted in with BX_D3D12_VMEM_FLAG_BACKING_HEAP, the region size is
 * non-zero, and the desc is the heap this file creates for the region:
 * Type CUSTOM, CPUPageProperty WRITE_BACK, MemoryPoolPreference L0,
 * SizeInBytes equal to the configured region, Alignment 0 or 64 KiB,
 * Flags exactly D3D12_HEAP_FLAG_NONE (the flag set used when the backing heap
 * is created), and CreationNodeMask and VisibleNodeMask each 0 or 1.
 * If the backing configuration changes during a backing request, CreateHeap
 * returns HRESULT_FROM_WIN32(ERROR_RETRY) and discards any newly allocated heap.
 * Any other CreateHeap is the host CreateHeap. The desc is not modified.
 * The live-heap budget charge is taken on create and dropped on the wrapper's
 * final Release. A resource created by CreatePlacedResource holds the wrapper
 * for its lifetime.
 * GetDevice on every wrapper returns the BxDeviceProxy. An ordinary heap AddRefs
 * that proxy until the wrapper's final Release, matching BxChildProxy, so the
 * caller can release the device and keep the heap. The process-wide backing heap
 * does not AddRef. The cache would keep the device and its charge alive. That
 * pointer is weak. It is cleared when the heap leaves the cache (a new region
 * or a different host device) and when that proxy's last reference drops.
 * CreatePlacedResource1 and CreatePlacedResource2 unwrap the wrapper to the
 * host heap before the host call. The cache records the host ID3D12Device
 * that built the heap. CreateHeap for a different host device releases the
 * heap and its buffer, credits the charge, and builds a new one. CreateHeap
 * for the same host device returns the cached heap. The check, the retarget,
 * and the caller's reference happen in one critical section.
 *
 * The layout must match tests/conformance/test_d3d12_placed/main.cpp.
 */
constexpr uint32_t BX_D3D12_VMEM_MAGIC = 0x4D565842u;
constexpr uint32_t BX_D3D12_VMEM_FLAG_BACKING_HEAP = 0x1u;

struct BxD3d12VirtualMemoryConfig {
    uint32_t magic;
    uint32_t struct_size;
    uint32_t version;
    uint32_t flags;
    uint64_t base_va;
    uint64_t size_bytes;
    uint64_t reserved;
    uint64_t cpu_pointer;
};
static_assert(sizeof(BxD3d12VirtualMemoryConfig) == 48, "config block is 48 bytes");
static_assert(offsetof(BxD3d12VirtualMemoryConfig, magic) == 0, "magic offset");
static_assert(offsetof(BxD3d12VirtualMemoryConfig, struct_size) == 4, "struct_size offset");
static_assert(offsetof(BxD3d12VirtualMemoryConfig, version) == 8, "version offset");
static_assert(offsetof(BxD3d12VirtualMemoryConfig, flags) == 12, "flags offset");
static_assert(offsetof(BxD3d12VirtualMemoryConfig, base_va) == 16, "base_va offset");
static_assert(offsetof(BxD3d12VirtualMemoryConfig, size_bytes) == 24, "size_bytes offset");
static_assert(offsetof(BxD3d12VirtualMemoryConfig, reserved) == 32, "reserved offset");
static_assert(offsetof(BxD3d12VirtualMemoryConfig, cpu_pointer) == 40, "cpu_pointer offset");

extern "C" HRESULT bx_d3d12_configure_virtual_memory(void* config);

/* Patches the device proxy vtable so CreateHeap and CreatePlacedResource run the code in vmem.cpp. */
bool bx_vmem_install_device_hooks(void* device);

/* 1->0 Release of a device proxy, under g_section. Compare-exchanges the count from
   1 to 0 and clears every weak backing-heap pointer aimed at that proxy. Returns false
   when the count was no longer 1, so the caller retries. A true return is the only
   path into final cleanup. */
bool bx_vmem_final_release(void* device_proxy);

/* AddRefs the host ID3D12Device that created the cached backing heap, or returns null.
   The test sets BX_D3D12_REUSE_BACKING_HOST=1 so a second proxy wraps this same host
   while the cache still holds it. */
void* bx_vmem_addref_backing_host();

#endif /* BETTERXEN_D3D12_XS_VMEM_H */

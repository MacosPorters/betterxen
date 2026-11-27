#ifndef BETTERXEN_BX_XMEM_INTERNAL_H
#define BETTERXEN_BX_XMEM_INTERNAL_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/*
 * Internal cross-DLL contract between Track B (xmem.dll) and Track C (d3d12_xs.dll).
 * Allows d3d12_xs placed-resource and virtual-memory translation to query allocations
 * created through XMemVirtualAlloc / XMemAllocatePhysicalPages / XMemMapPhysicalPages.
 */

typedef struct bx_xmem_region_info {
    uint64_t base_va;
    uint64_t size_bytes;
    uint64_t page_size_bytes;
    uint32_t protect_flags;
    uint32_t mem_type_flags;
    uint64_t physical_tag;
} bx_xmem_region_info_t;

typedef struct bx_xmem_stats {
    uint64_t title_limit_bytes;
    uint64_t committed_virtual_bytes;
    uint64_t allocated_physical_bytes;
    uint64_t peak_committed_bytes;
} bx_xmem_stats_t;

/* Query the XMem region covering `va`. Returns 1 if tracked, 0 otherwise. */
int bx_xmem_query_region(uint64_t va, bx_xmem_region_info_t* out_info);

/* Snapshot current XMem usage against the active Lockhart/Anaconda cap. */
void bx_xmem_get_internal_stats(bx_xmem_stats_t* out_stats);

/*
 * Reserve bytes for a private allocation by another DLL in this process.
 * Admission is atomic with XMem allocation and includes both XMem usage and
 * all outstanding external reservations under the active title limit.
 * Returns 1 on success and 0 on refusal. Refusal leaves accounting intact.
 * Reserve before allocating, and hold the charge until the allocation and
 * all dependent objects are released. Roll back when allocation fails.
 */
int bx_xmem_try_reserve_external(uint64_t bytes);

/*
 * Credit a matching successful reservation once. Returns 1 on success, or
 * 0 if bytes exceeds outstanding external reservations (no accounting change).
 * A zero-byte operation succeeds without changing the budget.
 * These reservations do not change the XMem-only statistics struct above.
 * Provider implementation and activation in the xmem export table are paired.
 */
int bx_xmem_release_external(uint64_t bytes);

#ifdef __cplusplus
}
#endif

#endif /* BETTERXEN_BX_XMEM_INTERNAL_H */

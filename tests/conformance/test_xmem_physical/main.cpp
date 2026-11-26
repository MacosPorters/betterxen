#include <windows.h>

#include <cstdint>
#include <cstdio>

#include "bx_xmem_internal.h"

extern "C" {
PVOID WINAPI XMemAllocDefault(SIZE_T, uint64_t);
VOID WINAPI XMemFreeDefault(PVOID, uint64_t);
BOOL WINAPI XMemAllocatePhysicalPages(SIZE_T*, PULONG_PTR, uint64_t);
BOOL WINAPI XMemFreePhysicalPages(SIZE_T*, PULONG_PTR, uint64_t);
PVOID WINAPI XMemMapPhysicalPages(PVOID, SIZE_T, uint64_t, PULONG_PTR);
PVOID WINAPI XMemMapPhysicalPagesScatter(PVOID, SIZE_T, uint64_t, PULONG_PTR);
BOOL WINAPI XMemGetWorkingSetStatistics(PVOID, SIZE_T);
}

#define CHECK(condition)                                                        \
    do {                                                                        \
        if (!(condition)) {                                                     \
            std::fprintf(stderr, "xmem physical:%d: %s (Win32 error %lu)\n",      \
                         __LINE__, #condition, static_cast<unsigned long>(GetLastError())); \
            return false;                                                       \
        }                                                                       \
    } while (false)

bool check_aliases(SIZE_T page_size, uint64_t flags) {
    bx_xmem_stats_t stats{};
    CHECK(XMemGetWorkingSetStatistics(&stats, sizeof(stats)));
    CHECK(stats.allocated_physical_bytes == 0);

    SIZE_T count = 2;
    ULONG_PTR pages[2]{};
    // Invalid flags must be rejected cleanly before touching stats or tags.
    CHECK(!XMemAllocatePhysicalPages(&count, pages, 0x12345));
    CHECK(GetLastError() == ERROR_INVALID_PARAMETER);

    CHECK(XMemAllocatePhysicalPages(&count, pages, flags));
    CHECK(count == 2 && pages[0] && pages[1] && pages[0] != pages[1]);

    // Working-set statistics and internal stats must both reflect physical pages.
    bx_xmem_get_internal_stats(&stats);
    CHECK(stats.allocated_physical_bytes == count * page_size);
    CHECK(stats.peak_committed_bytes >= count * page_size);
    bx_xmem_stats_t ws_stats{};
    CHECK(XMemGetWorkingSetStatistics(&ws_stats, sizeof(ws_stats)));
    CHECK(ws_stats.allocated_physical_bytes == count * page_size);

    // Shared title budget cap: existing physical pages must block both full-cap
    // physical and full-cap heap allocations without allocating gigabytes.
    SIZE_T cap_pages = static_cast<SIZE_T>(stats.title_limit_bytes / page_size);
    CHECK(!XMemAllocatePhysicalPages(&cap_pages, pages, flags));
    CHECK(GetLastError() == ERROR_NOT_ENOUGH_MEMORY);
    CHECK(!XMemAllocDefault(static_cast<SIZE_T>(stats.title_limit_bytes), 0));
    CHECK(GetLastError() == ERROR_NOT_ENOUGH_MEMORY);
    bx_xmem_get_internal_stats(&stats);
    CHECK(stats.allocated_physical_bytes == count * page_size);

    CHECK(!XMemMapPhysicalPages(nullptr, count, 0x12345, pages));
    CHECK(GetLastError() == ERROR_INVALID_PARAMETER);

    auto* linear = static_cast<BYTE*>(XMemMapPhysicalPages(nullptr, count, flags, pages));
    CHECK(linear && reinterpret_cast<uintptr_t>(linear) % page_size == 0);
    ULONG_PTR reversed[] = {pages[1], pages[0]};
    auto* scatter = static_cast<BYTE*>(
        XMemMapPhysicalPagesScatter(nullptr, count, flags, reversed));
    CHECK(scatter && scatter != linear && reinterpret_cast<uintptr_t>(scatter) % page_size == 0);

    linear[7] = 0x41;
    linear[page_size + 7] = 0x73;
    CHECK(scatter[7] == 0x73 && scatter[page_size + 7] == 0x41);
    scatter[page_size + 30] = 0xf2;
    CHECK(linear[30] == 0xf2);

    // Duplicate tags are valid when mapping (intra-span ring-buffer/tile aliasing).
    ULONG_PTR dup_map[] = {pages[0], pages[0]};
    auto* dup_view = static_cast<BYTE*>(
        XMemMapPhysicalPagesScatter(nullptr, count, flags, dup_map));
    CHECK(dup_view && reinterpret_cast<uintptr_t>(dup_view) % page_size == 0);
    dup_view[13] = 0x9b;
    CHECK(dup_view[page_size + 13] == 0x9b && linear[13] == 0x9b);
    bx_xmem_region_info_t dup0{};
    bx_xmem_region_info_t dup1{};
    CHECK(bx_xmem_query_region(reinterpret_cast<uintptr_t>(dup_view), &dup0));
    CHECK(bx_xmem_query_region(reinterpret_cast<uintptr_t>(dup_view + page_size), &dup1));
    CHECK(dup0.physical_tag == pages[0] && dup1.physical_tag == pages[0]);
    CHECK(XMemMapPhysicalPagesScatter(dup_view, count, flags, nullptr) == dup_view);
    CHECK(!bx_xmem_query_region(reinterpret_cast<uintptr_t>(dup_view), &dup0));

    bx_xmem_region_info_t first{};
    bx_xmem_region_info_t alias{};
    CHECK(bx_xmem_query_region(reinterpret_cast<uintptr_t>(linear + 7), &first));
    CHECK(bx_xmem_query_region(reinterpret_cast<uintptr_t>(scatter + page_size + 7), &alias));
    CHECK(first.base_va == reinterpret_cast<uintptr_t>(linear));
    CHECK(alias.base_va == reinterpret_cast<uintptr_t>(scatter + page_size));
    CHECK(alias.physical_tag == first.physical_tag && first.physical_tag == pages[0]);
    CHECK(first.size_bytes == page_size && alias.page_size_bytes == page_size);
    CHECK(first.protect_flags == PAGE_READWRITE && first.mem_type_flags == MEM_MAPPED);

    // Rejected frees (stale or duplicate tags) must fail without modifying
    // page_array or disturbing live mappings.
    ULONG_PTR stale[] = {pages[0], 0};
    CHECK(!XMemFreePhysicalPages(&count, stale, flags));
    CHECK(GetLastError() == ERROR_INVALID_PARAMETER && stale[0] == pages[0] && stale[1] == 0);
    CHECK(linear[7] == 0x41);
    ULONG_PTR dup_free[] = {pages[0], pages[0]};
    CHECK(!XMemFreePhysicalPages(&count, dup_free, flags));
    CHECK(GetLastError() == ERROR_INVALID_PARAMETER
          && dup_free[0] == pages[0] && dup_free[1] == pages[0]);
    CHECK(linear[7] == 0x41 && scatter[7] == 0x73);

    CHECK(!XMemMapPhysicalPagesScatter(nullptr, count, flags, stale));
    CHECK(GetLastError() == ERROR_INVALID_PARAMETER);
    CHECK(!XMemMapPhysicalPages(linear, count, flags, pages));
    CHECK(linear[7] == 0x41 && scatter[7] == 0x73);

    // Partial/over-wide unmap must fail cleanly and leave mapped pages intact.
    CHECK(!XMemMapPhysicalPagesScatter(scatter, count + 1, flags, nullptr));
    CHECK(GetLastError() == ERROR_INVALID_PARAMETER);
    CHECK(bx_xmem_query_region(reinterpret_cast<uintptr_t>(scatter), &alias));
    CHECK(scatter[7] == 0x73 && scatter[page_size + 7] == 0x41);

    // Exact unmap succeeds; repeating unmap on the now-empty span must fail.
    CHECK(XMemMapPhysicalPagesScatter(scatter, count, flags, nullptr) == scatter);
    CHECK(!bx_xmem_query_region(reinterpret_cast<uintptr_t>(scatter), &alias));
    CHECK(!XMemMapPhysicalPagesScatter(scatter, count, flags, nullptr));
    CHECK(GetLastError() == ERROR_INVALID_PARAMETER);

    CHECK(XMemMapPhysicalPagesScatter(scatter, count, flags, reversed) == scatter);
    CHECK(scatter[7] == 0x73 && scatter[page_size + 7] == 0x41);
    CHECK(XMemFreePhysicalPages(&count, pages, flags));
    CHECK(!pages[0] && !pages[1]);
    CHECK(!bx_xmem_query_region(reinterpret_cast<uintptr_t>(linear), &first));
    CHECK(!bx_xmem_query_region(reinterpret_cast<uintptr_t>(scatter), &alias));
    CHECK(!XMemMapPhysicalPages(nullptr, count, flags, reversed));
    CHECK(GetLastError() == ERROR_INVALID_PARAMETER);

    bx_xmem_get_internal_stats(&stats);
    CHECK(stats.allocated_physical_bytes == 0);
    return true;
}

int main() {
    if (!check_aliases(64 * 1024, 0) || !check_aliases(2 * 1024 * 1024, 2 * 1024 * 1024)) {
        return 1;
    }
    std::puts("test_xmem_physical: passed");
}

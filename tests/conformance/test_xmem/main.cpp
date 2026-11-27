#include <windows.h>

#include <cstdint>
#include <cstdio>
#include <cstring>

#include "bx_log.h"
#include "bx_xmem_internal.h"

extern "C" {
PVOID WINAPI XMemAlloc(SIZE_T, uint64_t);
PVOID WINAPI XMemAllocDefault(SIZE_T, uint64_t);
VOID WINAPI XMemFree(PVOID, uint64_t);
VOID WINAPI XMemFreeDefault(PVOID, uint64_t);
/* The export takes five parameters so a guest built against the public
 * Xbox-GDK-Samples call sites can pass (address, size, alloc_type, XMEM_*,
 * PAGE_*). A four-argument caller must still work: the shim recognises the
 * shape of arg 4 (a single PAGE_* bit) and uses it as the protection.
 *
 * To exercise the real four-argument calling convention we take the export's
 * address and call it through a four-parameter function pointer, which is what
 * a four-argument caller's compiled code does. */
PVOID WINAPI XMemVirtualAlloc(PVOID, SIZE_T, DWORD, DWORD, DWORD);
using XMemVirtualAlloc4 = PVOID (WINAPI*)(PVOID, SIZE_T, DWORD, DWORD);
static XMemVirtualAlloc4 alloc4() {
    return reinterpret_cast<XMemVirtualAlloc4>(reinterpret_cast<void*>(&XMemVirtualAlloc));
}
SIZE_T WINAPI XMemVirtualQuery(LPCVOID, PMEMORY_BASIC_INFORMATION, SIZE_T);
BOOL WINAPI XMemTransferMemory(PVOID, LPCVOID, SIZE_T, uint64_t);
BOOL WINAPI XMemGetWorkingSetStatistics(PVOID, SIZE_T);
BOOL WINAPI XMemIsToolsMemAvailable();
}

#define CHECK(condition)                                                        \
    do {                                                                        \
        if (!(condition)) {                                                     \
            std::fprintf(stderr, "xmem:%d: %s (Win32 error %lu)\n", __LINE__,       \
                         #condition, static_cast<unsigned long>(GetLastError())); \
            return 1;                                                           \
        }                                                                       \
    } while (false)

int main() {
    SYSTEM_INFO sys{};
    GetSystemInfo(&sys);
    const SIZE_T page = sys.dwPageSize;
    const auto* config = bx_get_runtime_config();

    bx_xmem_stats_t stats{};
    CHECK(config && XMemGetWorkingSetStatistics(&stats, sizeof(stats)));
    char profile[16]{};
    GetEnvironmentVariableA("BX_PROFILE", profile, sizeof(profile));
    const uint64_t expected_limit = std::strcmp(profile, "anaconda") == 0
        ? 13500ULL * 1024 * 1024
        : 8ULL * 1024 * 1024 * 1024;
    CHECK(stats.title_limit_bytes == expected_limit);
    CHECK(stats.title_limit_bytes == config->title_memory_limit_bytes);
    CHECK(stats.committed_virtual_bytes == 0 && stats.allocated_physical_bytes == 0);
    CHECK(!XMemIsToolsMemAvailable());
    CHECK(!XMemGetWorkingSetStatistics(&stats, sizeof(stats) - 1));
    CHECK(GetLastError() == ERROR_INVALID_PARAMETER);

    unsigned char large_stats[sizeof(bx_xmem_stats_t) + 16];
    std::memset(large_stats, 0xCC, sizeof(large_stats));
    CHECK(XMemGetWorkingSetStatistics(large_stats, sizeof(large_stats)));
    for (size_t i = sizeof(bx_xmem_stats_t); i < sizeof(large_stats); ++i) {
        CHECK(large_stats[i] == 0);
    }

    auto* ordinary = XMemAlloc(73, 0);
    auto* empty = XMemAllocDefault(0, 0);
    CHECK(ordinary && empty);
    CHECK(reinterpret_cast<uintptr_t>(ordinary) % 64 == 0);
    CHECK(reinterpret_cast<uintptr_t>(empty) % 64 == 0);
    bx_xmem_get_internal_stats(&stats);
    CHECK(stats.committed_virtual_bytes == 89);

    // The existing heap charge should reject requests at the full cap
    // without allocating gigabytes on the test host.
    CHECK(!XMemAllocDefault(stats.title_limit_bytes, 0));
    CHECK(GetLastError() == ERROR_NOT_ENOUGH_MEMORY);
    CHECK(!alloc4()(nullptr, stats.title_limit_bytes,
                              MEM_COMMIT | MEM_RESERVE, PAGE_READWRITE));
    CHECK(GetLastError() == ERROR_NOT_ENOUGH_MEMORY);

    XMemFreeDefault(empty, 0);
    XMemFree(ordinary, 0);

    auto* reserved = static_cast<unsigned char*>(
        alloc4()(nullptr, page * 3, MEM_RESERVE, PAGE_READWRITE));
    CHECK(reserved && reinterpret_cast<uintptr_t>(reserved) % page == 0);
    CHECK(alloc4()(reserved, page * 2, MEM_COMMIT, PAGE_READWRITE) == reserved);
    CHECK(alloc4()(reserved + page + 1, page * 2 - 1, MEM_COMMIT, PAGE_READWRITE)
          == reserved + page);

    MEMORY_BASIC_INFORMATION region{};
    CHECK(XMemVirtualQuery(reserved + page, &region, sizeof(region)) == sizeof(region));
    CHECK(region.AllocationBase == reserved && region.State == MEM_COMMIT);
    CHECK(region.Protect == PAGE_READWRITE && region.RegionSize == page * 2);

    constexpr char sample[] = "betterxen";
    CHECK(XMemTransferMemory(reserved, sample, sizeof(sample), 0));
    CHECK(XMemTransferMemory(reserved + 1, reserved, sizeof(sample), 0));
    CHECK(std::memcmp(reserved + 1, sample, sizeof(sample)) == 0);

    bx_xmem_get_internal_stats(&stats);
    CHECK(stats.committed_virtual_bytes == page * 3 && stats.peak_committed_bytes == page * 3);
    CHECK(VirtualFree(reserved + page, page, MEM_DECOMMIT));
    bx_xmem_get_internal_stats(&stats);
    CHECK(stats.committed_virtual_bytes == page * 2 && stats.peak_committed_bytes == page * 3);
    CHECK(VirtualFree(reserved, 0, MEM_RELEASE));
    bx_xmem_get_internal_stats(&stats);
    CHECK(stats.committed_virtual_bytes == 0 && stats.peak_committed_bytes == page * 3);

    // Console callers pass multi-bit attributes (e.g. 0x21) in arg 4 and high bits in alloc_type.
    auto* console_res = static_cast<unsigned char*>(
        alloc4()(nullptr, page * 2, MEM_RESERVE | 0x20400000u, 0x21u));
    CHECK(console_res != nullptr);
    CHECK(alloc4()(console_res, page, MEM_COMMIT, 0x21u) == console_res);
    console_res[0] = 0x5Au;
    CHECK(console_res[0] == 0x5Au);
    /* FIVE-argument form, as every public Xbox-GDK-Samples call site writes it:
     * (address, size, alloc_type, XMEM_* attributes, PAGE_* protect). The
     * XMEM_* word here is a plausible multi-bit value; its numeric meaning is
     * unknown from public sources, so the shim must not interpret it. The
     * PAGE_* argument is what must reach the host.
     *
     * The fixture cannot prove what the real console header declares, only
     * that both call shapes behave as stated. */
    auto* five_arg = static_cast<unsigned char*>(
        XMemVirtualAlloc(nullptr, page * 2, MEM_RESERVE | MEM_COMMIT, 0x21u,
                         PAGE_READWRITE | PAGE_WRITECOMBINE));
    CHECK(five_arg != nullptr);
    MEMORY_BASIC_INFORMATION five_region{};
    CHECK(XMemVirtualQuery(five_arg, &five_region, sizeof(five_region)) == sizeof(five_region));
    CHECK(five_region.Protect == (PAGE_READWRITE | PAGE_WRITECOMBINE));
    /* The XMEM_* word must not have been taken as the protection: 0x21 is not
     * a Win32 base page protection, so a misread would have produced
     * PAGE_READWRITE with no WRITECOMBINE, or a different Protect entirely. */
    CHECK(five_region.Protect != PAGE_READWRITE);
    CHECK(VirtualFree(five_arg, 0, MEM_RELEASE));

    /* The same PAGE_* through the four-argument form must give the same mapping,
     * which is what makes the discrimination rule self-consistent. */
    auto* four_arg = static_cast<unsigned char*>(
        alloc4()(nullptr, page * 2, MEM_RESERVE | MEM_COMMIT,
                 PAGE_READWRITE | PAGE_WRITECOMBINE));
    CHECK(four_arg != nullptr);
    MEMORY_BASIC_INFORMATION four_region{};
    CHECK(XMemVirtualQuery(four_arg, &four_region, sizeof(four_region)) == sizeof(four_region));
    CHECK(four_region.Protect == (PAGE_READWRITE | PAGE_WRITECOMBINE));
    CHECK(VirtualFree(four_arg, 0, MEM_RELEASE));

    CHECK(VirtualFree(console_res, 0, MEM_RELEASE));
    bx_xmem_get_internal_stats(&stats);
    CHECK(stats.committed_virtual_bytes == 0);
    std::puts("test_xmem: passed");
}

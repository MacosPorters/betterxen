#include <windows.h>

#include <cstdint>
#include <limits>
#include <new>

#include "bx_log.h"
#include "bx_xmem_internal.h"
#include "xmem_accounting.h"

namespace {
constexpr SIZE_T small_page = 64 * 1024;
constexpr SIZE_T large_page = 2 * 1024 * 1024;

struct PageBlock;

struct Page {
    PageBlock* block;
    HANDLE section;
    bool releasing;
};

struct PageBlock {
    ULONG_PTR first_tag;
    SIZE_T count;
    SIZE_T size;
    SIZE_T live;
    Page* entries;
    PageBlock* next;
};

struct View {
    Page* page;
    BYTE* address;
    View* next;
};

SRWLOCK memory_lock = SRWLOCK_INIT;
PageBlock* blocks = nullptr;
View* views = nullptr;
ULONG_PTR next_tag = 1;

struct Lock {
    Lock() { AcquireSRWLockExclusive(&memory_lock); }
    ~Lock() { ReleaseSRWLockExclusive(&memory_lock); }
};

using ReservePlaceholder = PVOID (WINAPI*)(HANDLE, PVOID, SIZE_T, ULONG, ULONG,
                                           MEM_EXTENDED_PARAMETER*, ULONG);
using ReplacePlaceholder = PVOID (WINAPI*)(HANDLE, HANDLE, PVOID, ULONG64, SIZE_T, ULONG,
                                           ULONG, MEM_EXTENDED_PARAMETER*, ULONG);

struct PlaceholderApi {
    HMODULE kernel = GetModuleHandleW(L"kernelbase.dll");
    ReservePlaceholder reserve = resolve<ReservePlaceholder>("VirtualAlloc2");
    ReplacePlaceholder replace = resolve<ReplacePlaceholder>("MapViewOfFile3");

    template<class Function>
    Function resolve(const char* name) const {
        return reinterpret_cast<Function>(reinterpret_cast<void*>(GetProcAddress(kernel, name)));
    }
};

// The public backlog defines the supported sizes. The flags accepted here
// belong to BetterXen's synthetic fixtures, pending a public console flags ABI.
SIZE_T page_size(uint64_t flags) {
    switch (flags) {
        case 0:
        case small_page: return small_page;
        case large_page: return large_page;
        default: return 0;
    }
}

Page* find_page(ULONG_PTR tag) {
    for (PageBlock* block = blocks; block; block = block->next) {
        if (tag >= block->first_tag && tag - block->first_tag < block->count) {
            Page* page = &block->entries[tag - block->first_tag];
            return page->section ? page : nullptr;
        }
    }
    return nullptr;
}

ULONG_PTR page_tag(const Page* page) {
    return page->block->first_tag + (page - page->block->entries);
}

void delete_block(PageBlock* block) {
    for (SIZE_T i = 0; i < block->count; ++i) {
        if (block->entries[i].section) CloseHandle(block->entries[i].section);
    }
    delete[] block->entries;
    delete block;
}

void destroy_views(View* first) {
    while (first) {
        View* next = first->next;
        delete first;
        first = next;
    }
}

bool valid_span(PVOID base, SIZE_T count, SIZE_T size) {
    const auto address = reinterpret_cast<uintptr_t>(base);
    return size && count && address % size == 0
        && count <= (std::numeric_limits<uintptr_t>::max() - address) / size;
}

bool remove_view(View** link) {
    View* view = *link;
    if (!UnmapViewOfFile(view->address)) return false;
    *link = view->next;
    delete view;
    return true;
}

// Require every page slot in [base, base + count * size) to be actively mapped
// with matching page size before unmapping; empty or partial spans fail without
// modifying any existing views.
PVOID unmap_span(BYTE* base, SIZE_T count, SIZE_T size) {
    if (!base) {
        SetLastError(ERROR_INVALID_PARAMETER);
        return nullptr;
    }
    const auto begin = reinterpret_cast<uintptr_t>(base);
    const auto end = begin + count * size;

    SIZE_T matched = 0;
    for (const View* view = views; view; view = view->next) {
        const auto address = reinterpret_cast<uintptr_t>(view->address);
        if (address >= begin && address < end && view->page->block->size == size) {
            ++matched;
        }
    }
    if (matched != count) {
        SetLastError(ERROR_INVALID_PARAMETER);
        return nullptr;
    }

    for (View** link = &views; *link;) {
        View* view = *link;
        const auto address = reinterpret_cast<uintptr_t>(view->address);
        if (address >= begin && address < end && view->page->block->size == size) {
            if (!remove_view(link)) return nullptr;
        } else {
            link = &view->next;
        }
    }
    return base;
}

// Duplicate tags in `tags` are intentionally permitted so a single virtual span
// can alias one physical page at multiple offsets (e.g. ring buffers / tiles).
PVOID map_pages(PVOID address, SIZE_T count, uint64_t flags, const ULONG_PTR* tags) {
    const SIZE_T size = page_size(flags);
    if (!valid_span(address, count, size)) {
        SetLastError(ERROR_INVALID_PARAMETER);
        return nullptr;
    }

    Lock lock;
    if (!tags) return unmap_span(static_cast<BYTE*>(address), count, size);

    static const PlaceholderApi api;
    if (!api.reserve || !api.replace) {
        BX_STUB_LOG("xmem.dll", "MapViewOfFile3");
        SetLastError(ERROR_CALL_NOT_IMPLEMENTED);
        return nullptr;
    }

    View* pending = nullptr;
    for (SIZE_T i = count; i > 0; --i) {
        Page* page = find_page(tags[i - 1]);
        if (!page || page->block->size != size) {
            destroy_views(pending);
            SetLastError(ERROR_INVALID_PARAMETER);
            return nullptr;
        }
        auto* view = new (std::nothrow) View{page, nullptr, pending};
        if (!view) {
            destroy_views(pending);
            SetLastError(ERROR_NOT_ENOUGH_MEMORY);
            return nullptr;
        }
        pending = view;
    }

    MEM_ADDRESS_REQUIREMENTS requirements{};
    requirements.Alignment = size;
    MEM_EXTENDED_PARAMETER alignment{};
    alignment.Type = MemExtendedParameterAddressRequirements;
    alignment.Pointer = &requirements;
    auto* base = static_cast<BYTE*>(api.reserve(nullptr, address, count * size,
        MEM_RESERVE | MEM_RESERVE_PLACEHOLDER, PAGE_NOACCESS,
        address ? nullptr : &alignment, address ? 0 : 1));
    if (!base) {
        const DWORD error = GetLastError();
        destroy_views(pending);
        SetLastError(error);
        return nullptr;
    }

    BYTE* cursor = base;
    SIZE_T remaining = count;
    View* view = pending;
    for (; view; view = view->next, cursor += size, --remaining) {
        const bool split = remaining > 1;
        if (split && !VirtualFree(cursor, size, MEM_RELEASE | MEM_PRESERVE_PLACEHOLDER)) break;
        if (!api.replace(view->page->section, nullptr, cursor, 0, size,
                         MEM_REPLACE_PLACEHOLDER, PAGE_READWRITE, nullptr, 0)) {
            const DWORD error = GetLastError();
            if (split) VirtualFree(cursor + size, 0, MEM_RELEASE);
            SetLastError(error);
            break;
        }
        view->address = cursor;
    }

    if (view) {
        const DWORD error = GetLastError();
        VirtualFree(cursor, 0, MEM_RELEASE);
        for (View* completed = pending; completed != view; completed = completed->next) {
            UnmapViewOfFile(completed->address);
        }
        destroy_views(pending);
        SetLastError(error);
        return nullptr;
    }

    while (pending) {
        View* next = pending->next;
        pending->next = views;
        views = pending;
        pending = next;
    }
    return base;
}
} // namespace

// Clean-room synthetic signatures modeled after public Win32 AWE
// AllocateUserPhysicalPages / FreeUserPhysicalPages (memoryapi.h, [in, out]
// page count pointer), pending a public console flags/signature ABI.
extern "C" BOOL WINAPI XMemAllocatePhysicalPages(SIZE_T* num_pages, PULONG_PTR page_array,
                                               uint64_t flags) {
    const SIZE_T size = page_size(flags);
    if (!num_pages || !page_array || !valid_span(nullptr, *num_pages, size)) {
        SetLastError(ERROR_INVALID_PARAMETER);
        return FALSE;
    }

    Lock lock;
    const SIZE_T count = *num_pages;
    if (count > std::numeric_limits<ULONG_PTR>::max() - next_tag
        || count > std::numeric_limits<SIZE_T>::max() / sizeof(Page)
        || count > std::numeric_limits<uint64_t>::max() / size) {
        SetLastError(ERROR_NOT_ENOUGH_MEMORY);
        return FALSE;
    }
    const uint64_t total_bytes = static_cast<uint64_t>(count) * size;
    if (!bx_xmem::try_reserve_physical(total_bytes)) {
        SetLastError(ERROR_NOT_ENOUGH_MEMORY);
        return FALSE;
    }

    auto* entries = new (std::nothrow) Page[count]{};
    if (!entries) {
        bx_xmem::release_physical(total_bytes);
        SetLastError(ERROR_NOT_ENOUGH_MEMORY);
        return FALSE;
    }
    auto* block = new (std::nothrow) PageBlock{next_tag, count, size, count, entries, blocks};
    if (!block) {
        delete[] entries;
        bx_xmem::release_physical(total_bytes);
        SetLastError(ERROR_NOT_ENOUGH_MEMORY);
        return FALSE;
    }

    for (SIZE_T i = 0; i < count; ++i) {
        HANDLE section = CreateFileMappingW(INVALID_HANDLE_VALUE, nullptr, PAGE_READWRITE,
                                            0, static_cast<DWORD>(size), nullptr);
        if (!section) {
            const DWORD error = GetLastError();
            delete_block(block);
            bx_xmem::release_physical(total_bytes);
            SetLastError(error);
            return FALSE;
        }
        entries[i] = {block, section, false};
    }

    blocks = block;
    for (SIZE_T i = 0; i < count; ++i) page_array[i] = next_tag + i;
    next_tag += count;
    return TRUE;
}

// Rejects unknown, wrong-size, or duplicate tags in `page_array` with
// ERROR_INVALID_PARAMETER without mutating `page_array` or live views.
extern "C" BOOL WINAPI XMemFreePhysicalPages(SIZE_T* num_pages, PULONG_PTR page_array,
                                           uint64_t flags) {
    const SIZE_T size = page_size(flags);
    if (!num_pages || !page_array || !valid_span(nullptr, *num_pages, size)) {
        SetLastError(ERROR_INVALID_PARAMETER);
        return FALSE;
    }

    Lock lock;
    const SIZE_T count = *num_pages;
    SIZE_T validated = 0;
    for (; validated < count; ++validated) {
        Page* page = find_page(page_array[validated]);
        if (!page || page->block->size != size || page->releasing) break;
        page->releasing = true;
    }
    if (validated != count) {
        for (SIZE_T i = 0; i < validated; ++i) find_page(page_array[i])->releasing = false;
        SetLastError(ERROR_INVALID_PARAMETER);
        return FALSE;
    }

    for (View** link = &views; *link;) {
        if ((*link)->page->releasing) {
            if (!remove_view(link)) {
                const DWORD error = GetLastError();
                for (SIZE_T i = 0; i < count; ++i) find_page(page_array[i])->releasing = false;
                SetLastError(error);
                return FALSE;
            }
        } else {
            link = &(*link)->next;
        }
    }

    for (SIZE_T i = 0; i < count; ++i) {
        Page* page = find_page(page_array[i]);
        CloseHandle(page->section);
        page->section = nullptr;
        --page->block->live;
        page_array[i] = 0;
    }
    for (PageBlock** link = &blocks; *link;) {
        PageBlock* block = *link;
        if (block->live) {
            link = &block->next;
        } else {
            *link = block->next;
            delete_block(block);
        }
    }
    bx_xmem::release_physical(static_cast<uint64_t>(count) * size);
    return TRUE;
}

extern "C" PVOID WINAPI XMemMapPhysicalPages(PVOID base, SIZE_T count, uint64_t flags,
                                            PULONG_PTR page_array) {
    return map_pages(base, count, flags, page_array);
}

extern "C" PVOID WINAPI XMemMapPhysicalPagesScatter(PVOID base, SIZE_T count, uint64_t flags,
                                                   PULONG_PTR page_array) {
    return map_pages(base, count, flags, page_array);
}

extern "C" int bx_xmem_query_region(uint64_t va, bx_xmem_region_info_t* out_info) {
    if (!out_info) return 0;

    Lock lock;
    for (const View* view = views; view; view = view->next) {
        const auto base = reinterpret_cast<uintptr_t>(view->address);
        const SIZE_T size = view->page->block->size;
        if (va >= base && va - base < size) {
            MEMORY_BASIC_INFORMATION region{};
            if (!VirtualQuery(view->address, &region, sizeof(region))
                || region.AllocationBase != view->address || region.Type != MEM_MAPPED) {
                return 0;
            }
            *out_info = {base, size, size, region.Protect, region.Type, page_tag(view->page)};
            return 1;
        }
    }
    return 0;
}

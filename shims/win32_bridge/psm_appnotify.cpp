#include <windows.h>
#include <cstddef>
#include <cstdint>

#include "bx_log.h"

namespace {

static_assert(sizeof(uintptr_t) == 8, "BetterXen Win32 shims target x86-64");

constexpr uintptr_t kPrefixMask = 0xFFFFFFFF00000000ULL;
constexpr uintptr_t kSequenceMask = 0x00000000FFFFFFFFULL;
constexpr uintptr_t kStatePrefix = 0x50534D3100000000ULL;       // "PSM1"
constexpr uintptr_t kConstrainedPrefix = 0x50534D3200000000ULL; // "PSM2"
constexpr uintptr_t kEmptySlot = 0;
constexpr uintptr_t kTombstoneSlot = ~uintptr_t{0};

struct SrwLockGuard {
    SRWLOCK& lock;
    explicit SrwLockGuard(SRWLOCK& l) : lock(l) { AcquireSRWLockExclusive(&lock); }
    ~SrwLockGuard() { ReleaseSRWLockExclusive(&lock); }
    SrwLockGuard(const SrwLockGuard&) = delete;
    SrwLockGuard& operator=(const SrwLockGuard&) = delete;
};

// Exception-free open-addressed hash set backed by the Win32 process heap so
// both PSM DLLs stay on -fno-exceptions and map OOM to ERROR_NOT_ENOUGH_MEMORY.
struct TokenSet {
    uintptr_t* slots = nullptr;
    size_t capacity = 0;
    size_t count = 0;
    size_t used = 0; // active entries + tombstones

    static size_t hash(uintptr_t value) {
        uint64_t x = static_cast<uint64_t>(value);
        x ^= x >> 30;
        x *= 0xbf58476d1ce4e5b9ULL;
        x ^= x >> 27;
        x *= 0x94d049bb133111ebULL;
        x ^= x >> 31;
        return static_cast<size_t>(x);
    }

    bool rehash(size_t new_capacity) {
        if (new_capacity == 0 || new_capacity > SIZE_MAX / sizeof(uintptr_t)) {
            return false;
        }
        HANDLE heap = GetProcessHeap();
        if (!heap) {
            return false;
        }
        auto* fresh = static_cast<uintptr_t*>(
            HeapAlloc(heap, HEAP_ZERO_MEMORY, new_capacity * sizeof(uintptr_t)));
        if (!fresh) {
            return false;
        }
        const size_t mask = new_capacity - 1;
        for (size_t i = 0; i < capacity; ++i) {
            const uintptr_t entry = slots[i];
            if (entry != kEmptySlot && entry != kTombstoneSlot) {
                size_t idx = hash(entry) & mask;
                while (fresh[idx] != kEmptySlot) {
                    idx = (idx + 1) & mask;
                }
                fresh[idx] = entry;
            }
        }
        if (slots) {
            HeapFree(heap, 0, slots);
        }
        slots = fresh;
        capacity = new_capacity;
        used = count;
        return true;
    }

    bool insert(uintptr_t token) {
        if (capacity == 0 || (used + 1) * 4 >= capacity * 3) {
            size_t next_capacity = 64;
            if (capacity != 0) {
                if (count * 2 >= capacity) {
                    if (capacity > (SIZE_MAX / sizeof(uintptr_t)) / 2) {
                        return false;
                    }
                    next_capacity = capacity * 2;
                } else {
                    // Compact tombstones in place without growing the table.
                    next_capacity = capacity;
                }
            }
            if (!rehash(next_capacity)) {
                return false;
            }
        }
        const size_t mask = capacity - 1;
        size_t idx = hash(token) & mask;
        size_t first_tombstone = capacity;
        while (true) {
            const uintptr_t entry = slots[idx];
            if (entry == token) {
                return true;
            }
            if (entry == kEmptySlot) {
                if (first_tombstone != capacity) {
                    slots[first_tombstone] = token;
                } else {
                    slots[idx] = token;
                    ++used;
                }
                ++count;
                return true;
            }
            if (entry == kTombstoneSlot && first_tombstone == capacity) {
                first_tombstone = idx;
            }
            idx = (idx + 1) & mask;
        }
    }

    bool erase(uintptr_t token) {
        if (capacity == 0 || count == 0 || token == kEmptySlot || token == kTombstoneSlot) {
            return false;
        }
        const size_t mask = capacity - 1;
        size_t idx = hash(token) & mask;
        for (size_t probed = 0; probed < capacity; ++probed) {
            const uintptr_t entry = slots[idx];
            if (entry == kEmptySlot) {
                return false;
            }
            if (entry == token) {
                slots[idx] = kTombstoneSlot;
                --count;
                return true;
            }
            idx = (idx + 1) & mask;
        }
        return false;
    }

    void reset() {
        if (slots) {
            HANDLE heap = GetProcessHeap();
            if (heap) {
                HeapFree(heap, 0, slots);
            }
            slots = nullptr;
        }
        capacity = 0;
        count = 0;
        used = 0;
    }
};

struct Registrations {
    SRWLOCK lock;
    TokenSet active;
    uintptr_t own_prefix;
    uintptr_t foreign_prefix;
    uint64_t next_seq;
};

// Each DLL exposes one family. Disjoint tagged token spaces prevent an
// unregister call on the wrong DLL from removing another registration and let
// logs distinguish foreign-family tokens from retired or invalid ones.
Registrations g_state{SRWLOCK_INIT, {}, kStatePrefix, kConstrainedPrefix, 1};
Registrations g_constrained{SRWLOCK_INIT, {}, kConstrainedPrefix, kStatePrefix, 1};

ULONG register_notification(Registrations& registrations, PVOID callback,
                            PVOID* out_reg, const char* event) {
    if (!out_reg) {
        return ERROR_INVALID_PARAMETER;
    }
    *out_reg = nullptr;
    if (!callback) {
        return ERROR_INVALID_PARAMETER;
    }

    uintptr_t token = 0;
    {
        const SrwLockGuard guard(registrations.lock);
        // Never recycle a token that a caller may still retain.
        if (registrations.next_seq > kSequenceMask) {
            return ERROR_NOT_ENOUGH_MEMORY;
        }
        token = registrations.own_prefix | static_cast<uintptr_t>(registrations.next_seq);
        if (!registrations.active.insert(token)) {
            return ERROR_NOT_ENOUGH_MEMORY;
        }
        ++registrations.next_seq;
    }

    *out_reg = reinterpret_cast<PVOID>(token);
    bx_log_emit(BX_LOG_INFO, "win32_bridge", event, "registered_offline");
    return ERROR_SUCCESS;
}

void unregister_notification(Registrations& registrations, PVOID reg, const char* event) {
    const uintptr_t token = reinterpret_cast<uintptr_t>(reg);
    bool removed = false;
    {
        const SrwLockGuard guard(registrations.lock);
        removed = registrations.active.erase(token);
    }
    const char* detail = "unknown_token";
    if (removed) {
        detail = "unregistered";
    } else if ((token & kPrefixMask) == registrations.foreign_prefix &&
               (token & kSequenceMask) != 0) {
        detail = "foreign_token";
    }
    bx_log_emit(BX_LOG_INFO, "win32_bridge", event, detail);
}

} // namespace

extern "C" {

BOOL WINAPI DllMain(HINSTANCE, DWORD reason, LPVOID reserved) {
    if (reason == DLL_PROCESS_DETACH && reserved == nullptr) {
        {
            const SrwLockGuard guard(g_state.lock);
            g_state.active.reset();
        }
        {
            const SrwLockGuard guard(g_constrained.lock);
            g_constrained.active.reset();
        }
    }
    return TRUE;
}

ULONG WINAPI RegisterAppStateChangeNotification(PVOID callback, PVOID /*context*/, PVOID* out_reg) {
    return register_notification(g_state, callback, out_reg, "RegisterAppStateChangeNotification");
}

VOID WINAPI UnregisterAppStateChangeNotification(PVOID reg) {
    unregister_notification(g_state, reg, "UnregisterAppStateChangeNotification");
}

ULONG WINAPI RegisterAppConstrainedChangeNotification(PVOID callback, PVOID /*context*/, PVOID* out_reg) {
    return register_notification(g_constrained, callback, out_reg, "RegisterAppConstrainedChangeNotification");
}

VOID WINAPI UnregisterAppConstrainedChangeNotification(PVOID reg) {
    unregister_notification(g_constrained, reg, "UnregisterAppConstrainedChangeNotification");
}

} // extern "C"

#include <windows.h>
#include <atomic>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <intrin.h>

#include "bx_config.h"
#include "bx_log.h"

namespace {

CRITICAL_SECTION g_log_lock;
bool g_log_lock_ready = false;
HANDLE g_log_handle = INVALID_HANDLE_VALUE;
PVOID g_veh_handle = nullptr;
LPTOP_LEVEL_EXCEPTION_FILTER g_previous_filter = nullptr;
std::atomic_flag g_crash_logged{};
std::atomic<bool> g_crash_in_flight{false};
std::atomic<uint64_t> g_clwb_traps{0};

// Other AMD-only instructions Rosetta 2 (and Intel CPUs) raise #UD on. Their
// counts reach the JSONL trace only: the frozen .def has no export for them.
enum AmdKind { kClzero, kMonitorx, kMwaitx, kRdpru, kExtrq, kInsertq, kAmdKinds };
constexpr const char* kAmdEvents[kAmdKinds] = {
    "clzero_trapped", "monitorx_trapped", "mwaitx_trapped",
    "rdpru_trapped", "extrq_trapped", "insertq_trapped"};
std::atomic<uint64_t> g_amd_traps[kAmdKinds]{};
bx_runtime_config_t g_config = {
    BX_ABI_VERSION,
    BX_PROFILE_LOCKHART,
    BX_LOG_INFO,
    8ULL * 1024ULL * 1024ULL * 1024ULL, /* Default 8 GiB Lockhart title budget */
    0u,
    1u,
    1u,
    0u
};

void write_log_line(const char* line, size_t len) {
    // Pause regular logging while a terminal crash record is being emitted so
    // background threads cannot interleave with the unlocked crash WriteFile.
    if (g_crash_in_flight.load(std::memory_order_acquire)) {
        return;
    }
    if (g_log_lock_ready) {
        EnterCriticalSection(&g_log_lock);
    }
    if (g_log_handle != INVALID_HANDLE_VALUE) {
        DWORD written = 0;
        WriteFile(g_log_handle, line, static_cast<DWORD>(len), &written, nullptr);
    } else {
        DWORD written = 0;
        HANDLE err = GetStdHandle(STD_ERROR_HANDLE);
        if (err && err != INVALID_HANDLE_VALUE) {
            WriteFile(err, line, static_cast<DWORD>(len), &written, nullptr);
        }
    }
    OutputDebugStringA(line);
    if (g_log_lock_ready) {
        LeaveCriticalSection(&g_log_lock);
    }
}

constexpr size_t kMaxInstructionLength = 15;

size_t read_instruction(uint64_t rip, uint8_t (&bytes)[kMaxInstructionLength]) {
    HANDLE process = GetCurrentProcess();
    if (ReadProcessMemory(process, reinterpret_cast<LPCVOID>(rip),
                          bytes, sizeof(bytes), nullptr)) {
        return sizeof(bytes);
    }

    // A short instruction can end immediately before an inaccessible page.
    size_t size = 0;
    while (size < sizeof(bytes) &&
           ReadProcessMemory(process, reinterpret_cast<LPCVOID>(rip + size),
                             &bytes[size], 1, nullptr)) {
        ++size;
    }
    return size;
}

// CLWB is 66 0F AE /6 with a memory operand. The address-size override
// selects 32-bit addressing but leaves the ModRM/SIB displacement lengths intact.
size_t decode_clwb_length(const uint8_t* ip, size_t size) {
    size_t idx = 0;
    bool has_66 = false;
    while (idx < size) {
        const uint8_t byte = ip[idx];
        if (byte == 0x66) {
            has_66 = true;
        } else if (byte != 0x67 && byte != 0x26 && byte != 0x2E &&
                   byte != 0x36 && byte != 0x3E && byte != 0x64 && byte != 0x65) {
            break;
        }
        ++idx;
    }

    // x86-64 allows at most one REX prefix immediately before the opcode.
    if (idx < size && (ip[idx] & 0xF0) == 0x40) {
        ++idx;
    }
    if (!has_66 || size - idx < 3 || ip[idx] != 0x0F || ip[idx + 1] != 0xAE) {
        return 0;
    }
    idx += 2;

    const uint8_t modrm = ip[idx++];
    const uint8_t mod = modrm >> 6;
    const uint8_t reg = (modrm >> 3) & 7;
    const uint8_t rm = modrm & 7;
    if (mod == 3 || reg != 6) {
        return 0;
    }

    uint8_t base = rm;
    if (rm == 4) {
        if (idx == size) {
            return 0;
        }
        base = ip[idx++] & 7;
    }
    if (mod == 1) {
        idx += 1;
    } else if (mod == 2 || (mod == 0 && base == 5)) {
        idx += 4;
    }
    return idx <= size ? idx : 0;
}

bool is_adx_opcode(const uint8_t* ip, size_t size) {
    size_t idx = 0;
    while (idx < size && (ip[idx] == 0x66 || ip[idx] == 0xF3)) {
        ++idx;
    }
    if (idx == 0) {
        return false;
    }
    if (idx < size && (ip[idx] & 0xF0) == 0x40) {
        ++idx;
    }
    // Require the 3-byte ADCX/ADOX opcode (0F 38 F6) plus at least 1 ModRM byte.
    return size - idx >= 4 && ip[idx] == 0x0F && ip[idx + 1] == 0x38 && ip[idx + 2] == 0xF6;
}

// Logs the first trap of a kind and then every power of two, as for clwb.
void log_sampled_trap(const char* event, uint64_t count, uint64_t rip, size_t len) {
    if (count == 1 || (count & (count - 1)) == 0) {
        char msg[160];
        std::snprintf(msg, sizeof(msg),
                      "rip=0x%016llx len=%zu total_traps=%llu",
                      static_cast<unsigned long long>(rip), len,
                      static_cast<unsigned long long>(count));
        bx_log_emit(BX_LOG_INFO, "runtime_veh", event, msg);
    }
}

// Whether the page holding `address` can take a plain store. A store to a
// page Wine write-watches still succeeds: Wine handles that fault itself.
// Callers pass a 64-byte-aligned line, which never crosses a page, so one
// query covers the whole line.
bool is_writable(uintptr_t address) {
    MEMORY_BASIC_INFORMATION region{};
    if (!VirtualQuery(reinterpret_cast<LPCVOID>(address), &region, sizeof(region)) ||
        region.State != MEM_COMMIT || (region.Protect & (PAGE_GUARD | PAGE_NOACCESS))) {
        return false;
    }
    const DWORD writable = PAGE_READWRITE | PAGE_WRITECOPY |
                           PAGE_EXECUTE_READWRITE | PAGE_EXECUTE_WRITECOPY;
    return (region.Protect & writable) != 0;
}

// SSE4a EXTRQ / INSERTQ, register forms (the only ones there are):
//   66 [REX] 0F 78 /0 ib ib   EXTRQ   xmm, len, idx
//   66 [REX] 0F 79 /r         EXTRQ   xmm, xmm   (len, idx in src[13:0])
//   F2 [REX] 0F 78 /r ib ib   INSERTQ xmm, xmm, len, idx
//   F2 [REX] 0F 79 /r         INSERTQ xmm, xmm   (len, idx in src[77:64])
// Works on the saved XMM registers. Returns the length, or 0 for anything
// else. The upper quadword of the destination is architecturally undefined
// and is left as it was.
size_t emulate_sse4a(const uint8_t* ip, size_t size, CONTEXT& ctx, AmdKind& kind) {
    if (size < 4 || (ip[0] != 0x66 && ip[0] != 0xF2)) {
        return 0;
    }
    size_t idx = 1;
    uint8_t rex = 0;
    if ((ip[idx] & 0xF0) == 0x40) {
        rex = ip[idx++];
    }
    if (size - idx < 3 || ip[idx] != 0x0F || (ip[idx + 1] != 0x78 && ip[idx + 1] != 0x79)) {
        return 0;
    }
    const bool immediate = ip[idx + 1] == 0x78;
    const uint8_t modrm = ip[idx + 2];
    idx += 3;
    if ((modrm >> 6) != 3) {
        return 0;
    }
    const bool extract = ip[0] == 0x66;
    if (extract && immediate && ((modrm >> 3) & 7) != 0) {
        return 0;
    }
    M128A* xmm = ctx.FltSave.XmmRegisters;
    const unsigned dst = ((modrm >> 3) & 7) | ((rex & 4) << 1);
    const unsigned src = (modrm & 7) | ((rex & 1) << 3);

    uint64_t fields;
    if (immediate) {
        if (size - idx < 2) {
            return 0;
        }
        fields = ip[idx] | (static_cast<uint64_t>(ip[idx + 1]) << 8);
        idx += 2;
    } else {
        fields = extract ? xmm[src].Low : static_cast<uint64_t>(xmm[src].High);
    }
    const unsigned length = fields & 63;
    const unsigned index = (fields >> 8) & 63;
    const uint64_t mask = length == 0 ? ~0ULL : (1ULL << length) - 1;

    if (extract) {
        // The immediate form names its one register in ModRM.rm.
        M128A& target = xmm[immediate ? src : dst];
        target.Low = (target.Low >> index) & mask;
        kind = kExtrq;
    } else {
        const uint64_t value = (xmm[src].Low & mask) << index;
        xmm[dst].Low = (xmm[dst].Low & ~(mask << index)) | value;
        kind = kInsertq;
    }
    return idx;
}

constexpr DWORD kRdpruFlags = 0x0001 | 0x0004 | 0x0010 | 0x0040 | 0x0080 | 0x0800;

// CLZERO, MONITORX, MWAITX, RDPRU (0F 01 FC/FA/FB/FD) and SSE4a: does what
// a Zen 2 CPU would and returns the instruction length, or 0 to leave the
// exception to the next handler. An unwritable CLZERO line also rewrites `rec`
// into the write fault a store would raise.
size_t emulate_amd_only(const uint8_t* ip, size_t size, CONTEXT& ctx, EXCEPTION_RECORD& rec,
                        AmdKind& kind) {
    // CLZERO takes an address-size override: 67 selects EAX in 64-bit mode.
    const bool addr32 = size >= 4 && ip[0] == 0x67 && ip[1] == 0x0F && ip[2] == 0x01 &&
                        ip[3] == 0xFC;
    if (addr32) {
        ip += 1;
        size -= 1;
    }
    if (size >= 3 && ip[0] == 0x0F && ip[1] == 0x01) {
        switch (ip[2]) {
        case 0xFC: {
            // Zeroes the 64-byte line holding rAX; a line never crosses a page.
            const uint64_t address = addr32 ? static_cast<uint32_t>(ctx.Rax) : ctx.Rax;
            const uintptr_t line = static_cast<uintptr_t>(address) & ~uintptr_t{63};
            if (!is_writable(line)) {
                // CLZERO takes the paging checks of a store: report the write
                // fault a store to the line would have raised, not a #UD.
                rec.ExceptionCode = EXCEPTION_ACCESS_VIOLATION;
                rec.NumberParameters = 2;
                rec.ExceptionInformation[0] = 1;
                rec.ExceptionInformation[1] = line;
                return 0;
            }
            std::memset(reinterpret_cast<void*>(line), 0, 64);
            kind = kClzero;
            return addr32 ? 4 : 3;
        }
        case 0xFA:
            // A wait hint with nothing to arm.
            kind = kMonitorx;
            return 3;
        case 0xFB:
            // Synthetic: the EBX timer and the ECX extension bits are ignored
            // and the wait is a single yield. The architecture lets a wait end
            // at any time, so a loop that re-checks its condition still makes
            // progress, but it spins where hardware would sleep.
            SwitchToThread();
            kind = kMwaitx;
            return 3;
        case 0xFD: {
            // ECX 0 is MPERF and 1 is APERF; both count at the time-stamp
            // rate, as on a core that never changes frequency. Other ECX
            // values read 0. OF, SF, ZF, AF and PF are cleared and CF is
            // modified (AMD APM vol. 3, RDPRU); CF is cleared here.
            ctx.EFlags &= ~kRdpruFlags;
            const uint64_t cycles = static_cast<uint32_t>(ctx.Rcx) <= 1 ? __rdtsc() : 0;
            ctx.Rax = cycles & 0xFFFFFFFFULL;
            ctx.Rdx = cycles >> 32;
            kind = kRdpru;
            return 3;
        }
        default:
            return 0;
        }
    }
    return emulate_sse4a(ip, size, ctx, kind);
}

LONG WINAPI log_unhandled_exception(PEXCEPTION_POINTERS ep) {
    // Wine can invoke the top-level filter again for the same terminal fault.
    if (ep && ep->ExceptionRecord && ep->ContextRecord &&
        !g_crash_logged.test_and_set(std::memory_order_relaxed)) {
        g_crash_in_flight.store(true, std::memory_order_release);
        const CONTEXT& ctx = *ep->ContextRecord;
        MEMORY_BASIC_INFORMATION region{};
        unsigned long long module_base = 0;
        if (VirtualQuery(reinterpret_cast<LPCVOID>(ctx.Rip), &region, sizeof(region)) &&
            region.Type == MEM_IMAGE) {
            module_base = reinterpret_cast<uintptr_t>(region.AllocationBase);
        }

        char line[1024];
        int length = std::snprintf(
            line, sizeof(line),
            "{\"ts_ms\":%llu,\"tid\":%lu,\"level\":\"ERROR\",\"sub\":\"runtime_veh\","
            "\"event\":\"unhandled_exception\",\"exception_code\":\"0x%08lx\","
            "\"module_base\":\"0x%016llx\",\"rva\":\"0x%016llx\","
            "\"rip\":\"0x%016llx\",\"rsp\":\"0x%016llx\","
            "\"registers\":{\"rax\":\"0x%016llx\",\"rbx\":\"0x%016llx\","
            "\"rcx\":\"0x%016llx\",\"rdx\":\"0x%016llx\",\"rsi\":\"0x%016llx\","
            "\"rdi\":\"0x%016llx\",\"rbp\":\"0x%016llx\",\"r8\":\"0x%016llx\","
            "\"r9\":\"0x%016llx\",\"r10\":\"0x%016llx\",\"r11\":\"0x%016llx\","
            "\"r12\":\"0x%016llx\",\"r13\":\"0x%016llx\",\"r14\":\"0x%016llx\","
            "\"r15\":\"0x%016llx\"}}\n",
            static_cast<unsigned long long>(GetTickCount64()), GetCurrentThreadId(),
            ep->ExceptionRecord->ExceptionCode, module_base,
            module_base ? ctx.Rip - module_base : 0ULL, ctx.Rip, ctx.Rsp,
            ctx.Rax, ctx.Rbx, ctx.Rcx, ctx.Rdx, ctx.Rsi, ctx.Rdi, ctx.Rbp,
            ctx.R8, ctx.R9, ctx.R10, ctx.R11, ctx.R12, ctx.R13, ctx.R14, ctx.R15);
        // A crashed thread may already own the regular logger's lock.
        HANDLE output = g_log_handle != INVALID_HANDLE_VALUE
            ? g_log_handle : GetStdHandle(STD_ERROR_HANDLE);
        if (length > 0 && length < static_cast<int>(sizeof(line)) &&
            output && output != INVALID_HANDLE_VALUE) {
            DWORD written = 0;
            WriteFile(output, line, static_cast<DWORD>(length), &written, nullptr);
        }
    }

    const LONG disposition = g_previous_filter ? g_previous_filter(ep) : EXCEPTION_CONTINUE_SEARCH;
    if (disposition == EXCEPTION_CONTINUE_EXECUTION) {
        g_crash_in_flight.store(false, std::memory_order_release);
        g_crash_logged.clear(std::memory_order_relaxed);
    }
    return disposition;
}

LONG CALLBACK bx_veh_handler(PEXCEPTION_POINTERS ep) {
    if (!ep || !ep->ExceptionRecord || !ep->ContextRecord) {
        return EXCEPTION_CONTINUE_SEARCH;
    }
    if (ep->ExceptionRecord->ExceptionCode != EXCEPTION_ILLEGAL_INSTRUCTION) {
        return EXCEPTION_CONTINUE_SEARCH;
    }

    uint8_t ip[kMaxInstructionLength]{};
    const size_t size = read_instruction(ep->ContextRecord->Rip, ip);
    if (g_config.enable_clwb_veh) {
        size_t len = decode_clwb_length(ip, size);
        if (len > 0) {
            uint64_t count = g_clwb_traps.fetch_add(1, std::memory_order_relaxed) + 1;
            log_sampled_trap("clwb_trapped", count, ep->ContextRecord->Rip, len);
            ep->ContextRecord->Rip += len;
            return EXCEPTION_CONTINUE_EXECUTION;
        }
    }

    AmdKind kind = kAmdKinds;
    if (size_t len = emulate_amd_only(ip, size, *ep->ContextRecord, *ep->ExceptionRecord, kind)) {
        uint64_t count = g_amd_traps[kind].fetch_add(1, std::memory_order_relaxed) + 1;
        log_sampled_trap(kAmdEvents[kind], count, ep->ContextRecord->Rip, len);
        ep->ContextRecord->Rip += len;
        return EXCEPTION_CONTINUE_EXECUTION;
    }

    if (g_config.enable_adx_tripwire && is_adx_opcode(ip, size)) {
        char msg[160];
        std::snprintf(msg, sizeof(msg),
                      "rip=0x%016llx bytes=%02x %02x %02x %02x %02x",
                      static_cast<unsigned long long>(ep->ContextRecord->Rip),
                      ip[0], ip[1], ip[2], ip[3], ip[4]);
        bx_log_emit(BX_LOG_ERROR, "runtime_veh", "adx_illegal_instruction", msg);
    }

    return EXCEPTION_CONTINUE_SEARCH;
}

void init_config_and_logging() {
    InitializeCriticalSection(&g_log_lock);
    g_log_lock_ready = true;

    char buf[512];
    if (GetEnvironmentVariableA(BX_ENV_PROFILE, buf, sizeof(buf)) > 0) {
        if (_stricmp(buf, "anaconda") == 0) {
            g_config.profile = BX_PROFILE_ANACONDA;
            g_config.title_memory_limit_bytes = 13500ULL * 1024ULL * 1024ULL;
        }
    }
    if (GetEnvironmentVariableA(BX_ENV_MEM_LIMIT_MB, buf, sizeof(buf)) > 0) {
        unsigned long long mb = std::strtoull(buf, nullptr, 10);
        if (mb >= 512) {
            g_config.title_memory_limit_bytes = mb * 1024ULL * 1024ULL;
        }
    }
    if (GetEnvironmentVariableA(BX_ENV_LOG_LEVEL, buf, sizeof(buf)) > 0) {
        if (_stricmp(buf, "error") == 0) g_config.log_level = BX_LOG_ERROR;
        else if (_stricmp(buf, "warn") == 0) g_config.log_level = BX_LOG_WARN;
        else if (_stricmp(buf, "trace") == 0) g_config.log_level = BX_LOG_TRACE;
    }
    if (GetEnvironmentVariableA(BX_ENV_TRACE_D3D, buf, sizeof(buf)) > 0 && buf[0] == '1') {
        g_config.trace_d3d = 1u;
    }
    if (GetEnvironmentVariableA(BX_ENV_LOG_FILE, buf, sizeof(buf)) > 0) {
        g_log_handle = CreateFileA(buf, FILE_APPEND_DATA, FILE_SHARE_READ | FILE_SHARE_WRITE,
                                   nullptr, OPEN_ALWAYS, FILE_ATTRIBUTE_NORMAL, nullptr);
    }
}

} // namespace

extern "C" {

void bx_log_emit(bx_log_level_t level, const char* subsystem, const char* event, const char* detail) {
    if (level > g_config.log_level) {
        return;
    }
    static const char* kLevels[] = {"ERROR", "WARN", "INFO", "TRACE"};
    const char* lvl = (level <= BX_LOG_TRACE) ? kLevels[level] : "INFO";
    char line[768];
    int n = std::snprintf(
        line, sizeof(line),
        "{\"ts_ms\":%llu,\"tid\":%lu,\"level\":\"%s\",\"sub\":\"%s\",\"event\":\"%s\",\"detail\":\"%s\"}\n",
        static_cast<unsigned long long>(GetTickCount64()),
        static_cast<unsigned long>(GetCurrentThreadId()),
        lvl,
        subsystem ? subsystem : "unknown",
        event ? event : "event",
        detail ? detail : "");
    if (n > 0) {
        write_log_line(line, static_cast<size_t>(n < static_cast<int>(sizeof(line)) ? n : sizeof(line) - 1));
    }
}

void bx_log_unimpl_api(const char* dll_name, const char* symbol_name) {
    char detail[256];
    std::snprintf(detail, sizeof(detail), "%s!%s",
                  dll_name ? dll_name : "?",
                  symbol_name ? symbol_name : "?");
    bx_log_emit(BX_LOG_WARN, "stub", "unimpl_api", detail);
}

void bx_log_unimpl_vtable(const char* iface_name, uint32_t slot_index, const char* method_name) {
    char detail[256];
    std::snprintf(detail, sizeof(detail), "%s[%u]::%s",
                  iface_name ? iface_name : "?",
                  slot_index,
                  method_name ? method_name : "?");
    bx_log_emit(BX_LOG_WARN, "com_vtable", "unimpl_vtable", detail);
}

const bx_runtime_config_t* bx_get_runtime_config(void) {
    return &g_config;
}

int bx_veh_install(void) {
    if (!g_veh_handle) {
        g_veh_handle = AddVectoredExceptionHandler(1, bx_veh_handler);
    }
    return g_veh_handle != nullptr ? 1 : 0;
}

uint64_t bx_veh_get_clwb_trap_count(void) {
    return g_clwb_traps.load(std::memory_order_relaxed);
}

BOOL WINAPI DllMain(HINSTANCE, DWORD reason, LPVOID) {
    if (reason == DLL_PROCESS_ATTACH) {
        init_config_and_logging();
        bx_veh_install();
        g_previous_filter = SetUnhandledExceptionFilter(log_unhandled_exception);
        bx_log_emit(BX_LOG_INFO, "runtime_veh", "attached",
                    g_config.profile == BX_PROFILE_ANACONDA ? "profile=anaconda" : "profile=lockhart");
    } else if (reason == DLL_PROCESS_DETACH) {
        // Restore g_previous_filter only when our filter is still the active
        // top-level filter; if the guest installed its own filter after attach,
        // put the guest filter back unchanged. (The frozen .def has no separate
        // uninstall export, so detach performs this bounded pointer swap.)
        const auto current_filter = SetUnhandledExceptionFilter(g_previous_filter);
        if (current_filter != log_unhandled_exception) {
            SetUnhandledExceptionFilter(current_filter);
        }
        if (g_veh_handle) {
            RemoveVectoredExceptionHandler(g_veh_handle);
            g_veh_handle = nullptr;
        }
        if (g_log_handle != INVALID_HANDLE_VALUE) {
            CloseHandle(g_log_handle);
            g_log_handle = INVALID_HANDLE_VALUE;
        }
        if (g_log_lock_ready) {
            DeleteCriticalSection(&g_log_lock);
            g_log_lock_ready = false;
        }
    }
    return TRUE;
}

} // extern "C"

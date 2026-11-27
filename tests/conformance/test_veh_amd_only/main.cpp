// Synthetic Windows x64 fixture for the Rosetta exception path. Expectations
// come from the AMD APM vol. 3 (24594, rev. 3.36), CLZERO, MONITORX, MWAITX,
// and RDPRU, and vol. 4 (26568, rev. 3.25), EXTRQ and INSERTQ.
#include <windows.h>
#include <cpuid.h>
#include <cstdint>
#include <cstdio>
#include <cstring>

extern "C" uint64_t bx_veh_get_clwb_trap_count(void);

namespace {

unsigned failures = 0;
unsigned checks = 0;

void expect(bool passed, const char* name) {
    ++checks;
    if (passed) {
        std::printf("ok: %s\n", name);
    } else {
        std::fprintf(stderr, "FAIL: %s\n", name);
        ++failures;
    }
}

bool equals(const uint8_t* bytes, size_t count, uint8_t value) {
    for (size_t index = 0; index < count; ++index) {
        if (bytes[index] != value) {
            return false;
        }
    }
    return true;
}

void clzero(uintptr_t address) {
    __asm__ volatile(".byte 0x0f, 0x01, 0xfc" :: "a"(address) : "memory");
}

void check_cache_lines() {
    alignas(64) uint8_t lines[192];
    std::memset(lines, 0xA5, sizeof(lines));
    clzero(reinterpret_cast<uintptr_t>(lines + 127));
    expect(equals(lines + 64, 64, 0), "CLZERO clears the selected 64 bytes");
    expect(equals(lines, 64, 0xA5) && equals(lines + 128, 64, 0xA5),
           "CLZERO preserves both adjacent cache lines");

    std::memset(lines, 0xA5, sizeof(lines));
    __asm__ volatile(".byte 0x48, 0x0f, 0x01, 0xfc" :: "a"(lines + 79) : "memory");
    expect(equals(lines + 64, 64, 0) && equals(lines, 64, 0xA5) &&
           equals(lines + 128, 64, 0xA5), "CLZERO accepts a redundant REX.W");
}

void check_address_override() {
    uint8_t* page = nullptr;
    for (uintptr_t address = 0x10000000; address < 0x70000000; address += 0x10000000) {
        page = static_cast<uint8_t*>(VirtualAlloc(reinterpret_cast<void*>(address), 4096,
                                                 MEM_COMMIT | MEM_RESERVE, PAGE_READWRITE));
        if (page) {
            break;
        }
    }
    expect(page != nullptr, "allocate a page below 4 GiB for address-size coverage");
    if (!page) {
        return;
    }

    std::memset(page, 0x6C, 192);
    const uint64_t address = 0x1234567800000000ULL | reinterpret_cast<uintptr_t>(page + 75);
    __asm__ volatile(".byte 0x67, 0x0f, 0x01, 0xfc" :: "a"(address) : "memory");
    expect(equals(page + 64, 64, 0) && equals(page, 64, 0x6C) && equals(page + 128, 64, 0x6C),
           "CLZERO address-size override selects EAX and resumes after four bytes");
    __asm__ volatile(".byte 0x67, 0x0f, 0x01, 0xfa"
                     :: "a"(address), "c"(0), "d"(0) : "memory");
    expect(page[75] == 0, "MONITORX with an address-size override resumes after four bytes");
    VirtualFree(page, 0, MEM_RELEASE);
}

struct WriteFault {
    uint8_t* page = nullptr;
    unsigned hits = 0;
    DWORD code = 0;
    ULONG_PTR operation = 0;
    ULONG_PTR address = 0;
};

WriteFault fault;

LONG CALLBACK retry_cache_line_store(EXCEPTION_POINTERS* pointers) {
    const EXCEPTION_RECORD& record = *pointers->ExceptionRecord;
    if (record.ExceptionCode != EXCEPTION_ACCESS_VIOLATION &&
        record.ExceptionCode != EXCEPTION_ILLEGAL_INSTRUCTION) {
        return EXCEPTION_CONTINUE_SEARCH;
    }

    ++fault.hits;
    fault.code = record.ExceptionCode;
    if (record.NumberParameters >= 2) {
        fault.operation = record.ExceptionInformation[0];
        fault.address = record.ExceptionInformation[1];
    }

    // Repair the page and retry at the original guest instruction.
    DWORD previous = 0;
    if (fault.hits == 1 && VirtualProtect(fault.page, 4096, PAGE_READWRITE, &previous)) {
        return EXCEPTION_CONTINUE_EXECUTION;
    }
    return EXCEPTION_CONTINUE_SEARCH;
}

void check_write_fault() {
    fault.page = static_cast<uint8_t*>(VirtualAlloc(nullptr, 4096, MEM_COMMIT | MEM_RESERVE,
                                                    PAGE_READWRITE));
    expect(fault.page != nullptr, "allocate the CLZERO write-fault page");
    if (!fault.page) {
        return;
    }

    std::memset(fault.page, 0xA5, 192);
    DWORD previous = 0;
    if (!VirtualProtect(fault.page, 4096, PAGE_READONLY, &previous)) {
        expect(false, "protect the CLZERO write-fault page");
        VirtualFree(fault.page, 0, MEM_RELEASE);
        return;
    }
    PVOID handler = AddVectoredExceptionHandler(0, retry_cache_line_store);
    expect(handler != nullptr, "install the guest write-fault handler");
    if (handler) {
        clzero(reinterpret_cast<uintptr_t>(fault.page + 99));
        RemoveVectoredExceptionHandler(handler);
        expect(fault.hits == 1 && fault.code == EXCEPTION_ACCESS_VIOLATION &&
               fault.operation == 1 && fault.address == reinterpret_cast<uintptr_t>(fault.page + 64),
               "CLZERO dispatches a write access violation at the cache-line address");
        expect(equals(fault.page + 64, 64, 0) && equals(fault.page, 64, 0xA5) &&
               equals(fault.page + 128, 64, 0xA5), "CLZERO retries after the guest repairs the page");
    }
    VirtualFree(fault.page, 0, MEM_RELEASE);
}

void check_wait_instructions() {
    alignas(64) uint64_t word = 0;
    __asm__ volatile(".byte 0x0f, 0x01, 0xfa" :: "a"(&word), "c"(0), "d"(0) : "memory");
    __asm__ volatile(".byte 0x0f, 0x01, 0xfb" :: "a"(0xF0), "b"(1024), "c"(2) : "memory");
    expect(word == 0, "MONITORX and MWAITX return and preserve the watched value");
    __asm__ volatile(".byte 0x48, 0x0f, 0x01, 0xfa"
                     :: "a"(&word), "c"(0), "d"(0) : "memory");
    __asm__ volatile(".byte 0x48, 0x0f, 0x01, 0xfb"
                     :: "a"(0xF0), "b"(1024), "c"(2) : "memory");
    expect(word == 0, "MONITORX and MWAITX also return with redundant REX.W");
}

uint64_t read_processor_register(uint32_t selector) {
    uint64_t low;
    uint64_t high;
    __asm__ volatile(".byte 0x0f, 0x01, 0xfd" : "=a"(low), "=d"(high) : "c"(selector) : "cc");
    return (high << 32) | low;
}

void check_processor_registers() {
    const uint64_t mperf = read_processor_register(0);
    const uint64_t aperf = read_processor_register(1);
    expect(mperf != 0 && read_processor_register(0) >= mperf, "RDPRU reports a plausible MPERF");
    expect(aperf != 0 && read_processor_register(1) >= aperf, "RDPRU reports a plausible APERF");
    expect(read_processor_register(UINT32_MAX) == 0, "RDPRU returns zero for an unknown selector");
    uint32_t low;
    uint32_t high;
    __asm__ volatile(".byte 0x48, 0x0f, 0x01, 0xfd"
                     : "=a"(low), "=d"(high) : "c"(1) : "cc");
    expect((low | high) != 0, "RDPRU accepts a redundant REX.W");

    uint64_t flags;
    __asm__ volatile("pushfq\n\t"
                     "orq $0x8d5, (%%rsp)\n\t"
                     "popfq\n\t"
                     ".byte 0x0f, 0x01, 0xfd\n\t"
                     "pushfq\n\t"
                     "popq %0"
                     : "=r"(flags) : "c"(0) : "rax", "rdx", "cc", "memory");
    expect((flags & 0x8D4) == 0, "RDPRU clears the five flags specified by the APM");
    // CF has no specified value on hardware. Check the shim's choice when
    // CPUID Fn8000_0008 EBX[4] does not advertise native RDPRU.
    unsigned eax, ebx, ecx, edx;
    if (!__get_cpuid(0x80000008, &eax, &ebx, &ecx, &edx) || !(ebx & (1u << 4))) {
        expect((flags & 1) == 0, "emulated RDPRU clears CF");
    }
}

struct Quadwords {
    uint64_t low;
    uint64_t high;
};

// The upper result quadword is undefined and is outside the assertion.
#define CHECK_SSE4A(dst, src, encoding, initial, field, expected, name)           \
    do {                                                                          \
        uint64_t result;                                                          \
        __asm__ volatile("movdqu %1, %%" dst "\n\t"                               \
                         "movdqu %2, %%" src "\n\t"                               \
                         ".byte " encoding "\n\t"                                 \
                         "movq %%" dst ", %0"                                     \
                         : "=r"(result) : "m"(initial), "m"(field) : dst, src);   \
        expect(result == (expected), name);                                       \
    } while (false)

void check_bit_fields() {
    const Quadwords initial{0xA1B2C3D4E5F60718ULL, 0};
    // Extract [63:60] from the destination.
    const Quadwords extract_field{0x3C04, 0};
    // Insert source nibble 5 into [63:60], or low byte 0xB5 into [15:8].
    const Quadwords insert_field{0x9876543210FEDCB5ULL, 0x3C04};
    CHECK_SSE4A("xmm3", "xmm4", "0x66, 0x0f, 0x78, 0xc3, 12, 12", initial, extract_field, 0xF60,
                "EXTRQ immediate without REX uses the destination low quadword");
    CHECK_SSE4A("xmm3", "xmm4", "0x66, 0x0f, 0x79, 0xdc", initial, extract_field, 0xA,
                "EXTRQ register without REX reads the source low quadword");
    CHECK_SSE4A("xmm3", "xmm4", "0xf2, 0x0f, 0x78, 0xdc, 8, 8", initial, insert_field,
                0xA1B2C3D4E5F6B518ULL, "INSERTQ immediate without REX preserves surrounding bits");
    CHECK_SSE4A("xmm3", "xmm4", "0xf2, 0x0f, 0x79, 0xdc", initial, insert_field,
                0x51B2C3D4E5F60718ULL, "INSERTQ register without REX reads the source high quadword");
    CHECK_SSE4A("xmm11", "xmm12", "0x66, 0x49, 0x0f, 0x78, 0xc3, 12, 12",
                initial, extract_field, 0xF60,
                "EXTRQ immediate uses REX.B and ignores REX.W");
    CHECK_SSE4A("xmm11", "xmm12", "0x66, 0x45, 0x0f, 0x79, 0xdc", initial, extract_field, 0xA,
                "EXTRQ register reads the source low quadword");
    CHECK_SSE4A("xmm11", "xmm12", "0xf2, 0x45, 0x0f, 0x78, 0xdc, 8, 8", initial, insert_field,
                0xA1B2C3D4E5F6B518ULL, "INSERTQ immediate preserves bits outside the field");
    CHECK_SSE4A("xmm11", "xmm12", "0xf2, 0x4d, 0x0f, 0x79, 0xdc", initial, insert_field,
                0x51B2C3D4E5F60718ULL, "INSERTQ register reads the source high quadword");
    CHECK_SSE4A("xmm11", "xmm12", "0x66, 0x41, 0x0f, 0x78, 0xc3, 0, 0", initial, extract_field,
                0xA1B2C3D4E5F60718ULL, "EXTRQ length zero selects all 64 bits");
    CHECK_SSE4A("xmm11", "xmm12", "0xf2, 0x45, 0x0f, 0x78, 0xdc, 0, 0", initial, insert_field,
                0x9876543210FEDCB5ULL, "INSERTQ length zero replaces all 64 bits");
}

#undef CHECK_SSE4A

// ADCX (66 0F 38 F6 /r) and ADOX (F3 0F 38 F6 /r) are the ADX extension
// (AMD APM vol. 3, 24594). Rosetta 2 may not implement them, so the VEH detects
// the opcode and writes an adx_illegal_instruction record. There is no counter
// export for this path (unlike bx_veh_get_clwb_trap_count), so the record is
// verified by reading BX_LOG_FILE, the same way test_d3d12_xs does.
bool file_has_line_with(const char* path, const char* a, const char* b) {
    HANDLE f = CreateFileA(path, GENERIC_READ, FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE, nullptr,
                           OPEN_EXISTING, FILE_ATTRIBUTE_NORMAL, nullptr);
    if (f == INVALID_HANDLE_VALUE) {
        return false;
    }
    bool found = false;
    LARGE_INTEGER size = {};
    if (GetFileSizeEx(f, &size) && size.QuadPart > 0 && size.QuadPart < (64ll << 20)) {
        auto* buf = static_cast<char*>(HeapAlloc(GetProcessHeap(), 0, static_cast<SIZE_T>(size.QuadPart) + 1));
        DWORD got = 0;
        if (buf && ReadFile(f, buf, static_cast<DWORD>(size.QuadPart), &got, nullptr)) {
            buf[got] = '\0';
            for (char* line = buf; line && *line && !found;) {
                char* end = std::strchr(line, '\n');
                if (end) *end = '\0';
                found = std::strstr(line, a) && std::strstr(line, b);
                line = end ? end + 1 : nullptr;
            }
        }
        if (buf) HeapFree(GetProcessHeap(), 0, buf);
    }
    CloseHandle(f);
    return found;
}

void adcx32(uint32_t* dst, uint32_t src, uint32_t carry) {
    uint64_t flags = carry ? 1u : 0u;
    __asm__ volatile("push %[f]\n\tpopfq\n\t.byte 0x66, 0x0f, 0x38, 0xf6, 0xc1"
                     : "+a"(*dst)
                     : "c"(src), [f] "r"(flags)
                     : "cc");
}

void adox32(uint32_t* dst, uint32_t src, uint32_t overflow) {
    uint64_t flags = overflow ? 0x800u : 0u;
    __asm__ volatile("push %[f]\n\tpopfq\n\t.byte 0xf3, 0x0f, 0x38, 0xf6, 0xc1"
                     : "+a"(*dst)
                     : "c"(src), [f] "r"(flags)
                     : "cc");
}

void adcx64(uint64_t* dst, uint64_t src, uint64_t carry) {
    uint64_t flags = carry ? 1u : 0u;
    __asm__ volatile("push %[f]\n\tpopfq\n\t.byte 0x66, 0x48, 0x0f, 0x38, 0xf6, 0xc1"
                     : "+a"(*dst)
                     : "c"(src), [f] "r"(flags)
                     : "cc");
}

void adox64(uint64_t* dst, uint64_t src, uint64_t overflow) {
    uint64_t flags = overflow ? 0x800u : 0u;
    __asm__ volatile("push %[f]\n\tpopfq\n\t.byte 0xf3, 0x48, 0x0f, 0x38, 0xf6, 0xc1"
                     : "+a"(*dst)
                     : "c"(src), [f] "r"(flags)
                     : "cc");
}

void check_adx_tripwire() {
    char log_path[MAX_PATH];
    const DWORD log_len = GetEnvironmentVariableA("BX_LOG_FILE", log_path, sizeof(log_path));
    const bool have_log = log_len > 0 && log_len < sizeof(log_path);
    if (!have_log) {
        std::printf("skip: BX_LOG_FILE is not set; ADX records not verified\n");
    }

    // The tripwire does NOT emulate: it logs and returns EXCEPTION_CONTINUE_SEARCH,
    // so the illegal-instruction exception propagates unless another handler takes
    // it. mingw-w64's g++ has no __try/__except, so this fixture installs its own
    // vectored handler for the duration of each probe, records whether it saw
    // EXCEPTION_ILLEGAL_INSTRUCTION, and continues execution there. If Rosetta 2
    // supports the instruction natively there is no exception and no record, which
    // is a legitimate outcome to report rather than a failure.
    static volatile LONG g_adx_seen = 0;
    auto run = [](void (*body)(void)) -> bool {
        g_adx_seen = 0;
        PVOID token = AddVectoredExceptionHandler(
            1,
            [](PEXCEPTION_POINTERS ep) -> LONG {
                if (ep && ep->ExceptionRecord &&
                    ep->ExceptionRecord->ExceptionCode == EXCEPTION_ILLEGAL_INSTRUCTION) {
                    InterlockedExchange(&g_adx_seen, 1);
                    // Skip the trapping instruction (6 bytes max for the forms
                    // above) and continue, so the probe returns normally.
                    ep->ContextRecord->Rip += 6;
                    return EXCEPTION_CONTINUE_EXECUTION;
                }
                return EXCEPTION_CONTINUE_SEARCH;
            });
        body();
        if (token) RemoveVectoredExceptionHandler(token);
        return g_adx_seen != 0;
    };

    struct Probes {
        static void adcx32_probe() {
            uint32_t dst = 5;
            adcx32(&dst, 7, 1);
        }
        static void adox32_probe() {
            uint32_t dst = 5;
            adox32(&dst, 7, 1);
        }
        static void adcx64_probe() {
            uint64_t dst = 5;
            adcx64(&dst, 7, 1);
        }
        static void adox64_probe() {
            uint64_t dst = 5;
            adox64(&dst, 7, 1);
        }
    };

    const unsigned traps = (run(Probes::adcx32_probe) ? 1u : 0u) + (run(Probes::adox32_probe) ? 1u : 0u) +
                           (run(Probes::adcx64_probe) ? 1u : 0u) + (run(Probes::adox64_probe) ? 1u : 0u);
    std::printf("ADX traps observed: %u of 4 encodings\n", traps);

    if (have_log) {
        if (traps > 0) {
            // A trap without a record means the tripwire regressed.
            expect(file_has_line_with(log_path, "\"event\":\"adx_illegal_instruction\"", "runtime_veh"),
                   "a trapped ADCX/ADOX produces an adx_illegal_instruction record");
            expect(file_has_line_with(log_path, "\"event\":\"adx_illegal_instruction\"", "rip=0x"),
                   "the ADX record carries the trapping RIP");
            expect(file_has_line_with(log_path, "\"event\":\"adx_illegal_instruction\"", "bytes="),
                   "the ADX record carries the opcode bytes");
        } else {
            std::printf("skip: ADCX/ADOX executed natively on this host; tripwire not exercised\n");
        }
    }
    expect(traps == 0 || traps == 4,
           "the four ADX encodings agree on whether Rosetta 2 traps them");

    // Negative controls: instructions that are NOT ADX must neither trap here nor
    // be tagged by the tripwire. A plain ADC and a plain ADD run normally.
    volatile uint32_t plain = 1;
    __asm__ volatile("clc\n\tadc $1, %0" : "+r"(plain) :: "cc");
    expect(plain == 2, "plain ADC executes and is not intercepted");
    volatile uint32_t added = 1;
    added += 2;
    expect(added == 3, "plain ADD executes and is not intercepted");
}

} // namespace

int main() {
    // Keep the DLL import so that its VEH is installed before this fixture.
    (void)bx_veh_get_clwb_trap_count();
    check_cache_lines();
    check_address_override();
    check_write_fault();
    check_wait_instructions();
    check_processor_registers();
    check_bit_fields();
    check_adx_tripwire();

    std::printf("%s: test_veh_amd_only (%u/%u checks)\n",
                failures ? "FAIL" : "PASS", checks - failures, checks);
    return failures ? 1 : 0;
}

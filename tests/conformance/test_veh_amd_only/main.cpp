// Synthetic Windows x64 fixture for the Rosetta exception path. Expectations
// come from the AMD APM vol. 3 (24594, rev. 3.36), CLZERO, MONITORX, MWAITX,
// and RDPRU, and vol. 4 (26568, rev. 3.25), EXTRQ and INSERTQ.
#include <windows.h>
#include <cpuid.h>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <string>
#include <vector>

extern "C" uint64_t bx_veh_get_clwb_trap_count(void);

// ---------------------------------------------------------------------------
// BX_LOG_FILE reader (same pattern as test_veh_clwb / test_win32_bridge).
// If BX_LOG_FILE is unset we create a temp log and point the variable at it
// so the runtime_veh logger writes somewhere we can read back.
// ---------------------------------------------------------------------------

struct LogReader {
    char path[MAX_PATH]{};
    bool created_temp = false;
    int64_t start_offset = 0;

    bool init() {
        const DWORD existing_len =
            GetEnvironmentVariableA("BX_LOG_FILE", path, sizeof(path));
        if (existing_len == 0 || existing_len >= sizeof(path)) {
            char temp_dir[MAX_PATH]{};
            const DWORD dir_len = GetTempPathA(sizeof(temp_dir), temp_dir);
            if (dir_len == 0 || dir_len >= sizeof(temp_dir) ||
                GetTempFileNameA(temp_dir, "bxa", 0, path) == 0) {
                std::fprintf(stderr, "FAIL: could not create temporary AMD log file\n");
                return false;
            }
            if (!SetEnvironmentVariableA("BX_LOG_FILE", path)) {
                DeleteFileA(path);
                std::fprintf(stderr, "FAIL: could not set BX_LOG_FILE\n");
                return false;
            }
            created_temp = true;
        }
        start_offset = query_file_size(path);
        return true;
    }

    ~LogReader() {
        if (created_temp) {
            DeleteFileA(path);
        }
    }

    static int64_t query_file_size(const char* p) {
        HANDLE f = CreateFileA(p, GENERIC_READ, FILE_SHARE_READ | FILE_SHARE_WRITE,
                                 nullptr, OPEN_EXISTING, FILE_ATTRIBUTE_NORMAL, nullptr);
        if (f == INVALID_HANDLE_VALUE) {
            return 0;
        }
        LARGE_INTEGER size{};
        const bool ok = GetFileSizeEx(f, &size) != 0;
        CloseHandle(f);
        return ok && size.QuadPart > 0 ? size.QuadPart : 0;
    }

    // Read only the bytes appended after start_offset.
    bool read_new(std::string& out) const {
        HANDLE f = CreateFileA(path, GENERIC_READ, FILE_SHARE_READ | FILE_SHARE_WRITE,
                                 nullptr, OPEN_EXISTING, FILE_ATTRIBUTE_NORMAL, nullptr);
        if (f == INVALID_HANDLE_VALUE) {
            std::fprintf(stderr, "FAIL: could not open AMD log %s (error %lu)\n",
                         path, GetLastError());
            return false;
        }
        LARGE_INTEGER size{};
        if (!GetFileSizeEx(f, &size) || size.QuadPart < start_offset) {
            CloseHandle(f);
            std::fprintf(stderr, "FAIL: invalid AMD log size for %s\n", path);
            return false;
        }
        const int64_t delta = size.QuadPart - start_offset;
        if (delta < 0 || delta > (64LL << 20)) {
            CloseHandle(f);
            std::fprintf(stderr, "FAIL: unexpected AMD log delta (%lld bytes)\n",
                         static_cast<long long>(delta));
            return false;
        }
        LARGE_INTEGER seek{};
        seek.QuadPart = start_offset;
        if (!SetFilePointerEx(f, seek, nullptr, FILE_BEGIN)) {
            CloseHandle(f);
            return false;
        }
        out.assign(static_cast<size_t>(delta), '\0');
        size_t total_read = 0;
        while (total_read < out.size()) {
            DWORD chunk = 0;
            const DWORD want = static_cast<DWORD>(out.size() - total_read);
            if (!ReadFile(f, out.data() + total_read, want, &chunk, nullptr) || chunk == 0) {
                break;
            }
            total_read += chunk;
        }
        CloseHandle(f);
        out.resize(total_read);
        return true;
    }
};

// One parsed `*_trapped` record.
struct AmdRecord {
    std::string event;
    uint64_t total_traps = 0;
    size_t len = 0;
};

// Parse every `"event":"<kind>_trapped"` line for `len=` and `total_traps=`.
static std::vector<AmdRecord> parse_amd_records(const std::string& data) {
    std::vector<AmdRecord> records;
    size_t pos = 0;
    while (pos < data.size()) {
        size_t nl = data.find('\n', pos);
        if (nl == std::string::npos) {
            nl = data.size();
        }
        const std::string line = data.substr(pos, nl - pos);
        pos = nl + 1;

        // Match any *_trapped event from runtime_veh.
        const size_t ev_key = line.find("\"event\":\"");
        if (ev_key == std::string::npos) {
            continue;
        }
        const size_t ev_start = ev_key + 9;
        const size_t ev_end = line.find('"', ev_start);
        if (ev_end == std::string::npos) {
            continue;
        }
        const std::string event = line.substr(ev_start, ev_end - ev_start);
        if (event.size() < 8 || event.compare(event.size() - 8, 8, "_trapped") != 0) {
            continue;
        }

        AmdRecord rec{};
        rec.event = event;
        const size_t len_key = line.find("len=");
        if (len_key == std::string::npos) {
            continue;
        }
        rec.len = std::stoull(line.substr(len_key + 4));
        const size_t tt_key = line.find("total_traps=");
        if (tt_key == std::string::npos) {
            continue;
        }
        rec.total_traps = std::stoull(line.substr(tt_key + 12));
        records.push_back(rec);
    }
    return records;
}

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

bool check_address_override() {
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
        return false;
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
    return true;
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

} // namespace

// Exercise the ADX tripwire. bx_runtime_veh logs
// `adx_illegal_instruction` when it sees ADCX/ADOX (0F 38 F6), and no fixture
// executed one before, so the record was never observed by a test.
//
// Encodings, re-derived by assembling with x86_64-w64-mingw32-as and
// disassembling with x86_64-w64-mingw32-objdump -d:
//
//   66 0f 38 f6 d8           adcx %eax,%ebx                 -> 5 bytes
//   f3 0f 38 f6 d8           adox %eax,%ebx                 -> 5 bytes
//   66 0f 38 f6 1c 88        adcx (%rax,%rcx,4),%ebx        -> 6 bytes (SIB)
//   66 0f 38 f6 5c 88 40     adcx 0x40(%rax,%rcx,4),%ebx    -> 7 bytes (SIB + disp8)
//   66 0f 38 f6 98 00 01 00 00  adcx 0x100(%rax),%ebx        -> 9 bytes (disp32)
//   66 0f 38 f6 1d <disp32>  adcx (%rip),%ebx                -> 9 bytes (RIP-relative disp32)
//
// The first revision of the shim decoder returned `idx + 4` — opcode plus one
// ModRM byte — so every memory form advanced Rip 1-4 bytes short of the real
// instruction (#545). The fixture therefore emits SIB, disp8, disp32 and
// RIP-relative forms and asserts the logged `len=`, not just that a record
// appeared.
//
// Whether these trap depends on the HOST CPU, not on the shim: without ADX
// they raise #UD and the tripwire logs; with ADX they execute and nothing is
// logged. The check reads CPUID leaf 7 EBX[19] and asserts the case that
// applies, so it never demands a trap that did not happen.
//
// The fixture emits six forms; on a host without ADX the shim must log each
// one's real length and advance Rip past it. The handler logs and resumes
// itself (`Rip += adx_instruction_length`), so reaching the next statement is
// the check that the guest was not left re-faulting; the logged `len=` is the
// check that Rip landed on the next instruction boundary rather than inside
// the SIB or displacement bytes.
uintptr_t adx_first_rip = 0;

// First `rip=0x...` value in the freshly appended log text, or 0 when absent.
uint64_t parse_first_rip(const std::string& text) {
    const size_t key = text.find("rip=0x");
    if (key == std::string::npos) {
        return 0;
    }
    return std::strtoull(text.c_str() + key + 6, nullptr, 16);
}

// Every `len=` on an `adx_illegal_instruction` record, in log order.
std::vector<size_t> parse_adx_lengths(const std::string& text) {
    std::vector<size_t> lengths;
    size_t pos = 0;
    while (pos < text.size()) {
        size_t nl = text.find('\n', pos);
        if (nl == std::string::npos) {
            nl = text.size();
        }
        const std::string line = text.substr(pos, nl - pos);
        pos = nl + 1;
        if (line.find("\"event\":\"adx_illegal_instruction\"") == std::string::npos) {
            continue;
        }
        const size_t key = line.find("len=");
        if (key == std::string::npos) {
            continue;
        }
        lengths.push_back(static_cast<size_t>(std::strtoull(line.c_str() + key + 4, nullptr, 10)));
    }
    return lengths;
}

void check_adx_tripwire(const LogReader& log, const std::string& before) {
    unsigned eax = 0, ebx = 0, ecx = 0, edx = 0;
    // CPUID leaf 7, EBX bit 19 = ADX.
    const bool host_has_adx =
        __get_cpuid_count(7, 0, &eax, &ebx, &ecx, &edx) && (ebx & (1u << 19)) != 0;
    std::printf("OBSERVED: CPUID.7.0:EBX[19] (ADX) = %u\n", host_has_adx ? 1u : 0u);

    // bx_runtime_veh advances Rip past ADCX/ADOX itself, so these execute
    // either natively (host has ADX) or trap, get logged, and resume. The
    // fixture does not need a handler of its own; reaching the next statement
    // is the check that the guest was not left re-faulting. The assembler
    // label `first_adcx` is emitted immediately before the first ADCX *inside
    // the same asm block*, so `lea first_adcx(%rip)` yields the exact address
    // of the faulting instruction. A C++ label (`&&label`) is NOT reliable
    // here: the compiler may place it on an earlier instruction (e.g. the
    // `lea`/`call` for the preceding printf), which made `adx_first_rip`
    // differ from the logged `rip=` and failed the live run.
    __asm__ volatile(
        "first_adcx:\n\t"
        ".byte 0x66, 0x0f, 0x38, 0xf6, 0xd8\n\t"  // adcx %eax,%ebx
        "lea first_adcx(%%rip), %0"
        : "=r"(adx_first_rip)
        :
        : "eax", "ebx", "cc");
    __asm__ volatile(".byte 0xf3, 0x0f, 0x38, 0xf6, 0xd8" ::: "eax", "ebx", "cc");
    expect(true, "ADCX and ADOX execute or trap-and-resume without wedging the guest");

    // Memory forms. `rax` and `rcx` are set explicitly so the emitted bytes
    // address a readable 32-bit word inside the fixture's own buffer. Each
    // form's byte sequence was derived by assembling with
    // x86_64-w64-mingw32-as and disassembling with
    // x86_64-w64-mingw32-objdump -d (see the table above).
    // 512 bytes, so the disp8 (0x40) and disp32 (0x100) forms below both land
    // inside it when they execute on a host that has ADX.
    static volatile uint32_t adx_words[128] = {0x11223344u, 0x55667788u, 0x99aabbccu, 0xddeeff00u};
    const uintptr_t words_base = reinterpret_cast<uintptr_t>(adx_words);
    // SIB, no displacement: adcx (%rax,%rcx,4),%ebx -> 66 0f 38 f6 1c 88 (6).
    __asm__ volatile("mov %0, %%rax\n\txor %%rcx, %%rcx\n\t"
                     ".byte 0x66, 0x0f, 0x38, 0xf6, 0x1c, 0x88"
                     :: "r"(words_base) : "rax", "rcx", "ebx", "cc", "memory");
    // SIB + disp8: adcx 0x40(%rax,%rcx,4),%ebx -> 66 0f 38 f6 5c 88 40 (7).
    __asm__ volatile("mov %0, %%rax\n\txor %%rcx, %%rcx\n\t"
                     ".byte 0x66, 0x0f, 0x38, 0xf6, 0x5c, 0x88, 0x40"
                     :: "r"(words_base) : "rax", "rcx", "ebx", "cc", "memory");
    // disp32: adcx 0x100(%rax),%ebx -> 66 0f 38 f6 98 00 01 00 00 (9).
    __asm__ volatile("mov %0, %%rax\n\t"
                     ".byte 0x66, 0x0f, 0x38, 0xf6, 0x98, 0x00, 0x01, 0x00, 0x00"
                     :: "r"(words_base) : "rax", "ebx", "cc", "memory");
    // RIP-relative disp32: adcx (%rip),%ebx -> 66 0f 38 f6 1d <disp32> (9).
    // The displacement is 0 here, which is a valid encoding of the form; the
    // decoder must consume all four displacement bytes regardless of value.
    __asm__ volatile(".byte 0x66, 0x0f, 0x38, 0xf6, 0x1d, 0x00, 0x00, 0x00, 0x00"
                     ::: "eax", "ebx", "cc", "memory");
    expect(true, "memory-operand ADCX forms execute or trap-and-resume");

    std::string after;
    if (!log.read_new(after)) {
        expect(false, "the ADX log could be read back");
        return;
    }
    // Only the bytes this check appended.
    const std::string fresh =
        after.size() > before.size() ? after.substr(before.size()) : std::string();
    const bool saw = fresh.find("\"event\":\"adx_illegal_instruction\"") != std::string::npos;

    if (host_has_adx) {
        // Nothing traps, so the shim must log nothing. Asserting a record here
        // would demand a trap that did not happen.
        expect(!saw, "with ADX advertised, the tripwire logged nothing because nothing trapped");
    } else {
        expect(saw, "without ADX advertised, the tripwire logged adx_illegal_instruction");
        if (saw) {
            expect(fresh.find("bytes=66 0f 38 f6 d8") != std::string::npos ||
                   fresh.find("bytes=f3 0f 38 f6 d8") != std::string::npos,
                   "the tripwire dumps the ADCX/ADOX bytes it saw");
            // The dumped rip must be the instruction that faulted, not one of
            // the earlier AMD instructions this fixture also trapped.
            const uint64_t logged = parse_first_rip(fresh);
            expect(logged == adx_first_rip,
                   "the logged rip is the faulting ADCX address");
            // Six forms, each logged once: 5, 5, 6, 7, 9 and 9 bytes. The old
            // `idx + 4` decoder reported 5 for all six, so a wrong length is
            // visible here even though every effect check would still pass.
            const std::vector<size_t> lengths = parse_adx_lengths(fresh);
            const std::vector<size_t> expected{5, 5, 6, 7, 9, 9};
            expect(lengths == expected,
                   "each ADCX/ADOX form logs its real length (5, 5, 6, 7, 9, 9)");
        }
    }
}

int main() {
    // Keep the DLL import so that its VEH is installed before this fixture.
    (void)bx_veh_get_clwb_trap_count();

    // Set up the log reader BEFORE the first AMD instruction executes so we
    // capture every *_trapped record the VEH emits. The counters are
    // per-kind, so log_sampled_trap() sees count == 1 for the first trap of
    // each kind. That means the first instance of all six kinds is always
    // logged, and all six lengths are observable.
    LogReader log;
    if (!log.init()) {
        return 5;
    }

    check_cache_lines();
    const bool low_page_allocated = check_address_override();
    check_write_fault();
    check_wait_instructions();
    check_processor_registers();
    check_bit_fields();

    // The ADX check needs a snapshot of the log taken before it runs.
    std::string adx_before;
    if (!log.read_new(adx_before)) {
        return 9;
    }
    check_adx_tripwire(log, adx_before);

    // -----------------------------------------------------------------------
    // Verify the decoded instruction length (`len=`) that the VEH logs and
    // uses to advance RIP. A wrong `len` lands the guest mid-instruction
    // while the effect checks still pass, so this is the field that decides
    // whether the guest resumes correctly.
    //
    // log_sampled_trap() logs when count == 1 or count is a power of two.
    // Because the counters are per-kind, the first trap of each kind is
    // logged with total_traps=1. We assert on that record for each kind.
    //
    // Expected lengths, derived from the byte sequences the fixture emits
    // (verified by assembling with x86_64-w64-mingw32-as and disassembling
    // with x86_64-w64-mingw32-objdump -d):
    //
    //   clzero:   0f 01 fc                    -> 3
    //   monitorx: 67 0f 01 fa                 -> 4  (addr32, if low page allocated)
    //             0f 01 fa                    -> 3  (otherwise, first in check_wait_instructions)
    //   mwaitx:   0f 01 fb                    -> 3
    //   rdpru:    0f 01 fd                    -> 3
    //   extrq:    66 0f 78 c3 0c 0c           -> 6  (first CHECK_SSE4A)
    //   insertq:  f2 0f 78 dc 08 08           -> 6  (first CHECK_SSE4A)
    //
    // Note: extrq and insertq appear multiple times with different lengths
    // (4, 5, 6, 7 bytes). total_traps=1 corresponds to the FIRST occurrence
    // in check_bit_fields(), which is the 6-byte immediate form.
    //
    // Note: when check_address_override() allocates its optional page below
    // 4 GiB, the first MONITORX is the 4-byte addr32 form (67 0f 01 fa).
    // If that allocation is unavailable, it returns early and the first
    // MONITORX is the ordinary 3-byte form (0f 01 fa) in
    // check_wait_instructions(). Select the expected length accordingly.
    // -----------------------------------------------------------------------
    {
        std::string log_data;
        if (!log.read_new(log_data)) {
            return 6;
        }
        const std::vector<AmdRecord> records = parse_amd_records(log_data);
        if (records.empty()) {
            std::fprintf(stderr,
                         "FAIL: no *_trapped records found in log %s "
                         "(BX_LOG_FILE must be readable)\n", log.path);
            return 7;
        }

        // Every observed record must have 0 < len <= 15 (x86 maximum).
        for (const auto& r : records) {
            if (r.len == 0 || r.len > 15) {
                std::fprintf(stderr,
                             "FAIL: %s total_traps=%llu has invalid len=%zu\n",
                             r.event.c_str(),
                             static_cast<unsigned long long>(r.total_traps), r.len);
                return 8;
            }
        }

        // Exact expectations for the first trap of each kind.
        struct Expected { const char* event; size_t len; const char* form; };
        const Expected kExpected[] = {
            {"clzero_trapped",   3, "clzero (0f 01 fc)"},
            {"monitorx_trapped",
             low_page_allocated ? 4u : 3u,
             low_page_allocated ? "monitorx addr32 (67 0f 01 fa)"
                                 : "monitorx (0f 01 fa)"},
            {"mwaitx_trapped",   3, "mwaitx (0f 01 fb)"},
            {"rdpru_trapped",    3, "rdpru (0f 01 fd)"},
            {"extrq_trapped",    6, "extrq $0xc,$0xc,%xmm3 (66 0f 78 c3 0c 0c)"},
            {"insertq_trapped",  6, "insertq $0x8,$0x8,%xmm4,%xmm3 (f2 0f 78 dc 08 08)"},
        };
        for (const auto& exp : kExpected) {
            bool found = false;
            for (const auto& r : records) {
                if (r.event == exp.event && r.total_traps == 1) {
                    found = true;
                    if (r.len != exp.len) {
                        std::fprintf(stderr,
                                     "FAIL: %s total_traps=1 (%s): "
                                     "expected len=%zu, got len=%zu\n",
                                     exp.event, exp.form, exp.len, r.len);
                        return 9;
                    }
                    break;
                }
            }
            if (!found) {
                std::fprintf(stderr,
                             "FAIL: missing %s record with total_traps=1 (%s)\n",
                             exp.event, exp.form);
                return 10;
            }
        }
        std::printf("AMD decoded lengths verified for first trap of all six kinds "
                    "(%zu records)\n", records.size());
    }

    std::printf("%s: test_veh_amd_only (%u/%u checks)\n",
                failures ? "FAIL" : "PASS", checks - failures, checks);
    return failures ? 1 : 0;
}

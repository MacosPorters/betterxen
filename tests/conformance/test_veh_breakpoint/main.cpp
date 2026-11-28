/*
 * runtime_veh records breakpoints and assertion failures even when the guest has replaced the
 * top-level exception filter, as a title with its own crash reporter does.
 *
 * The test installs its own top-level filter and a last-registered vectored handler that
 * resumes after each int3, then raises breakpoints at a labelled site and one
 * STATUS_ASSERTION_FAILURE. The expected RVA is the label's address minus this module's base,
 * computed here, not by the shim. Needs BX_LOG_FILE (bx-cli --log).
 */
#include <windows.h>

#include <cstdint>
#include <cstdio>
#include <string>

extern "C" uint64_t bx_veh_get_clwb_trap_count(void);

extern "C" void bx_breakpoint_site(void);
__asm__(".text\n"
        ".globl bx_breakpoint_site\n"
        "bx_breakpoint_site:\n"
        "    int3\n"
        "    ret\n");

namespace {

constexpr DWORD kStatusAssertionFailure = 0xC0000420;
constexpr int kRecordLimit = 16;
int g_failures = 0;
volatile LONG g_resumed = 0;
volatile LONG g_filter_calls = 0;

void check(bool ok, const char* what) {
    if (!ok) {
        std::fprintf(stderr, "FAIL: test_veh_breakpoint: %s\n", what);
        ++g_failures;
    }
}

LONG WINAPI guest_filter(PEXCEPTION_POINTERS) {
    InterlockedIncrement(&g_filter_calls);
    return EXCEPTION_CONTINUE_SEARCH;
}

LONG CALLBACK resume(PEXCEPTION_POINTERS ep) {
    const DWORD code = ep->ExceptionRecord->ExceptionCode;
    if (code == EXCEPTION_BREAKPOINT) {
        ep->ContextRecord->Rip =
            reinterpret_cast<uintptr_t>(ep->ExceptionRecord->ExceptionAddress) + 1;
        InterlockedIncrement(&g_resumed);
        return EXCEPTION_CONTINUE_EXECUTION;
    }
    if (code == kStatusAssertionFailure) {
        InterlockedIncrement(&g_resumed);
        return EXCEPTION_CONTINUE_EXECUTION;
    }
    return EXCEPTION_CONTINUE_SEARCH;
}

std::string read_log_from(long offset) {
    char path[MAX_PATH];
    const DWORD n = GetEnvironmentVariableA("BX_LOG_FILE", path, sizeof(path));
    if (n == 0 || n >= sizeof(path)) return {};
    std::string text;
    if (FILE* file = std::fopen(path, "rb")) {
        if (std::fseek(file, offset, SEEK_SET) == 0) {
            char buffer[4096];
            size_t got;
            while ((got = std::fread(buffer, 1, sizeof(buffer), file)) > 0) text.append(buffer, got);
        }
        std::fclose(file);
    }
    return text;
}

long log_size() {
    char path[MAX_PATH];
    const DWORD n = GetEnvironmentVariableA("BX_LOG_FILE", path, sizeof(path));
    if (n == 0 || n >= sizeof(path)) return -1;
    long size = 0;
    if (FILE* file = std::fopen(path, "rb")) {
        if (std::fseek(file, 0, SEEK_END) == 0) size = std::ftell(file);
        std::fclose(file);
    }
    return size < 0 ? 0 : size;
}

int count_lines_with(const std::string& log, const char* a, const char* b = nullptr) {
    int count = 0;
    size_t start = 0;
    while (start < log.size()) {
        size_t end = log.find('\n', start);
        if (end == std::string::npos) end = log.size();
        const std::string line = log.substr(start, end - start);
        if (line.find(a) != std::string::npos && (!b || line.find(b) != std::string::npos)) {
            ++count;
        }
        start = end + 1;
    }
    return count;
}

}  // namespace

int main() {
    // Keep a static import on bx_runtime_veh.dll so its handler is installed at attach.
    volatile uint64_t attached = bx_veh_get_clwb_trap_count();
    (void)attached;

    const long mark = log_size();
    if (mark < 0) {
        std::fprintf(stderr, "FAIL: test_veh_breakpoint: BX_LOG_FILE must be set\n");
        return 1;
    }

    // A title's crash reporter takes over the top-level filter after runtime_veh attached.
    SetUnhandledExceptionFilter(guest_filter);
    PVOID handler = AddVectoredExceptionHandler(0, resume);
    check(handler != nullptr, "AddVectoredExceptionHandler");

    bx_breakpoint_site();
    RaiseException(kStatusAssertionFailure, 0, 0, nullptr);
    for (int i = 0; i < kRecordLimit + 4; ++i) bx_breakpoint_site();
    RemoveVectoredExceptionHandler(handler);

    check(g_resumed == kRecordLimit + 6, "every exception reached the guest's own handler");
    check(g_filter_calls == 0, "no exception went unhandled");

    const uintptr_t base = reinterpret_cast<uintptr_t>(GetModuleHandleW(nullptr));
    const uintptr_t site = reinterpret_cast<uintptr_t>(&bx_breakpoint_site);
    char rva[64];
    std::snprintf(rva, sizeof(rva), "\"rva\":\"0x%016llx\"",
                  static_cast<unsigned long long>(site - base));

    const std::string log = read_log_from(mark);
    const int records = count_lines_with(log, "\"event\":\"breakpoint_exception\"");
    check(records == kRecordLimit, "records stop at the per-process limit of 16");
    check(count_lines_with(log, "\"exception_code\":\"0x80000003\"", rva) == kRecordLimit - 1,
          "each recorded int3 names the labelled site's RVA");
    check(count_lines_with(log, "\"exception_code\":\"0xc0000420\"") == 1,
          "the assertion failure is recorded");
    // RaiseException reports an address inside kernelbase, so only the int3 records name this
    // module.
    check(count_lines_with(log, "\"module\":\"test_veh_breakpoint.exe\"") == kRecordLimit - 1,
          "int3 records name this module");
    check(count_lines_with(log, "\"event\":\"unhandled_exception\"") == 0,
          "nothing was reported as unhandled");

    if (g_failures) return 1;
    std::printf("PASS: test_veh_breakpoint (%d records, cap %d)\n", records, kRecordLimit);
    return 0;
}

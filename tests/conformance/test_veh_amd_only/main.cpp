// Runs each AMD-only instruction bx_runtime_veh emulates and checks the
// architectural result. On a CPU that has the instruction it runs natively
// and must give the same answer, so the checks hold on either path.
#include <windows.h>
#include <cstdint>
#include <cstdio>
#include <cstring>

#include "bx_config.h"

extern "C" {
uint64_t bx_veh_get_clwb_trap_count(void);
}

namespace {

int g_failures = 0;

void check(bool ok, const char* what, unsigned long long got, unsigned long long want) {
    if (!ok) {
        std::fprintf(stderr, "FAIL: %s: got 0x%016llx, want 0x%016llx\n", what, got, want);
        ++g_failures;
    } else {
        std::printf("ok: %s\n", what);
    }
}

void test_clzero() {
    alignas(64) uint8_t buf[192];
    std::memset(buf, 0xAB, sizeof(buf));
    // Any address inside the line selects it.
    __asm__ volatile(".byte 0x0f, 0x01, 0xfc" /* clzero */
                     :: "a"(buf + 64 + 17) : "memory");
    bool zeroed = true;
    for (int i = 64; i < 128; ++i) zeroed &= buf[i] == 0;
    bool kept = true;
    for (int i = 0; i < 64; ++i) kept &= buf[i] == 0xAB;
    for (int i = 128; i < 192; ++i) kept &= buf[i] == 0xAB;
    check(zeroed, "clzero zeroes the line holding rax", buf[64], 0);
    check(kept, "clzero leaves the neighbouring lines", buf[0], 0xAB);
}

void test_clzero_address_size_override() {
    // 67 0F 01 FC selects EAX, so the line must sit below 4 GiB. Ask for a low
    // address; if it is taken, skip rather than fail.
    uint8_t* buf = static_cast<uint8_t*>(VirtualAlloc(reinterpret_cast<void*>(0x20000000),
                                                      4096, MEM_COMMIT | MEM_RESERVE,
                                                      PAGE_READWRITE));
    if (!buf || (reinterpret_cast<uintptr_t>(buf) >> 32) != 0) {
        std::printf("skip: clzero with 67 prefix: no buffer below 4 GiB\n");
        return;
    }
    std::memset(buf, 0xAB, 256);
    // The high half of RAX must not matter.
    const uint64_t rax = (0xDEADBEEFULL << 32) | static_cast<uint32_t>(
                             reinterpret_cast<uintptr_t>(buf + 64 + 5));
    __asm__ volatile(".byte 0x67, 0x0f, 0x01, 0xfc" /* clzero (EAX) */
                     :: "a"(rax) : "memory");
    bool zeroed = true;
    for (int i = 64; i < 128; ++i) zeroed &= buf[i] == 0;
    bool kept = buf[63] == 0xAB && buf[128] == 0xAB;
    check(zeroed && kept, "67 clzero zeroes exactly the line held in eax", buf[64], 0);
    VirtualFree(buf, 0, MEM_RELEASE);
}

DWORD g_fault_code = 0;
ULONG_PTR g_fault_write = ~0ULL;
ULONG_PTR g_fault_address = 0;

LONG CALLBACK record_fault(PEXCEPTION_POINTERS ep) {
    // Added last, so it runs after bx_runtime_veh and sees what that left.
    if (ep->ExceptionRecord->ExceptionCode != EXCEPTION_ACCESS_VIOLATION &&
        ep->ExceptionRecord->ExceptionCode != EXCEPTION_ILLEGAL_INSTRUCTION) {
        return EXCEPTION_CONTINUE_SEARCH;
    }
    g_fault_code = ep->ExceptionRecord->ExceptionCode;
    if (ep->ExceptionRecord->NumberParameters >= 2) {
        g_fault_write = ep->ExceptionRecord->ExceptionInformation[0];
        g_fault_address = ep->ExceptionRecord->ExceptionInformation[1];
    }
    ep->ContextRecord->Rip += 3;  // step over the 3-byte clzero
    return EXCEPTION_CONTINUE_EXECUTION;
}

void test_clzero_fault() {
    void* page = VirtualAlloc(nullptr, 4096, MEM_COMMIT | MEM_RESERVE, PAGE_READONLY);
    if (!page) {
        check(false, "clzero fault: VirtualAlloc", 0, 1);
        return;
    }
    PVOID handler = AddVectoredExceptionHandler(0, record_fault);
    __asm__ volatile(".byte 0x0f, 0x01, 0xfc" /* clzero */
                     :: "a"(static_cast<char*>(page) + 100) : "memory");
    RemoveVectoredExceptionHandler(handler);
    check(g_fault_code == EXCEPTION_ACCESS_VIOLATION,
          "clzero on a read-only page raises an access violation", g_fault_code,
          EXCEPTION_ACCESS_VIOLATION);
    check(g_fault_write == 1, "the access violation is a write", g_fault_write, 1);
    check(g_fault_address == (reinterpret_cast<uintptr_t>(page) + 64),
          "the access violation names the line", g_fault_address,
          reinterpret_cast<uintptr_t>(page) + 64);
    VirtualFree(page, 0, MEM_RELEASE);
}

void test_waits() {
    alignas(64) volatile uint64_t word = 0;
    // monitorx rax, ecx, edx; mwaitx eax, ecx, ebx with the timer enabled
    // (ecx bit 1) so a native wait ends after ebx TSC ticks at most.
    __asm__ volatile(".byte 0x0f, 0x01, 0xfa" /* monitorx */
                     :: "a"(&word), "c"(0), "d"(0) : "memory");
    __asm__ volatile(".byte 0x0f, 0x01, 0xfb" /* mwaitx */
                     :: "a"(0xF0), "c"(2), "b"(10000) : "memory");
    check(true, "monitorx and mwaitx return", 0, 0);
}

uint64_t rdpru(uint32_t which) {
    uint32_t lo, hi;
    __asm__ volatile(".byte 0x0f, 0x01, 0xfd" /* rdpru */
                     : "=a"(lo), "=d"(hi) : "c"(which));
    return (static_cast<uint64_t>(hi) << 32) | lo;
}

void test_rdpru() {
    const uint64_t m0 = rdpru(0);
    const uint64_t a0 = rdpru(1);
    Sleep(1);
    const uint64_t m1 = rdpru(0);
    const uint64_t a1 = rdpru(1);
    // Only monotonicity is asserted: a native core's MPERF/APERF stop counting
    // while it sleeps, so a strict increase across Sleep(1) is not guaranteed.
    check(m0 != 0 && m1 >= m0, "rdpru ecx=0 (MPERF) does not go backwards", m1, m0);
    check(a0 != 0 && a1 >= a0, "rdpru ecx=1 (APERF) does not go backwards", a1, a0);
    check(rdpru(2) == 0, "rdpru with an unknown ecx reads 0", rdpru(2), 0);
}

void test_rdpru_flags() {
    // Every arithmetic flag set going in; the manual lists OF, SF, ZF, AF and PF
    // as cleared and CF as modified, so only those five are asserted.
    uint64_t flags;
    __asm__ volatile("pushfq\n\t"
                     "orq $0x8d5, (%%rsp)\n\t"
                     "popfq\n\t"
                     ".byte 0x0f, 0x01, 0xfd\n\t"
                     "pushfq\n\t"
                     "popq %0"
                     : "=r"(flags) : "c"(0) : "rax", "rdx", "cc", "memory");
    const uint64_t cleared = 0x800 | 0x80 | 0x40 | 0x10 | 0x4;
    check((flags & cleared) == 0, "rdpru clears OF, SF, ZF, AF and PF", flags & cleared, 0);
}

struct Xmm {
    uint64_t low;
    uint64_t high;
};

void test_sse4a() {
    const uint64_t value = 0x123456789ABCDEF0ULL;
    const uint64_t field = (value >> 8) & 0xFFFF;  // 16 bits from bit 8
    Xmm a = {value, 0x1111111111111111ULL};
    Xmm out = {};

    // extrq xmm1, 16, 8
    __asm__ volatile("movdqu (%1), %%xmm1\n\t"
                     ".byte 0x66, 0x0f, 0x78, 0xc1, 0x10, 0x08\n\t"
                     "movdqu %%xmm1, (%0)"
                     :: "r"(&out), "r"(&a) : "xmm1", "memory");
    check(out.low == field, "extrq xmm1, imm, imm", out.low, field);

    // extrq xmm9, 16, 8 (REX.B)
    out = {};
    __asm__ volatile("movdqu (%1), %%xmm9\n\t"
                     ".byte 0x66, 0x41, 0x0f, 0x78, 0xc1, 0x10, 0x08\n\t"
                     "movdqu %%xmm9, (%0)"
                     :: "r"(&out), "r"(&a) : "xmm9", "memory");
    check(out.low == field, "extrq xmm9, imm, imm", out.low, field);

    // extrq xmm1, xmm2: length in xmm2[5:0], index in xmm2[13:8]
    Xmm fields = {(8u << 8) | 16u, 0};
    out = {};
    __asm__ volatile("movdqu (%1), %%xmm1\n\t"
                     "movdqu (%2), %%xmm2\n\t"
                     ".byte 0x66, 0x0f, 0x79, 0xca\n\t"
                     "movdqu %%xmm1, (%0)"
                     :: "r"(&out), "r"(&a), "r"(&fields) : "xmm1", "xmm2", "memory");
    check(out.low == field, "extrq xmm1, xmm2", out.low, field);

    // extrq with length 0 means 64 bits.
    out = {};
    __asm__ volatile("movdqu (%1), %%xmm1\n\t"
                     ".byte 0x66, 0x0f, 0x78, 0xc1, 0x00, 0x00\n\t"
                     "movdqu %%xmm1, (%0)"
                     :: "r"(&out), "r"(&a) : "xmm1", "memory");
    check(out.low == value, "extrq with length 0 keeps 64 bits", out.low, value);

    const uint64_t base = ~0ULL;
    const uint64_t inserted = (base & ~(0xFFFFULL << 8)) | (0x1234ULL << 8);
    Xmm dst = {base, 0};
    Xmm src = {0xFFFF1234ULL, (8u << 8) | 16u};  // high quadword: register-form fields

    // insertq xmm1, xmm2, 16, 8
    out = {};
    __asm__ volatile("movdqu (%1), %%xmm1\n\t"
                     "movdqu (%2), %%xmm2\n\t"
                     ".byte 0xf2, 0x0f, 0x78, 0xca, 0x10, 0x08\n\t"
                     "movdqu %%xmm1, (%0)"
                     :: "r"(&out), "r"(&dst), "r"(&src) : "xmm1", "xmm2", "memory");
    check(out.low == inserted, "insertq xmm1, xmm2, imm, imm", out.low, inserted);

    // insertq xmm1, xmm2: length in xmm2[69:64], index in xmm2[77:72]
    out = {};
    __asm__ volatile("movdqu (%1), %%xmm1\n\t"
                     "movdqu (%2), %%xmm2\n\t"
                     ".byte 0xf2, 0x0f, 0x79, 0xca\n\t"
                     "movdqu %%xmm1, (%0)"
                     :: "r"(&out), "r"(&dst), "r"(&src) : "xmm1", "xmm2", "memory");
    check(out.low == inserted, "insertq xmm1, xmm2", out.low, inserted);

    // insertq xmm10, xmm3, 16, 8 (REX.R)
    out = {};
    __asm__ volatile("movdqu (%1), %%xmm10\n\t"
                     "movdqu (%2), %%xmm3\n\t"
                     ".byte 0xf2, 0x44, 0x0f, 0x78, 0xd3, 0x10, 0x08\n\t"
                     "movdqu %%xmm10, (%0)"
                     :: "r"(&out), "r"(&dst), "r"(&src) : "xmm10", "xmm3", "memory");
    check(out.low == inserted, "insertq xmm10, xmm3, imm, imm", out.low, inserted);

    // extrq xmm9, xmm2 (REX.R on the destination)
    out = {};
    __asm__ volatile("movdqu (%1), %%xmm9\n\t"
                     "movdqu (%2), %%xmm2\n\t"
                     ".byte 0x66, 0x44, 0x0f, 0x79, 0xca\n\t"
                     "movdqu %%xmm9, (%0)"
                     :: "r"(&out), "r"(&a), "r"(&fields) : "xmm9", "xmm2", "memory");
    check(out.low == field, "extrq xmm9, xmm2", out.low, field);

    // insertq xmm1, xmm10 (REX.B on the source)
    out = {};
    __asm__ volatile("movdqu (%1), %%xmm1\n\t"
                     "movdqu (%2), %%xmm10\n\t"
                     ".byte 0xf2, 0x41, 0x0f, 0x79, 0xca\n\t"
                     "movdqu %%xmm1, (%0)"
                     :: "r"(&out), "r"(&dst), "r"(&src) : "xmm1", "xmm10", "memory");
    check(out.low == inserted, "insertq xmm1, xmm10", out.low, inserted);
}

}  // namespace

int main() {
    // Linking the runtime installs its handler at load; touch an export so
    // the import is kept.
    (void)bx_veh_get_clwb_trap_count();

    test_clzero();
    test_clzero_address_size_override();
    test_clzero_fault();
    test_waits();
    test_rdpru();
    test_rdpru_flags();
    test_sse4a();

    if (g_failures != 0) {
        std::fprintf(stderr, "FAIL: test_veh_amd_only (%d checks)\n", g_failures);
        return 1;
    }
    std::printf("PASS: test_veh_amd_only\n");
    return 0;
}

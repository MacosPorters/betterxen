/*
 * #459: Synthetic conformance fixture for bx-cli reversible system32 shim staging.
 *
 * Does NOT statically import xgameruntime.dll. Instead, mirrors the dynamic loader path
 * by calling LoadLibraryExW(L"xgameruntime.dll", nullptr, LOAD_LIBRARY_SEARCH_SYSTEM32),
 * verifying that the loaded module resolves from \windows\system32\xgameruntime.dll and
 * exports InitializeApiImpl, then unloading cleanly.
 */
#include <windows.h>

#include <cstdio>
#include <cwchar>
#include <cwctype>

namespace {

bool ends_with_ci(const wchar_t* str, const wchar_t* suffix) {
    const size_t slen = std::wcslen(str);
    const size_t suflen = std::wcslen(suffix);
    if (slen < suflen) return false;
    const wchar_t* tail = str + (slen - suflen);
    for (size_t i = 0; i < suflen; ++i) {
        if (std::towlower(tail[i]) != std::towlower(suffix[i])) return false;
    }
    return true;
}

} // namespace

int main() {
    HMODULE mod = LoadLibraryExW(L"xgameruntime.dll", nullptr, LOAD_LIBRARY_SEARCH_SYSTEM32);
    if (!mod) {
        std::fprintf(stderr, "FAIL: LoadLibraryExW(xgameruntime.dll, LOAD_LIBRARY_SEARCH_SYSTEM32) failed (err=%lu)\n",
                     static_cast<unsigned long>(GetLastError()));
        return 1;
    }

    wchar_t path[MAX_PATH] = {};
    const DWORD len = GetModuleFileNameW(mod, path, MAX_PATH);
    if (len == 0 || len >= MAX_PATH) {
        std::fprintf(stderr, "FAIL: GetModuleFileNameW failed (err=%lu)\n", static_cast<unsigned long>(GetLastError()));
        FreeLibrary(mod);
        return 1;
    }
    if (!ends_with_ci(path, L"\\windows\\system32\\xgameruntime.dll")) {
        std::fprintf(stderr, "FAIL: unexpected module path: %ls\n", path);
        FreeLibrary(mod);
        return 1;
    }

    FARPROC init_fn = GetProcAddress(mod, "InitializeApiImpl");
    if (!init_fn) {
        std::fprintf(stderr, "FAIL: GetProcAddress(InitializeApiImpl) returned null\n");
        FreeLibrary(mod);
        return 1;
    }

    FreeLibrary(mod);
    std::printf("PASS: test_system32_staging (%ls)\n", path);
    return 0;
}

#include <windows.h>
#include <unknwn.h>

#include <cstdint>
#include <cstdio>
#include <new>

#define CHECK(expr) do { if (!(expr)) { std::fprintf(stderr, "line %d: %s\n", __LINE__, #expr); return 1; } } while (0)

namespace {
constexpr GUID game_input_v0 = {0x11be2a7e, 0x4254, 0x445a,
                                {0x9c, 0x09, 0xff, 0xc4, 0x0f, 0x00, 0x69, 0x18}};
using CreateFn = HRESULT (WINAPI*)(void**);
using RegisterDeviceCb = HRESULT (WINAPI*)(IUnknown*, void*, uint32_t, uint32_t,
                                           uint32_t, void*, void*, uint64_t*);
using UnregisterCb = bool (WINAPI*)(IUnknown*, uint64_t, uint64_t);

void WINAPI dummy_device_callback(uint64_t, void*, void*, uint64_t, uint32_t, uint32_t) {}

bool fail_next_allocation = false;
} // namespace

// Resolved by GameInput.dll through the host executable, so the armed hook is
// the one the DLL's callback map actually uses. Exporting it does not change
// any ABI in abi/defs: this symbol belongs to the fixture. Returning null asks
// the DLL's allocator to report allocation failure, so no exception crosses the
// DLL boundary.
extern "C" __declspec(dllexport) void* BxGameInputTestAllocate(std::size_t size) {
    if (fail_next_allocation) {
        fail_next_allocation = false;
        return nullptr;
    }
    return ::operator new(size);
}

int main() {
    HMODULE dll = LoadLibraryW(L"GameInput.dll");
    CHECK(dll);
    FARPROC ordinal_create = GetProcAddress(dll, MAKEINTRESOURCEA(1));
    CHECK(ordinal_create && ordinal_create == GetProcAddress(dll, "GameInputCreate"));
    auto create = reinterpret_cast<CreateFn>(reinterpret_cast<void*>(ordinal_create));
    void* object = nullptr;
    CHECK(create(&object) == S_OK && object);
    auto* input = static_cast<IUnknown*>(object);
    CHECK(input->QueryInterface(game_input_v0, &object) == S_OK && object == input);
    auto* v0 = static_cast<IUnknown*>(object);
    void** table = *reinterpret_cast<void***>(v0);
    auto reg_dev = reinterpret_cast<RegisterDeviceCb>(table[9]);
    auto unreg_cb = reinterpret_cast<UnregisterCb>(table[13]);
    void* cb_fn = reinterpret_cast<void*>(&dummy_device_callback);

    uint64_t token = 99;
    CHECK(reg_dev(v0, nullptr, 0x10000, 0x1, 2, nullptr, cb_fn, &token) == S_OK && token != 0);
    CHECK(unreg_cb(v0, token, 5000));

    CHECK(GetProcAddress(GetModuleHandleW(nullptr), "BxGameInputTestAllocate"));
    fail_next_allocation = true;
    token = 99;
    const HRESULT oom = reg_dev(v0, nullptr, 0x10000, 0x1, 2, nullptr, cb_fn, &token);
    CHECK(oom == E_OUTOFMEMORY && token == 0);
    CHECK(!fail_next_allocation);

    // The failed insertion must not consume a token or leave the map unusable.
    token = 99;
    CHECK(reg_dev(v0, nullptr, 0x10000, 0x1, 2, nullptr, cb_fn, &token) == S_OK && token == 2);
    CHECK(unreg_cb(v0, token, 5000));

    CHECK(v0->Release() == 1);
    CHECK(input->Release() == 0);
    FreeLibrary(dll);
    std::puts("test_gameinput: allocation failure and normal registration passed");
    return 0;
}

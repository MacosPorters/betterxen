#include <windows.h>
#include <unknwn.h>
#include <atomic>
#include <cstdint>
#include <cstdio>
#include <thread>

extern "C" {
HRESULT WINAPI AcpHalCreate(void**);
HRESULT WINAPI ApuCreateHeap(uint32_t, uint32_t);
HRESULT WINAPI ApuAlloc(void**, uint32_t*, uint32_t, uint32_t);
HRESULT WINAPI ApuFree(void*);
BOOL WINAPI ApuIsVirtualAddressValid(const void*, uint32_t);
void* WINAPI ApuMapApuAddress(uint32_t);
uint32_t WINAPI ApuMapVirtualAddress(const void*);
HRESULT WINAPI XApuConnect(void**);
HRESULT WINAPI XApuDisconnect(void*);
HRESULT WINAPI XApuEnqueueCommand(void*, const void*, uint32_t);
HRESULT WINAPI XApuDequeueResult(void*, void*, uint32_t);
uint32_t WINAPI XApuGetResultCount(void*);
HRESULT WINAPI XDspConnectWithMaximumStreamLimit(uint32_t, void**);
HRESULT WINAPI XDspActivate(void*);
HRESULT WINAPI XDspDeactivate(void*);
HRESULT WINAPI XDspSubmitCommand(void*, const void*, uint32_t);
HRESULT WINAPI XDspDisconnect(void*);
}

#define CHECK(expr) do { if (!(expr)) { std::fprintf(stderr, "line %d: %s\n", __LINE__, #expr); return 1; } } while (0)

int main() {
    void* object = nullptr;
    CHECK(AcpHalCreate(&object) == S_OK && object);
    auto* hal = static_cast<IUnknown*>(object);
    CHECK(hal->QueryInterface(IID_IUnknown, &object) == S_OK && object == hal);
    CHECK(hal->Release() == 1);
    void *va = nullptr, *second = nullptr;
    uint32_t base = 0, other = 0;
    CHECK(ApuAlloc(&va, &base, 64, 64) == E_OUTOFMEMORY && !va && !base);
    CHECK(ApuCreateHeap(64, 64) == S_OK);
    CHECK(ApuAlloc(&va, nullptr, 64, 64) == S_OK && va);
    CHECK(ApuFree(va) == S_OK);
    va = nullptr;
    CHECK(ApuAlloc(nullptr, &base, 64, 64) == S_OK && base);
    va = ApuMapApuAddress(base);
    CHECK(va && ApuFree(va) == S_OK);
    va = nullptr;
    base = 0;
    CHECK(ApuAlloc(&va, &base, 64, 256) == S_OK && va && base);
    CHECK(reinterpret_cast<uintptr_t>(va) % 256 == 0 && base % 256 == 0);
    CHECK(ApuAlloc(&second, &other, 64, 64) == S_OK && other != base);
    CHECK(ApuCreateHeap(0, 0) == HRESULT_FROM_WIN32(ERROR_BUSY));
    void* excess = nullptr;
    CHECK(ApuAlloc(&excess, nullptr, 1, 64) == E_OUTOFMEMORY);
    auto* tail = static_cast<unsigned char*>(va) + 63;
    CHECK(ApuMapApuAddress(base + 63) == tail && ApuMapVirtualAddress(tail) == base + 63);
    CHECK(ApuIsVirtualAddressValid(tail, 1) && !ApuIsVirtualAddressValid(tail, 2));
    CHECK(ApuFree(tail) == E_INVALIDARG);
    CHECK(ApuFree(va) == S_OK && !ApuMapApuAddress(base) && !ApuMapVirtualAddress(va));
    CHECK(ApuFree(va) == E_INVALIDARG && ApuMapApuAddress(other) == second);
    CHECK(ApuFree(second) == S_OK && hal->Release() == 0);

    void *apu = nullptr, *peer = nullptr, *dsp = nullptr;
    CHECK(XApuConnect(&apu) == S_OK && XApuConnect(&peer) == S_OK && peer != apu);
    unsigned result = 0;
    CHECK(XApuDequeueResult(apu, &result, sizeof(result)) == S_FALSE);
    for (unsigned cycle = 0; cycle < 3; ++cycle) {
        HRESULT status = S_OK;
        unsigned accepted = 0;
        std::thread producer([&] {
            for (; accepted < 1024; ++accepted) {
                unsigned command = cycle * 1024 + accepted;
                status = XApuEnqueueCommand(apu, &command, sizeof(command));
                if (status != S_OK) break;
            }
        });
        producer.join();
        CHECK(accepted && status == HRESULT_FROM_WIN32(ERROR_BUSY));
        CHECK(XApuGetResultCount(apu) == accepted && XApuGetResultCount(peer) == 0);
        CHECK(XApuDequeueResult(apu, &result, 1) == HRESULT_FROM_WIN32(ERROR_INSUFFICIENT_BUFFER));
        for (unsigned i = 0; i < accepted; ++i) {
            CHECK(XApuDequeueResult(apu, &result, sizeof(result)) == S_OK);
            CHECK(result == cycle * 1024 + i);
        }
        CHECK(XApuGetResultCount(apu) == 0);
    }
    // A stalled peer must fail this test rather than wait forever.
    constexpr unsigned messages = 4096;
    const auto deadline = GetTickCount64() + 10000;
    std::atomic<bool> failed{false};
    std::thread worker([&] {
        for (unsigned i = 0; i < messages && !failed;) {
            const auto hr = XApuEnqueueCommand(apu, &i, sizeof(i));
            if (hr == S_OK) ++i;
            else if (hr != HRESULT_FROM_WIN32(ERROR_BUSY) || GetTickCount64() >= deadline)
                failed = true;
            else std::this_thread::yield();
        }
    });
    for (unsigned i = 0; i < messages && !failed;) {
        const auto hr = XApuDequeueResult(apu, &result, sizeof(result));
        if (hr == S_OK && result == i) ++i;
        else if (hr != S_FALSE || GetTickCount64() >= deadline) failed = true;
        else std::this_thread::yield();
    }
    worker.join();
    CHECK(!failed && XApuGetResultCount(apu) == 0);
    CHECK(XApuDisconnect(apu) == S_OK && XApuDisconnect(peer) == S_OK);
    CHECK(XApuEnqueueCommand(apu, &result, sizeof(result)) == E_HANDLE);

    CHECK(XDspConnectWithMaximumStreamLimit(0, &dsp) == E_INVALIDARG && !dsp);
    CHECK(XDspConnectWithMaximumStreamLimit(2, &dsp) == S_OK && dsp);
    for (unsigned cycle = 0; cycle < 2; ++cycle) {
        unsigned queued = 0;
        HRESULT hr = S_OK;
        while (queued < 1024 && (hr = XDspSubmitCommand(dsp, &result, sizeof(result))) == S_OK)
            ++queued;
        CHECK(queued && hr == HRESULT_FROM_WIN32(ERROR_BUSY));
        CHECK(XDspActivate(dsp) == S_OK);
        for (unsigned i = 0; i <= queued; ++i)
            CHECK(XDspSubmitCommand(dsp, &result, sizeof(result)) == S_OK);
        CHECK(XDspDeactivate(dsp) == S_OK);
    }
    CHECK(XDspDisconnect(dsp) == S_OK && XDspActivate(dsp) == E_HANDLE);

    HMODULE dll = LoadLibraryW(L"GameInput.dll");
    CHECK(dll);
    auto proc = GetProcAddress(dll, MAKEINTRESOURCEA(1));
    CHECK(proc && proc == GetProcAddress(dll, "GameInputCreate"));
    using Factory = HRESULT (WINAPI*)(void**);
    auto create = reinterpret_cast<Factory>(reinterpret_cast<void*>(proc));
    CHECK(create(&object) == S_OK && object);
    auto* input = static_cast<IUnknown*>(object);
    CHECK(create(&object) == S_OK && object == input && input->Release() == 1);
    CHECK(input->QueryInterface(IID_IUnknown, &object) == S_OK && object == input);
    CHECK(input->Release() == 1);
    constexpr GUID kGameInputV0 = {0x11be2a7e, 0x4254, 0x445a,
                                   {0x9c, 0x09, 0xff, 0xc4, 0x0f, 0x00, 0x69, 0x18}};
    constexpr GUID kUnknownIid = {0x00000001, 0x0002, 0x0003,
                                  {0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b}};
    void* unknown = reinterpret_cast<void*>(uintptr_t{1});
    CHECK(input->QueryInterface(kUnknownIid, &unknown) == E_NOINTERFACE && !unknown);
    void* v0_obj = nullptr;
    CHECK(input->QueryInterface(kGameInputV0, &v0_obj) == S_OK && v0_obj == input);
    auto* v0 = static_cast<IUnknown*>(v0_obj);
    auto** table = *reinterpret_cast<void***>(v0);
    using Timestamp = uint64_t (WINAPI*)(IUnknown*);
    CHECK(reinterpret_cast<Timestamp>(table[3])(v0) > 0);
    using Reading = HRESULT (WINAPI*)(IUnknown*, uint32_t, void*, void**);
    void* reading = reinterpret_cast<void*>(uintptr_t{1});
    CHECK(reinterpret_cast<Reading>(table[4])(v0, 0, nullptr, &reading) == E_NOTIMPL && !reading);
    CHECK(v0->Release() == 1);
    CHECK(input->Release() == 0);
    FreeLibrary(dll);
    std::puts("PASS: audio mappings, APU and DSP queues, GameInput ordinal 1");
}

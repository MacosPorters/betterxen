#include <windows.h>
#include <unknwn.h>
#include <cstdint>

#include "bx_log.h"

namespace {
// GameInputCreate @1 exposes the legacy v0 COM layout.
constexpr GUID game_input_v0 = {0x11be2a7e, 0x4254, 0x445a,
                                {0x9c, 0x09, 0xff, 0xc4, 0x0f, 0x00, 0x69, 0x18}};
using Callback = void (WINAPI*)();

template<class T>
HRESULT unavailable(T* out) {
    if (!out) return E_POINTER;
    *out = {};
    return E_NOTIMPL;
}

class GameInput final : public IUnknown {
    LONG refs = 0;
public:
    HRESULT WINAPI QueryInterface(REFIID iid, void** out) override {
        if (!out) return E_POINTER;
        *out = nullptr;
        if (iid != IID_IUnknown && iid != game_input_v0) return E_NOINTERFACE;
        AddRef();
        *out = this;
        return S_OK;
    }
    ULONG WINAPI AddRef() override { return InterlockedIncrement(&refs); }
    ULONG WINAPI Release() override { return InterlockedDecrement(&refs); }

    virtual uint64_t WINAPI GetCurrentTimestamp() {
        return GetTickCount64() * 1000;
    }
    virtual HRESULT WINAPI GetCurrentReading(uint32_t, void*, void** out) {
        BX_VTABLE_STUB_LOG("IGameInput", 4, "GetCurrentReading");
        return unavailable(out);
    }
    virtual HRESULT WINAPI GetNextReading(void*, uint32_t, void*, void** out) {
        BX_VTABLE_STUB_LOG("IGameInput", 5, "GetNextReading");
        return unavailable(out);
    }
    virtual HRESULT WINAPI GetPreviousReading(void*, uint32_t, void*, void** out) {
        BX_VTABLE_STUB_LOG("IGameInput", 6, "GetPreviousReading");
        return unavailable(out);
    }
    virtual HRESULT WINAPI GetTemporalReading(uint64_t, void*, void** out) {
        BX_VTABLE_STUB_LOG("IGameInput", 7, "GetTemporalReading");
        return unavailable(out);
    }
    virtual HRESULT WINAPI RegisterReadingCallback(void*, uint32_t, float, void*, Callback, uint64_t* out) {
        BX_VTABLE_STUB_LOG("IGameInput", 8, "RegisterReadingCallback");
        return unavailable(out);
    }
    virtual HRESULT WINAPI RegisterDeviceCallback(void*, uint32_t, uint32_t, uint32_t, void*, Callback, uint64_t* out) {
        BX_VTABLE_STUB_LOG("IGameInput", 9, "RegisterDeviceCallback");
        return unavailable(out);
    }
    virtual HRESULT WINAPI RegisterSystemButtonCallback(void*, uint32_t, void*, Callback, uint64_t* out) {
        BX_VTABLE_STUB_LOG("IGameInput", 10, "RegisterSystemButtonCallback");
        return unavailable(out);
    }
    virtual HRESULT WINAPI RegisterKeyboardLayoutCallback(void*, void*, Callback, uint64_t* out) {
        BX_VTABLE_STUB_LOG("IGameInput", 11, "RegisterKeyboardLayoutCallback");
        return unavailable(out);
    }
    virtual void WINAPI StopCallback(uint64_t) {
        BX_VTABLE_STUB_LOG("IGameInput", 12, "StopCallback");
    }
    virtual bool WINAPI UnregisterCallback(uint64_t, uint64_t) {
        BX_VTABLE_STUB_LOG("IGameInput", 13, "UnregisterCallback");
        return false;
    }
    virtual HRESULT WINAPI CreateDispatcher(void** out) {
        BX_VTABLE_STUB_LOG("IGameInput", 14, "CreateDispatcher");
        return unavailable(out);
    }
    virtual HRESULT WINAPI CreateAggregateDevice(uint32_t, void** out) {
        BX_VTABLE_STUB_LOG("IGameInput", 15, "CreateAggregateDevice");
        return unavailable(out);
    }
    virtual HRESULT WINAPI FindDeviceFromId(const void*, void** out) {
        BX_VTABLE_STUB_LOG("IGameInput", 16, "FindDeviceFromId");
        return unavailable(out);
    }
    virtual HRESULT WINAPI FindDeviceFromObject(IUnknown*, void** out) {
        BX_VTABLE_STUB_LOG("IGameInput", 17, "FindDeviceFromObject");
        return unavailable(out);
    }
    virtual HRESULT WINAPI FindDeviceFromPlatformHandle(HANDLE, void** out) {
        BX_VTABLE_STUB_LOG("IGameInput", 18, "FindDeviceFromPlatformHandle");
        return unavailable(out);
    }
    virtual HRESULT WINAPI FindDeviceFromPlatformString(const wchar_t*, void** out) {
        BX_VTABLE_STUB_LOG("IGameInput", 19, "FindDeviceFromPlatformString");
        return unavailable(out);
    }
    virtual HRESULT WINAPI EnableOemDeviceSupport(uint16_t, uint16_t, uint8_t, uint8_t) {
        BX_VTABLE_STUB_LOG("IGameInput", 20, "EnableOemDeviceSupport");
        return E_NOTIMPL;
    }
    virtual void WINAPI SetFocusPolicy(uint32_t) {
        BX_VTABLE_STUB_LOG("IGameInput", 21, "SetFocusPolicy");
    }
};

GameInput singleton;
}

extern "C" HRESULT WINAPI GameInputCreate(void** out) {
    if (!out) return E_POINTER;
    singleton.AddRef();
    *out = &singleton;
    return S_OK;
}

#include <windows.h>
#include <unknwn.h>
#include <atomic>
#include <cstdint>
#include <unordered_map>

#include "bx_log.h"
#include "gameinput_allocation.h"

namespace {
// GameInputCreate @1 exposes the legacy v0 COM layout.
constexpr GUID game_input_v0 = {0x11be2a7e, 0x4254, 0x445a,
                                {0x9c, 0x09, 0xff, 0xc4, 0x0f, 0x00, 0x69, 0x18}};
using Callback = void (WINAPI*)();

// GameInputEnumerationKind values from Microsoft's public GDK GameInput (v0) reference:
// 0 = GameInputNoEnumeration, 1 = GameInputAsyncEnumeration, 2 = GameInputBlockingEnumeration.
constexpr uint32_t kMaxEnumerationKind = 2;

struct SrwExclusiveLock {
    explicit SrwExclusiveLock(SRWLOCK& lock) : lock_(lock) { AcquireSRWLockExclusive(&lock_); }
    ~SrwExclusiveLock() { ReleaseSRWLockExclusive(&lock_); }
    SrwExclusiveLock(const SrwExclusiveLock&) = delete;
    SrwExclusiveLock& operator=(const SrwExclusiveLock&) = delete;

private:
    SRWLOCK& lock_;
};

struct RegisteredDeviceCallback {
    void* device = nullptr;
    uint32_t input_kind = 0;
    uint32_t status_filter = 0;
    uint32_t enumeration_kind = 0;
    void* context = nullptr;
    Callback callback = nullptr;
    bool stopped = false;
};

template<class T>
HRESULT unavailable(T* out) {
    if (!out) return E_POINTER;
    *out = {};
    return E_NOTIMPL;
}

class GameInput final : public IUnknown {
    LONG refs = 0;
    SRWLOCK callback_lock_ = SRWLOCK_INIT;
    uint64_t next_token_ = 1;
    std::unordered_map<uint64_t, RegisteredDeviceCallback, std::hash<uint64_t>,
                       std::equal_to<uint64_t>,
                       CallbackAllocator<std::pair<const uint64_t, RegisteredDeviceCallback>>> callbacks_;
    std::atomic<uint32_t> focus_policy_{0};

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
    virtual HRESULT WINAPI RegisterDeviceCallback(
        void* device,
        uint32_t input_kind,
        uint32_t status_filter,
        uint32_t enumeration_kind,
        void* context,
        Callback callback,
        uint64_t* out_token
    ) {
        if (out_token) *out_token = 0;
        if (!callback || input_kind == 0 || status_filter == 0 || enumeration_kind > kMaxEnumerationKind) {
            return E_INVALIDARG;
        }
        SrwExclusiveLock guard(callback_lock_);
        const uint64_t token = next_token_;
        try {
            const auto inserted = callbacks_.emplace(token, RegisteredDeviceCallback{
                device,
                input_kind,
                status_filter,
                enumeration_kind,
                context,
                callback,
                false,
            });
            if (!inserted.second) return E_FAIL;
        } catch (const std::bad_alloc&) {
            return E_OUTOFMEMORY;
        }
        ++next_token_;
        if (out_token) *out_token = token;
        return S_OK;
    }
    virtual HRESULT WINAPI RegisterSystemButtonCallback(void*, uint32_t, void*, Callback, uint64_t* out) {
        BX_VTABLE_STUB_LOG("IGameInput", 10, "RegisterSystemButtonCallback");
        return unavailable(out);
    }
    virtual HRESULT WINAPI RegisterKeyboardLayoutCallback(void*, void*, Callback, uint64_t* out) {
        BX_VTABLE_STUB_LOG("IGameInput", 11, "RegisterKeyboardLayoutCallback");
        return unavailable(out);
    }
    virtual void WINAPI StopCallback(uint64_t token) {
        if (token == 0) return;
        SrwExclusiveLock guard(callback_lock_);
        auto it = callbacks_.find(token);
        if (it != callbacks_.end()) {
            it->second.stopped = true;
        }
    }
    virtual bool WINAPI UnregisterCallback(uint64_t token, uint64_t /*timeout_us*/) {
        if (token == 0) return false;
        SrwExclusiveLock guard(callback_lock_);
        return callbacks_.erase(token) > 0;
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
    virtual void WINAPI SetFocusPolicy(uint32_t policy) {
        focus_policy_.store(policy, std::memory_order_relaxed);
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

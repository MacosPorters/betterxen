#include <windows.h>
#include <unknwn.h>
#include <atomic>
#include <cstdint>
#include <cstdio>
#include <new>
#include <unordered_map>

#include "bx_log.h"

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

class GameInputDispatcher;

class GameInput final : public IUnknown {
    LONG refs = 0;
    SRWLOCK callback_lock_ = SRWLOCK_INIT;
    uint64_t next_token_ = 1;
    std::unordered_map<uint64_t, RegisteredDeviceCallback> callbacks_;
    std::atomic<uint32_t> focus_policy_{0};
    // Live IGameInputDispatcher reference count. >0 means GameInput is in
    // "manual dispatch" mode; 0 means "automatic dispatch" mode.
    std::atomic<uint32_t> dispatcher_refs_{0};

public:
    // Logs the manual/automatic dispatch mode transition. Purely atomic: it
    // never takes callback_lock_, so it cannot introduce lock ordering hazards.
    void note_dispatch_mode(const char* mode, uint32_t count) {
        char detail[96];
        std::snprintf(detail, sizeof(detail), "mode=%s live_dispatchers=%u", mode,
                      static_cast<unsigned>(count));
        bx_log_emit(BX_LOG_INFO, "gameinput", "dispatch_mode", detail);
    }

    void on_dispatcher_created() {
        const uint32_t count = dispatcher_refs_.fetch_add(1, std::memory_order_acq_rel) + 1;
        if (count == 1) note_dispatch_mode("manual", count);
    }

    void on_dispatcher_released() {
        const uint32_t count = dispatcher_refs_.fetch_sub(1, std::memory_order_acq_rel) - 1;
        if (count == 0) note_dispatch_mode("automatic", count);
    }

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
        const uint64_t token = next_token_++;
        callbacks_.emplace(token, RegisteredDeviceCallback{
            device,
            input_kind,
            status_filter,
            enumeration_kind,
            context,
            callback,
            false,
        });
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
    // Slot 14. Defined out of line below because it allocates
    // GameInputDispatcher, which in turn calls back into this class.
    virtual HRESULT WINAPI CreateDispatcher(void** out);
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

// IGameInputDispatcher (GameInput v0): IUnknown + Dispatch + OpenWaitHandle.
// Holding a reference puts GameInput in manual dispatch mode; releasing the
// last one returns it to automatic dispatch mode.
class GameInputDispatcher final : public IUnknown {
    LONG refs = 1;
    GameInput* owner_ = nullptr;

public:
    explicit GameInputDispatcher(GameInput* owner) : owner_(owner) {}

    HRESULT WINAPI QueryInterface(REFIID iid, void** out) override {
        if (!out) return E_POINTER;
        *out = nullptr;
        if (iid == IID_IUnknown) {
            AddRef();
            *out = this;
            return S_OK;
        }
        // The GameInput v0 IGameInputDispatcher IID is not published in the
        // public Microsoft Learn / GDK documentation, so it is NOT guessed
        // here. Only IID_IUnknown is served; every other queried id is logged
        // (never silently ignored) and answered with E_NOINTERFACE.
        char guid[40];
        std::snprintf(guid, sizeof(guid), "%08lx-%04x-%04x-%02x%02x-%02x%02x%02x%02x%02x%02x",
                      static_cast<unsigned long>(iid.Data1), iid.Data2, iid.Data3,
                      iid.Data4[0], iid.Data4[1], iid.Data4[2], iid.Data4[3],
                      iid.Data4[4], iid.Data4[5], iid.Data4[6], iid.Data4[7]);
        char detail[160];
        std::snprintf(detail, sizeof(detail),
                      "IGameInputDispatcher iid=%s unsupported (v0 dispatcher IID not publicly documented)",
                      guid);
        bx_log_emit(BX_LOG_WARN, "gameinput", "qi_unsupported", detail);
        return E_NOINTERFACE;
    }
    ULONG WINAPI AddRef() override { return InterlockedIncrement(&refs); }
    ULONG WINAPI Release() override {
        const LONG count = InterlockedDecrement(&refs);
        if (count == 0) {
            GameInput* owner = owner_;
            owner_ = nullptr;
            if (owner) owner->on_dispatcher_released();
            delete this;
        }
        return static_cast<ULONG>(count);
    }

    // Slot 3: bool Dispatch(uint64_t quotaInMicroseconds).
    // This shim has no real background input work, so the dispatch queue is
    // always empty. The documented "flush at least one item even at quota 0"
    // shape is honoured trivially: there is nothing to flush, so it returns
    // false (no work items remain) for every quota, including 0. No real work
    // is scheduled, and it never busy-waits or sleeps.
    virtual bool WINAPI Dispatch(uint64_t /*quota_us*/) {
        return false;
    }

    // Slot 4: HRESULT OpenWaitHandle(HANDLE* waitHandle).
    // Returns a fresh manual-reset event on every call. The dispatcher owns
    // nothing after returning it: the caller closes it via CloseHandle.
    virtual HRESULT WINAPI OpenWaitHandle(HANDLE* out) {
        if (!out) return E_POINTER;
        *out = nullptr;
        HANDLE handle = CreateEventW(nullptr, TRUE, FALSE, nullptr);
        if (!handle) {
            return HRESULT_FROM_WIN32(GetLastError());
        }
        *out = handle;
        return S_OK;
    }
};

HRESULT WINAPI GameInput::CreateDispatcher(void** out) {
    if (!out) return E_POINTER;
    *out = nullptr;
    GameInputDispatcher* dispatcher = new (std::nothrow) GameInputDispatcher(this);
    if (!dispatcher) return E_OUTOFMEMORY;
    *out = dispatcher;
    // The GameInput singleton's own refcount is intentionally unchanged:
    // the dispatcher only moves the manual-dispatch mode counter.
    on_dispatcher_created();
    return S_OK;
}

GameInput singleton;
}

extern "C" HRESULT WINAPI GameInputCreate(void** out) {
    if (!out) return E_POINTER;
    singleton.AddRef();
    *out = &singleton;
    return S_OK;
}

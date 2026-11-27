#include <windows.h>
#include <atomic>
#include <cstddef>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>

namespace {

struct Async {
    void* queue;
    void* context;
    void (CALLBACK* callback)(Async*);
    unsigned char reserved[32];
};

struct GameLicense {
    char sku[18];
    bool active, trial_owner, disc, trial;
    uint32_t trial_remaining;
    char trial_id[64];
    int64_t expiration;
};

struct RuntimeOptions {
    uint32_t source;
    const char* config;
};

struct SpaceInfo {
    uint64_t available, free, used, total;
};

static_assert(sizeof(GameLicense) == 104);
static_assert(offsetof(GameLicense, active) == 18);

extern "C" {
HRESULT WINAPI XGameRuntimeInitialize();
HRESULT WINAPI XGameRuntimeInitializeWithOptions(const RuntimeOptions*);
void WINAPI XGameRuntimeUninitialize();
HRESULT WINAPI XTaskQueueCreate(uint32_t, uint32_t, void**);
void WINAPI XTaskQueueCloseHandle(void*);
bool WINAPI XTaskQueueDispatch(void*, uint32_t, uint32_t);
HRESULT WINAPI XTaskQueueSubmitCallback(void*, uint32_t, void*, void (CALLBACK*)(void*, bool));
bool WINAPI XTaskQueueGetCurrentProcessTaskQueue(void**);
HRESULT WINAPI XUserAddAsync(uint32_t, Async*);
HRESULT WINAPI XUserAddResult(Async*, void**);
void WINAPI XUserCloseHandle(void*);
HRESULT WINAPI XUserGetId(void*, uint64_t*);
HRESULT WINAPI XUserGetGamertag(void*, uint32_t, size_t, char*, size_t*);
HRESULT WINAPI XStoreCreateContext(void*, void**);
void WINAPI XStoreCloseContextHandle(void*);
HRESULT WINAPI XStoreQueryGameLicenseAsync(void*, Async*);
HRESULT WINAPI XStoreQueryGameLicenseResult(Async*, GameLicense*);
HRESULT WINAPI XAsyncGetStatus(Async*, bool);
HRESULT WINAPI XAsyncGetResultSize(Async*, size_t*);
void WINAPI XAsyncCancel(Async*);
uint32_t WINAPI XSystemGetDeviceType();
HRESULT WINAPI XSystemGetConsoleId(size_t, char*, size_t*);
HRESULT WINAPI XPersistentLocalStorageGetPathSize(size_t*);
HRESULT WINAPI XPersistentLocalStorageGetPath(size_t, char*, size_t*);
HRESULT WINAPI XPersistentLocalStorageGetSpaceInfo(SpaceInfo*);
}

#define CHECK(expr) do { \
    if (!(expr)) { std::fprintf(stderr, "line %d: %s\n", __LINE__, #expr); std::exit(1); } \
} while (false)

constexpr uint32_t manual = 0, pool = 1, serialized = 2, immediate = 3;
constexpr uint32_t work = 0, completion = 1;
constexpr DWORD timeout_ms = 5000;

struct Completion {
    unsigned count = 0;
    DWORD thread = 0;
    HRESULT status = E_PENDING;
};

void CALLBACK completed(Async* async) {
    auto* result = static_cast<Completion*>(async->context);
    ++result->count;
    result->thread = GetCurrentThreadId();
    result->status = XAsyncGetStatus(async, true);
}

void CALLBACK record_thread(void* context, bool canceled) {
    CHECK(!canceled);
    *static_cast<DWORD*>(context) = GetCurrentThreadId();
}

void CALLBACK record_canceled(void* context, bool canceled) {
    CHECK(canceled);
    ++*static_cast<unsigned*>(context);
}

struct SerialCallbacks {
    std::atomic<unsigned> count{0};
    HANDLE done;
};

void CALLBACK serial_callback(void* context, bool canceled) {
    CHECK(!canceled);
    auto& state = *static_cast<SerialCallbacks*>(context);
    const unsigned order = state.count.load();
    Sleep(10);
    CHECK(state.count.fetch_add(1) == order);
    if (order == 7) SetEvent(state.done);
}

void check_storage() {
    size_t required = 0, used = 0;
    char path[MAX_PATH * 4]{};
    CHECK(XPersistentLocalStorageGetPathSize(&required) == S_OK);
    CHECK(required > 1 && required <= sizeof(path));
    CHECK(FAILED(XPersistentLocalStorageGetPath(required - 1, path, nullptr)));
    CHECK(XPersistentLocalStorageGetPath(sizeof(path), path, &used) == S_OK);
    CHECK(used == required && std::strlen(path) + 1 == required);
    CHECK(std::strncmp(path, "C:\\bx_pls\\", 10) == 0);
    const auto attributes = GetFileAttributesA(path);
    CHECK(attributes != INVALID_FILE_ATTRIBUTES && (attributes & FILE_ATTRIBUTE_DIRECTORY));

    char file[MAX_PATH * 4 + 16];
    std::snprintf(file, sizeof(file), "%s\\write-check", path);
    auto handle = CreateFileA(file, GENERIC_WRITE, 0, nullptr, CREATE_ALWAYS, FILE_ATTRIBUTE_NORMAL, nullptr);
    CHECK(handle != INVALID_HANDLE_VALUE);
    DWORD bytes;
    CHECK(WriteFile(handle, "BX", 2, &bytes, nullptr) && bytes == 2);
    CHECK(CloseHandle(handle) && DeleteFileA(file));

    SpaceInfo space{};
    CHECK(XPersistentLocalStorageGetSpaceInfo(&space) == S_OK);
    CHECK(space.total > 0 && space.available <= space.free && space.free <= space.total);
    std::printf("storage path: %s\n", path);
}

} // namespace

bool valid_utf8(const unsigned char* s, size_t n) {
    size_t i = 0;
    while (i < n) {
        const unsigned char c = s[i];
        size_t len = 0;
        if (c < 0x80) len = 1;
        else if (c >= 0xC2 && c <= 0xDF) len = 2;
        else if (c == 0xE0 || (c >= 0xE1 && c <= 0xEC) || c == 0xED || (c >= 0xEE && c <= 0xEF)) len = 3;
        else if (c == 0xF0 || (c >= 0xF1 && c <= 0xF3) || c == 0xF4) len = 4;
        else return false;
        if (i + len > n) return false;
        for (size_t k = 1; k < len; ++k) {
            if ((s[i + k] & 0xC0) != 0x80) return false;
        }
        if (c == 0xE0 && s[i + 1] < 0xA0) return false;
        if (c == 0xED && s[i + 1] > 0x9F) return false;
        if (c == 0xF0 && s[i + 1] < 0x90) return false;
        if (c == 0xF4 && s[i + 1] > 0x8F) return false;
        i += len;
    }
    return true;
}

#ifndef BX_TEST_UNENTITLED
extern "C" __declspec(dllexport) void BetterXenSyntheticStoreFixture() {}
#endif

// A non-zero delay must not run early, and must run once it is due. The delayed
// entry point is IXGameRuntimeCore slot 19, reached through QueryApiImpl; it is
// not an export, so it is called through the COM vtable exactly as a title
// would. Margins are generous so a slow host does not make this flaky: assert
// "not run" a few milliseconds in, then require it within a wide window well
// past the deadline.
void check_delayed_callback(HRESULT(WINAPI* query_api)(const GUID*, const GUID*, void**)) {
    constexpr GUID kCore = {0x073B7DCB, 0x1FCF, 0x4030, {0x94, 0xBE, 0xE3, 0xC9, 0xEB, 0x62, 0x34, 0x28}};
    void* core = nullptr;
    CHECK(SUCCEEDED(query_api(&kCore, &kCore, &core)) && core);
    auto** vtbl = *static_cast<void***>(core);
    using SubmitDelayed = HRESULT(WINAPI*)(void*, void*, uint32_t, uint32_t, void*, void (CALLBACK*)(void*, bool));
    auto submit_delayed = reinterpret_cast<SubmitDelayed>(vtbl[19]);

    void* queue = nullptr;
    CHECK(XTaskQueueCreate(manual, manual, &queue) == S_OK && queue);

    static std::atomic<int> runs{0};
    auto cb = +[](void* ctx, bool) { static_cast<std::atomic<int>*>(ctx)->fetch_add(1); };

    // delay_ms == 0 keeps the old immediate behaviour on the next dispatch.
    runs.store(0);
    CHECK(SUCCEEDED(submit_delayed(core, queue, work, 0, &runs, cb)) == true);
    CHECK(XTaskQueueDispatch(queue, work, 0) && runs.load() == 1);

    // A 100 ms delay must not run on an immediate pass, and must run later.
    runs.store(0);
    CHECK(SUCCEEDED(submit_delayed(core, queue, work, 100, &runs, cb)) == true);
    CHECK(!XTaskQueueDispatch(queue, work, 0));
    CHECK(runs.load() == 0);
    Sleep(10);
    CHECK(!XTaskQueueDispatch(queue, work, 0));
    CHECK(runs.load() == 0);
    Sleep(250);
    CHECK(XTaskQueueDispatch(queue, work, 0) && runs.load() == 1);

    XTaskQueueCloseHandle(queue);
}

int main() {
    char profile[16]{};
    CHECK(GetEnvironmentVariableA("BX_PROFILE", profile, sizeof(profile)) > 0);
    const bool anaconda = std::strcmp(profile, "anaconda") == 0;
    CHECK(anaconda || std::strcmp(profile, "lockhart") == 0);
    CHECK(XSystemGetDeviceType() == (anaconda ? 7u : 6u));

#ifdef BX_TEST_UNENTITLED
    constexpr bool entitled = false;
#else
    constexpr bool entitled = true;
#endif

    Async uninitialized{};
    CHECK(FAILED(XUserAddAsync(1, &uninitialized)));
    const RuntimeOptions options{};
    CHECK(XGameRuntimeInitializeWithOptions(&options) == S_OK);
    CHECK(XGameRuntimeInitialize() == S_OK);
    XGameRuntimeUninitialize();

    void* process_queue = nullptr;
    CHECK(XTaskQueueGetCurrentProcessTaskQueue(&process_queue) && process_queue);
    XTaskQueueCloseHandle(process_queue);

    void* queue = nullptr;
    CHECK(XTaskQueueCreate(manual, manual, &queue) == S_OK && queue);
    const auto main_thread = GetCurrentThreadId();
    DWORD task_thread = 0;
    CHECK(XTaskQueueSubmitCallback(queue, work, &task_thread, record_thread) == S_OK);
    CHECK(task_thread == 0 && XTaskQueueDispatch(queue, work, 0));
    CHECK(task_thread == main_thread && !XTaskQueueDispatch(queue, work, 0));

    Completion result{};
    Async async{queue, &result, completed, {}};
    CHECK(XUserAddAsync(1, &async) == S_OK);
    CHECK(XAsyncGetStatus(&async, false) == E_PENDING);
    CHECK(FAILED(XUserAddAsync(1, &async)));
    size_t size = 0;
    CHECK(XAsyncGetResultSize(&async, &size) == E_PENDING);
    CHECK(XTaskQueueDispatch(queue, work, 0) && result.count == 0);
    CHECK(XAsyncGetResultSize(&async, &size) == S_OK && size == sizeof(void*));
    CHECK(XTaskQueueDispatch(queue, completion, 0));
    CHECK(result.count == 1 && result.status == S_OK && result.thread == main_thread);

    void* user = nullptr;
    CHECK(XUserAddResult(&async, &user) == S_OK && user);
    void* duplicate = nullptr;
    CHECK(FAILED(XUserAddResult(&async, &duplicate)) && !duplicate);
    uint64_t id = 0;
    char tag[16]{};
    CHECK(XUserGetId(user, &id) == S_OK && id == 0x0009000000000001ULL);
    CHECK(FAILED(XUserGetGamertag(user, 0, 1, tag, nullptr)));
    CHECK(XUserGetGamertag(user, 0, sizeof(tag), tag, &size) == S_OK);
    CHECK(std::strcmp(tag, "BetterXenLocal") == 0 && size == sizeof("BetterXenLocal"));

    void* store = nullptr;
    CHECK(XStoreCreateContext(user, &store) == S_OK && store);
    XUserCloseHandle(user);
    CHECK(FAILED(XUserGetId(user, &id)));
    CHECK(XStoreQueryGameLicenseAsync(store, &async) == S_OK);
    XStoreCloseContextHandle(store);
    CHECK(FAILED(XStoreQueryGameLicenseAsync(store, &uninitialized)));
    CHECK(XTaskQueueDispatch(queue, work, 0));
    CHECK(XTaskQueueDispatch(queue, completion, 0) && result.count == 2);
    CHECK(XAsyncGetResultSize(&async, &size) == S_OK && size == sizeof(GameLicense));
    struct {
        GameLicense license;
        uint64_t canary = 0x123456789abcdef0ULL;
    } payload;
    CHECK(XStoreQueryGameLicenseResult(&async, &payload.license) == S_OK);
    CHECK(payload.canary == 0x123456789abcdef0ULL);
    CHECK(payload.license.active == entitled && !payload.license.trial && !payload.license.disc);
    if (entitled) CHECK(std::strcmp(payload.license.sku, "BXTEST000001/0001") == 0);

    CHECK(XUserAddAsync(0, &async) == S_OK);
    XAsyncCancel(&async);
    CHECK(XTaskQueueDispatch(queue, work, 0));
    CHECK(XTaskQueueDispatch(queue, completion, 0) && result.count == 3);
    CHECK(result.status == E_ABORT && XAsyncGetStatus(&async, true) == E_ABORT);
    CHECK(XUserAddResult(&async, &user) == E_ABORT && !user);
    CHECK(!XTaskQueueDispatch(queue, completion, 0));
    CHECK(XUserAddAsync(0, &async) == S_OK);
    unsigned dropped = 0;
    CHECK(XTaskQueueSubmitCallback(queue, work, &dropped, record_canceled) == S_OK);
    CHECK(XTaskQueueSubmitCallback(queue, completion, &dropped, record_canceled) == S_OK);
    XTaskQueueCloseHandle(queue);
    CHECK(dropped == 2 && result.count == 4 && result.status == E_ABORT);
    CHECK(XAsyncGetStatus(&async, true) == E_ABORT);
    CHECK(!XTaskQueueDispatch(queue, completion, 0));
    CHECK(XUserAddAsync(0, &async) == E_HANDLE && result.count == 4);

    CHECK(XTaskQueueCreate(manual, manual, &queue) == S_OK);
    async = {queue, &result, completed, {}};
    CHECK(XUserAddAsync(0, &async) == S_OK);
    CHECK(XTaskQueueDispatch(queue, work, 0) && result.count == 4);
    XTaskQueueCloseHandle(queue);
    CHECK(result.count == 5 && result.status == S_OK);
    CHECK(XUserAddResult(&async, &user) == S_OK);
    XUserCloseHandle(user);

    CHECK(XTaskQueueCreate(pool, manual, &queue) == S_OK);
    async = {queue, &result, completed, {}};
    CHECK(XUserAddAsync(0, &async) == S_OK);
    CHECK(XTaskQueueDispatch(queue, completion, timeout_ms) && result.count == 6);
    CHECK(XUserAddResult(&async, &user) == S_OK);
    XUserCloseHandle(user);
    XTaskQueueCloseHandle(queue);

    async = {nullptr, &result, completed, {}};
    CHECK(XUserAddAsync(0, &async) == S_OK);
    CHECK(XAsyncGetStatus(&async, true) == S_OK);
    CHECK(result.count == 7 && result.thread != main_thread);
    CHECK(XUserAddResult(&async, &user) == S_OK);
    XUserCloseHandle(user);

    CHECK(XTaskQueueCreate(serialized, immediate, &queue) == S_OK);
    task_thread = 0;
    CHECK(XTaskQueueSubmitCallback(queue, completion, &task_thread, record_thread) == S_OK);
    CHECK(task_thread == main_thread);
    CHECK(!XTaskQueueDispatch(queue, completion, INFINITE));
    SerialCallbacks serial{0, CreateEventW(nullptr, TRUE, FALSE, nullptr)};
    CHECK(serial.done);
    for (unsigned i = 0; i < 8; ++i) {
        CHECK(XTaskQueueSubmitCallback(queue, work, &serial, serial_callback) == S_OK);
    }
    CHECK(WaitForSingleObject(serial.done, timeout_ms) == WAIT_OBJECT_0 && serial.count == 8);
    CloseHandle(serial.done);
    XTaskQueueCloseHandle(queue);

    char console_id[39];
    CHECK(XSystemGetConsoleId(sizeof(console_id), console_id, &size) == S_OK);
    CHECK(size == sizeof(console_id) && console_id[38] == '\0');
    CHECK(std::strcmp(console_id, "00000000.00000000.00000000.00000000.00") == 0);
    CHECK(FAILED(XSystemGetConsoleId(sizeof(console_id) - 1, console_id, nullptr)));
    check_storage();
    XGameRuntimeUninitialize();
    CHECK(FAILED(XUserAddAsync(1, &uninitialized)));

    // Dynamic loader entry points. Synthetic family GUIDs match xgameruntime.cpp:
    // Data2 = 0x5853, Data3 = 0x4000, Data4 = 80 00 00 00 00 00 00 01.
    HMODULE module = GetModuleHandleA("xgameruntime.dll");
    CHECK(module);
    const char* names[] = {"InitializeApiImpl", "InitializeApiImplEx", "InitializeApiImplEx2",
                           "UninitializeApiImpl", "QueryApiImpl", "XErrorReport"};
    FARPROC procs[6]{};
    for (int i = 0; i < 6; ++i) {
        procs[i] = GetProcAddress(module, names[i]);
        CHECK(procs[i]);
    }
    const auto load = [](FARPROC proc, auto& fn) { std::memcpy(&fn, &proc, sizeof(fn)); };
    using InitFn = HRESULT(WINAPI*)(uint32_t, uint32_t);
    using InitExFn = HRESULT(WINAPI*)(uint32_t, uint32_t, char);
    using InitEx2Fn = HRESULT(WINAPI*)(uint32_t, uint32_t, char, const void*);
    using UninitFn = HRESULT(WINAPI*)();
    using QueryFn = HRESULT(WINAPI*)(const GUID*, const GUID*, void**);
    using ErrorFn = void(WINAPI*)(HRESULT, const char*);
    InitFn init_api{};
    InitExFn init_ex{};
    InitEx2Fn init_ex2{};
    UninitFn uninit_api{};
    QueryFn query_api{};
    ErrorFn error_report{};
    load(procs[0], init_api);
    load(procs[1], init_ex);
    load(procs[2], init_ex2);
    load(procs[3], uninit_api);
    load(procs[4], query_api);
    load(procs[5], error_report);

    struct ApiInitOptions {
        uint32_t unknown;
        uint8_t is_inline;
        uint8_t pad[3];
        const char* game_config;
    };
    static_assert(offsetof(ApiInitOptions, game_config) == 8);
    const auto family_guid = [](uint32_t data1) {
        GUID guid{};
        guid.Data1 = data1;
        guid.Data2 = 0x5853;
        guid.Data3 = 0x4000;
        guid.Data4[0] = 0x80;
        guid.Data4[7] = 0x01;
        return guid;
    };
    const GUID xsystem = family_guid(0xB7581401);
    const GUID xsystem_iid = family_guid(0xB7581402);
    const GUID wrong_iid = family_guid(0xB75814FF);
    GUID bogus{};
    bogus.Data1 = 0xDEADBEEF;
    bogus.Data4[0] = 0x11;
    constexpr HRESULT not_initialized = static_cast<HRESULT>(0x89240100u);
    auto* sentinel = reinterpret_cast<void*>(static_cast<uintptr_t>(1));
    void* api = sentinel;

    CHECK(query_api(&xsystem, &xsystem_iid, nullptr) == E_POINTER);
    CHECK(api == sentinel);
    api = sentinel;
    CHECK(query_api(nullptr, &xsystem_iid, &api) == E_INVALIDARG && !api);
    api = sentinel;
    CHECK(query_api(&xsystem, nullptr, &api) == E_INVALIDARG && !api);
    api = sentinel;
    CHECK(query_api(&xsystem, &xsystem_iid, &api) == not_initialized && !api);
    CHECK(init_api(0, 1) == E_INVALIDARG);
    CHECK(init_ex(1, 0, 2) == E_INVALIDARG);
    const ApiInitOptions broken{1u, 1u, {0, 0, 0}, nullptr};
    CHECK(init_ex2(1, 1, 2, &broken) == E_INVALIDARG);
    const ApiInitOptions file_opt{1u, 0u, {0, 0, 0}, "C:\\bx_synthetic\\MicrosoftGame.config"};
    CHECK(init_ex2(1, 1, 2, &file_opt) == E_NOTIMPL);
    void* probe = nullptr;
    CHECK(XStoreCreateContext(nullptr, &probe) == not_initialized);

    CHECK(init_api(1, 1) == S_OK);
    CHECK(XGameRuntimeInitialize() == S_OK);
    CHECK(uninit_api() == S_OK);
    CHECK(XStoreCreateContext(nullptr, &probe) == S_OK && probe);
    XStoreCloseContextHandle(probe);
    XGameRuntimeUninitialize();
    CHECK(XStoreCreateContext(nullptr, &probe) == not_initialized);
    CHECK(uninit_api() == not_initialized);

    CHECK(init_ex(1, 1, 2) == S_OK);
    CHECK(init_ex2(1, 1, 6, nullptr) == S_OK);
    CHECK(uninit_api() == S_OK);
    CHECK(XStoreCreateContext(nullptr, &probe) == S_OK && probe);
    XStoreCloseContextHandle(probe);
    XGameRuntimeUninitialize();
    CHECK(XStoreCreateContext(nullptr, &probe) == not_initialized);

    const ApiInitOptions default_opts{1u, 0u, {0, 0, 0}, nullptr};
    CHECK(init_ex2(1, 1, 6, &default_opts) == S_OK);
    CHECK(XStoreCreateContext(nullptr, &probe) == S_OK && probe);
    XStoreCloseContextHandle(probe);
    CHECK(uninit_api() == S_OK);
    CHECK(XStoreCreateContext(nullptr, &probe) == not_initialized);

    CHECK(init_api(1, 1) == S_OK);
    const uint32_t families[][2] = {
        {0xB7581001, 0xB7581002}, {0xB7581101, 0xB7581102}, {0xB7581201, 0xB7581202},
        {0xB7581301, 0xB7581302}, {0xB7581401, 0xB7581402}, {0xB7581501, 0xB7581502},
    };
    struct SystemHead {
        uint32_t(WINAPI* get_device_type)();
    };
    for (const auto& ids : families) {
        const GUID clsid = family_guid(ids[0]);
        const GUID iid = family_guid(ids[1]);
        void* table = nullptr;
        CHECK(query_api(&clsid, &iid, &table) == S_OK && table);
        if (ids[0] == 0xB7581401) {
            CHECK(static_cast<SystemHead*>(table)->get_device_type() == XSystemGetDeviceType());
        }
    }
    struct LogSpan {
        bool from_file = false;
        char path[512]{};
        long mark = 0;
        HANDLE pipe_read = nullptr;
        HANDLE pipe_write = nullptr;
        HANDLE saved_err = nullptr;
        char text[64 * 1024]{};

        bool start() {
            const auto n = GetEnvironmentVariableA("BX_LOG_FILE", path, sizeof(path));
            if (n > 0 && n < sizeof(path)) {
                from_file = true;
                if (auto* file = std::fopen(path, "rb")) {
                    std::fseek(file, 0, SEEK_END);
                    mark = std::ftell(file);
                    std::fclose(file);
                }
                return true;
            }
            if (!CreatePipe(&pipe_read, &pipe_write, nullptr, sizeof(text))) return false;
            saved_err = GetStdHandle(STD_ERROR_HANDLE);
            return SetStdHandle(STD_ERROR_HANDLE, pipe_write) != 0;
        }

        void finish() {
            size_t got = 0;
            if (from_file) {
                if (auto* file = std::fopen(path, "rb")) {
                    std::fseek(file, mark, SEEK_SET);
                    got = std::fread(text, 1, sizeof(text) - 1, file);
                    std::fclose(file);
                }
            } else if (pipe_read) {
                SetStdHandle(STD_ERROR_HANDLE, saved_err);
                CloseHandle(pipe_write);
                DWORD chunk = 0;
                while (got + 1 < sizeof(text) &&
                       ReadFile(pipe_read, text + got, static_cast<DWORD>(sizeof(text) - 1 - got), &chunk, nullptr) &&
                       chunk) {
                    got += chunk;
                }
                CloseHandle(pipe_read);
            }
            text[got] = '\0';
        }

        bool has(const char* needle) const { return std::strstr(text, needle) != nullptr; }
    } span;
    CHECK(span.start());
    api = sentinel;
    const auto mismatched = query_api(&xsystem, &wrong_iid, &api);
    CHECK(mismatched == E_NOINTERFACE && !api);
    api = sentinel;
    const auto unsupported = query_api(&bogus, &bogus, &api);
    error_report(E_FAIL, "synthetic-gdk-error");
    error_report(E_FAIL, nullptr);
    error_report(E_FAIL, "err \"quoted\" \\path\nline2");
    error_report(E_FAIL, "caf\xE9");
    char long_msg[701];
    long_msg[0] = 'x';
    size_t long_n = 1;
    while (long_n + 2 <= 700) {
        long_msg[long_n++] = '\xC3';
        long_msg[long_n++] = '\xA9';
    }
    long_msg[long_n] = '\0';
    error_report(E_FAIL, long_msg);
    span.finish();
    CHECK(unsupported == E_NOINTERFACE && !api);
    CHECK(span.has("\"sub\":\"xgameruntime\",\"event\":\"QueryApiImpl\"") && span.has("unsupported clsid=DEADBEEF"));
    CHECK(span.has("\"event\":\"unimpl_api\"") &&
          span.has("xgameruntime.dll!QueryApiImpl(B7581401-5853-4000-8000-000000000001/"
                   "B75814FF-5853-4000-8000-000000000001)") &&
          span.has("xgameruntime.dll!QueryApiImpl(DEADBEEF-0000-0000-1100-000000000000/"
                   "DEADBEEF-0000-0000-1100-000000000000)"));
    CHECK(span.has("\"sub\":\"xgameruntime\",\"event\":\"XErrorReport\"") &&
          span.has("hr=0x80004005 msg=synthetic-gdk-error"));
    CHECK(span.has("hr=0x80004005 msg=(null)"));
    CHECK(span.has("msg=err \\\"quoted\\\" \\\\path\\u000Aline2"));
    CHECK(span.has("caf\\u00E9"));
    CHECK(std::memchr(span.text, static_cast<unsigned char>(0xE9), std::strlen(span.text)) == nullptr);
    CHECK(valid_utf8(reinterpret_cast<const unsigned char*>(span.text), std::strlen(span.text)));
    size_t eacute = 0;
    const auto* bytes = reinterpret_cast<const unsigned char*>(span.text);
    const size_t nbytes = std::strlen(span.text);
    for (size_t i = 0; i + 1 < nbytes; ++i) {
        if (bytes[i] == 0xC3 && bytes[i + 1] == 0xA9) ++eacute;
    }
    CHECK(eacute > 0 && eacute < (long_n - 1) / 2);

    // Public Microsoft.GDK.Windows 2610.0.9661 COM IUnknown QueryApiImpl dispatch checks (#456).
    {
        const GUID core_guid = {0x073B7DCB, 0x1FCF, 0x4030, {0x94, 0xBE, 0xE3, 0xC9, 0xEB, 0x62, 0x34, 0x28}};
        const GUID sys_clsid = {0xE349BD1A, 0xFC20, 0x4E40, {0xB9, 0x9C, 0x41, 0x78, 0xCC, 0x6B, 0x40, 0x9F}};
        const GUID sys_iid = {0x1861CF2E, 0xE18B, 0x4834, {0xA9, 0xF5, 0xB4, 0xA4, 0xE6, 0xEF, 0xB4, 0xCF}};
        const GUID game_clsid = {0x973A344E, 0x24BF, 0x4D0F, {0x84, 0x57, 0x56, 0xC5, 0x34, 0x89, 0x2B, 0x29}};
        const GUID game_iid = {0x2549F142, 0x6419, 0x4A06, {0x97, 0xB5, 0x93, 0x1A, 0xAB, 0x7C, 0x2F, 0x34}};
        const GUID net_clsid = {0x37E56907, 0x2F10, 0x41E8, {0xB7, 0x2F, 0x36, 0xED, 0xB1, 0x85, 0x33, 0x1A}};
        const GUID net_iid = {0xBF2346B2, 0x39AF, 0x4658, {0xB5, 0xEA, 0x44, 0x71, 0x3C, 0x7E, 0x83, 0xB3}};
        const GUID iunknown = {0x00000000, 0x0000, 0x0000, {0xC0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x46}};

        void* com_core = nullptr;
        CHECK(SUCCEEDED(query_api(&core_guid, &core_guid, &com_core)) && com_core);
        auto** core_vtbl = *static_cast<void***>(com_core);
        auto qi = reinterpret_cast<HRESULT(WINAPI*)(void*, const GUID*, void**)>(core_vtbl[0]);
        auto release = reinterpret_cast<ULONG(WINAPI*)(void*)>(core_vtbl[2]);
        auto tq_create = reinterpret_cast<HRESULT(WINAPI*)(void*, uint32_t, uint32_t, void**)>(core_vtbl[12]);
        auto tq_close = reinterpret_cast<void(WINAPI*)(void*, void*)>(core_vtbl[17]);
        auto unimpl_slot = reinterpret_cast<HRESULT(WINAPI*)(void*)>(core_vtbl[14]);
        void* qi_out = nullptr;
        CHECK(SUCCEEDED(qi(com_core, &iunknown, &qi_out)) && qi_out == com_core);
        void* q = nullptr;
        CHECK(SUCCEEDED(tq_create(com_core, 2, 2, &q)) && q);
        tq_close(com_core, q);
        CHECK(unimpl_slot(com_core) == E_NOTIMPL);
        CHECK(release(com_core) >= 1);

        void* com_sys = nullptr;
        CHECK(SUCCEEDED(query_api(&sys_clsid, &sys_iid, &com_sys)) && com_sys);
        auto** sys_vtbl = *static_cast<void***>(com_sys);
        auto get_cid = reinterpret_cast<HRESULT(WINAPI*)(void*, size_t, char*, size_t*)>(sys_vtbl[3]);
        char cid[64]{};
        size_t cid_used = 0;
        CHECK(SUCCEEDED(get_cid(com_sys, sizeof(cid), cid, &cid_used)) && cid_used > 1);

        void* com_game = nullptr;
        CHECK(SUCCEEDED(query_api(&game_clsid, &game_iid, &com_game)) && com_game);
        auto** game_vtbl = *static_cast<void***>(com_game);
        auto get_tid = reinterpret_cast<HRESULT(WINAPI*)(void*, uint32_t*)>(game_vtbl[3]);
        uint32_t title_id = 0;
        CHECK(SUCCEEDED(get_tid(com_game, &title_id)) && title_id != 0);

        void* com_net = nullptr;
        CHECK(SUCCEEDED(query_api(&net_clsid, &net_iid, &com_net)) && com_net);
        auto** net_vtbl = *static_cast<void***>(com_net);
        struct NetHint { uint32_t lvl, cost, iana; bool init, app, over, roam; };
        auto get_hint = reinterpret_cast<HRESULT(WINAPI*)(void*, NetHint*)>(net_vtbl[15]);
        NetHint hint{};
        CHECK(SUCCEEDED(get_hint(com_net, &hint)) && hint.init);

        const GUID user_guid = {0x01ACD177, 0x91F9, 0x4763, {0xA3, 0x8E, 0xCC, 0xBB, 0x55, 0xCE, 0x32, 0xE0}};
        void* com_user = nullptr;
        CHECK(SUCCEEDED(query_api(&user_guid, &user_guid, &com_user)) && com_user);
        auto** user_vtbl = *static_cast<void***>(com_user);
        auto reg_ev = reinterpret_cast<HRESULT(WINAPI*)(void*, void*, void*, void*, uint64_t*)>(user_vtbl[33]);
        auto unreg_ev = reinterpret_cast<bool(WINAPI*)(void*, uint64_t, bool)>(user_vtbl[34]);
        uint64_t ev_tok = 0;
        auto dummy_cb = [](void*, uint64_t, uint32_t) {};
        CHECK(SUCCEEDED(reg_ev(com_user, nullptr, nullptr, reinterpret_cast<void*>(+dummy_cb), &ev_tok)) && ev_tok != 0);
        CHECK(unreg_ev(com_user, ev_tok, false));
        CHECK(!unreg_ev(com_user, ev_tok, false));

        check_delayed_callback(query_api);
    }

    XGameRuntimeUninitialize();
    CHECK(XStoreCreateContext(nullptr, &probe) == not_initialized);

    std::printf("xgameruntime conformance passed (%s, synthetic license %s)\n",
                profile, entitled ? "active" : "inactive");
    return 0;
}

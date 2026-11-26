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

#ifndef BX_TEST_UNENTITLED
extern "C" __declspec(dllexport) void BetterXenSyntheticStoreFixture() {}
#endif

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
    std::printf("xgameruntime conformance passed (%s, synthetic license %s)\n",
                profile, entitled ? "active" : "inactive");
    return 0;
}

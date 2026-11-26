// Offline XGameSave and XPackage through QueryApiImpl (#468).
//
// This fixture declares its own copies of the BetterXen synthetic keys and slot orders and
// of the public structures, instead of including the shim's headers, so a mistake there fails
// here. The keys must match shims/xgameruntime/synthetic_keys.h. Structure layouts and error
// values follow the Microsoft Learn XGameSave and XPackage reference pages.
#include <windows.h>

#include <cstddef>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <ctime>
#include <string>
#include <vector>

namespace {

#define CHECK(expr) do { \
    if (!(expr)) { std::fprintf(stderr, "FAIL: test_xgameruntime_saves line %d: %s\n", __LINE__, #expr); std::exit(1); } \
} while (false)

struct Async {
    void* queue;
    void* context;
    void (CALLBACK* callback)(Async*);
    unsigned char reserved[32];
};

struct ContainerInfo {  // XGameSaveContainerInfo
    const char* name;
    const char* display_name;
    uint32_t blob_count;
    uint64_t total_size;
    time_t last_modified_time;
    bool needs_sync;
};
static_assert(offsetof(ContainerInfo, total_size) == 24 && offsetof(ContainerInfo, needs_sync) == 40);

struct Blob {  // XGameSaveBlob { XGameSaveBlobInfo { name, size }, data }
    const char* name;
    uint32_t size;
    uint8_t* data;
};
static_assert(sizeof(Blob) == 24 && offsetof(Blob, data) == 16);

extern "C" {
HRESULT WINAPI XGameRuntimeInitialize();
void WINAPI XGameRuntimeUninitialize();
HRESULT WINAPI XTaskQueueCreate(uint32_t, uint32_t, void**);
void WINAPI XTaskQueueCloseHandle(void*);
bool WINAPI XTaskQueueDispatch(void*, uint32_t, uint32_t);
HRESULT WINAPI XUserAddAsync(uint32_t, Async*);
HRESULT WINAPI XUserAddResult(Async*, void**);
void WINAPI XUserCloseHandle(void*);
HRESULT WINAPI XAsyncGetStatus(Async*, bool);
HRESULT WINAPI XAsyncGetResultSize(Async*, size_t*);
void WINAPI XAsyncCancel(Async*);
HRESULT WINAPI XPersistentLocalStorageGetPath(size_t, char*, size_t*);
HRESULT WINAPI QueryApiImpl(const GUID*, const GUID*, void**);
}

// Expected values, from the public reference pages and the existing runtime.
constexpr HRESULT kInvalidContainerName = static_cast<HRESULT>(0x80830001u);
constexpr HRESULT kUpdateTooBig = static_cast<HRESULT>(0x80830005u);
constexpr HRESULT kBufferTooSmall = static_cast<HRESULT>(0x80830007u);
constexpr HRESULT kBlobNotFound = static_cast<HRESULT>(0x80830008u);
constexpr HRESULT kInvalidHandle = static_cast<HRESULT>(0x8924010cu);
constexpr HRESULT kNotInitialized = static_cast<HRESULT>(0x89240100u);
constexpr HRESULT kIllegalCall = static_cast<HRESULT>(0x8000000eu);
constexpr HRESULT kInsufficient = HRESULT_FROM_WIN32(ERROR_INSUFFICIENT_BUFFER);
constexpr HRESULT kNotFound = HRESULT_FROM_WIN32(ERROR_NOT_FOUND);
constexpr uint32_t kManual = 0, kWork = 0;
constexpr size_t kMiB16 = 16u * 1024u * 1024u;

GUID synthetic(uint32_t data1) {
    return {data1, 0x5853, 0x4000, {0x80, 0, 0, 0, 0, 0, 0, 0x01}};
}

// Slot numbers from the synthetic_keys.h contract, written out independently.
enum : int {
    kQueryInterface = 0, kAddRef = 1, kRelease = 2,
    kInitAsync = 3, kInitResult = 4, kCloseProvider = 5, kCreateContainer = 6, kCloseContainer = 7,
    kCreateUpdate = 8, kCloseUpdate = 9, kSubmitBlobWrite = 10, kSubmitUpdateAsync = 11,
    kSubmitUpdateResult = 12, kReadAsync = 13, kReadResult = 14, kEnumerate = 15,
};
enum : int { kGetId = 3, kGetKind = 4, kMount = 5, kGetMountPathSize = 6, kGetMountPath = 7, kCloseMount = 8 };

template <typename Fn>
Fn slot(void* object, int index) {
    void** vtbl = *static_cast<void***>(object);
    Fn fn;
    std::memcpy(&fn, &vtbl[index], sizeof(fn));
    return fn;
}

using QiFn = HRESULT(WINAPI*)(void*, const GUID*, void**);
using RefFn = ULONG(WINAPI*)(void*);
using InitAsyncFn = HRESULT(WINAPI*)(void*, void*, const char*, bool, Async*);
using InitResultFn = HRESULT(WINAPI*)(void*, Async*, void**);
using CloseFn = void(WINAPI*)(void*, void*);
using CreateContainerFn = HRESULT(WINAPI*)(void*, void*, const char*, void**);
using CreateUpdateFn = HRESULT(WINAPI*)(void*, void*, const char*, void**);
using BlobWriteFn = HRESULT(WINAPI*)(void*, void*, const char*, const uint8_t*, size_t);
using SubmitAsyncFn = HRESULT(WINAPI*)(void*, void*, Async*);
using AsyncResultFn = HRESULT(WINAPI*)(void*, Async*);
using ReadAsyncFn = HRESULT(WINAPI*)(void*, void*, const char**, uint32_t, Async*);
using ReadResultFn = HRESULT(WINAPI*)(void*, Async*, size_t, Blob*, uint32_t*);
using InfoCallback = bool(CALLBACK*)(const ContainerInfo*, void*);
using EnumerateFn = HRESULT(WINAPI*)(void*, void*, void*, InfoCallback);
using GetIdFn = HRESULT(WINAPI*)(void*, size_t, char*);
using GetKindFn = HRESULT(WINAPI*)(void*, const char*, uint32_t*);
using MountFn = HRESULT(WINAPI*)(void*, const char*, void**);
using PathSizeFn = HRESULT(WINAPI*)(void*, void*, size_t*);
using PathFn = HRESULT(WINAPI*)(void*, void*, size_t, char*);

struct Saves {
    void* api;
    void* queue;

    // Runs the queued work item on this thread (manual queue).
    void run() const { CHECK(XTaskQueueDispatch(queue, kWork, 0)); }

    void* provider(void* user) const {
        Async async{queue, nullptr, nullptr, {}};
        CHECK(slot<InitAsyncFn>(api, kInitAsync)(api, user, "00000000-0000-0000-0000-000000000000", false, &async) ==
              S_OK);
        CHECK(XAsyncGetStatus(&async, false) == E_PENDING);
        run();
        size_t size = 0;
        CHECK(XAsyncGetResultSize(&async, &size) == S_OK && size == sizeof(void*));
        void* provider = nullptr;
        CHECK(slot<InitResultFn>(api, kInitResult)(api, &async, &provider) == S_OK && provider);
        void* again = nullptr;
        CHECK(slot<InitResultFn>(api, kInitResult)(api, &async, &again) == kIllegalCall && !again);
        return provider;
    }

    void* container(void* provider, const char* name) const {
        void* handle = nullptr;
        CHECK(slot<CreateContainerFn>(api, kCreateContainer)(api, provider, name, &handle) == S_OK && handle);
        return handle;
    }

    void* update(void* container, const char* display_name) const {
        void* handle = nullptr;
        CHECK(slot<CreateUpdateFn>(api, kCreateUpdate)(api, container, display_name, &handle) == S_OK && handle);
        return handle;
    }

    HRESULT write(void* update, const char* blob, const void* data, size_t size) const {
        return slot<BlobWriteFn>(api, kSubmitBlobWrite)(api, update, blob, static_cast<const uint8_t*>(data), size);
    }

    void submit(void* update) const {
        Async async{queue, nullptr, nullptr, {}};
        CHECK(slot<SubmitAsyncFn>(api, kSubmitUpdateAsync)(api, update, &async) == S_OK);
        run();
        CHECK(slot<AsyncResultFn>(api, kSubmitUpdateResult)(api, &async) == S_OK);
    }

    void close(int which, void* handle) const { slot<CloseFn>(api, which)(api, handle); }

    // Reads blobs (all when names is empty) and returns them as name -> bytes.
    std::vector<std::pair<std::string, std::string>> read(void* container, std::vector<const char*> names) const {
        Async async{queue, nullptr, nullptr, {}};
        CHECK(slot<ReadAsyncFn>(api, kReadAsync)(api, container, names.empty() ? nullptr : names.data(),
                                                 static_cast<uint32_t>(names.size()), &async) == S_OK);
        run();
        size_t size = 0;
        CHECK(XAsyncGetResultSize(&async, &size) == S_OK);
        std::vector<unsigned char> buffer(size + 16, 0xCD);
        auto* blobs = reinterpret_cast<Blob*>(buffer.data());
        uint32_t count = 7;
        const auto read_result = slot<ReadResultFn>(api, kReadResult);
        if (size) CHECK(read_result(api, &async, size - 1, blobs, &count) == kBufferTooSmall && count == 0);
        CHECK(read_result(api, &async, size, size ? blobs : nullptr, &count) == S_OK);
        uint32_t consumed = 7;
        CHECK(read_result(api, &async, size, blobs, &consumed) == kIllegalCall && consumed == 0);
        for (size_t i = size; i < buffer.size(); ++i) CHECK(buffer[i] == 0xCD);

        std::vector<std::pair<std::string, std::string>> out;
        size_t expected = count * sizeof(Blob);
        for (uint32_t i = 0; i < count; ++i) {
            const auto* base = reinterpret_cast<const unsigned char*>(buffer.data());
            CHECK(reinterpret_cast<const unsigned char*>(blobs[i].name) >= base &&
                  reinterpret_cast<const unsigned char*>(blobs[i].name) < base + size);
            CHECK(blobs[i].size == 0 || (blobs[i].data >= base && blobs[i].data + blobs[i].size <= base + size));
            out.emplace_back(blobs[i].name, std::string(reinterpret_cast<const char*>(blobs[i].data), blobs[i].size));
            expected += std::strlen(blobs[i].name) + 1 + blobs[i].size;
        }
        CHECK(expected == size);
        return out;
    }

    HRESULT read_status(void* container, std::vector<const char*> names) const {
        Async async{queue, nullptr, nullptr, {}};
        const auto hr = slot<ReadAsyncFn>(api, kReadAsync)(api, container, names.empty() ? nullptr : names.data(),
                                                           static_cast<uint32_t>(names.size()), &async);
        if (FAILED(hr)) return hr;
        run();
        uint32_t count = 0;
        const auto status = XAsyncGetStatus(&async, false);
        CHECK(slot<ReadResultFn>(api, kReadResult)(api, &async, 0, nullptr, &count) == status);
        return status;
    }
};

const std::string* find_blob(const std::vector<std::pair<std::string, std::string>>& blobs, const char* name) {
    for (const auto& [blob, bytes] : blobs) {
        if (blob == name) return &bytes;
    }
    return nullptr;
}

struct Seen {
    std::string name, display_name;
    uint32_t blob_count;
    uint64_t total_size;
    time_t modified;
    bool needs_sync;
};

struct Listing {
    std::vector<Seen> seen;
    size_t stop_after = SIZE_MAX;
};

bool CALLBACK collect(const ContainerInfo* info, void* context) {
    auto& listing = *static_cast<Listing*>(context);
    listing.seen.push_back({info->name, info->display_name, info->blob_count, info->total_size,
                            info->last_modified_time, info->needs_sync});
    return listing.seen.size() < listing.stop_after;
}

std::string saves_directory() {
    char path[MAX_PATH * 4]{};
    CHECK(XPersistentLocalStorageGetPath(sizeof(path), path, nullptr) == S_OK);
    return std::string(path) + "\\saves";
}

// Container files left by an earlier run of this fixture (storage is bottle-persistent).
size_t container_files(const std::string& dir, bool remove) {
    WIN32_FIND_DATAA found;
    const HANDLE search = FindFirstFileA((dir + "\\*.bxgs").c_str(), &found);
    if (search == INVALID_HANDLE_VALUE) return 0;
    size_t count = 0;
    do {
        if (found.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY) continue;
        ++count;
        if (remove) CHECK(DeleteFileA((dir + "\\" + found.cFileName).c_str()));
    } while (FindNextFileA(search, &found));
    FindClose(search);
    return count;
}

void check_com(void* object, const GUID& own, const GUID& other) {
    const GUID iunknown = {0x00000000, 0x0000, 0x0000, {0xC0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x46}};
    void* out = nullptr;
    CHECK(slot<QiFn>(object, kQueryInterface)(object, &iunknown, &out) == S_OK && out == object);
    CHECK(slot<QiFn>(object, kQueryInterface)(object, &own, &out) == S_OK && out == object);
    CHECK(slot<QiFn>(object, kQueryInterface)(object, &other, &out) == E_NOINTERFACE && !out);
    CHECK(slot<QiFn>(object, kQueryInterface)(object, &own, nullptr) == E_POINTER);
    CHECK(slot<RefFn>(object, kAddRef)(object) >= 1 && slot<RefFn>(object, kRelease)(object) >= 1);
}

} // namespace

int main() {
    const time_t started = std::time(nullptr);
    CHECK(XGameRuntimeInitialize() == S_OK);

    // --- QueryApiImpl reaches both families as COM objects ---
    const GUID save_clsid = synthetic(0xB7581601), save_iid = synthetic(0xB7581602);
    const GUID package_clsid = synthetic(0xB7581701), package_iid = synthetic(0xB7581702);
    void* api = nullptr;
    void* package = nullptr;
    CHECK(QueryApiImpl(&save_clsid, &save_iid, &api) == S_OK && api);
    CHECK(QueryApiImpl(&package_clsid, &package_iid, &package) == S_OK && package && package != api);
    void* wrong = reinterpret_cast<void*>(static_cast<uintptr_t>(1));
    CHECK(QueryApiImpl(&save_clsid, &package_iid, &wrong) == E_NOINTERFACE && !wrong);
    check_com(api, save_iid, package_iid);
    check_com(package, package_iid, save_iid);

    void* queue = nullptr;
    CHECK(XTaskQueueCreate(kManual, kManual, &queue) == S_OK && queue);
    const Saves saves{api, queue};
    Async async{queue, nullptr, nullptr, {}};
    void* user = nullptr;
    CHECK(XUserAddAsync(0, &async) == S_OK);
    saves.run();
    CHECK(XUserAddResult(&async, &user) == S_OK && user);

    const std::string dir = saves_directory();
    container_files(dir, true);

    // --- Provider ---
    const auto init_async = slot<InitAsyncFn>(api, kInitAsync);
    CHECK(init_async(api, nullptr, "scid", false, &async) == kInvalidHandle);
    CHECK(init_async(api, user, nullptr, false, &async) == E_INVALIDARG);
    CHECK(init_async(api, user, "scid", false, nullptr) == E_POINTER);
    void* provider = saves.provider(user);

    // --- Container names (XGameSaveCreateContainer rules) ---
    const auto create_container = slot<CreateContainerFn>(api, kCreateContainer);
    const std::string too_long(257, 'a');
    for (const char* bad : {"", "a..b", "trail.", "has space", "dir.x/file", "dir/", "semi;colon", too_long.c_str()}) {
        void* handle = nullptr;
        CHECK(create_container(api, provider, bad, &handle) == kInvalidContainerName && !handle);
    }
    void* handle = nullptr;
    CHECK(create_container(api, provider, nullptr, &handle) == kInvalidContainerName);
    CHECK(create_container(api, nullptr, "slot", &handle) == kInvalidHandle);
    const std::string longest(256, 'a');
    for (const char* good : {"slot_1", "profiles/main", "/a", "a-b.c_d", longest.c_str()}) {
        saves.close(kCloseContainer, saves.container(provider, good));
    }

    // --- A container that was never submitted ---
    void* main_container = saves.container(provider, "profiles/main");
    CHECK(saves.read(main_container, {}).empty());
    CHECK(saves.read_status(main_container, {"progress"}) == kBlobNotFound);
    CHECK(saves.read_status(main_container, {"progress", "progress"}) == E_INVALIDARG);
    {
        Async read{queue, nullptr, nullptr, {}};
        CHECK(slot<ReadAsyncFn>(api, kReadAsync)(api, main_container, nullptr, 1, &read) == E_INVALIDARG);
    }
    CHECK(container_files(dir, false) == 0);

    // --- Write, then read back ---
    std::string progress(300, '\0');
    for (size_t i = 0; i < progress.size(); ++i) progress[i] = static_cast<char>(i * 7 + 3);
    const std::string settings = "volume=7";
    void* update = saves.update(main_container, "Main profile");
    CHECK(saves.write(update, "progress", progress.data(), progress.size()) == S_OK);
    CHECK(saves.write(update, "progress", progress.data(), progress.size()) == E_INVALIDARG);
    CHECK(saves.write(update, "", "x", 1) == E_INVALIDARG);
    CHECK(saves.write(update, "settings", settings.data(), settings.size()) == S_OK);
    {
        // The staged total may not exceed GS_MAX_BLOB_SIZE (16 MB, read as 16 MiB).
        std::vector<char> big(kMiB16);
        const size_t staged = progress.size() + settings.size();
        CHECK(saves.write(update, "big", big.data(), kMiB16 - staged + 1) == kUpdateTooBig);
    }
    {
        Async submit{queue, nullptr, nullptr, {}};
        CHECK(slot<SubmitAsyncFn>(api, kSubmitUpdateAsync)(api, update, &submit) == S_OK);
        Async twice{queue, nullptr, nullptr, {}};
        CHECK(slot<SubmitAsyncFn>(api, kSubmitUpdateAsync)(api, update, &twice) == kIllegalCall);
        CHECK(saves.write(update, "late", "x", 1) == kIllegalCall);
        saves.run();
        size_t size = 1;
        CHECK(XAsyncGetResultSize(&submit, &size) == S_OK && size == 0);
        CHECK(slot<AsyncResultFn>(api, kSubmitUpdateResult)(api, &submit) == S_OK);
    }
    saves.close(kCloseUpdate, update);

    auto blobs = saves.read(main_container, {});
    CHECK(blobs.size() == 2);
    CHECK(find_blob(blobs, "progress") && *find_blob(blobs, "progress") == progress);
    CHECK(find_blob(blobs, "settings") && *find_blob(blobs, "settings") == settings);
    blobs = saves.read(main_container, {"settings"});
    CHECK(blobs.size() == 1 && blobs[0].first == "settings" && blobs[0].second == settings);

    // --- A second container, then enumeration ---
    void* slot_container = saves.container(provider, "slot_1");
    void* slot_update = saves.update(slot_container, nullptr);
    CHECK(saves.write(slot_update, "a", "A", 1) == S_OK);
    saves.submit(slot_update);
    saves.close(kCloseUpdate, slot_update);
    saves.close(kCloseContainer, slot_container);
    CHECK(container_files(dir, false) == 2);

    const auto enumerate = slot<EnumerateFn>(api, kEnumerate);
    Listing listing;
    CHECK(enumerate(api, provider, &listing, collect) == S_OK && listing.seen.size() == 2);
    const time_t now = std::time(nullptr);
    for (const auto& seen : listing.seen) {
        CHECK(!seen.needs_sync && seen.modified >= started - 5 && seen.modified <= now + 5);
        if (seen.name == "profiles/main") {
            CHECK(seen.display_name == "Main profile" && seen.blob_count == 2 &&
                  seen.total_size == progress.size() + settings.size());
        } else {
            CHECK(seen.name == "slot_1" && seen.display_name.empty() && seen.blob_count == 1 && seen.total_size == 1);
        }
    }
    CHECK(listing.seen[0].name != listing.seen[1].name);
    // A file that does not parse is skipped (and logged as corrupt_container), not reported.
    {
        const std::string junk = dir + "\\0000000000000000.bxgs";
        const HANDLE file = CreateFileA(junk.c_str(), GENERIC_WRITE, 0, nullptr, CREATE_ALWAYS, FILE_ATTRIBUTE_NORMAL,
                                        nullptr);
        CHECK(file != INVALID_HANDLE_VALUE);
        DWORD wrote = 0;
        CHECK(WriteFile(file, "not a container", 15, &wrote, nullptr) && wrote == 15 && CloseHandle(file));
        Listing with_junk;
        CHECK(enumerate(api, provider, &with_junk, collect) == S_OK && with_junk.seen.size() == 2);
        CHECK(DeleteFileA(junk.c_str()));
    }
    Listing first_only;
    first_only.stop_after = 1;
    CHECK(enumerate(api, provider, &first_only, collect) == S_OK && first_only.seen.size() == 1);
    CHECK(enumerate(api, provider, nullptr, nullptr) == E_INVALIDARG);

    // --- Persistence across provider close and reopen ---
    saves.close(kCloseContainer, main_container);
    saves.close(kCloseProvider, provider);
    CHECK(create_container(api, provider, "slot_1", &handle) == kInvalidHandle && !handle);
    provider = saves.provider(user);
    main_container = saves.container(provider, "profiles/main");
    blobs = saves.read(main_container, {"settings", "progress"});
    CHECK(blobs.size() == 2 && *find_blob(blobs, "progress") == progress && *find_blob(blobs, "settings") == settings);

    // Overwrite one blob through the process task queue; the other blob and the display name stay.
    const std::string progress2 = "lvl=9";
    update = saves.update(main_container, nullptr);
    CHECK(saves.write(update, "progress", progress2.data(), progress2.size()) == S_OK);
    {
        Async pooled{nullptr, nullptr, nullptr, {}};
        CHECK(slot<SubmitAsyncFn>(api, kSubmitUpdateAsync)(api, update, &pooled) == S_OK);
        CHECK(XAsyncGetStatus(&pooled, true) == S_OK);
        CHECK(slot<AsyncResultFn>(api, kSubmitUpdateResult)(api, &pooled) == S_OK);
    }
    saves.close(kCloseUpdate, update);
    blobs = saves.read(main_container, {});
    CHECK(blobs.size() == 2 && *find_blob(blobs, "progress") == progress2 && *find_blob(blobs, "settings") == settings);
    listing = {};
    CHECK(enumerate(api, provider, &listing, collect) == S_OK);
    for (const auto& seen : listing.seen) {
        if (seen.name == "profiles/main") CHECK(seen.display_name == "Main profile");
    }

    // A canceled submit writes nothing.
    update = saves.update(main_container, "Canceled");
    CHECK(saves.write(update, "progress", "X", 1) == S_OK);
    {
        Async canceled{queue, nullptr, nullptr, {}};
        CHECK(slot<SubmitAsyncFn>(api, kSubmitUpdateAsync)(api, update, &canceled) == S_OK);
        XAsyncCancel(&canceled);
        saves.run();
        CHECK(XAsyncGetStatus(&canceled, false) == E_ABORT);
        CHECK(slot<AsyncResultFn>(api, kSubmitUpdateResult)(api, &canceled) == E_ABORT);
    }
    saves.close(kCloseUpdate, update);
    blobs = saves.read(main_container, {"progress"});
    CHECK(blobs.size() == 1 && blobs[0].second == progress2);

    // A closed container handle is rejected.
    saves.close(kCloseContainer, main_container);
    CHECK(saves.read_status(main_container, {}) == kInvalidHandle);
    update = nullptr;
    CHECK(slot<CreateUpdateFn>(api, kCreateUpdate)(api, main_container, "x", &update) == kInvalidHandle && !update);

    // --- XPackage ---
    char id[64]{};
    CHECK(slot<GetIdFn>(package, kGetId)(package, sizeof(id), id) == S_OK);
    CHECK(std::strncmp(id, "BXPKG", 5) == 0 && std::strlen(id) == 21);
    for (const char* c = id + 5; *c; ++c) CHECK((*c >= '0' && *c <= '9') || (*c >= 'a' && *c <= 'f'));
    char again[64]{};
    CHECK(slot<GetIdFn>(package, kGetId)(package, sizeof(again), again) == S_OK && std::strcmp(id, again) == 0);
    CHECK(slot<GetIdFn>(package, kGetId)(package, std::strlen(id), again) == kInsufficient);
    CHECK(slot<GetIdFn>(package, kGetId)(package, sizeof(again), nullptr) == E_POINTER);

    uint32_t kind = 99;
    CHECK(slot<GetKindFn>(package, kGetKind)(package, id, &kind) == S_OK && kind == 0);  // XPackageKind::Game
    CHECK(slot<GetKindFn>(package, kGetKind)(package, "BXPKG0000000000000000", &kind) == kNotFound);

    char exe[MAX_PATH]{};
    CHECK(GetModuleFileNameA(nullptr, exe, MAX_PATH) > 0);
    *std::strrchr(exe, '\\') = '\0';
    void* mount = nullptr;
    CHECK(slot<MountFn>(package, kMount)(package, "BXPKG0000000000000000", &mount) == kNotFound && !mount);
    CHECK(slot<MountFn>(package, kMount)(package, id, &mount) == S_OK && mount);
    size_t path_size = 0;
    CHECK(slot<PathSizeFn>(package, kGetMountPathSize)(package, mount, &path_size) == S_OK);
    CHECK(path_size == std::strlen(exe) + 1);
    std::vector<char> path(path_size);
    CHECK(slot<PathFn>(package, kGetMountPath)(package, mount, path_size - 1, path.data()) == kInsufficient);
    CHECK(slot<PathFn>(package, kGetMountPath)(package, mount, path_size, path.data()) == S_OK);
    CHECK(std::strcmp(path.data(), exe) == 0);
    slot<CloseFn>(package, kCloseMount)(package, mount);
    CHECK(slot<PathSizeFn>(package, kGetMountPathSize)(package, mount, &path_size) == kInvalidHandle);

    // --- After uninitialize, handles are stale ---
    XUserCloseHandle(user);
    XTaskQueueCloseHandle(queue);
    XGameRuntimeUninitialize();
    CHECK(create_container(api, provider, "slot_1", &handle) == kNotInitialized);
    CHECK(slot<GetIdFn>(package, kGetId)(package, sizeof(id), id) == kNotInitialized);

    std::printf("PASS: test_xgameruntime_saves (%llu bytes round-tripped across provider reopen; package %s)\n",
                static_cast<unsigned long long>(progress.size() + settings.size()), id);
    return 0;
}

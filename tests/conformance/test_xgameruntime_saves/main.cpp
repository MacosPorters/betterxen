// Offline XGameSave and XPackage through QueryApiImpl (#468).
//
// This fixture declares its own copies of the BetterXen synthetic keys and slot orders, of
// the public structures, and of the documented storage layout, instead of including the
// shim's headers, so a mistake there fails here. The keys must match
// shims/xgameruntime/synthetic_keys.h. Structure layouts and error values follow the Microsoft
// Learn XGameSave and XPackage reference pages; the file layout and naming follow the header
// comment of shims/xgameruntime/game_save.cpp.
//
// Run with no arguments. "--writer <id> <count> <event>" is the child mode used by the
// cross-process check.
#include <windows.h>

#include <cstddef>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <cwchar>
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
constexpr HRESULT kCorrupt = HRESULT_FROM_WIN32(ERROR_FILE_CORRUPT);
constexpr uint32_t kManual = 0, kWork = 0;
constexpr size_t kMiB16 = 16u * 1024u * 1024u;
constexpr const char* kScid = "00000000-0000-0000-0000-000000000000";

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

using Blobs = std::vector<std::pair<std::string, std::string>>;

void* query(uint32_t clsid, uint32_t iid) {
    const GUID c = synthetic(clsid), i = synthetic(iid);
    void* api = nullptr;
    CHECK(QueryApiImpl(&c, &i, &api) == S_OK && api);
    return api;
}

void* add_user(void* queue) {
    Async async{queue, nullptr, nullptr, {}};
    void* user = nullptr;
    CHECK(XUserAddAsync(0, &async) == S_OK);
    if (queue) CHECK(XTaskQueueDispatch(queue, kWork, 0));
    else CHECK(XAsyncGetStatus(&async, true) == S_OK);
    CHECK(XUserAddResult(&async, &user) == S_OK && user);
    return user;
}

// queue null: the process task queue (thread pool); otherwise a manual queue run inline.
struct Saves {
    void* api;
    void* queue;

    void wait(Async& async) const {
        if (queue) CHECK(XTaskQueueDispatch(queue, kWork, 0));
        else XAsyncGetStatus(&async, true);
    }

    void* provider(void* user, const char* scid = kScid) const {
        Async async{queue, nullptr, nullptr, {}};
        CHECK(slot<InitAsyncFn>(api, kInitAsync)(api, user, scid, false, &async) == S_OK);
        if (queue) CHECK(XAsyncGetStatus(&async, false) == E_PENDING);
        wait(async);
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

    HRESULT submit_status(void* update) const {
        Async async{queue, nullptr, nullptr, {}};
        const auto hr = slot<SubmitAsyncFn>(api, kSubmitUpdateAsync)(api, update, &async);
        if (FAILED(hr)) return hr;
        wait(async);
        const auto status = XAsyncGetStatus(&async, false);
        CHECK(slot<AsyncResultFn>(api, kSubmitUpdateResult)(api, &async) == status);
        return status;
    }

    void submit(void* update) const {
        const auto status = submit_status(update);
        if (status != S_OK) std::fprintf(stderr, "submit status 0x%08lx\n", static_cast<unsigned long>(status));
        CHECK(status == S_OK);
    }

    // One update writing one blob, submitted and closed.
    void put(void* container, const char* blob, const std::string& data) const {
        void* staged = update(container, nullptr);
        CHECK(write(staged, blob, data.data(), data.size()) == S_OK);
        submit(staged);
        close(kCloseUpdate, staged);
    }

    void close(int which, void* handle) const { slot<CloseFn>(api, which)(api, handle); }

    // Reads blobs (all when names is empty) and returns them as name -> bytes.
    Blobs read(void* container, std::vector<const char*> names) const {
        Async async{queue, nullptr, nullptr, {}};
        CHECK(slot<ReadAsyncFn>(api, kReadAsync)(api, container, names.empty() ? nullptr : names.data(),
                                                 static_cast<uint32_t>(names.size()), &async) == S_OK);
        wait(async);
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

        Blobs out;
        size_t expected = count * sizeof(Blob);
        const auto* base = reinterpret_cast<const unsigned char*>(buffer.data());
        for (uint32_t i = 0; i < count; ++i) {
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
        wait(async);
        uint32_t count = 0;
        const auto status = XAsyncGetStatus(&async, false);
        size_t size = 0;
        if (SUCCEEDED(status)) CHECK(XAsyncGetResultSize(&async, &size) == S_OK);
        std::vector<unsigned char> buffer(size);
        CHECK(slot<ReadResultFn>(api, kReadResult)(api, &async, size,
                                                   size ? reinterpret_cast<Blob*>(buffer.data()) : nullptr,
                                                   &count) == status);
        return status;
    }
};

const std::string* find_blob(const Blobs& blobs, const char* name) {
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

Listing list(const Saves& saves, void* provider) {
    Listing listing;
    CHECK(slot<EnumerateFn>(saves.api, kEnumerate)(saves.api, provider, &listing, collect) == S_OK);
    return listing;
}

// --- The documented storage layout, re-derived here ---

// FNV-1a (64-bit) over each byte widened to a UTF-16 code unit, printed as 16 hex digits.
std::string hex_id(const std::string& text) {
    uint64_t hash = 14695981039346656037ULL;
    for (unsigned char c : text) {
        hash ^= c;
        hash *= 1099511628211ULL;
    }
    char out[17];
    std::snprintf(out, sizeof(out), "%016llx", static_cast<unsigned long long>(hash));
    return out;
}

std::string saves_directory(const char* scid) {
    char path[MAX_PATH * 4]{};
    CHECK(XPersistentLocalStorageGetPath(sizeof(path), path, nullptr) == S_OK);
    return std::string(path) + "\\saves\\" + hex_id(scid);
}

std::string container_path(const std::string& dir, const std::string& name) {
    return dir + "\\" + hex_id(name) + ".bxgs";
}

void put32(std::string& out, uint32_t value) {
    for (int i = 0; i < 4; ++i) out.push_back(static_cast<char>(value >> (8 * i)));
}

void put_str(std::string& out, const std::string& text) {
    put32(out, static_cast<uint32_t>(text.size()));
    out += text;
}

// "BXGS", version 1, name, display name, i64 time, blob count, then name / size / bytes.
std::string container_bytes(const std::string& name, const std::vector<std::pair<std::string, std::string>>& blobs) {
    std::string out = "BXGS";
    put32(out, 1);
    put_str(out, name);
    put_str(out, "crafted");
    put32(out, 1700000000);
    put32(out, 0);
    put32(out, static_cast<uint32_t>(blobs.size()));
    for (const auto& [blob, data] : blobs) {
        put_str(out, blob);
        put_str(out, data);
    }
    return out;
}

void write_file(const std::string& path, const std::string& bytes) {
    const HANDLE file = CreateFileA(path.c_str(), GENERIC_WRITE, 0, nullptr, CREATE_ALWAYS, FILE_ATTRIBUTE_NORMAL,
                                    nullptr);
    CHECK(file != INVALID_HANDLE_VALUE);
    DWORD wrote = 0;
    CHECK(WriteFile(file, bytes.data(), static_cast<DWORD>(bytes.size()), &wrote, nullptr) && wrote == bytes.size());
    CHECK(CloseHandle(file));
}

// Files matching pattern in dir; optionally deletes them.
size_t files(const std::string& dir, const char* pattern, bool remove) {
    WIN32_FIND_DATAA found;
    const HANDLE search = FindFirstFileA((dir + "\\" + pattern).c_str(), &found);
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

// Committed private memory, summed over the address space (Wine reports PrivateUsage as 0).
uint64_t private_bytes() {
    uint64_t total = 0;
    MEMORY_BASIC_INFORMATION region{};
    for (auto* at = static_cast<const char*>(nullptr);
         VirtualQuery(at, &region, sizeof(region)) == sizeof(region);
         at = static_cast<const char*>(region.BaseAddress) + region.RegionSize) {
        if (region.State == MEM_COMMIT && region.Type == MEM_PRIVATE) total += region.RegionSize;
        if (region.RegionSize == 0) break;
    }
    return total;
}

std::string utf8(const wchar_t* text) {
    const int bytes = WideCharToMultiByte(CP_UTF8, 0, text, -1, nullptr, 0, nullptr, nullptr);
    CHECK(bytes > 0);
    std::string out(static_cast<size_t>(bytes), '\0');
    WideCharToMultiByte(CP_UTF8, 0, text, -1, out.data(), bytes, nullptr, nullptr);
    out.pop_back();
    return out;
}

// --- Concurrency ---

constexpr char kShared[] = "shared/race";

struct ThreadJob {
    void* api;
    void* container;
    int id;
    int count;
};

DWORD WINAPI thread_writer(void* context) {
    const auto& job = *static_cast<ThreadJob*>(context);
    const Saves saves{job.api, nullptr};
    for (int k = 0; k < job.count; ++k) {
        char blob[32];
        std::snprintf(blob, sizeof(blob), "t%d_%d", job.id, k);
        saves.put(job.container, blob, blob);
    }
    return 0;
}

// Cross-process payload: the blob name, then padding to 64 KiB so each rewrite takes a while.
std::string payload(const char* blob) {
    std::string data(blob);
    data.resize(64 * 1024, '.');
    return data;
}

// Child process: submits count blobs "w<id>_<k>" to kShared once the parent's event is set.
int writer_main(int id, int count, const wchar_t* event_name) {
    CHECK(XGameRuntimeInitialize() == S_OK);
    const Saves saves{query(0xB7581601, 0xB7581602), nullptr};
    void* user = add_user(nullptr);
    void* provider = saves.provider(user);
    void* container = saves.container(provider, kShared);
    const HANDLE go = OpenEventW(SYNCHRONIZE, FALSE, event_name);
    CHECK(go && WaitForSingleObject(go, 30000) == WAIT_OBJECT_0);
    CloseHandle(go);
    for (int k = 0; k < count; ++k) {
        char blob[32];
        std::snprintf(blob, sizeof(blob), "w%d_%d", id, k);
        saves.put(container, blob, payload(blob));
    }
    XGameRuntimeUninitialize();
    return 0;
}

// Public COM ids and slot numbers counted from xodus-gaming/xgameruntime-docs
// commit 7090008692569aa6081e30dfdd9de58237741e24. IUnknown is slots 0-2.
// The test keeps its own copies. IXGameSaveImpl lists 27 methods in slots 3-29.
// IXGameSaveImpl2 adds 3 methods in slots 30-32 and IXGameSaveImpl3 adds none.
// IXPackageImpl lists 32 methods in slots 3-34. IXPackageImpl2 adds 4, IXPackageImpl3
// adds 2, and IXPackageImpl4 adds 1.
enum : int {
    kGsInit = 3, kGsInitAsync, kGsInitResult, kGsCloseProvider, kGsQuota, kGsQuotaAsync, kGsQuotaResult,
    kGsDelete, kGsDeleteAsync, kGsDeleteResult, kGsGetContainerInfo, kGsEnumerate, kGsEnumerateByName,
    kGsCreateContainer, kGsCloseContainer, kGsEnumerateBlobs, kGsEnumerateBlobsByName, kGsRead, kGsReadAsync,
    kGsReadResult, kGsCreateUpdate, kGsCloseUpdate, kGsBlobWrite, kGsBlobDelete, kGsSubmit, kGsSubmitAsync,
    kGsSubmitResult, kGsFilesFolderAsync, kGsFilesFolderResult, kGsFilesQuota,
};
static_assert(kGsQuota == 7 && kGsEnumerate == 14 && kGsCreateContainer == 16 && kGsRead == 20);
static_assert(kGsCreateUpdate == 23 && kGsBlobWrite == 25 && kGsSubmit == 27 && kGsFilesQuota == 32);

enum : int {
    kPkId = 3, kPkIsPackaged, kPkCreateMonitor, kPkCloseMonitor, kPkGetProgress, kPkUpdateMonitor,
    kPkRegisterProgress, kPkUnregisterProgress, kPkLocale, kPkFindChunk, kPkEnumChunk, kPkChangeOrder,
    kPkInstall, kPkInstallAsync, kPkInstallResult, kPkEstimate, kPkUninstallChunks, kPkUnknown20,
    kPkUnknown21, kPkUnregisterInstalled, kPkMount, kPkPathSize, kPkPath, kPkCloseMount, kPkUnknown27,
    kPkEnumPackages, kPkRegisterInstalled, kPkWriteStats, kPkUnknown31, kPkUninstallUwp, kPkEnumFeatures,
    kPkUninstall, kPkEnumPackages2, kPkRegisterInstalled2, kPkMountUiAsync, kPkMountUiResult,
    kPkEnumPackages3, kPkRegisterInstalled3, kPkKind,
};
static_assert(kPkUnknown20 == 20 && kPkMount == 23 && kPkCloseMount == 26 && kPkKind == 41);

GUID guid_bytes(uint32_t a, uint16_t b, uint16_t c, uint8_t d0, uint8_t d1, uint8_t d2, uint8_t d3, uint8_t d4,
                uint8_t d5, uint8_t d6, uint8_t d7) {
    return {a, b, c, {d0, d1, d2, d3, d4, d5, d6, d7}};
}

void expect_same_object(void* object, const GUID& iid) {
    void* out = nullptr;
    CHECK(slot<QiFn>(object, kQueryInterface)(object, &iid, &out) == S_OK && out == object);
}

// QueryApiImpl with the public class and interface ids, then one save and one package round trip
// through those vtable slots. Expected bytes and the mount path come from this function's inputs.
void check_public_dispatch(void* user, void* synthetic_save, void* synthetic_package, time_t started) {
    const GUID save_class = guid_bytes(0x704C3F58, 0xE629, 0x4CC2, 0xB1, 0x97, 0x30, 0x51, 0x1B, 0x99, 0x6F, 0xE2);
    const GUID save_iid2 = guid_bytes(0x704C3F58, 0xE629, 0x4CC2, 0xB1, 0x97, 0x30, 0x51, 0x1B, 0x99, 0x6E, 0xE2);
    const GUID save_iid3 = guid_bytes(0x1BFFF3AF, 0xF14A, 0x40A3, 0x8E, 0x35, 0x9A, 0xDA, 0x90, 0x65, 0x93, 0xF9);
    const GUID package_class = guid_bytes(0xAF406016, 0xE850, 0x4AA8, 0xA8, 0x8D, 0x2F, 0x3D, 0xCB, 0x9D, 0xAC, 0x7E);
    const GUID package_iid = guid_bytes(0x3720DE07, 0xE8E4, 0x44A3, 0xAD, 0x32, 0xB3, 0x59, 0xE8, 0xAD, 0xBE, 0x55);
    const GUID package_iid2 = guid_bytes(0xF92D8712, 0x2B27, 0x4D8A, 0xBF, 0x01, 0x11, 0xA6, 0xF8, 0xE3, 0xEB, 0x42);
    const GUID package_iid3 = guid_bytes(0xE2A4734B, 0x2F4A, 0x456D, 0xAA, 0x8F, 0xD0, 0x65, 0xE0, 0x4F, 0xB2, 0x09);
    const GUID package_iid4 = guid_bytes(0x0B9D556E, 0xAB70, 0x43F4, 0xBA, 0xA0, 0x99, 0x99, 0xAC, 0x4C, 0xB0, 0x40);
    const GUID iunknown = {0x00000000, 0x0000, 0x0000, {0xC0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x46}};
    const GUID unknown = {0xDEADBEEF, 0x0000, 0x0000, {0x11, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00}};

    void* save = nullptr;
    CHECK(QueryApiImpl(&save_class, &save_class, &save) == S_OK && save && save != synthetic_save);
    void* again = nullptr;
    CHECK(QueryApiImpl(&save_class, &save_iid2, &again) == S_OK && again == save);
    CHECK(QueryApiImpl(&save_class, &save_iid3, &again) == S_OK && again == save);
    CHECK(QueryApiImpl(&save_class, &iunknown, &again) == S_OK && again == save);
    expect_same_object(save, iunknown);
    expect_same_object(save, save_class);
    expect_same_object(save, save_iid2);
    expect_same_object(save, save_iid3);
    void* refused = reinterpret_cast<void*>(static_cast<uintptr_t>(1));
    CHECK(slot<QiFn>(save, kQueryInterface)(save, &unknown, &refused) == E_NOINTERFACE && !refused);
    refused = reinterpret_cast<void*>(static_cast<uintptr_t>(1));
    CHECK(QueryApiImpl(&save_class, &unknown, &refused) == E_NOINTERFACE && !refused);
    CHECK(slot<RefFn>(save, kAddRef)(save) >= 1 && slot<RefFn>(save, kRelease)(save) >= 1);

    using Hr0 = HRESULT(WINAPI*)(void*);
    CHECK(slot<Hr0>(save, kGsQuota)(save) == E_NOTIMPL);
    CHECK(slot<Hr0>(save, kGsFilesQuota)(save) == E_NOTIMPL);

    constexpr const char* kPublicScid = "11111111-1111-1111-1111-111111111111";
    constexpr const char* kContainer = "pub_slot";
    constexpr const char* kDisplay = "Round";
    constexpr const char* kBlob = "note";
    const uint8_t payload[] = {0x10, 0x20, 0x30, 0xFF};
    using InitFn = HRESULT(WINAPI*)(void*, void*, const char*, bool, void**);
    void* provider = nullptr;
    CHECK(slot<InitFn>(save, kGsInit)(save, user, kPublicScid, false, &provider) == S_OK && provider);

    void* container = nullptr;
    CHECK(slot<CreateContainerFn>(save, kGsCreateContainer)(save, provider, kContainer, &container) == S_OK &&
          container);
    void* update = nullptr;
    CHECK(slot<CreateUpdateFn>(save, kGsCreateUpdate)(save, container, kDisplay, &update) == S_OK && update);
    CHECK(slot<BlobWriteFn>(save, kGsBlobWrite)(save, update, kBlob, payload, sizeof(payload)) == S_OK);
    using SubmitFn = HRESULT(WINAPI*)(void*, void*);
    CHECK(slot<SubmitFn>(save, kGsSubmit)(save, update) == S_OK);

    const char* names[] = {kBlob};
    uint32_t count = 1;
    using ReadFn = HRESULT(WINAPI*)(void*, void*, const char**, uint32_t*, size_t, Blob*);
    CHECK(slot<ReadFn>(save, kGsRead)(save, container, names, &count, 0, nullptr) == kBufferTooSmall && count == 0);
    count = 1;
    const size_t need = sizeof(Blob) + std::strlen(kBlob) + 1 + sizeof(payload);
    std::vector<unsigned char> buffer(need + 8, 0xCD);
    auto* blobs = reinterpret_cast<Blob*>(buffer.data());
    CHECK(slot<ReadFn>(save, kGsRead)(save, container, names, &count, need, blobs) == S_OK && count == 1);
    CHECK(std::strcmp(blobs[0].name, kBlob) == 0 && blobs[0].size == sizeof(payload));
    CHECK(std::memcmp(blobs[0].data, payload, sizeof(payload)) == 0);
    const auto* base = buffer.data();
    CHECK(reinterpret_cast<const unsigned char*>(blobs[0].name) >= base &&
          reinterpret_cast<const unsigned char*>(blobs[0].name) + std::strlen(kBlob) + 1 <= base + need);
    CHECK(blobs[0].data >= base && blobs[0].data + blobs[0].size <= base + need);
    for (size_t i = need; i < buffer.size(); ++i) CHECK(buffer[i] == 0xCD);

    Listing listing;
    CHECK(slot<EnumerateFn>(save, kGsEnumerate)(save, provider, &listing, collect) == S_OK);
    CHECK(listing.seen.size() == 1);
    CHECK(listing.seen[0].name == kContainer && listing.seen[0].display_name == kDisplay);
    CHECK(listing.seen[0].blob_count == 1 && listing.seen[0].total_size == sizeof(payload));
    CHECK(!listing.seen[0].needs_sync && listing.seen[0].modified >= started);

    slot<CloseFn>(save, kGsCloseUpdate)(save, update);
    slot<CloseFn>(save, kGsCloseContainer)(save, container);
    slot<CloseFn>(save, kGsCloseProvider)(save, provider);

    void* package = nullptr;
    CHECK(QueryApiImpl(&package_class, &package_iid4, &package) == S_OK && package && package != synthetic_package);
    CHECK(QueryApiImpl(&package_class, &package_iid, &again) == S_OK && again == package);
    CHECK(QueryApiImpl(&package_class, &package_iid2, &again) == S_OK && again == package);
    CHECK(QueryApiImpl(&package_class, &package_iid3, &again) == S_OK && again == package);
    CHECK(QueryApiImpl(&package_class, &iunknown, &again) == S_OK && again == package);
    expect_same_object(package, iunknown);
    expect_same_object(package, package_iid);
    expect_same_object(package, package_iid2);
    expect_same_object(package, package_iid3);
    expect_same_object(package, package_iid4);
    refused = reinterpret_cast<void*>(static_cast<uintptr_t>(1));
    CHECK(slot<QiFn>(package, kQueryInterface)(package, &unknown, &refused) == E_NOINTERFACE && !refused);
    refused = reinterpret_cast<void*>(static_cast<uintptr_t>(1));
    CHECK(QueryApiImpl(&package_class, &save_class, &refused) == E_NOINTERFACE && !refused);
    refused = reinterpret_cast<void*>(static_cast<uintptr_t>(1));
    CHECK(QueryApiImpl(&package_class, &package_class, &refused) == E_NOINTERFACE && !refused);

    using Bool1 = bool(WINAPI*)(void*);
    CHECK(slot<Bool1>(package, kPkIsPackaged)(package) == true);
    using Bool2 = bool(WINAPI*)(void*, void*);
    CHECK(slot<Bool2>(package, kPkUpdateMonitor)(package, nullptr) == false);
    using Void2 = void(WINAPI*)(void*, void*);
    slot<Void2>(package, kPkCloseMonitor)(package, nullptr);
    CHECK(slot<Hr0>(package, kPkUnknown20)(package) == E_NOTIMPL);

    char id[64]{};
    CHECK(slot<GetIdFn>(package, kPkId)(package, sizeof(id), id) == S_OK);
    CHECK(std::strncmp(id, "BXPKG", 5) == 0 && std::strlen(id) == 21);
    uint32_t kind = 99;
    CHECK(slot<GetKindFn>(package, kPkKind)(package, id, &kind) == S_OK && kind == 0);
    wchar_t exe_w[MAX_PATH]{};
    CHECK(GetModuleFileNameW(nullptr, exe_w, MAX_PATH) > 0);
    *std::wcsrchr(exe_w, L'\\') = L'\0';
    const std::string exe_dir = utf8(exe_w);
    void* mount = nullptr;
    CHECK(slot<MountFn>(package, kPkMount)(package, id, &mount) == S_OK && mount);
    size_t path_size = 0;
    CHECK(slot<PathSizeFn>(package, kPkPathSize)(package, mount, &path_size) == S_OK);
    CHECK(path_size == exe_dir.size() + 1);
    std::vector<char> path(path_size);
    CHECK(slot<PathFn>(package, kPkPath)(package, mount, path_size, path.data()) == S_OK);
    CHECK(exe_dir == path.data());
    slot<CloseFn>(package, kPkCloseMount)(package, mount);
    CHECK(slot<PathSizeFn>(package, kPkPathSize)(package, mount, &path_size) == kInvalidHandle);

    // XPackageMountWithUiAsync / Result (IXPackageImpl2 slots 37-38): the same mount, as an XAsync
    // operation on the process task queue. A foreign identifier is the operation's status.
    using MountAsyncFn = HRESULT(WINAPI*)(void*, const char*, Async*);
    using MountResultFn = HRESULT(WINAPI*)(void*, Async*, void**);
    const auto mount_async = slot<MountAsyncFn>(package, kPkMountUiAsync);
    const auto mount_result = slot<MountResultFn>(package, kPkMountUiResult);
    CHECK(mount_async(package, id, nullptr) == E_POINTER);
    Async probe{nullptr, nullptr, nullptr, {}};
    CHECK(mount_async(package, nullptr, &probe) == E_INVALIDARG);
    CHECK(mount_result(package, &probe, nullptr) == E_POINTER);
    {
        Async async{nullptr, nullptr, nullptr, {}};
        CHECK(mount_async(package, id, &async) == S_OK);
        CHECK(XAsyncGetStatus(&async, true) == S_OK);
        size_t size = 0;
        CHECK(XAsyncGetResultSize(&async, &size) == S_OK && size == sizeof(void*));
        void* ui_mount = nullptr;
        CHECK(mount_result(package, &async, &ui_mount) == S_OK && ui_mount);
        void* twice = reinterpret_cast<void*>(static_cast<uintptr_t>(1));
        CHECK(mount_result(package, &async, &twice) == kIllegalCall && !twice);
        CHECK(slot<PathSizeFn>(package, kPkPathSize)(package, ui_mount, &path_size) == S_OK &&
              path_size == exe_dir.size() + 1);
        std::vector<char> ui_path(path_size);
        CHECK(slot<PathFn>(package, kPkPath)(package, ui_mount, path_size, ui_path.data()) == S_OK);
        CHECK(exe_dir == ui_path.data());
        slot<CloseFn>(package, kPkCloseMount)(package, ui_mount);
        CHECK(slot<PathSizeFn>(package, kPkPathSize)(package, ui_mount, &path_size) == kInvalidHandle);
    }
    {
        Async async{nullptr, nullptr, nullptr, {}};
        CHECK(mount_async(package, "BXPKG0000000000000000", &async) == S_OK);
        CHECK(XAsyncGetStatus(&async, true) == kNotFound);
        void* none = reinterpret_cast<void*>(static_cast<uintptr_t>(1));
        CHECK(mount_result(package, &async, &none) == kNotFound && !none);
    }

    // IXError slot 3 answers false ("continue"); a true answer stops GDK calls such as XPackageMount.
    const GUID error_class = guid_bytes(0x8CA467F7, 0x22E8, 0x4096, 0x84, 0x56, 0xBB, 0x8A, 0xA1, 0x3F, 0x79, 0xD8);
    void* error = nullptr;
    CHECK(QueryApiImpl(&error_class, &error_class, &error) == S_OK && error);
    CHECK(slot<Bool1>(error, 3)(error) == false);
}

void check_blobs(const Blobs& blobs, char prefix, int writers, int count) {
    for (int id = 0; id < writers; ++id) {
        for (int k = 0; k < count; ++k) {
            char blob[32];
            std::snprintf(blob, sizeof(blob), "%c%d_%d", prefix, id, k);
            const auto* data = find_blob(blobs, blob);
            CHECK(data && *data == (prefix == 'w' ? payload(blob) : std::string(blob)));
        }
    }
}

} // namespace

int main() {
    int argc = 0;
    wchar_t** argv = CommandLineToArgvW(GetCommandLineW(), &argc);
    if (argc == 5 && std::wcscmp(argv[1], L"--writer") == 0) {
        return writer_main(std::wcstol(argv[2], nullptr, 10), std::wcstol(argv[3], nullptr, 10), argv[4]);
    }

    const time_t started = std::time(nullptr);
    CHECK(XGameRuntimeInitialize() == S_OK);

    // --- QueryApiImpl reaches both families as COM objects ---
    const GUID save_clsid = synthetic(0xB7581601), save_iid = synthetic(0xB7581602);
    const GUID package_clsid = synthetic(0xB7581701), package_iid = synthetic(0xB7581702);
    const GUID iunknown = {0x00000000, 0x0000, 0x0000, {0xC0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x46}};
    void* api = query(0xB7581601, 0xB7581602);
    void* package = query(0xB7581701, 0xB7581702);
    CHECK(package != api);
    void* wrong = reinterpret_cast<void*>(static_cast<uintptr_t>(1));
    CHECK(QueryApiImpl(&save_clsid, &package_iid, &wrong) == E_NOINTERFACE && !wrong);
    void* via_unknown = nullptr;
    CHECK(QueryApiImpl(&save_clsid, &iunknown, &via_unknown) == S_OK && via_unknown == api);
    CHECK(QueryApiImpl(&package_clsid, &iunknown, &via_unknown) == S_OK && via_unknown == package);
    check_com(api, save_iid, package_iid);
    check_com(package, package_iid, save_iid);

    void* queue = nullptr;
    CHECK(XTaskQueueCreate(kManual, kManual, &queue) == S_OK && queue);
    const Saves saves{api, queue};
    void* user = add_user(queue);
    check_public_dispatch(user, api, package, started);

    const std::string dir = saves_directory(kScid);
    const std::string other_dir = saves_directory("scid-b");
    files(dir, "*", true);
    files(other_dir, "*", true);

    // --- Provider ---
    Async async{queue, nullptr, nullptr, {}};
    const auto init_async = slot<InitAsyncFn>(api, kInitAsync);
    CHECK(init_async(api, nullptr, kScid, false, &async) == kInvalidHandle);
    CHECK(init_async(api, user, nullptr, false, &async) == E_INVALIDARG);
    CHECK(init_async(api, user, kScid, false, nullptr) == E_POINTER);
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
    CHECK(files(dir, "*.bxgs", false) == 0);

    // --- Write, then read back ---
    std::string progress(300, '\0');
    for (size_t i = 0; i < progress.size(); ++i) progress[i] = static_cast<char>(i * 7 + 3);
    const std::string settings = "volume=7";
    void* update = saves.update(main_container, "Main profile");
    CHECK(saves.write(update, "progress", progress.data(), progress.size()) == S_OK);
    CHECK(saves.write(update, "progress", progress.data(), progress.size()) == E_INVALIDARG);
    CHECK(saves.write(update, "", "x", 1) == E_INVALIDARG);
    CHECK(saves.write(update, std::string(257, 'n').c_str(), "x", 1) == E_INVALIDARG);  // 256-byte name limit
    CHECK(saves.write(update, "settings", settings.data(), settings.size()) == S_OK);
    {
        Async submit{queue, nullptr, nullptr, {}};
        CHECK(slot<SubmitAsyncFn>(api, kSubmitUpdateAsync)(api, update, &submit) == S_OK);
        Async twice{queue, nullptr, nullptr, {}};
        CHECK(slot<SubmitAsyncFn>(api, kSubmitUpdateAsync)(api, update, &twice) == kIllegalCall);  // pending
        CHECK(saves.write(update, "late", "x", 1) == kIllegalCall);
        saves.wait(submit);
        size_t size = 1;
        CHECK(XAsyncGetResultSize(&submit, &size) == S_OK && size == 0);
        CHECK(slot<AsyncResultFn>(api, kSubmitUpdateResult)(api, &submit) == S_OK);
    }
    saves.close(kCloseUpdate, update);
    CHECK(saves.write(update, "closed", "x", 1) == kInvalidHandle);  // CloseUpdate closes it

    auto blobs = saves.read(main_container, {});
    CHECK(blobs.size() == 2);
    CHECK(find_blob(blobs, "progress") && *find_blob(blobs, "progress") == progress);
    CHECK(find_blob(blobs, "settings") && *find_blob(blobs, "settings") == settings);
    blobs = saves.read(main_container, {"settings"});
    CHECK(blobs.size() == 1 && blobs[0].first == "settings" && blobs[0].second == settings);

    // The 16 MiB limit counts blob names: "big" (3 bytes) plus 16 MiB - 3 bytes fills it exactly.
    {
        std::vector<char> big(kMiB16);
        void* full = saves.update(main_container, nullptr);
        CHECK(saves.write(full, "big", big.data(), kMiB16 - 3 + 1) == kUpdateTooBig);
        CHECK(saves.write(full, "big", big.data(), kMiB16 - 3) == S_OK);
        CHECK(saves.write(full, "x", nullptr, 0) == kUpdateTooBig);
        saves.close(kCloseUpdate, full);
    }

    // --- A second container, then enumeration ---
    void* slot_container = saves.container(provider, "slot_1");
    saves.put(slot_container, "a", "A");
    saves.close(kCloseContainer, slot_container);
    CHECK(files(dir, "*.bxgs", false) == 2);

    const auto enumerate = slot<EnumerateFn>(api, kEnumerate);
    Listing listing = list(saves, provider);
    CHECK(listing.seen.size() == 2);
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
    Listing first_only;
    first_only.stop_after = 1;
    CHECK(enumerate(api, provider, &first_only, collect) == S_OK && first_only.seen.size() == 1);
    CHECK(enumerate(api, provider, nullptr, nullptr) == E_INVALIDARG);

    // --- Saves are separated by SCID ---
    {
        void* other = saves.provider(user, "scid-b");
        CHECK(list(saves, other).seen.empty());
        void* same_name = saves.container(other, "profiles/main");
        CHECK(saves.read(same_name, {}).empty());
        saves.put(same_name, "progress", "other scid");
        CHECK(files(other_dir, "*.bxgs", false) == 1);
        void* staged = saves.update(same_name, nullptr);
        {
            std::vector<char> big(kMiB16 - 16);
            CHECK(saves.write(staged, "staged", big.data(), big.size()) == S_OK);
        }
        const uint64_t open_bytes = private_bytes();
        saves.close(kCloseProvider, other);
        // Closing a provider closes its containers and their updates, and frees what they staged.
        CHECK(private_bytes() + 8 * (1u << 20) < open_bytes);
        CHECK(saves.read_status(same_name, {}) == kInvalidHandle);
        CHECK(saves.write(staged, "x", "x", 1) == kInvalidHandle);
        CHECK(*find_blob(saves.read(main_container, {"progress"}), "progress") == progress);
    }

    // --- Files that are damaged, foreign or unreadable ---
    {
        const auto crafted = [&](const char* name, std::string bytes) {
            write_file(container_path(dir, name), bytes);
            void* container = saves.container(provider, name);
            const auto status = saves.read_status(container, {});
            saves.close(kCloseContainer, container);
            return status;
        };
        CHECK(crafted("craft_ok", container_bytes("craft_ok", {{"a", "1"}})) == S_OK);  // the format is as documented
        CHECK(crafted("craft_tail", container_bytes("craft_tail", {{"a", "1"}}) + "!") == kCorrupt);
        CHECK(crafted("craft_dup", container_bytes("craft_dup", {{"a", "1"}, {"a", "2"}})) == kCorrupt);
        std::string truncated = container_bytes("craft_cut", {{"a", "12345"}});
        truncated.pop_back();
        CHECK(crafted("craft_cut", truncated) == kCorrupt);
        CHECK(crafted("craft_nul", container_bytes(std::string("craft_nul\0x", 11), {})) == kCorrupt);
        CHECK(crafted("craft_blobnul", container_bytes("craft_blobnul", {{std::string("a\0b", 3), "1"}})) == kCorrupt);
        CHECK(crafted("craft_badname", container_bytes("bad name", {})) == kCorrupt);
        // Enumeration lists the two real containers and the valid crafted one, skipping the rest.
        CHECK(list(saves, provider).seen.size() == 3);

        // A file another process holds exclusively is skipped, not fatal to enumeration.
        const HANDLE locked = CreateFileA(container_path(dir, "craft_ok").c_str(), GENERIC_READ, 0, nullptr,
                                          OPEN_EXISTING, FILE_ATTRIBUTE_NORMAL, nullptr);
        CHECK(locked != INVALID_HANDLE_VALUE);
        CHECK(list(saves, provider).seen.size() == 2);
        CloseHandle(locked);
        CHECK(files(dir, "*.bxgs", false) == 9);
        for (const char* name : {"craft_ok", "craft_tail", "craft_dup", "craft_cut", "craft_nul", "craft_blobnul",
                                 "craft_badname"}) {
            CHECK(DeleteFileA(container_path(dir, name).c_str()));
        }
    }

    // --- A failed replace keeps the old save; a successful one replaces the file ---
    {
        const auto file_id = [&] {
            const HANDLE file = CreateFileA(container_path(dir, "profiles/main").c_str(), 0,
                                            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE, nullptr,
                                            OPEN_EXISTING, FILE_ATTRIBUTE_NORMAL, nullptr);
            CHECK(file != INVALID_HANDLE_VALUE);
            BY_HANDLE_FILE_INFORMATION info{};
            CHECK(GetFileInformationByHandle(file, &info));
            CloseHandle(file);
            return (static_cast<uint64_t>(info.nFileIndexHigh) << 32) | info.nFileIndexLow;
        };
        const uint64_t original = file_id();
        // A reader that allows writes but not deletion blocks the rename, not an in-place write.
        const HANDLE reader = CreateFileA(container_path(dir, "profiles/main").c_str(), GENERIC_READ,
                                          FILE_SHARE_READ | FILE_SHARE_WRITE, nullptr, OPEN_EXISTING,
                                          FILE_ATTRIBUTE_NORMAL, nullptr);
        CHECK(reader != INVALID_HANDLE_VALUE);
        void* blocked = saves.update(main_container, nullptr);
        CHECK(saves.write(blocked, "progress", "BLOCKED", 7) == S_OK);
        CHECK(FAILED(saves.submit_status(blocked)));
        CloseHandle(reader);
        CHECK(*find_blob(saves.read(main_container, {"progress"}), "progress") == progress);
        CHECK(files(dir, "*.tmp", false) == 0);
        // The failed update was not consumed: submitting it again now succeeds.
        CHECK(file_id() == original);
        saves.submit(blocked);
        saves.close(kCloseUpdate, blocked);
        CHECK(*find_blob(saves.read(main_container, {"progress"}), "progress") == "BLOCKED");
        CHECK(file_id() != original);  // written to a new file and renamed over, not in place
    }

    // --- Persistence across provider close and reopen ---
    saves.close(kCloseContainer, main_container);
    saves.close(kCloseProvider, provider);
    CHECK(create_container(api, provider, "slot_1", &handle) == kInvalidHandle && !handle);
    provider = saves.provider(user);
    main_container = saves.container(provider, "profiles/main");
    blobs = saves.read(main_container, {"settings", "progress"});
    CHECK(blobs.size() == 2 && *find_blob(blobs, "progress") == "BLOCKED" && *find_blob(blobs, "settings") == settings);

    // Overwrite one blob through the process task queue; the other blob and the display name stay.
    const std::string progress2 = "lvl=9";
    {
        const Saves pooled{api, nullptr};
        pooled.put(main_container, "progress", progress2);
    }
    blobs = saves.read(main_container, {});
    CHECK(blobs.size() == 2 && *find_blob(blobs, "progress") == progress2 && *find_blob(blobs, "settings") == settings);
    for (const auto& seen : list(saves, provider).seen) {
        if (seen.name == "profiles/main") CHECK(seen.display_name == "Main profile");
    }

    // A canceled submit writes nothing, and the update can then be submitted again.
    update = saves.update(main_container, nullptr);
    CHECK(saves.write(update, "progress", "X", 1) == S_OK);
    {
        Async canceled{queue, nullptr, nullptr, {}};
        CHECK(slot<SubmitAsyncFn>(api, kSubmitUpdateAsync)(api, update, &canceled) == S_OK);
        XAsyncCancel(&canceled);
        saves.wait(canceled);
        CHECK(XAsyncGetStatus(&canceled, false) == E_ABORT);
        CHECK(slot<AsyncResultFn>(api, kSubmitUpdateResult)(api, &canceled) == E_ABORT);
    }
    CHECK(*find_blob(saves.read(main_container, {"progress"}), "progress") == progress2);
    saves.submit(update);
    saves.close(kCloseUpdate, update);
    CHECK(*find_blob(saves.read(main_container, {"progress"}), "progress") == "X");

    // A closed container handle is rejected, and so are its updates.
    void* orphan = saves.update(main_container, nullptr);
    saves.close(kCloseContainer, main_container);
    CHECK(saves.read_status(main_container, {}) == kInvalidHandle);
    CHECK(saves.write(orphan, "x", "x", 1) == kInvalidHandle);
    update = nullptr;
    CHECK(slot<CreateUpdateFn>(api, kCreateUpdate)(api, main_container, "x", &update) == kInvalidHandle && !update);

    // --- Concurrent submits to one container: threads, then processes ---
    void* shared = saves.container(provider, kShared);
    {
        constexpr int kThreads = 8, kEach = 8;
        ThreadJob jobs[kThreads];
        HANDLE threads[kThreads];
        for (int i = 0; i < kThreads; ++i) {
            jobs[i] = {api, shared, i, kEach};
            threads[i] = CreateThread(nullptr, 0, thread_writer, &jobs[i], 0, nullptr);
            CHECK(threads[i]);
        }
        CHECK(WaitForMultipleObjects(kThreads, threads, TRUE, 60000) == WAIT_OBJECT_0);
        for (HANDLE thread : threads) CloseHandle(thread);
        blobs = saves.read(shared, {});
        CHECK(blobs.size() == kThreads * kEach);
        check_blobs(blobs, 't', kThreads, kEach);
    }
    {
        constexpr int kWriters = 4, kEach = 40;
        wchar_t event_name[64];
        std::swprintf(event_name, 64, L"Local\\bx_saves_go_%lu", GetCurrentProcessId());
        const HANDLE go = CreateEventW(nullptr, TRUE, FALSE, event_name);
        CHECK(go);
        wchar_t exe[MAX_PATH];
        CHECK(GetModuleFileNameW(nullptr, exe, MAX_PATH) > 0);
        PROCESS_INFORMATION children[kWriters];
        for (int id = 0; id < kWriters; ++id) {
            wchar_t command[MAX_PATH + 128];
            std::swprintf(command, MAX_PATH + 128, L"\"%ls\" --writer %d %d %ls", exe, id, kEach, event_name);
            STARTUPINFOW startup{};
            startup.cb = sizeof(startup);
            CHECK(CreateProcessW(exe, command, nullptr, nullptr, FALSE, 0, nullptr, nullptr, &startup, &children[id]));
        }
        CHECK(SetEvent(go));
        for (auto& child : children) {
            DWORD code = 99;
            CHECK(WaitForSingleObject(child.hProcess, 120000) == WAIT_OBJECT_0);
            CHECK(GetExitCodeProcess(child.hProcess, &code) && code == 0);
            CloseHandle(child.hProcess);
            CloseHandle(child.hThread);
        }
        CloseHandle(go);
        blobs = saves.read(shared, {});
        CHECK(blobs.size() == 8 * 8 + kWriters * kEach);
        check_blobs(blobs, 'w', kWriters, kEach);
    }
    saves.close(kCloseContainer, shared);

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

    // The mount path is the executable's directory in UTF-8 (W API, so non-ASCII paths work).
    wchar_t exe_w[MAX_PATH]{};
    CHECK(GetModuleFileNameW(nullptr, exe_w, MAX_PATH) > 0);
    *std::wcsrchr(exe_w, L'\\') = L'\0';
    const std::string exe_dir = utf8(exe_w);
    void* mount = nullptr;
    CHECK(slot<MountFn>(package, kMount)(package, "BXPKG0000000000000000", &mount) == kNotFound && !mount);
    CHECK(slot<MountFn>(package, kMount)(package, id, &mount) == S_OK && mount);
    size_t path_size = 0;
    CHECK(slot<PathSizeFn>(package, kGetMountPathSize)(package, mount, &path_size) == S_OK);
    CHECK(path_size == exe_dir.size() + 1);
    std::vector<char> path(path_size);
    CHECK(slot<PathFn>(package, kGetMountPath)(package, mount, path_size - 1, path.data()) == kInsufficient);
    CHECK(slot<PathFn>(package, kGetMountPath)(package, mount, path_size, path.data()) == S_OK);
    CHECK(exe_dir == path.data());
    // Handle types do not overlap: a mount handle is not a provider handle.
    saves.close(kCloseProvider, mount);
    CHECK(list(saves, provider).seen.size() == 3);
    CHECK(slot<PathSizeFn>(package, kGetMountPathSize)(package, mount, &path_size) == S_OK);
    slot<CloseFn>(package, kCloseMount)(package, mount);
    CHECK(slot<PathSizeFn>(package, kGetMountPathSize)(package, mount, &path_size) == kInvalidHandle);

    // --- Uninitialize then reinitialize: old handles stay dead, queued work does not run ---
    {
        void* old_container = saves.container(provider, "profiles/main");
        void* old_update = saves.update(old_container, nullptr);
        CHECK(saves.write(old_update, "progress", "STALE", 5) == S_OK);
        void* old_mount = nullptr;
        CHECK(slot<MountFn>(package, kMount)(package, id, &old_mount) == S_OK);

        // Results and staged data held across the uninitialize must be released by it.
        std::vector<char> big(kMiB16 - 8);
        void* heavy = saves.update(old_container, nullptr);
        CHECK(saves.write(heavy, "heavy", big.data(), big.size()) == S_OK);
        std::vector<Async> unread(6, Async{queue, nullptr, nullptr, {}});
        void* big_container = saves.container(provider, "big");
        {
            void* filler = saves.update(big_container, nullptr);
            CHECK(saves.write(filler, "data", big.data(), big.size()) == S_OK);
            saves.submit(filler);
            saves.close(kCloseUpdate, filler);
        }
        big = {};
        const uint64_t before = private_bytes();
        for (auto& read : unread) {
            CHECK(slot<ReadAsyncFn>(api, kReadAsync)(api, big_container, nullptr, 0, &read) == S_OK);
            CHECK(XTaskQueueDispatch(queue, kWork, 0));
        }
        const uint64_t held = private_bytes();
        // Queued last on the manual queue, so the dispatches above do not run it.
        Async pending{queue, nullptr, nullptr, {}};
        CHECK(slot<SubmitAsyncFn>(api, kSubmitUpdateAsync)(api, old_update, &pending) == S_OK);

        XGameRuntimeUninitialize();
        CHECK(XGameRuntimeInitialize() == S_OK);
        const uint64_t after = private_bytes();
        std::printf("memory: before reads %llu MiB, with 6 unread results %llu MiB, after uninitialize %llu MiB\n",
                    static_cast<unsigned long long>(before >> 20), static_cast<unsigned long long>(held >> 20),
                    static_cast<unsigned long long>(after >> 20));
        // The measurement sees the six results (about 16 MiB each)...
        CHECK(held > before + 80 * (1u << 20));
        // ...and uninitialize released them and the open 16 MiB update counted in "before".
        CHECK(after + 8 * (1u << 20) < before);

        // The submit queued before uninitialize must not write now.
        CHECK(XTaskQueueDispatch(queue, kWork, 0));
        CHECK(XAsyncGetStatus(&pending, false) == kNotInitialized);
        CHECK(slot<AsyncResultFn>(api, kSubmitUpdateResult)(api, &pending) == kNotInitialized);
        CHECK(slot<CreateContainerFn>(api, kCreateContainer)(api, provider, "slot_1", &handle) == kInvalidHandle);
        CHECK(saves.read_status(old_container, {}) == kInvalidHandle);
        CHECK(saves.write(old_update, "x", "x", 1) == kInvalidHandle);
        CHECK(slot<PathSizeFn>(package, kGetMountPathSize)(package, old_mount, &path_size) == kInvalidHandle);

        void* fresh_user = add_user(queue);
        void* fresh = saves.provider(fresh_user);
        void* check = saves.container(fresh, "profiles/main");
        CHECK(*find_blob(saves.read(check, {"progress"}), "progress") == "X");
        XUserCloseHandle(fresh_user);
    }

    // --- After the last uninitialize, handles are stale ---
    XUserCloseHandle(user);
    XTaskQueueCloseHandle(queue);
    XGameRuntimeUninitialize();
    CHECK(create_container(api, provider, "slot_1", &handle) == kNotInitialized);
    CHECK(slot<GetIdFn>(package, kGetId)(package, sizeof(id), id) == kNotInitialized);

    std::printf("PASS: test_xgameruntime_saves (round trip, SCID isolation, damaged files, failed replace, "
                "64 threaded + 160 cross-process submits, uninitialize, public COM ids; package %s)\n", id);
    return 0;
}

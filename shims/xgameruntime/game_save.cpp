/*
 * Offline XGameSave over bottle-local storage (#468).
 *
 * Reached only through QueryApiImpl with the BetterXen synthetic xgamesave key; the key and
 * the vtable slot order are in synthetic_keys.h. Nothing here is exported.
 *
 * Public reference (Microsoft Learn, gdk-2610, checked 2026-11-26): the XGameSave members
 * page and the function and structure pages it links. The signatures, the
 * XGameSaveContainerInfo / XGameSaveBlobInfo / XGameSaveBlob layouts, the E_GS_* values,
 * the container-name rules and the 16 MB update limit come from there:
 * https://learn.microsoft.com/en-us/gaming/gdk/docs/reference/system/xgamesave/xgamesave_members
 * No Microsoft header, source or binary was used.
 *
 * Storage: each container is one file, <XPersistentLocalStorage path>\saves\<id>.bxgs, where
 * <id> is the FNV-1a hash of the container name. A submitted update rewrites the file through
 * a temporary file and MoveFileExW, so an update lands entirely or not at all. The format is
 * BetterXen-defined, little-endian:
 *   u32 magic "BXGS", u32 version 1, str name, str display name, i64 last-modified time_t,
 *   u32 blob count, then per blob: str name, u32 size, size bytes.   (str = u32 length + bytes)
 *
 * Offline behaviour, where the public pages leave it open:
 *  - There is no service. syncOnDemand has no effect and needsSync is always false.
 *  - The provider needs a live XUser handle and a non-null SCID; the SCID is not checked.
 *  - A second write of the same blob in one update fails with E_INVALIDARG (the page allows
 *    one modification per blob per update but names no error).
 *  - "16 MB" for GS_MAX_BLOB_SIZE is read as 16 MiB, enforced on the staged total.
 *  - GS_MAX_BLOB_NAME_SIZE has no published value, so only empty blob names are rejected.
 *  - A null display name in CreateUpdate keeps the container's current display name.
 *  - An update can be submitted once; a second submit fails with E_ILLEGAL_METHOD_CALL.
 *  - Reading named blobs of a container that was never submitted fails with
 *    E_GS_BLOB_NOT_FOUND; reading all of them returns zero blobs.
 */
#include "runtime_internal.h"
#include "synthetic_keys.h"

#include <algorithm>
#include <array>
#include <cstddef>
#include <cstdio>
#include <cstring>
#include <ctime>
#include <iterator>
#include <map>
#include <mutex>
#include <new>
#include <string>
#include <unordered_map>
#include <utility>
#include <vector>

#include "bx_log.h"

namespace bx::gdk {
namespace {

using synthetic::GameSaveSlot;

// E_GS_* values listed on the XGameSave members page.
constexpr HRESULT kInvalidContainerName = static_cast<HRESULT>(0x80830001u);
constexpr HRESULT kUpdateTooBig = static_cast<HRESULT>(0x80830005u);
constexpr HRESULT kProvidedBufferTooSmall = static_cast<HRESULT>(0x80830007u);
constexpr HRESULT kBlobNotFound = static_cast<HRESULT>(0x80830008u);

constexpr size_t kMaxUpdateBytes = 16u * 1024u * 1024u;
constexpr size_t kMaxContainerName = 256;

struct ContainerInfo {  // XGameSaveContainerInfo
    const char* name;
    const char* display_name;
    uint32_t blob_count;
    uint64_t total_size;
    time_t last_modified_time;
    bool needs_sync;
};
static_assert(sizeof(time_t) == 8);
static_assert(offsetof(ContainerInfo, blob_count) == 16 && offsetof(ContainerInfo, total_size) == 24);
static_assert(offsetof(ContainerInfo, last_modified_time) == 32 && offsetof(ContainerInfo, needs_sync) == 40);

struct BlobInfo {  // XGameSaveBlobInfo
    const char* name;
    uint32_t size;
};

struct Blob {  // XGameSaveBlob
    BlobInfo info;
    uint8_t* data;
};
static_assert(sizeof(BlobInfo) == 16 && offsetof(Blob, data) == 16 && sizeof(Blob) == 24);

using ContainerInfoCallback = bool(CALLBACK*)(const ContainerInfo*, void*);

// --- Container files ---

struct ContainerData {
    std::string name;
    std::string display_name;
    int64_t last_modified = 0;
    std::map<std::string, std::vector<uint8_t>> blobs;
};

constexpr uint32_t kMagic = 0x53475842;  // "BXGS"
constexpr uint32_t kFormatVersion = 1;
constexpr wchar_t kExtension[] = L".bxgs";

// Serializes reads and read-modify-writes of container files within this process.
std::mutex io_mutex;

void put_u32(std::vector<uint8_t>& out, uint32_t value) {
    for (int shift = 0; shift < 32; shift += 8) out.push_back(static_cast<uint8_t>(value >> shift));
}

void put_u64(std::vector<uint8_t>& out, uint64_t value) {
    put_u32(out, static_cast<uint32_t>(value));
    put_u32(out, static_cast<uint32_t>(value >> 32));
}

void put_bytes(std::vector<uint8_t>& out, const void* data, size_t size) {
    const auto* bytes = static_cast<const uint8_t*>(data);
    out.insert(out.end(), bytes, bytes + size);
}

void put_string(std::vector<uint8_t>& out, const std::string& text) {
    put_u32(out, static_cast<uint32_t>(text.size()));
    put_bytes(out, text.data(), text.size());
}

std::vector<uint8_t> serialize(const ContainerData& data) {
    std::vector<uint8_t> out;
    put_u32(out, kMagic);
    put_u32(out, kFormatVersion);
    put_string(out, data.name);
    put_string(out, data.display_name);
    put_u64(out, static_cast<uint64_t>(data.last_modified));
    put_u32(out, static_cast<uint32_t>(data.blobs.size()));
    for (const auto& [name, bytes] : data.blobs) {
        put_string(out, name);
        put_u32(out, static_cast<uint32_t>(bytes.size()));
        put_bytes(out, bytes.data(), bytes.size());
    }
    return out;
}

class Reader {
public:
    explicit Reader(const std::vector<uint8_t>& bytes) : next_(bytes.data()), left_(bytes.size()) {}

    bool u32(uint32_t& value) {
        if (left_ < 4) return false;
        value = 0;
        for (int i = 0; i < 4; ++i) value |= static_cast<uint32_t>(next_[i]) << (8 * i);
        next_ += 4;
        left_ -= 4;
        return true;
    }

    bool u64(uint64_t& value) {
        uint32_t low, high;
        if (!u32(low) || !u32(high)) return false;
        value = (static_cast<uint64_t>(high) << 32) | low;
        return true;
    }

    bool bytes(size_t size, const uint8_t*& data) {
        if (left_ < size) return false;
        data = next_;
        next_ += size;
        left_ -= size;
        return true;
    }

    bool string(std::string& text) {
        uint32_t size;
        const uint8_t* data;
        if (!u32(size) || !bytes(size, data)) return false;
        text.assign(reinterpret_cast<const char*>(data), size);
        return true;
    }

    bool done() const { return left_ == 0; }

private:
    const uint8_t* next_;
    size_t left_;
};

bool parse(const std::vector<uint8_t>& bytes, ContainerData& data) {
    Reader in(bytes);
    uint32_t magic, version, count;
    uint64_t modified;
    if (!in.u32(magic) || magic != kMagic || !in.u32(version) || version != kFormatVersion ||
        !in.string(data.name) || !in.string(data.display_name) || !in.u64(modified) || !in.u32(count)) {
        return false;
    }
    data.last_modified = static_cast<int64_t>(modified);
    for (uint32_t i = 0; i < count; ++i) {
        std::string name;
        uint32_t size;
        const uint8_t* blob;
        if (!in.string(name) || !in.u32(size) || !in.bytes(size, blob)) return false;
        if (!data.blobs.emplace(std::move(name), std::vector<uint8_t>(blob, blob + size)).second) return false;
    }
    return in.done();
}

std::wstring container_file(const std::wstring& dir, const std::string& name) {
    // Valid container names are ASCII, so widening each byte is lossless.
    const std::wstring wide(name.begin(), name.end());
    wchar_t id[17];
    std::swprintf(id, 17, L"%016llx", static_cast<unsigned long long>(fnv1a(wide.c_str())));
    return dir + L"\\" + id + kExtension;
}

HRESULT read_file(const std::wstring& path, std::vector<uint8_t>& out, bool& exists) {
    exists = false;
    const HANDLE file = CreateFileW(path.c_str(), GENERIC_READ, FILE_SHARE_READ, nullptr, OPEN_EXISTING,
                                    FILE_ATTRIBUTE_NORMAL, nullptr);
    if (file == INVALID_HANDLE_VALUE) {
        const auto error = GetLastError();
        return error == ERROR_FILE_NOT_FOUND || error == ERROR_PATH_NOT_FOUND ? S_OK : HRESULT_FROM_WIN32(error);
    }
    exists = true;
    HRESULT hr = S_OK;
    LARGE_INTEGER size{};
    if (!GetFileSizeEx(file, &size)) {
        hr = HRESULT_FROM_WIN32(GetLastError());
    } else {
        try {
            out.resize(static_cast<size_t>(size.QuadPart));
        } catch (const std::bad_alloc&) {
            hr = E_OUTOFMEMORY;
        }
        size_t done = 0;
        while (SUCCEEDED(hr) && done < out.size()) {
            const DWORD chunk = static_cast<DWORD>(std::min<size_t>(out.size() - done, 1u << 30));
            DWORD got = 0;
            if (!ReadFile(file, out.data() + done, chunk, &got, nullptr)) hr = HRESULT_FROM_WIN32(GetLastError());
            else if (got == 0) hr = HRESULT_FROM_WIN32(ERROR_HANDLE_EOF);
            done += got;
        }
    }
    CloseHandle(file);
    return hr;
}

HRESULT write_file_atomic(const std::wstring& path, const std::vector<uint8_t>& bytes) {
    const std::wstring temp = path + L"." + std::to_wstring(GetCurrentProcessId()) + L".tmp";
    const HANDLE file =
        CreateFileW(temp.c_str(), GENERIC_WRITE, 0, nullptr, CREATE_ALWAYS, FILE_ATTRIBUTE_NORMAL, nullptr);
    if (file == INVALID_HANDLE_VALUE) return HRESULT_FROM_WIN32(GetLastError());
    HRESULT hr = S_OK;
    size_t done = 0;
    while (SUCCEEDED(hr) && done < bytes.size()) {
        const DWORD chunk = static_cast<DWORD>(std::min<size_t>(bytes.size() - done, 1u << 30));
        DWORD wrote = 0;
        if (!WriteFile(file, bytes.data() + done, chunk, &wrote, nullptr)) hr = HRESULT_FROM_WIN32(GetLastError());
        done += wrote;
    }
    if (SUCCEEDED(hr) && !FlushFileBuffers(file)) hr = HRESULT_FROM_WIN32(GetLastError());
    CloseHandle(file);
    if (SUCCEEDED(hr) && !MoveFileExW(temp.c_str(), path.c_str(), MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH)) {
        hr = HRESULT_FROM_WIN32(GetLastError());
    }
    if (FAILED(hr)) DeleteFileW(temp.c_str());
    return hr;
}

// Logs the file name only: bx_log_emit does not escape, and a Windows file name cannot hold
// a backslash, quote or control character, so it is always valid JSON string content. The
// directory is the title's saves directory.
void log_bad_file(const char* event, const std::wstring& path) {
    const auto slash = path.find_last_of(L'\\');
    const std::wstring name = slash == std::wstring::npos ? path : path.substr(slash + 1);
    char detail[MAX_PATH * 3 + 8];
    const int bytes = WideCharToMultiByte(CP_UTF8, 0, name.c_str(), -1, detail, sizeof(detail), nullptr, nullptr);
    if (bytes <= 0) std::snprintf(detail, sizeof(detail), "(unprintable file name)");
    bx_log_emit(BX_LOG_WARN, "xgamesave", event, detail);
}

// Caller holds io_mutex.
HRESULT load_container(const std::wstring& dir, const std::string& name, ContainerData& data, bool& exists) {
    const auto path = container_file(dir, name);
    std::vector<uint8_t> bytes;
    auto hr = read_file(path, bytes, exists);
    if (FAILED(hr) || !exists) return hr;
    if (!parse(bytes, data)) {
        log_bad_file("corrupt_container", path);
        return HRESULT_FROM_WIN32(ERROR_FILE_CORRUPT);
    }
    if (data.name != name) {
        log_bad_file("container_id_collision", path);
        return HRESULT_FROM_WIN32(ERROR_FILE_EXISTS);
    }
    return S_OK;
}

// Caller holds io_mutex. Files that do not parse are logged and skipped.
HRESULT list_containers(const std::wstring& dir, std::vector<ContainerData>& out) {
    WIN32_FIND_DATAW found;
    const HANDLE search = FindFirstFileW((dir + L"\\*" + kExtension).c_str(), &found);
    if (search == INVALID_HANDLE_VALUE) {
        const auto error = GetLastError();
        return error == ERROR_FILE_NOT_FOUND ? S_OK : HRESULT_FROM_WIN32(error);
    }
    HRESULT hr = S_OK;
    do {
        const std::wstring file = found.cFileName;
        constexpr size_t ext = std::size(kExtension) - 1;
        if ((found.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY) || file.size() <= ext ||
            file.compare(file.size() - ext, ext, kExtension) != 0) {
            continue;
        }
        const std::wstring path = dir + L"\\" + file;
        std::vector<uint8_t> bytes;
        bool exists;
        ContainerData data;
        hr = read_file(path, bytes, exists);
        if (FAILED(hr)) break;
        if (!exists) continue;  // Replaced or removed since the search returned it.
        if (!parse(bytes, data) || container_file(dir, data.name) != path) {
            log_bad_file("corrupt_container", path);
            continue;
        }
        try {
            out.push_back(std::move(data));
        } catch (const std::bad_alloc&) {
            hr = E_OUTOFMEMORY;
        }
    } while (SUCCEEDED(hr) && FindNextFileW(search, &found));
    FindClose(search);
    return hr;
}

HRESULT saves_directory(std::wstring& dir) {
    wchar_t root[MAX_PATH];
    const auto hr = title_storage_directory(root);
    if (FAILED(hr)) return hr;
    dir = std::wstring(root) + L"\\saves";
    return ensure_directory(dir.c_str());
}

bool name_char(char c) {
    return (c >= 'A' && c <= 'Z') || (c >= 'a' && c <= 'z') || (c >= '0' && c <= '9') || c == '_';
}

// Rules from the XGameSaveCreateContainer page: an optional path part up to the last '/'
// (A-Z, a-z, 0-9, _, /) and a non-empty file part (A-Z, a-z, 0-9, _, ., -) that does not
// end in a period or contain "..". At most 256 characters.
bool valid_container_name(const char* name) {
    if (!name) return false;
    const size_t length = strnlen(name, kMaxContainerName + 1);
    if (length == 0 || length > kMaxContainerName) return false;
    const char* slash = std::strrchr(name, '/');
    const char* file = slash ? slash + 1 : name;
    for (const char* c = name; c < file; ++c) {
        if (!name_char(*c) && *c != '/') return false;
    }
    if (!*file || name[length - 1] == '.') return false;
    for (const char* c = file; *c; ++c) {
        if (!name_char(*c) && *c != '.' && *c != '-') return false;
        if (c[0] == '.' && c[1] == '.') return false;
    }
    return true;
}

// --- Handles ---
// Lock order: the runtime lock (taken by active_runtime_generation, is_valid_user and around
// consume callbacks) before handle_mutex. Nothing here calls into the runtime while holding it.

struct Provider {
    uint32_t generation;
    std::wstring dir;
};

struct Container {
    void* provider;
    std::string name;
};

struct Update {
    void* container;
    bool keep_display_name;
    std::string display_name;
    std::map<std::string, std::vector<uint8_t>> writes;
    size_t staged_bytes = 0;
    bool submitted = false;
};

std::mutex handle_mutex;
uintptr_t last_handle = 0;
std::unordered_map<void*, Provider> providers;
std::unordered_map<void*, Container> containers;
std::unordered_map<void*, Update> updates;

void* next_handle() {
    last_handle += 0x10;
    return reinterpret_cast<void*>(last_handle);
}

// The finders below require handle_mutex. A handle from before the last uninitialize, or
// one whose provider was closed, is not found.
const Provider* find_provider(void* handle, uint32_t generation) {
    const auto found = providers.find(handle);
    if (found == providers.end() || generation == 0 || found->second.generation != generation) return nullptr;
    return &found->second;
}

const Container* find_container(void* handle, uint32_t generation, const Provider** provider) {
    const auto found = containers.find(handle);
    if (found == containers.end()) return nullptr;
    *provider = find_provider(found->second.provider, generation);
    return *provider ? &found->second : nullptr;
}

Update* find_update(void* handle, uint32_t generation, const Container** container, const Provider** provider) {
    const auto found = updates.find(handle);
    if (found == updates.end()) return nullptr;
    *container = find_container(found->second.container, generation, provider);
    return *container ? &found->second : nullptr;
}

template <typename Map, typename Value>
HRESULT insert_handle(Map& map, Value&& value, void** out) {
    void* handle = next_handle();
    try {
        map.emplace(handle, std::forward<Value>(value));
    } catch (const std::bad_alloc&) {
        return E_OUTOFMEMORY;
    }
    *out = handle;
    return S_OK;
}

// --- Async operations ---

enum : uint32_t { kInitProviderOp = 0x58475301, kSubmitUpdateOp = 0x58475302, kReadBlobsOp = 0x58475303 };

class InitProviderOp final : public AsyncOp {
public:
    uint32_t generation = 0;
    std::wstring dir;

    uint32_t kind() const override { return kInitProviderOp; }
    HRESULT execute() override { return saves_directory(dir); }
    size_t result_size() const override { return sizeof(void*); }
};

class SubmitUpdateOp final : public AsyncOp {
public:
    std::wstring dir;
    std::string name;
    bool keep_display_name = false;
    std::string display_name;
    std::map<std::string, std::vector<uint8_t>> writes;

    uint32_t kind() const override { return kSubmitUpdateOp; }
    size_t result_size() const override { return 0; }

    HRESULT execute() override try {
        std::lock_guard lock(io_mutex);
        ContainerData data;
        bool exists = false;
        auto hr = load_container(dir, name, data, exists);
        if (FAILED(hr)) return hr;
        if (!exists) data.name = name;
        if (!keep_display_name) data.display_name = display_name;
        for (auto& [blob, bytes] : writes) data.blobs[blob] = std::move(bytes);
        data.last_modified = static_cast<int64_t>(std::time(nullptr));
        return write_file_atomic(container_file(dir, name), serialize(data));
    } catch (const std::bad_alloc&) {
        return E_OUTOFMEMORY;
    }
};

class ReadBlobsOp final : public AsyncOp {
public:
    std::wstring dir;
    std::string name;
    std::vector<std::string> wanted;  // Empty: every blob.
    std::vector<std::pair<std::string, std::vector<uint8_t>>> found;
    size_t bytes = 0;

    uint32_t kind() const override { return kReadBlobsOp; }
    size_t result_size() const override { return bytes; }

    HRESULT execute() override try {
        std::lock_guard lock(io_mutex);
        ContainerData data;
        bool exists = false;
        const auto hr = load_container(dir, name, data, exists);
        if (FAILED(hr)) return hr;
        if (wanted.empty()) {
            for (auto& [blob, content] : data.blobs) found.emplace_back(blob, std::move(content));
        } else {
            for (const auto& blob : wanted) {
                const auto match = data.blobs.find(blob);
                if (match == data.blobs.end()) return kBlobNotFound;
                found.emplace_back(blob, std::move(match->second));
            }
        }
        bytes = found.size() * sizeof(Blob);
        for (const auto& [blob, content] : found) bytes += blob.size() + 1 + content.size();
        return S_OK;
    } catch (const std::bad_alloc&) {
        return E_OUTOFMEMORY;
    }

    // Layout in the caller's buffer: the Blob array, then all data, then the names.
    void copy_to(Blob* out) const {
        auto* cursor = reinterpret_cast<uint8_t*>(out) + found.size() * sizeof(Blob);
        for (size_t i = 0; i < found.size(); ++i) {
            const auto& content = found[i].second;
            out[i].info.size = static_cast<uint32_t>(content.size());
            out[i].data = cursor;
            if (!content.empty()) std::memcpy(cursor, content.data(), content.size());
            cursor += content.size();
        }
        for (size_t i = 0; i < found.size(); ++i) {
            const auto& blob = found[i].first;
            std::memcpy(cursor, blob.c_str(), blob.size() + 1);
            out[i].info.name = reinterpret_cast<const char*>(cursor);
            cursor += blob.size() + 1;
        }
    }
};

HRESULT stale_handle_error(uint32_t generation) {
    return generation ? kRuntimeInvalidHandle : kRuntimeNotInitialized;
}

// --- Slots ---

HRESULT WINAPI initialize_provider_async(void*, void* user, const char* scid, bool /*sync_on_demand*/,
                                         void* async) {
    if (!async) return E_POINTER;
    if (!scid) return E_INVALIDARG;
    const auto generation = active_runtime_generation();
    if (!is_valid_user(user)) return stale_handle_error(generation);
    std::unique_ptr<InitProviderOp> op(new (std::nothrow) InitProviderOp);
    if (!op) return E_OUTOFMEMORY;
    op->generation = generation;
    return begin_async_op(async, std::move(op));
}

HRESULT WINAPI initialize_provider_result(void*, void* async, void** provider) {
    if (!provider) return E_POINTER;
    *provider = nullptr;
    return consume_async_op(async, kInitProviderOp, [&](AsyncOp& base) {
        auto& op = static_cast<InitProviderOp&>(base);
        std::lock_guard lock(handle_mutex);
        return insert_handle(providers, Provider{op.generation, op.dir}, provider);
    });
}

void WINAPI close_provider(void*, void* provider) {
    std::lock_guard lock(handle_mutex);
    providers.erase(provider);
}

HRESULT WINAPI create_container(void*, void* provider, const char* name, void** out) {
    if (!out) return E_POINTER;
    *out = nullptr;
    const auto generation = active_runtime_generation();
    std::lock_guard lock(handle_mutex);
    if (!find_provider(provider, generation)) return stale_handle_error(generation);
    if (!valid_container_name(name)) return kInvalidContainerName;
    try {
        return insert_handle(containers, Container{provider, name}, out);
    } catch (const std::bad_alloc&) {
        return E_OUTOFMEMORY;
    }
}

void WINAPI close_container(void*, void* container) {
    std::lock_guard lock(handle_mutex);
    containers.erase(container);
}

HRESULT WINAPI create_update(void*, void* container, const char* display_name, void** out) {
    if (!out) return E_POINTER;
    *out = nullptr;
    const auto generation = active_runtime_generation();
    std::lock_guard lock(handle_mutex);
    const Provider* provider = nullptr;
    if (!find_container(container, generation, &provider)) return stale_handle_error(generation);
    try {
        Update staged;
        staged.container = container;
        staged.keep_display_name = display_name == nullptr;
        staged.display_name = display_name ? display_name : "";
        return insert_handle(updates, std::move(staged), out);
    } catch (const std::bad_alloc&) {
        return E_OUTOFMEMORY;
    }
}

void WINAPI close_update(void*, void* update) {
    std::lock_guard lock(handle_mutex);
    updates.erase(update);
}

HRESULT WINAPI submit_blob_write(void*, void* update, const char* blob_name, const uint8_t* data, size_t size) {
    if (!blob_name || !*blob_name) return E_INVALIDARG;
    if (!data && size) return E_POINTER;
    const auto generation = active_runtime_generation();
    std::lock_guard lock(handle_mutex);
    const Container* container = nullptr;
    const Provider* provider = nullptr;
    auto* staged = find_update(update, generation, &container, &provider);
    if (!staged) return stale_handle_error(generation);
    if (staged->submitted) return kIllegalMethodCall;
    if (size > kMaxUpdateBytes - staged->staged_bytes) return kUpdateTooBig;
    try {
        if (!staged->writes.emplace(blob_name, std::vector<uint8_t>(data, data + size)).second) return E_INVALIDARG;
    } catch (const std::bad_alloc&) {
        return E_OUTOFMEMORY;
    }
    staged->staged_bytes += size;
    return S_OK;
}

HRESULT WINAPI submit_update_async(void*, void* update, void* async) {
    if (!async) return E_POINTER;
    std::unique_ptr<SubmitUpdateOp> op(new (std::nothrow) SubmitUpdateOp);
    if (!op) return E_OUTOFMEMORY;
    const auto generation = active_runtime_generation();
    {
        std::lock_guard lock(handle_mutex);
        const Container* container = nullptr;
        const Provider* provider = nullptr;
        auto* staged = find_update(update, generation, &container, &provider);
        if (!staged) return stale_handle_error(generation);
        if (staged->submitted) return kIllegalMethodCall;
        try {
            op->dir = provider->dir;
            op->name = container->name;
            op->keep_display_name = staged->keep_display_name;
            op->display_name = staged->display_name;
            op->writes = staged->writes;
        } catch (const std::bad_alloc&) {
            return E_OUTOFMEMORY;
        }
        staged->submitted = true;
    }
    const auto hr = begin_async_op(async, std::move(op));
    if (FAILED(hr)) {
        std::lock_guard lock(handle_mutex);
        const auto found = updates.find(update);
        if (found != updates.end()) found->second.submitted = false;
    }
    return hr;
}

HRESULT WINAPI submit_update_result(void*, void* async) {
    return consume_async_op(async, kSubmitUpdateOp, [](AsyncOp&) { return S_OK; });
}

HRESULT WINAPI read_blob_data_async(void*, void* container, const char** names, uint32_t count, void* async) {
    if (!async) return E_POINTER;
    if (count && !names) return E_INVALIDARG;
    std::unique_ptr<ReadBlobsOp> op(new (std::nothrow) ReadBlobsOp);
    if (!op) return E_OUTOFMEMORY;
    try {
        for (uint32_t i = 0; i < count; ++i) {
            if (!names[i] || !*names[i]) return E_INVALIDARG;
            if (std::find(op->wanted.begin(), op->wanted.end(), names[i]) != op->wanted.end()) return E_INVALIDARG;
            op->wanted.emplace_back(names[i]);
        }
        const auto generation = active_runtime_generation();
        std::lock_guard lock(handle_mutex);
        const Provider* provider = nullptr;
        const auto* open = find_container(container, generation, &provider);
        if (!open) return stale_handle_error(generation);
        op->dir = provider->dir;
        op->name = open->name;
    } catch (const std::bad_alloc&) {
        return E_OUTOFMEMORY;
    }
    return begin_async_op(async, std::move(op));
}

HRESULT WINAPI read_blob_data_result(void*, void* async, size_t size, Blob* blobs, uint32_t* count) {
    if (!count) return E_POINTER;
    *count = 0;
    return consume_async_op(async, kReadBlobsOp, [&](AsyncOp& base) {
        const auto& op = static_cast<const ReadBlobsOp&>(base);
        if (size < op.bytes) return kProvidedBufferTooSmall;
        if (op.bytes && !blobs) return E_POINTER;
        op.copy_to(blobs);
        *count = static_cast<uint32_t>(op.found.size());
        return S_OK;
    });
}

HRESULT WINAPI enumerate_container_info(void*, void* provider, void* context, ContainerInfoCallback callback) {
    if (!callback) return E_INVALIDARG;
    const auto generation = active_runtime_generation();
    std::vector<ContainerData> list;
    try {
        std::wstring dir;
        {
            std::lock_guard lock(handle_mutex);
            const auto* open = find_provider(provider, generation);
            if (!open) return stale_handle_error(generation);
            dir = open->dir;
        }
        std::lock_guard lock(io_mutex);
        const auto hr = list_containers(dir, list);
        if (FAILED(hr)) return hr;
    } catch (const std::bad_alloc&) {
        return E_OUTOFMEMORY;
    }
    std::sort(list.begin(), list.end(), [](const auto& a, const auto& b) { return a.name < b.name; });
    // No lock is held here, so the callback may call back into XGameSave.
    for (const auto& data : list) {
        uint64_t total = 0;
        for (const auto& [blob, bytes] : data.blobs) total += bytes.size();
        const ContainerInfo info{data.name.c_str(), data.display_name.c_str(),
                                 static_cast<uint32_t>(data.blobs.size()), total,
                                 static_cast<time_t>(data.last_modified), false};
        if (!callback(&info, context)) break;
    }
    return S_OK;
}

// --- COM object ---

HRESULT WINAPI query_interface(void* self, const GUID* iid, void** out) {
    if (!out) return E_POINTER;
    *out = nullptr;
    if (!iid) return E_INVALIDARG;
    constexpr GUID iunknown = {0x00000000, 0x0000, 0x0000, {0xC0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x46}};
    constexpr GUID own = synthetic::key(synthetic::kGameSave.iid);
    if (IsEqualGUID(*iid, iunknown) || IsEqualGUID(*iid, own)) {
        *out = self;
        return S_OK;
    }
    return E_NOINTERFACE;
}

// A process-lifetime singleton: reference counting has nothing to free.
ULONG WINAPI add_ref(void*) { return 1; }
ULONG WINAPI release(void*) { return 1; }

template <typename Fn>
const void* slot_fn(Fn fn) {
    return reinterpret_cast<const void*>(fn);
}

constexpr size_t at(GameSaveSlot slot) { return static_cast<size_t>(slot); }

const std::array<const void*, at(GameSaveSlot::Count)> kVtbl = [] {
    std::array<const void*, at(GameSaveSlot::Count)> v{};
    v[at(GameSaveSlot::QueryInterface)] = slot_fn(&query_interface);
    v[at(GameSaveSlot::AddRef)] = slot_fn(&add_ref);
    v[at(GameSaveSlot::Release)] = slot_fn(&release);
    v[at(GameSaveSlot::InitializeProviderAsync)] = slot_fn(&initialize_provider_async);
    v[at(GameSaveSlot::InitializeProviderResult)] = slot_fn(&initialize_provider_result);
    v[at(GameSaveSlot::CloseProvider)] = slot_fn(&close_provider);
    v[at(GameSaveSlot::CreateContainer)] = slot_fn(&create_container);
    v[at(GameSaveSlot::CloseContainer)] = slot_fn(&close_container);
    v[at(GameSaveSlot::CreateUpdate)] = slot_fn(&create_update);
    v[at(GameSaveSlot::CloseUpdate)] = slot_fn(&close_update);
    v[at(GameSaveSlot::SubmitBlobWrite)] = slot_fn(&submit_blob_write);
    v[at(GameSaveSlot::SubmitUpdateAsync)] = slot_fn(&submit_update_async);
    v[at(GameSaveSlot::SubmitUpdateResult)] = slot_fn(&submit_update_result);
    v[at(GameSaveSlot::ReadBlobDataAsync)] = slot_fn(&read_blob_data_async);
    v[at(GameSaveSlot::ReadBlobDataResult)] = slot_fn(&read_blob_data_result);
    v[at(GameSaveSlot::EnumerateContainerInfo)] = slot_fn(&enumerate_container_info);
    return v;
}();

struct ComObject {
    const void* const* vtbl;
};
const ComObject kObject{kVtbl.data()};

} // namespace

const void* const game_save_object = &kObject;

} // namespace bx::gdk

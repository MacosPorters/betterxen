/*
 * Offline XPackage identity and mount queries (#468).
 *
 * Reached through QueryApiImpl. The BetterXen synthetic key and its vtable order are in
 * synthetic_keys.h. Public class and interface ids use the const vtable in com_dispatch.cpp.
 * Nothing here is exported.
 *
 * Public reference (Microsoft Learn, gdk-2610, checked 2026-11-26): the XPackage members
 * page, the pages for the functions below, and XPackageKind (enum class : uint32_t
 * { Game, Content, PublisherContent }, so Game is 0):
 * https://learn.microsoft.com/en-us/gaming/gdk/docs/reference/system/xpackage/xpackage_members
 * No Microsoft header, source or binary was used.
 *
 * Offline behaviour: the running executable's directory stands in for the installed package.
 *  - The identifier is "BXPKG" + 16 hex digits of the FNV-1a hash of that directory
 *    (lowercased), so it is stable for a directory and differs between directories.
 *    XPACKAGE_IDENTIFIER_MAX_LENGTH has no published value; this identifier needs 22 bytes.
 *  - GetPackageKind reports Game for that identifier and ERROR_NOT_FOUND for any other.
 *  - Mount (the documented, deprecated XPackageMount) and its replacement
 *    XPackageMountWithUiAsync / XPackageMountWithUiResult accept only that identifier, and the
 *    mount path is the executable's directory. On Windows only content packages can be
 *    mounted; BetterXen allows the game's own package so titles can locate their files. Offline
 *    there is no license or parental-consent prompt, so the asynchronous form shows no UI; an
 *    identifier mismatch is reported as the operation's status (ERROR_NOT_FOUND).
 *  - The last XGameRuntimeUninitialize closes every mount handle.
 *  - An executable path of MAX_PATH or more fails with ERROR_FILENAME_EXCED_RANGE.
 */
#include "runtime_internal.h"
#include "synthetic_keys.h"

#include <array>
#include <cstdio>
#include <cstring>
#include <cwchar>
#include <memory>
#include <mutex>
#include <new>
#include <string>
#include <unordered_map>

namespace bx::gdk {
namespace {

using synthetic::PackageSlot;

constexpr uint32_t kPackageKindGame = 0;
constexpr HRESULT kNotFound = HRESULT_FROM_WIN32(ERROR_NOT_FOUND);
constexpr char kIdPrefix[] = "BXPKG";
constexpr size_t kIdBytes = sizeof(kIdPrefix) - 1 + 16 + 1;

HRESULT executable_directory(std::wstring& dir) {
    wchar_t path[MAX_PATH];
    const auto hr = executable_path(path);
    if (FAILED(hr)) return hr;
    wchar_t* slash = std::wcsrchr(path, L'\\');
    if (!slash) return HRESULT_FROM_WIN32(ERROR_BAD_PATHNAME);
    // Keep the separator of a drive root ("C:\").
    slash[slash == path + 2 && path[1] == L':' ? 1 : 0] = L'\0';
    dir = path;
    return S_OK;
}

HRESULT current_identifier(char (&id)[kIdBytes]) {
    std::wstring dir;
    auto hr = executable_directory(dir);
    if (FAILED(hr)) return hr;
    wchar_t lower[MAX_PATH];
    if (!LCMapStringEx(LOCALE_NAME_INVARIANT, LCMAP_LOWERCASE, dir.c_str(), -1, lower, MAX_PATH, nullptr, nullptr, 0)) {
        return HRESULT_FROM_WIN32(GetLastError());
    }
    std::snprintf(id, kIdBytes, "%s%016llx", kIdPrefix, static_cast<unsigned long long>(fnv1a(lower)));
    return S_OK;
}

HRESULT is_current_package(const char* identifier, bool& match) {
    char id[kIdBytes];
    const auto hr = current_identifier(id);
    match = SUCCEEDED(hr) && std::strcmp(identifier, id) == 0;
    return hr;
}

struct Mount {
    uint32_t generation;
    std::string path;  // UTF-8
};

std::mutex mount_mutex;
uintptr_t handle_counter = 0;
std::unordered_map<void*, Mount> mounts;

// Mount handles use tag 4 in bits 40-43, a range no XGameSave handle uses (game_save.cpp).
constexpr uintptr_t kMountTag = 4;

// Caller holds mount_mutex.
const Mount* find_mount(void* handle, uint32_t generation) {
    const auto found = mounts.find(handle);
    if (found == mounts.end() || generation == 0 || found->second.generation != generation) return nullptr;
    return &found->second;
}

HRESULT stale_handle_error(uint32_t generation) {
    return generation ? kRuntimeInvalidHandle : kRuntimeNotInitialized;
}

// Checks that identifier names this process's package and returns its mount path in UTF-8.
HRESULT resolve_mount(const char* identifier, std::string& path) {
    bool match = false;
    auto hr = is_current_package(identifier, match);
    if (FAILED(hr)) return hr;
    if (!match) return kNotFound;
    std::wstring dir;
    hr = executable_directory(dir);
    if (FAILED(hr)) return hr;
    const int bytes = WideCharToMultiByte(CP_UTF8, WC_ERR_INVALID_CHARS, dir.c_str(), -1, nullptr, 0, nullptr, nullptr);
    if (bytes <= 0) return HRESULT_FROM_WIN32(GetLastError());
    try {
        path.assign(static_cast<size_t>(bytes), '\0');
        WideCharToMultiByte(CP_UTF8, WC_ERR_INVALID_CHARS, dir.c_str(), -1, path.data(), bytes, nullptr, nullptr);
        path.pop_back();  // The terminator is added back on copy-out.
    } catch (const std::bad_alloc&) {
        return E_OUTOFMEMORY;
    }
    return S_OK;
}

HRESULT insert_mount(uint32_t generation, std::string path, void** out) {
    try {
        std::lock_guard lock(mount_mutex);
        ++handle_counter;
        void* handle = reinterpret_cast<void*>(kMountTag << 40 | (handle_counter & 0xFFFFFFFFFu) << 4);
        mounts.emplace(handle, Mount{generation, std::move(path)});
        *out = handle;
    } catch (const std::bad_alloc&) {
        return E_OUTOFMEMORY;
    }
    return S_OK;
}

constexpr uint32_t kMountWithUiOp = 0x58504B01;

// XPackageMountWithUiAsync: the identity check runs on the work port; the Result call then
// creates the mount handle, as XPackageMount does synchronously.
class MountWithUiOp final : public AsyncOp {
public:
    uint32_t generation = 0;
    std::string identifier;
    std::string path;

    uint32_t kind() const override { return kMountWithUiOp; }
    HRESULT execute() override { return resolve_mount(identifier.c_str(), path); }
    size_t result_size() const override { return sizeof(void*); }
};

}  // anonymous helpers

// Slot functions declared in runtime_internal.h. The const public vtable takes their addresses
// at static initialization.

bool WINAPI is_packaged_process(void*) {
    if (!active_runtime_generation()) return false;
    char id[kIdBytes];
    return SUCCEEDED(current_identifier(id));
}

HRESULT WINAPI get_current_process_package_identifier(void*, size_t size, char* buffer) {
    if (!buffer) return E_POINTER;
    if (!active_runtime_generation()) return kRuntimeNotInitialized;
    char id[kIdBytes];
    const auto hr = current_identifier(id);
    if (FAILED(hr)) return hr;
    if (size < kIdBytes) return kInsufficientBuffer;
    std::memcpy(buffer, id, kIdBytes);
    return S_OK;
}

HRESULT WINAPI get_package_kind(void*, const char* identifier, uint32_t* kind) {
    if (!kind) return E_POINTER;
    if (!identifier) return E_INVALIDARG;
    if (!active_runtime_generation()) return kRuntimeNotInitialized;
    bool match = false;
    const auto hr = is_current_package(identifier, match);
    if (FAILED(hr)) return hr;
    if (!match) return kNotFound;
    *kind = kPackageKindGame;
    return S_OK;
}

HRESULT WINAPI mount(void*, const char* identifier, void** out) {
    if (!out) return E_POINTER;
    *out = nullptr;
    if (!identifier) return E_INVALIDARG;
    const auto generation = active_runtime_generation();
    if (!generation) return kRuntimeNotInitialized;
    std::string path;
    const auto hr = resolve_mount(identifier, path);
    return FAILED(hr) ? hr : insert_mount(generation, std::move(path), out);
}

HRESULT WINAPI mount_with_ui_async(void*, const char* identifier, void* async) {
    if (!async) return E_POINTER;
    if (!identifier) return E_INVALIDARG;
    const auto generation = active_runtime_generation();
    if (!generation) return kRuntimeNotInitialized;
    std::unique_ptr<MountWithUiOp> op(new (std::nothrow) MountWithUiOp);
    if (!op) return E_OUTOFMEMORY;
    try {
        op->identifier = identifier;
    } catch (const std::bad_alloc&) {
        return E_OUTOFMEMORY;
    }
    op->generation = generation;
    return begin_async_op(async, std::move(op));
}

HRESULT WINAPI mount_with_ui_result(void*, void* async, void** out) {
    if (!out) return E_POINTER;
    *out = nullptr;
    return consume_async_op(async, kMountWithUiOp, [&](AsyncOp& base) {
        auto& op = static_cast<MountWithUiOp&>(base);
        return insert_mount(op.generation, op.path, out);
    });
}

HRESULT WINAPI get_mount_path_size(void*, void* handle, size_t* size) {
    if (!size) return E_POINTER;
    *size = 0;
    const auto generation = active_runtime_generation();
    std::lock_guard lock(mount_mutex);
    const auto* open = find_mount(handle, generation);
    if (!open) return stale_handle_error(generation);
    *size = open->path.size() + 1;
    return S_OK;
}

HRESULT WINAPI get_mount_path(void*, void* handle, size_t size, char* path) {
    if (!path) return E_POINTER;
    const auto generation = active_runtime_generation();
    std::lock_guard lock(mount_mutex);
    const auto* open = find_mount(handle, generation);
    if (!open) return stale_handle_error(generation);
    if (size < open->path.size() + 1) return kInsufficientBuffer;
    std::memcpy(path, open->path.c_str(), open->path.size() + 1);
    return S_OK;
}

void WINAPI close_mount_handle(void*, void* handle) {
    std::lock_guard lock(mount_mutex);
    mounts.erase(handle);
}

namespace {

// --- COM object ---

HRESULT WINAPI query_interface(void* self, const GUID* iid, void** out) {
    if (!out) return E_POINTER;
    *out = nullptr;
    if (!iid) return E_INVALIDARG;
    constexpr GUID own = synthetic::key(synthetic::kPackage.iid);
    if (IsEqualGUID(*iid, kIUnknownIid) || IsEqualGUID(*iid, own)) {
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

constexpr size_t at(PackageSlot slot) { return static_cast<size_t>(slot); }

const std::array<const void*, at(PackageSlot::Count)> kVtbl = [] {
    std::array<const void*, at(PackageSlot::Count)> v{};
    v[at(PackageSlot::QueryInterface)] = slot_fn(&query_interface);
    v[at(PackageSlot::AddRef)] = slot_fn(&add_ref);
    v[at(PackageSlot::Release)] = slot_fn(&release);
    v[at(PackageSlot::GetCurrentProcessPackageIdentifier)] = slot_fn(&get_current_process_package_identifier);
    v[at(PackageSlot::GetPackageKind)] = slot_fn(&get_package_kind);
    v[at(PackageSlot::Mount)] = slot_fn(&mount);
    v[at(PackageSlot::GetMountPathSize)] = slot_fn(&get_mount_path_size);
    v[at(PackageSlot::GetMountPath)] = slot_fn(&get_mount_path);
    v[at(PackageSlot::CloseMountHandle)] = slot_fn(&close_mount_handle);
    return v;
}();

struct ComObject {
    const void* const* vtbl;
};
const ComObject kObject{kVtbl.data()};

} // namespace

const void* const package_object = &kObject;

void release_package_state() {
    std::lock_guard lock(mount_mutex);
    mounts.clear();
}

} // namespace bx::gdk

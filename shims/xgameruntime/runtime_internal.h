#pragma once

// Internal interfaces shared by the xgameruntime.dll translation units. Nothing here is exported.

#include <windows.h>

#include <cstddef>
#include <cstdint>
#include <functional>
#include <memory>

namespace bx::gdk {

// ABI values checked against Microsoft's public PC GDK 2510.5.6322 (see xgameruntime.cpp).
constexpr HRESULT kRuntimeNotInitialized = static_cast<HRESULT>(0x89240100u);
constexpr HRESULT kRuntimeInvalidHandle = static_cast<HRESULT>(0x8924010cu);
constexpr HRESULT kIllegalMethodCall = static_cast<HRESULT>(0x8000000eu);
constexpr HRESULT kInsufficientBuffer = HRESULT_FROM_WIN32(ERROR_INSUFFICIENT_BUFFER);

// Work for an XAsyncBlock, run on the block's work port. execute() is skipped, and the block
// completes with E_GAMERUNTIME_NOT_INITIALIZED (0x89240100), if the runtime was uninitialized
// after the operation started; an execute() that is already running finishes. A finished
// object, including its result data, is kept until its Result call consumes it, the same
// block starts another operation, or the last uninitialize. A caller that frees a block
// without calling Result leaves that result in memory until then; the retention is not
// bounded.
class AsyncOp {
public:
    virtual ~AsyncOp() = default;
    // Identifies the family/function, so a Result call can reject a foreign block.
    virtual uint32_t kind() const = 0;
    // Runs without the runtime lock held. Not called if the operation was canceled.
    virtual HRESULT execute() = 0;
    // Value reported by XAsyncGetResultSize after execute() succeeded.
    virtual size_t result_size() const = 0;
};

// Starts op on block (an XAsyncBlock*). Returns the submission status, as XUserAddAsync does.
HRESULT begin_async_op(void* block, std::unique_ptr<AsyncOp> op);

// Checks that block holds a finished op of this kind, then calls consume while the runtime
// lock is held. The result is marked consumed only when consume succeeds, so a caller can
// retry with a larger buffer. consume must not call back into the runtime.
HRESULT consume_async_op(void* block, uint32_t kind, const std::function<HRESULT(AsyncOp&)>& consume);

// Zero while the runtime is not initialized; otherwise changes on every first initialize,
// so handles created before an uninitialize can be recognized as stale.
uint32_t active_runtime_generation();

bool is_valid_user(void* user);

// Full Windows path of the running executable.
HRESULT executable_path(wchar_t (&path)[MAX_PATH]);

// The executable's C:\bx_pls\<hash> directory (XPersistentLocalStorageGetPath), created if needed.
HRESULT title_storage_directory(wchar_t (&path)[MAX_PATH]);

// Creates path if missing. Fails if it exists and is not a plain directory.
HRESULT ensure_directory(const wchar_t* path);

// FNV-1a over UTF-16 code units, as used for the storage directory name.
uint64_t fnv1a(const wchar_t* text);

constexpr GUID kIUnknownIid = {0x00000000, 0x0000, 0x0000, {0xC0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x46}};

// COM IUnknown objects QueryApiImpl returns for the synthetic xgamesave and xpackage keys.
extern const void* const game_save_object;  // game_save.cpp
extern const void* const package_object;    // package.cpp

// Close every XGameSave / XPackage handle. Called by the last uninitialize, with the runtime
// lock held (the runtime lock is always taken before these files' own locks).
void release_game_save_state();  // game_save.cpp
void release_package_state();    // package.cpp

} // namespace bx::gdk

#ifndef BETTERXEN_SYNTHETIC_STORAGE_H
#define BETTERXEN_SYNTHETIC_STORAGE_H

#include <windows.h>
#include <d3d12.h>
#include <unknwn.h>

// BetterXen-only COM contract for original conformance guests. The Xbox
// DirectStorage interface IDs and binary layouts are independent of these types.
namespace bx::storage {
inline constexpr IID factory_iid{0xaad71506, 0x7beb, 0x49fc, {0x94, 0x1d, 0x46, 0x58, 0x75, 0xed, 0x35, 0x00}};
inline constexpr IID file_iid{0x723738ce, 0x6772, 0x47dc, {0x95, 0x47, 0x27, 0x7d, 0x67, 0x22, 0xb0, 0xc7}};
inline constexpr IID queue_iid{0xd895469a, 0xd810, 0x4dea, {0xb4, 0x48, 0xf4, 0x42, 0x5e, 0x52, 0x9e, 0x69}};
inline constexpr IID status_iid{0xfb214629, 0xa3ae, 0x4a81, {0x9c, 0xe3, 0xe8, 0x42, 0x6b, 0x08, 0x62, 0xd7}};

struct File : IUnknown {
    virtual HRESULT STDMETHODCALLTYPE Close() = 0;
    virtual HRESULT STDMETHODCALLTYPE GetFileInformation(BY_HANDLE_FILE_INFORMATION*) = 0;
    virtual HRESULT STDMETHODCALLTYPE GetHandle(HANDLE*) = 0;
};

// Status entries are queue-position scoped: each slot reports the first error
// among entries submitted before it since the previous status entry, and is
// not bound to a specific request handle.
struct Status : IUnknown {
    virtual HRESULT STDMETHODCALLTYPE GetResult(UINT index) = 0;
};

// Requests keep source and destination buffers borrowed until completion.
// File requests retain a duplicate of the file handle when enqueued.
// Exactly one of `file` or `source` may be non-null; setting both returns E_INVALIDARG.
struct Request {
    File* file;
    UINT64 file_offset;
    const void* source;
    void* destination;
    DWORD size;
    UINT compression;
};

struct ErrorRecord {
    HRESULT first_error;
    UINT64 failure_count;
};

struct QueueInfo {
    UINT capacity;
    UINT available;
};

struct Queue : IUnknown {
    virtual HRESULT STDMETHODCALLTYPE EnqueueRequest(const Request*) = 0;
    virtual HRESULT STDMETHODCALLTYPE EnqueueStatus(Status*, UINT index) = 0;
    virtual HRESULT STDMETHODCALLTYPE EnqueueSignal(ID3D12Fence*, UINT64 value) = 0;
    virtual HRESULT STDMETHODCALLTYPE EnqueueSetEvent(HANDLE) = 0;
    virtual HRESULT STDMETHODCALLTYPE Submit() = 0;
    virtual HRESULT STDMETHODCALLTYPE RetrieveErrorRecord(ErrorRecord*) = 0;
    virtual HRESULT STDMETHODCALLTYPE Query(QueueInfo*) = 0;
    virtual HRESULT STDMETHODCALLTYPE Close() = 0;
};

struct Factory : IUnknown {
    virtual HRESULT STDMETHODCALLTYPE OpenFile(PCWSTR, REFIID, void**) = 0;
    virtual HRESULT STDMETHODCALLTYPE CreateQueue(UINT capacity, REFIID, void**) = 0;
    virtual HRESULT STDMETHODCALLTYPE CreateStatusArray(UINT count, REFIID, void**) = 0;
    // 0 succeeds as a no-op; non-zero sizes log and return E_NOTIMPL.
    virtual HRESULT STDMETHODCALLTYPE SetStagingBufferSize(UINT size) = 0;
};
} // namespace bx::storage

extern "C" HRESULT WINAPI DStorageGetFactory(REFIID, void**);

#endif

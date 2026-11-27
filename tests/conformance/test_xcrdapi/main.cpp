#include "synthetic_storage.h"

// Public PC DirectStorage contract (SDK 1.4.0, MIT per file), vendored under shims/xcrdapi/dstorage.
#include "dstorage_mingw.h"

#include <array>
#include <cstdio>
#include <cstring>
#include <limits>

extern "C" __declspec(dllimport) HRESULT WINAPI D3D12XboxCreateDevice(PVOID params, REFIID riid, void** ppv);

using namespace bx::storage;

#define CHECK(condition)                                                        \
    do {                                                                        \
        if (!(condition)) {                                                     \
            std::fprintf(stderr, "xcrdapi:%d: %s (Win32 error %lu)\n",            \
                         __LINE__, #condition, static_cast<unsigned long>(GetLastError())); \
            return 1;                                                           \
        }                                                                       \
    } while (false)

template<class Interface>
struct Com {
    Interface* value = nullptr;

    ~Com() { if (value) value->Release(); }
    Interface* operator->() const { return value; }
    void** out() { return reinterpret_cast<void**>(&value); }
};

struct TemporaryFile {
    WCHAR name[MAX_PATH]{};
    ~TemporaryFile() { if (*name) DeleteFileW(name); }
};

struct Event {
    HANDLE handle = CreateEventW(nullptr, FALSE, FALSE, nullptr);
    ~Event() { if (handle) CloseHandle(handle); }
};

struct ForeignStatus final : Status {
    HRESULT STDMETHODCALLTYPE QueryInterface(REFIID, void**) override { return E_NOINTERFACE; }
    ULONG STDMETHODCALLTYPE AddRef() override { return 1; }
    ULONG STDMETHODCALLTYPE Release() override { return 1; }
    HRESULT STDMETHODCALLTYPE GetResult(UINT) override { return E_NOTIMPL; }
};

struct MockFence final : ID3D12Fence {
    LONG refs = 1;
    UINT64 value = 0;

    HRESULT STDMETHODCALLTYPE QueryInterface(REFIID, void**) override { return E_NOINTERFACE; }
    ULONG STDMETHODCALLTYPE AddRef() override { return InterlockedIncrement(&refs); }
    ULONG STDMETHODCALLTYPE Release() override { return InterlockedDecrement(&refs); }
    HRESULT STDMETHODCALLTYPE GetPrivateData(REFGUID, UINT*, void*) override { return E_NOTIMPL; }
    HRESULT STDMETHODCALLTYPE SetPrivateData(REFGUID, UINT, const void*) override { return E_NOTIMPL; }
    HRESULT STDMETHODCALLTYPE SetPrivateDataInterface(REFGUID, const IUnknown*) override { return E_NOTIMPL; }
    HRESULT STDMETHODCALLTYPE SetName(LPCWSTR) override { return E_NOTIMPL; }
    HRESULT STDMETHODCALLTYPE GetDevice(REFIID, void**) override { return E_NOTIMPL; }
    UINT64 STDMETHODCALLTYPE GetCompletedValue() override { return value; }
    HRESULT STDMETHODCALLTYPE SetEventOnCompletion(UINT64, HANDLE) override { return E_NOTIMPL; }
    HRESULT STDMETHODCALLTYPE Signal(UINT64 v) override {
        value = v;
        return S_OK;
    }
};

unsigned char pattern_byte(unsigned index) {
    return static_cast<unsigned char>(index * 17u + 3u);
}

bool bytes_match(const unsigned char* got, const unsigned char* expected, unsigned count, const char* label) {
    for (unsigned i = 0; i < count; ++i) {
        if (got[i] == expected[i]) continue;
        std::fprintf(stderr, "xcrdapi: %s[%u]=0x%02x expected 0x%02x\n", label, i, got[i], expected[i]);
        return false;
    }
    return true;
}

int map_fill(ID3D12Resource* resource, unsigned char value, unsigned count) {
    D3D12_RANGE no_read{0, 0};
    void* mapped = nullptr;
    const HRESULT hr = resource->Map(0, &no_read, &mapped);
    if (hr != S_OK || !mapped) {
        std::fprintf(stderr, "xcrdapi: Map(fill) hr=0x%08lx\n", static_cast<unsigned long>(hr));
        return 1;
    }
    std::memset(mapped, value, count);
    resource->Unmap(0, nullptr);
    return 0;
}

int map_copy(ID3D12Resource* resource, unsigned char* dest, unsigned count) {
    D3D12_RANGE read{0, count};
    void* mapped = nullptr;
    const HRESULT hr = resource->Map(0, &read, &mapped);
    if (hr != S_OK || !mapped) {
        std::fprintf(stderr, "xcrdapi: Map(read) hr=0x%08lx\n", static_cast<unsigned long>(hr));
        return 1;
    }
    std::memcpy(dest, mapped, count);
    D3D12_RANGE none{0, 0};
    resource->Unmap(0, &none);
    return 0;
}

HRESULT create_buffer(ID3D12Device* device, D3D12_HEAP_TYPE heap, D3D12_RESOURCE_STATES state,
                      UINT64 width, ID3D12Resource** out) {
    if (!out) return E_POINTER;
    *out = nullptr;
    D3D12_HEAP_PROPERTIES props{};
    props.Type = heap;
    D3D12_RESOURCE_DESC desc{};
    desc.Dimension = D3D12_RESOURCE_DIMENSION_BUFFER;
    desc.Width = width;
    desc.Height = 1;
    desc.DepthOrArraySize = 1;
    desc.MipLevels = 1;
    desc.SampleDesc.Count = 1;
    desc.Layout = D3D12_TEXTURE_LAYOUT_ROW_MAJOR;
    return device->CreateCommittedResource(&props, D3D12_HEAP_FLAG_NONE, &desc, state, nullptr,
                                           __uuidof(ID3D12Resource), reinterpret_cast<void**>(out));
}

void test_transition(ID3D12GraphicsCommandList* list, ID3D12Resource* resource,
                     D3D12_RESOURCE_STATES before, D3D12_RESOURCE_STATES after) {
    if (before == after) return;
    D3D12_RESOURCE_BARRIER barrier{};
    barrier.Type = D3D12_RESOURCE_BARRIER_TYPE_TRANSITION;
    barrier.Transition.pResource = resource;
    barrier.Transition.Subresource = D3D12_RESOURCE_BARRIER_ALL_SUBRESOURCES;
    barrier.Transition.StateBefore = before;
    barrier.Transition.StateAfter = after;
    list->ResourceBarrier(1, &barrier);
}

int copy_region(ID3D12Resource* dst, UINT64 dst_offset, ID3D12Resource* src, UINT64 src_offset, UINT64 size,
                D3D12_RESOURCE_STATES dst_state, D3D12_RESOURCE_STATES dst_for_copy,
                D3D12_RESOURCE_STATES src_state, D3D12_RESOURCE_STATES src_for_copy) {
    Com<ID3D12Device> device;
    HRESULT hr = dst->GetDevice(__uuidof(ID3D12Device), device.out());
    if (hr != S_OK || !device.value) {
        std::fprintf(stderr, "xcrdapi: GetDevice hr=0x%08lx\n", static_cast<unsigned long>(hr));
        return 1;
    }
    D3D12_COMMAND_QUEUE_DESC queue_desc{};
    queue_desc.Type = D3D12_COMMAND_LIST_TYPE_DIRECT;
    Com<ID3D12CommandQueue> queue;
    hr = device->CreateCommandQueue(&queue_desc, __uuidof(ID3D12CommandQueue), queue.out());
    if (hr != S_OK || !queue.value) {
        std::fprintf(stderr, "xcrdapi: CreateCommandQueue hr=0x%08lx\n", static_cast<unsigned long>(hr));
        return 1;
    }
    Com<ID3D12CommandAllocator> allocator;
    hr = device->CreateCommandAllocator(D3D12_COMMAND_LIST_TYPE_DIRECT, __uuidof(ID3D12CommandAllocator), allocator.out());
    if (hr != S_OK || !allocator.value) {
        std::fprintf(stderr, "xcrdapi: CreateCommandAllocator hr=0x%08lx\n", static_cast<unsigned long>(hr));
        return 1;
    }
    Com<ID3D12GraphicsCommandList> list;
    hr = device->CreateCommandList(0, D3D12_COMMAND_LIST_TYPE_DIRECT, allocator.value, nullptr,
                                  __uuidof(ID3D12GraphicsCommandList), list.out());
    if (hr != S_OK || !list.value) {
        std::fprintf(stderr, "xcrdapi: CreateCommandList hr=0x%08lx\n", static_cast<unsigned long>(hr));
        return 1;
    }
    test_transition(list.value, dst, dst_state, dst_for_copy);
    test_transition(list.value, src, src_state, src_for_copy);
    list->CopyBufferRegion(dst, dst_offset, src, src_offset, size);
    test_transition(list.value, dst, dst_for_copy, dst_state);
    test_transition(list.value, src, src_for_copy, src_state);
    hr = list->Close();
    if (hr != S_OK) {
        std::fprintf(stderr, "xcrdapi: command list Close hr=0x%08lx\n", static_cast<unsigned long>(hr));
        return 1;
    }
    ID3D12CommandList* lists[] = {list.value};
    queue->ExecuteCommandLists(1, lists);
    Com<ID3D12Fence> fence;
    hr = device->CreateFence(0, D3D12_FENCE_FLAG_NONE, __uuidof(ID3D12Fence), fence.out());
    if (hr != S_OK || !fence.value) {
        std::fprintf(stderr, "xcrdapi: CreateFence hr=0x%08lx\n", static_cast<unsigned long>(hr));
        return 1;
    }
    hr = queue->Signal(fence.value, 1);
    if (hr != S_OK) {
        std::fprintf(stderr, "xcrdapi: Signal hr=0x%08lx\n", static_cast<unsigned long>(hr));
        return 1;
    }
    Event done;
    if (!done.handle) return 1;
    hr = fence->SetEventOnCompletion(1, done.handle);
    if (hr != S_OK) {
        std::fprintf(stderr, "xcrdapi: SetEventOnCompletion hr=0x%08lx\n", static_cast<unsigned long>(hr));
        return 1;
    }
    const DWORD waited = WaitForSingleObject(done.handle, 10000);
    if (waited != WAIT_OBJECT_0) {
        std::fprintf(stderr, "xcrdapi: copy wait=%lu\n", static_cast<unsigned long>(waited));
        return 1;
    }
    return 0;
}

int check_compressed_rejection(Factory* factory) {
    unsigned char source[4];
    std::memset(source, 0x11, sizeof(source));
    unsigned char unchanged[4];
    std::memset(unchanged, 0x3C, sizeof(unchanged));
    unsigned char dests[3][4];
    for (auto& dest : dests) std::memset(dest, 0x3C, 4);
    const UINT formats[] = {compression_lz, compression_bcpack, compression_gdeflate};

    Com<Status> status;
    CHECK(factory->CreateStatusArray(3, status_iid, status.out()) == S_OK);
    Com<Queue> queue;
    CHECK(factory->CreateQueue(8, queue_iid, queue.out()) == S_OK);
    Event event;
    CHECK(event.handle);

    for (UINT i = 0; i < 3; ++i) {
        Request request{nullptr, 0, source, dests[i], 4, formats[i]};
        CHECK(queue->EnqueueRequest(&request) == S_OK);
        CHECK(queue->EnqueueStatus(status.value, i) == S_OK);
    }
    CHECK(queue->EnqueueSetEvent(event.handle) == S_OK);
    CHECK(status->GetResult(0) == E_PENDING);
    CHECK(queue->Submit() == S_OK);
    CHECK(WaitForSingleObject(event.handle, 5000) == WAIT_OBJECT_0);

    for (UINT i = 0; i < 3; ++i) {
        const HRESULT hr = status->GetResult(i);
        if (hr != E_NOTIMPL) {
            std::fprintf(stderr, "xcrdapi: compression %u hr=0x%08lx\n", i, static_cast<unsigned long>(hr));
            return 1;
        }
        CHECK(bytes_match(dests[i], unchanged, 4, "compressed"));
    }
    ErrorRecord error{};
    CHECK(queue->RetrieveErrorRecord(&error) == S_OK);
    CHECK(error.failure_count == 3 && error.first_error == E_NOTIMPL);
    CHECK(queue->Close() == S_OK);
    return 0;
}

int check_batched_buffer_transfers(Factory* factory) {
    // Expected bytes come from pattern_byte, the same function that fills the file.
    // Two slices overlap on purpose: index 8 matches the later slice only when Submit
    // runs the requests in enqueue order.
    constexpr unsigned kBytes = 32;
    constexpr unsigned kFileBytes = 40;
    CHECK(pattern_byte(8) != pattern_byte(24));

    unsigned char expect_mem[kBytes] = {};
    unsigned char expect_gpu[kBytes];
    std::memset(expect_gpu, 0xA5, kBytes);
    for (unsigned i = 0; i < 16; ++i) {
        expect_mem[i] = pattern_byte(i);
        expect_gpu[i] = pattern_byte(i);
    }
    for (unsigned i = 0; i < 16; ++i) {
        expect_mem[8 + i] = pattern_byte(24 + i);
        expect_gpu[8 + i] = pattern_byte(24 + i);
    }
    CHECK(expect_mem[8] == pattern_byte(24) && expect_mem[24] == 0);
    CHECK(expect_gpu[8] == pattern_byte(24) && expect_gpu[24] == 0xA5);

    SetErrorMode(SEM_FAILCRITICALERRORS | SEM_NOGPFAULTERRORBOX);
    Com<ID3D12Device> device;
    HRESULT hr = D3D12XboxCreateDevice(nullptr, __uuidof(ID3D12Device), device.out());
    if (hr != S_OK || !device.value) {
        std::fprintf(stderr, "xcrdapi: D3D12XboxCreateDevice hr=0x%08lx\n", static_cast<unsigned long>(hr));
        return 1;
    }
    Com<ID3D12Resource> upload;
    Com<ID3D12Resource> gpu;
    Com<ID3D12Resource> readback;
    hr = create_buffer(device.value, D3D12_HEAP_TYPE_UPLOAD, D3D12_RESOURCE_STATE_GENERIC_READ, kBytes, &upload.value);
    if (hr != S_OK || !upload.value) {
        std::fprintf(stderr, "xcrdapi: upload buffer hr=0x%08lx\n", static_cast<unsigned long>(hr));
        return 1;
    }
    hr = create_buffer(device.value, D3D12_HEAP_TYPE_DEFAULT, D3D12_RESOURCE_STATE_COMMON, kBytes, &gpu.value);
    if (hr != S_OK || !gpu.value) {
        std::fprintf(stderr, "xcrdapi: default buffer hr=0x%08lx\n", static_cast<unsigned long>(hr));
        return 1;
    }
    hr = create_buffer(device.value, D3D12_HEAP_TYPE_READBACK, D3D12_RESOURCE_STATE_COPY_DEST, kBytes, &readback.value);
    if (hr != S_OK || !readback.value) {
        std::fprintf(stderr, "xcrdapi: readback buffer hr=0x%08lx\n", static_cast<unsigned long>(hr));
        return 1;
    }
    D3D12_HEAP_PROPERTIES heap{};
    hr = upload->GetHeapProperties(&heap, nullptr);
    if (hr != S_OK || heap.Type != D3D12_HEAP_TYPE_UPLOAD) {
        std::fprintf(stderr, "xcrdapi: upload heap hr=0x%08lx type=%d\n", static_cast<unsigned long>(hr), static_cast<int>(heap.Type));
        return 1;
    }
    hr = gpu->GetHeapProperties(&heap, nullptr);
    if (hr != S_OK || heap.Type != D3D12_HEAP_TYPE_DEFAULT) {
        std::fprintf(stderr, "xcrdapi: default heap hr=0x%08lx type=%d\n", static_cast<unsigned long>(hr), static_cast<int>(heap.Type));
        return 1;
    }

    if (map_fill(upload.value, 0xA5, kBytes) != 0) return 1;
    if (copy_region(gpu.value, 0, upload.value, 0, kBytes,
                    D3D12_RESOURCE_STATE_COMMON, D3D12_RESOURCE_STATE_COPY_DEST,
                    D3D12_RESOURCE_STATE_GENERIC_READ, D3D12_RESOURCE_STATE_GENERIC_READ) != 0) {
        return 1;
    }

    unsigned char file_bytes[kFileBytes];
    for (unsigned i = 0; i < kFileBytes; ++i) file_bytes[i] = pattern_byte(i);
    TemporaryFile temporary;
    WCHAR directory[MAX_PATH];
    const DWORD directory_len = GetTempPathW(MAX_PATH, directory);
    CHECK(directory_len && directory_len < MAX_PATH);
    CHECK(GetTempFileNameW(directory, L"bxg", 0, temporary.name));
    const HANDLE writer = CreateFileW(temporary.name, GENERIC_WRITE, 0, nullptr, TRUNCATE_EXISTING,
                                      FILE_ATTRIBUTE_NORMAL, nullptr);
    CHECK(writer != INVALID_HANDLE_VALUE);
    DWORD written = 0;
    const BOOL wrote = WriteFile(writer, file_bytes, kFileBytes, &written, nullptr);
    CloseHandle(writer);
    CHECK(wrote && written == kFileBytes);

    Com<File> file;
    CHECK(factory->OpenFile(temporary.name, file_iid, file.out()) == S_OK);
    unsigned char memory[kBytes] = {};
    Com<Status> status;
    CHECK(factory->CreateStatusArray(6, status_iid, status.out()) == S_OK);
    Com<Queue> queue;
    CHECK(factory->CreateQueue(16, queue_iid, queue.out()) == S_OK);
    Event event;
    CHECK(event.handle);

    Request requests[6] = {
        {file.value, 0, nullptr, memory, 16, compression_none},
        {file.value, 24, nullptr, memory + 8, 16, compression_none},
        {file.value, 0, nullptr, nullptr, 16, compression_none, upload.value, 0},
        {file.value, 24, nullptr, nullptr, 16, compression_none, upload.value, 8},
        {file.value, 0, nullptr, nullptr, 16, compression_none, gpu.value, 0},
        {file.value, 24, nullptr, nullptr, 16, compression_none, gpu.value, 8},
    };
    for (UINT i = 0; i < 6; ++i) {
        CHECK(queue->EnqueueRequest(&requests[i]) == S_OK);
        CHECK(queue->EnqueueStatus(status.value, i) == S_OK);
    }
    CHECK(queue->EnqueueSetEvent(event.handle) == S_OK);
    CHECK(status->GetResult(0) == E_PENDING);
    CHECK(WaitForSingleObject(event.handle, 0) == WAIT_TIMEOUT);
    unsigned char zeros[kBytes] = {};
    CHECK(bytes_match(memory, zeros, kBytes, "memory-before"));
    unsigned char upload_before[kBytes];
    if (map_copy(upload.value, upload_before, kBytes) != 0) return 1;
    for (unsigned i = 0; i < kBytes; ++i) CHECK(upload_before[i] == 0xA5);

    CHECK(queue->Submit() == S_OK);
    CHECK(WaitForSingleObject(event.handle, 15000) == WAIT_OBJECT_0);
    for (UINT i = 0; i < 6; ++i) {
        const HRESULT slot = status->GetResult(i);
        if (slot != S_OK) {
            std::fprintf(stderr, "xcrdapi: batch status %u hr=0x%08lx\n", i, static_cast<unsigned long>(slot));
            return 1;
        }
    }
    CHECK(bytes_match(memory, expect_mem, kBytes, "memory"));
    unsigned char upload_got[kBytes];
    if (map_copy(upload.value, upload_got, kBytes) != 0) return 1;
    CHECK(bytes_match(upload_got, expect_gpu, kBytes, "upload"));

    if (copy_region(readback.value, 0, gpu.value, 0, kBytes,
                    D3D12_RESOURCE_STATE_COPY_DEST, D3D12_RESOURCE_STATE_COPY_DEST,
                    D3D12_RESOURCE_STATE_COMMON, D3D12_RESOURCE_STATE_COPY_SOURCE) != 0) {
        return 1;
    }
    unsigned char gpu_got[kBytes];
    if (map_copy(readback.value, gpu_got, kBytes) != 0) return 1;
    CHECK(bytes_match(gpu_got, expect_gpu, kBytes, "default"));

    ErrorRecord error{};
    CHECK(queue->RetrieveErrorRecord(&error) == S_OK && error.failure_count == 0);
    CHECK(queue->Close() == S_OK);

    Com<Queue> rejected;
    CHECK(factory->CreateQueue(4, queue_iid, rejected.out()) == S_OK);
    Request both{nullptr, 0, memory, memory, 4, compression_none, upload.value, 0};
    CHECK(rejected->EnqueueRequest(&both) == E_INVALIDARG);
    Com<Status> rejected_status;
    CHECK(factory->CreateStatusArray(1, status_iid, rejected_status.out()) == S_OK);
    Request past{file.value, 0, nullptr, nullptr, 1, compression_none, upload.value, kBytes};
    CHECK(rejected->EnqueueRequest(&past) == S_OK);
    CHECK(rejected->EnqueueStatus(rejected_status.value, 0) == S_OK);
    CHECK(rejected->Submit() == S_OK);
    CHECK(rejected->Close() == S_OK);
    const HRESULT past_hr = rejected_status->GetResult(0);
    if (past_hr != E_INVALIDARG) {
        std::fprintf(stderr, "xcrdapi: past-end buffer hr=0x%08lx\n", static_cast<unsigned long>(past_hr));
        return 1;
    }
    unsigned char upload_after[kBytes];
    if (map_copy(upload.value, upload_after, kBytes) != 0) return 1;
    CHECK(bytes_match(upload_after, expect_gpu, kBytes, "upload-after-reject"));
    return 0;
}

ULONG live_refs(IUnknown* object) {
    const ULONG held = object->AddRef();
    object->Release();
    return held - 1;
}

// Proves the post-ExecuteCommandLists timeout path: with a 0 ms wait and the
// copy queue held until that wait is decided, at least one request returns
// HRESULT_FROM_WIN32(WAIT_TIMEOUT), the device is not removed, the shim still
// holds the destination, and the destination bytes match after the GPU drains.
int check_unconfirmed_staged_copy(Factory* factory) {
    constexpr unsigned kBytes = 16;
    constexpr int kAttempts = 4;
    const HRESULT kTimeout = HRESULT_FROM_WIN32(WAIT_TIMEOUT);

    struct RestoreEnv {
        ~RestoreEnv() {
            SetEnvironmentVariableW(L"BX_XCRDAPI_COPY_TIMEOUT_MS", nullptr);
            SetEnvironmentVariableW(L"BX_XCRDAPI_DEFER_COPY", nullptr);
        }
    } restore_env;
    CHECK(SetEnvironmentVariableW(L"BX_XCRDAPI_COPY_TIMEOUT_MS", L"0"));
    CHECK(SetEnvironmentVariableW(L"BX_XCRDAPI_DEFER_COPY", L"1"));

    SetErrorMode(SEM_FAILCRITICALERRORS | SEM_NOGPFAULTERRORBOX);
    Com<ID3D12Device> device;
    HRESULT hr = D3D12XboxCreateDevice(nullptr, __uuidof(ID3D12Device), device.out());
    if (hr != S_OK || !device.value) {
        std::fprintf(stderr, "xcrdapi: unconfirmed D3D12XboxCreateDevice hr=0x%08lx\n", static_cast<unsigned long>(hr));
        return 1;
    }
    Com<ID3D12Resource> upload;
    Com<ID3D12Resource> readback;
    hr = create_buffer(device.value, D3D12_HEAP_TYPE_UPLOAD, D3D12_RESOURCE_STATE_GENERIC_READ, kBytes, &upload.value);
    if (hr != S_OK || !upload.value) {
        std::fprintf(stderr, "xcrdapi: unconfirmed upload hr=0x%08lx\n", static_cast<unsigned long>(hr));
        return 1;
    }
    hr = create_buffer(device.value, D3D12_HEAP_TYPE_READBACK, D3D12_RESOURCE_STATE_COPY_DEST, kBytes, &readback.value);
    if (hr != S_OK || !readback.value) {
        std::fprintf(stderr, "xcrdapi: unconfirmed readback hr=0x%08lx\n", static_cast<unsigned long>(hr));
        return 1;
    }
    if (map_fill(upload.value, 0x5A, kBytes) != 0) return 1;

    unsigned char file_bytes[kBytes];
    unsigned char expect[kBytes];
    for (unsigned i = 0; i < kBytes; ++i) {
        file_bytes[i] = pattern_byte(i);
        expect[i] = pattern_byte(i);
    }
    CHECK(expect[0] != 0x5A);
    TemporaryFile temporary;
    WCHAR directory[MAX_PATH];
    const DWORD directory_len = GetTempPathW(MAX_PATH, directory);
    CHECK(directory_len && directory_len < MAX_PATH);
    CHECK(GetTempFileNameW(directory, L"bxu", 0, temporary.name));
    const HANDLE writer = CreateFileW(temporary.name, GENERIC_WRITE, 0, nullptr, TRUNCATE_EXISTING,
                                      FILE_ATTRIBUTE_NORMAL, nullptr);
    CHECK(writer != INVALID_HANDLE_VALUE);
    DWORD written = 0;
    const BOOL wrote = WriteFile(writer, file_bytes, kBytes, &written, nullptr);
    CloseHandle(writer);
    CHECK(wrote && written == kBytes);
    Com<File> file;
    CHECK(factory->OpenFile(temporary.name, file_iid, file.out()) == S_OK);

    HRESULT last_hr = E_FAIL;
    ULONG last_before = 0;
    ULONG last_after = 0;
    for (int attempt = 0; attempt < kAttempts; ++attempt) {
        Com<ID3D12Resource> dest;
        hr = create_buffer(device.value, D3D12_HEAP_TYPE_DEFAULT, D3D12_RESOURCE_STATE_COMMON, kBytes, &dest.value);
        if (hr != S_OK || !dest.value) {
            std::fprintf(stderr, "xcrdapi: unconfirmed dest hr=0x%08lx\n", static_cast<unsigned long>(hr));
            return 1;
        }
        D3D12_HEAP_PROPERTIES heap{};
        hr = dest->GetHeapProperties(&heap, nullptr);
        if (hr != S_OK || heap.Type != D3D12_HEAP_TYPE_DEFAULT) {
            std::fprintf(stderr, "xcrdapi: unconfirmed heap hr=0x%08lx type=%d\n",
                         static_cast<unsigned long>(hr), static_cast<int>(heap.Type));
            return 1;
        }
        if (copy_region(dest.value, 0, upload.value, 0, kBytes,
                        D3D12_RESOURCE_STATE_COMMON, D3D12_RESOURCE_STATE_COPY_DEST,
                        D3D12_RESOURCE_STATE_GENERIC_READ, D3D12_RESOURCE_STATE_GENERIC_READ) != 0) {
            return 1;
        }
        const ULONG before = live_refs(dest.value);
        const HRESULT removed_before = device->GetDeviceRemovedReason();
        if (removed_before != S_OK) {
            std::fprintf(stderr, "xcrdapi: device removed before unconfirmed copy hr=0x%08lx\n",
                         static_cast<unsigned long>(removed_before));
            return 1;
        }

        Com<Status> status;
        CHECK(factory->CreateStatusArray(1, status_iid, status.out()) == S_OK);
        Com<Queue> queue;
        CHECK(factory->CreateQueue(4, queue_iid, queue.out()) == S_OK);
        Event event;
        CHECK(event.handle);
        Request request{file.value, 0, nullptr, nullptr, kBytes, compression_none, dest.value, 0};
        CHECK(queue->EnqueueRequest(&request) == S_OK);
        CHECK(queue->EnqueueStatus(status.value, 0) == S_OK);
        CHECK(queue->EnqueueSetEvent(event.handle) == S_OK);
        CHECK(queue->Submit() == S_OK);
        CHECK(WaitForSingleObject(event.handle, 5000) == WAIT_OBJECT_0);
        CHECK(queue->Close() == S_OK);

        last_hr = status->GetResult(0);
        last_before = before;
        last_after = live_refs(dest.value);
        const HRESULT removed = device->GetDeviceRemovedReason();
        if (removed != S_OK) {
            std::fprintf(stderr, "xcrdapi: device removed during unconfirmed copy hr=0x%08lx\n",
                         static_cast<unsigned long>(removed));
            return 1;
        }
        // In-flight driver references can sit on top of the shim's hold. The
        // count after the GPU drains is the one that has to be exactly one
        // above the pre-request count.
        if (last_hr != kTimeout || last_after < before + 1) continue;

        ErrorRecord error{};
        CHECK(queue->RetrieveErrorRecord(&error) == S_OK);
        if (error.failure_count != 1 || error.first_error != kTimeout) {
            std::fprintf(stderr, "xcrdapi: unconfirmed error record count=%llu hr=0x%08lx\n",
                         static_cast<unsigned long long>(error.failure_count),
                         static_cast<unsigned long>(error.first_error));
            return 1;
        }

        bool bytes_ok = false;
        unsigned char got[kBytes];
        for (int spin = 0; spin < 8 && !bytes_ok; ++spin) {
            Sleep(spin == 0 ? 200 : 100);
            if (device->GetDeviceRemovedReason() != S_OK) break;
            if (copy_region(readback.value, 0, dest.value, 0, kBytes,
                            D3D12_RESOURCE_STATE_COPY_DEST, D3D12_RESOURCE_STATE_COPY_DEST,
                            D3D12_RESOURCE_STATE_COMMON, D3D12_RESOURCE_STATE_COPY_SOURCE) != 0) {
                return 1;
            }
            if (map_copy(readback.value, got, kBytes) != 0) return 1;
            bytes_ok = true;
            for (unsigned i = 0; i < kBytes; ++i) {
                if (got[i] != expect[i]) bytes_ok = false;
            }
            if (!bytes_ok && spin == 7) bytes_match(got, expect, kBytes, "unconfirmed");
        }
        if (!bytes_ok) return 1;
        const HRESULT removed_after = device->GetDeviceRemovedReason();
        const ULONG still_held = live_refs(dest.value);
        if (removed_after != S_OK || still_held != before + 1) {
            std::fprintf(stderr, "xcrdapi: after drain removed=0x%08lx refs=%lu expected=%lu (at timeout %lu)\n",
                         static_cast<unsigned long>(removed_after),
                         static_cast<unsigned long>(still_held),
                         static_cast<unsigned long>(before + 1),
                         static_cast<unsigned long>(last_after));
            return 1;
        }
        std::printf("test_xcrdapi: unconfirmed copy attempt=%d hr=0x%08lx refs %lu -> %lu device_removed=0x%08lx bytes=%u\n",
                    attempt,
                    static_cast<unsigned long>(last_hr),
                    static_cast<unsigned long>(before),
                    static_cast<unsigned long>(still_held),
                    static_cast<unsigned long>(removed_after),
                    kBytes);
        return 0;
    }

    std::fprintf(stderr, "xcrdapi: unconfirmed path not observed in %d attempts (last hr=0x%08lx refs %lu -> %lu)\n",
                 kAttempts, static_cast<unsigned long>(last_hr),
                 static_cast<unsigned long>(last_before), static_cast<unsigned long>(last_after));
    return 1;
}

// Public PC DirectStorage interfaces. Expected values come from the documentation
// comments in dstorage.h 1.4.0, not from the shim: capacity limits, "capacity - 1"
// empty slots, automatic submission at half capacity, IsComplete == (GetHResult !=
// E_PENDING), status entries scoped to the requests since the previous one, and the
// E_DSTORAGE_* codes in dstorageerr.h. Where the header is silent (unsupported
// destinations and compression), the expected E_NOTIMPL is BetterXen's documented choice.
struct ForeignStatusArray final : IDStorageStatusArray {
    HRESULT STDMETHODCALLTYPE QueryInterface(REFIID, void**) override { return E_NOINTERFACE; }
    ULONG STDMETHODCALLTYPE AddRef() override { return 1; }
    ULONG STDMETHODCALLTYPE Release() override { return 1; }
    bool STDMETHODCALLTYPE IsComplete(UINT32) override { return true; }
    HRESULT STDMETHODCALLTYPE GetHResult(UINT32) override { return S_OK; }
};

DSTORAGE_REQUEST file_to_memory(IDStorageFile* file, UINT64 offset, void* dest, UINT32 size) {
    DSTORAGE_REQUEST request{};
    request.Options.SourceType = DSTORAGE_REQUEST_SOURCE_FILE;
    request.Options.DestinationType = DSTORAGE_REQUEST_DESTINATION_MEMORY;
    request.Source.File.Source = file;
    request.Source.File.Offset = offset;
    request.Source.File.Size = size;
    request.Destination.Memory.Buffer = dest;
    request.Destination.Memory.Size = size;
    return request;
}

DSTORAGE_REQUEST memory_to_memory(const void* source, void* dest, UINT32 size) {
    DSTORAGE_REQUEST request{};
    request.Options.SourceType = DSTORAGE_REQUEST_SOURCE_MEMORY;
    request.Options.DestinationType = DSTORAGE_REQUEST_DESTINATION_MEMORY;
    request.Source.Memory.Source = source;
    request.Source.Memory.Size = size;
    request.Destination.Memory.Buffer = dest;
    request.Destination.Memory.Size = size;
    return request;
}

int check_public_interfaces(const WCHAR* path, const char* contents) {
    // The generated MinGW IID declarations must match values typed independently from
    // Silk.NET's IDStorageFactory binding and the #483 issue text, not from dstorage_mingw.h.
    const IID typed_factory{0x6924ea0c, 0xc3cd, 0x4826, {0xb1, 0x0a, 0xf6, 0x4f, 0x4e, 0xd9, 0x27, 0xc1}};
    CHECK(IsEqualIID(__uuidof(IDStorageFactory), typed_factory));

    // Factory: public IIDs are served, an unknown IID is refused with ppv cleared.
    const IID unknown_iid{0x0badf00d, 0x1234, 0x4abc, {0x80, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07}};
    void* refused = reinterpret_cast<void*>(1);
    CHECK(DStorageGetFactory(unknown_iid, &refused) == E_NOINTERFACE && !refused);
    Com<IDStorageFactory> factory;
    CHECK(DStorageGetFactory(__uuidof(IDStorageFactory), factory.out()) == S_OK && factory.value);
    Com<IDStorageFactory1> factory1;
    CHECK(factory->QueryInterface(__uuidof(IDStorageFactory1), factory1.out()) == S_OK);
    Com<IUnknown> identity;
    CHECK(factory->QueryInterface(IID_IUnknown, identity.out()) == S_OK);
    Com<IUnknown> identity1;
    CHECK(factory1->QueryInterface(IID_IUnknown, identity1.out()) == S_OK && identity.value == identity1.value);
    Com<IDStorageFactory1> direct1;
    CHECK(DStorageGetFactory(__uuidof(IDStorageFactory1), direct1.out()) == S_OK && direct1.value);
    CHECK(factory->SetStagingBufferSize(DSTORAGE_STAGING_BUFFER_SIZE_32MB) == S_OK);

    // Queue creation limits: DSTORAGE_MIN/MAX_QUEUE_CAPACITY and DSTORAGE_PRIORITY range.
    DSTORAGE_QUEUE_DESC desc{};
    desc.SourceType = DSTORAGE_REQUEST_SOURCE_FILE;
    desc.Capacity = DSTORAGE_MIN_QUEUE_CAPACITY;
    desc.Priority = DSTORAGE_PRIORITY_NORMAL;
    desc.Name = "bx file queue";
    void* bad_queue = reinterpret_cast<void*>(1);
    DSTORAGE_QUEUE_DESC bad = desc;
    bad.Capacity = DSTORAGE_MIN_QUEUE_CAPACITY - 1;
    CHECK(factory->CreateQueue(&bad, __uuidof(IDStorageQueue), &bad_queue) == E_DSTORAGE_INVALID_QUEUE_CAPACITY
          && !bad_queue);
    bad.Capacity = DSTORAGE_MAX_QUEUE_CAPACITY + 1;
    CHECK(factory->CreateQueue(&bad, __uuidof(IDStorageQueue), &bad_queue) == E_DSTORAGE_INVALID_QUEUE_CAPACITY);
    bad = desc;
    bad.Priority = static_cast<DSTORAGE_PRIORITY>(DSTORAGE_PRIORITY_LAST + 1);
    CHECK(factory->CreateQueue(&bad, __uuidof(IDStorageQueue), &bad_queue) == E_DSTORAGE_INVALID_QUEUE_PRIORITY);

    Com<IDStorageFile> file;
    CHECK(factory->OpenFile(path, __uuidof(IDStorageFile), file.out()) == S_OK);
    BY_HANDLE_FILE_INFORMATION file_info{};
    CHECK(file->GetFileInformation(&file_info) == S_OK && file_info.nFileSizeLow == std::strlen(contents) + 1);
    Com<IDStorageStatusArray> status;
    CHECK(factory->CreateStatusArray(4, "bx status", __uuidof(IDStorageStatusArray), status.out()) == S_OK);

    Com<IDStorageQueue> queue;
    CHECK(factory->CreateQueue(&desc, __uuidof(IDStorageQueue), queue.out()) == S_OK);
    Com<IDStorageQueue4> queue4;
    CHECK(queue->QueryInterface(__uuidof(IDStorageQueue4), queue4.out()) == S_OK);
    DSTORAGE_QUEUE_INFO info{};
    queue->Query(&info);
    CHECK(info.Desc.Capacity == DSTORAGE_MIN_QUEUE_CAPACITY && info.Desc.SourceType == DSTORAGE_REQUEST_SOURCE_FILE);
    CHECK(info.Desc.Name && std::strcmp(info.Desc.Name, "bx file queue") == 0);
    CHECK(info.EmptySlotCount == DSTORAGE_MIN_QUEUE_CAPACITY - 1);
    CHECK(info.RequestCountUntilAutoSubmit == DSTORAGE_MIN_QUEUE_CAPACITY / 2);
    CHECK(queue4->GetCompressionSupport(DSTORAGE_COMPRESSION_FORMAT_GDEFLATE) == DSTORAGE_COMPRESSION_SUPPORT_NONE);

    // Two file reads, a status entry and an event; nothing runs before Submit.
    std::array<char, 5> first{}, second{};
    Event done;
    CHECK(done.handle);
    DSTORAGE_REQUEST read1 = file_to_memory(file.value, 2, first.data(), 4);
    DSTORAGE_REQUEST read2 = file_to_memory(file.value, 10, second.data(), 4);
    queue->EnqueueRequest(&read1);
    queue->EnqueueRequest(&read2);
    queue->EnqueueStatus(status.value, 0);
    queue4->EnqueueSetEvent(done.handle);
    DSTORAGE_QUEUE_INFO1 info1{};
    queue4->Query1(&info1);
    CHECK(info1.EmptySlotCount == DSTORAGE_MIN_QUEUE_CAPACITY - 1 - 4);
    CHECK(info1.RequestCountUntilAutoSubmit == DSTORAGE_MIN_QUEUE_CAPACITY / 2 - 4);
    CHECK(!status->IsComplete(0) && status->GetHResult(0) == E_PENDING);
    queue->Submit();
    CHECK(WaitForSingleObject(done.handle, 5000) == WAIT_OBJECT_0);
    CHECK(status->IsComplete(0) && status->GetHResult(0) == S_OK);
    CHECK(std::memcmp(first.data(), contents + 2, 4) == 0 && std::memcmp(second.data(), contents + 10, 4) == 0);

    // Rejected requests are reported in queue order, scoped by status entries.
    const HANDLE error_event = queue->GetErrorEvent();
    CHECK(error_event && WaitForSingleObject(error_event, 0) == WAIT_TIMEOUT);
    std::array<char, 5> scratch{}, third{};
    DSTORAGE_REQUEST reserved = file_to_memory(file.value, 0, scratch.data(), 4);
    reserved.Options.Reserved1[0] = 1;
    const char memory_source[4] = {'w', 'x', 'y', 'z'};
    DSTORAGE_REQUEST wrong_source = memory_to_memory(memory_source, scratch.data(), 4);
    DSTORAGE_REQUEST read3 = file_to_memory(file.value, 4, third.data(), 4);
    queue->EnqueueRequest(&reserved);
    queue->EnqueueRequest(&wrong_source);
    queue->EnqueueStatus(status.value, 1);
    queue->EnqueueRequest(&read3);
    queue->EnqueueStatus(status.value, 2);
    queue4->EnqueueSetEvent(done.handle);
    queue->Submit();
    CHECK(WaitForSingleObject(done.handle, 5000) == WAIT_OBJECT_0);
    CHECK(status->GetHResult(1) == E_DSTORAGE_RESERVED_FIELDS);
    CHECK(status->GetHResult(2) == S_OK && std::memcmp(third.data(), contents + 4, 4) == 0);
    CHECK(scratch[0] == 0);
    CHECK(WaitForSingleObject(error_event, 0) == WAIT_OBJECT_0);
    DSTORAGE_ERROR_RECORD record{};
    queue->RetrieveErrorRecord(&record);
    CHECK(record.FailureCount == 2 && record.FirstFailure.HResult == E_DSTORAGE_RESERVED_FIELDS);
    CHECK(record.FirstFailure.CommandType == DSTORAGE_COMMAND_TYPE_REQUEST);
    queue->RetrieveErrorRecord(&record);
    CHECK(record.FailureCount == 0 && record.FirstFailure.CommandType == DSTORAGE_COMMAND_TYPE_NONE);

    // A foreign status array and a null fence are rejected without poisoning the next status.
    ForeignStatusArray foreign;
    queue->EnqueueStatus(&foreign, 0);
    queue->EnqueueSignal(nullptr, 1);
    queue->EnqueueStatus(status.value, 3);
    queue4->EnqueueSetEvent(done.handle);
    queue->Submit();
    CHECK(WaitForSingleObject(done.handle, 5000) == WAIT_OBJECT_0);
    CHECK(status->GetHResult(3) == S_OK);
    queue->RetrieveErrorRecord(&record);
    CHECK(record.FailureCount == 2 && record.FirstFailure.HResult == E_DSTORAGE_INVALID_STATUS_ARRAY);
    CHECK(record.FirstFailure.CommandType == DSTORAGE_COMMAND_TYPE_STATUS);

    // Unsupported shapes on a memory queue without a device: each is its own status scope.
    DSTORAGE_QUEUE_DESC memory_desc = desc;
    memory_desc.SourceType = DSTORAGE_REQUEST_SOURCE_MEMORY;
    memory_desc.Name = nullptr;
    Com<IDStorageQueue1> memory_queue;
    CHECK(factory->CreateQueue(&memory_desc, __uuidof(IDStorageQueue1), memory_queue.out()) == S_OK);
    Com<IDStorageStatusArray> shapes;
    CHECK(factory->CreateStatusArray(4, nullptr, __uuidof(IDStorageStatusArray), shapes.out()) == S_OK);
    unsigned char untouched[4] = {0x5A, 0x5A, 0x5A, 0x5A};
    DSTORAGE_REQUEST buffer_without_device = memory_to_memory(memory_source, untouched, 4);
    buffer_without_device.Options.DestinationType = DSTORAGE_REQUEST_DESTINATION_BUFFER;
    buffer_without_device.Destination.Buffer.Resource = nullptr;
    buffer_without_device.Destination.Buffer.Offset = 0;
    buffer_without_device.Destination.Buffer.Size = 4;
    DSTORAGE_REQUEST texture = memory_to_memory(memory_source, untouched, 4);
    texture.Options.DestinationType = DSTORAGE_REQUEST_DESTINATION_TEXTURE_REGION;
    DSTORAGE_REQUEST zstd = memory_to_memory(memory_source, untouched, 4);
    zstd.Options.CompressionFormat = DSTORAGE_COMPRESSION_FORMAT_ZSTD;
    DSTORAGE_REQUEST short_destination = memory_to_memory(memory_source, untouched, 4);
    short_destination.Destination.Memory.Size = 3;
    const DSTORAGE_REQUEST* shaped[] = {&buffer_without_device, &texture, &zstd, &short_destination};
    const HRESULT expected[] = {E_DSTORAGE_INVALID_DESTINATION_TYPE, E_NOTIMPL, E_NOTIMPL,
                                E_DSTORAGE_INVALID_DESTINATION_SIZE};
    for (UINT i = 0; i < 4; ++i) {
        memory_queue->EnqueueRequest(shaped[i]);
        memory_queue->EnqueueStatus(shapes.value, i);
    }
    memory_queue->EnqueueSetEvent(done.handle);
    memory_queue->Submit();
    CHECK(WaitForSingleObject(done.handle, 5000) == WAIT_OBJECT_0);
    for (UINT i = 0; i < 4; ++i) {
        const HRESULT hr = shapes->GetHResult(i);
        if (hr != expected[i]) {
            std::fprintf(stderr, "xcrdapi: public shape %u hr=0x%08lx expected 0x%08lx\n", i,
                         static_cast<unsigned long>(hr), static_cast<unsigned long>(expected[i]));
            return 1;
        }
    }
    CHECK(untouched[0] == 0x5A && untouched[3] == 0x5A);

    // Half capacity triggers submission: the event is the 64th entry, and Submit is never called.
    constexpr UINT kAuto = DSTORAGE_MIN_QUEUE_CAPACITY / 2;
    static unsigned char auto_dest[kAuto - 1][4];
    std::memset(auto_dest, 0, sizeof(auto_dest));
    for (UINT i = 0; i + 1 < kAuto; ++i) {
        DSTORAGE_REQUEST copy = memory_to_memory(memory_source, auto_dest[i], 4);
        memory_queue->EnqueueRequest(&copy);
    }
    memory_queue->EnqueueSetEvent(done.handle);
    CHECK(WaitForSingleObject(done.handle, 5000) == WAIT_OBJECT_0);
    for (UINT i = 0; i + 1 < kAuto; ++i) CHECK(std::memcmp(auto_dest[i], memory_source, 4) == 0);

    // More requests than the queue holds: all are accepted, the caller waiting for free slots.
    constexpr UINT kBlocking = 3 * DSTORAGE_MIN_QUEUE_CAPACITY;
    static unsigned char blocking_dest[kBlocking][4];
    std::memset(blocking_dest, 0, sizeof(blocking_dest));
    for (UINT i = 0; i < kBlocking; ++i) {
        DSTORAGE_REQUEST copy = memory_to_memory(memory_source, blocking_dest[i], 4);
        memory_queue->EnqueueRequest(&copy);
    }
    memory_queue->EnqueueStatus(shapes.value, 0);
    memory_queue->EnqueueSetEvent(done.handle);
    memory_queue->Submit();
    CHECK(WaitForSingleObject(done.handle, 10000) == WAIT_OBJECT_0);
    CHECK(shapes->GetHResult(0) == S_OK);
    for (UINT i = 0; i < kBlocking; ++i) CHECK(std::memcmp(blocking_dest[i], memory_source, 4) == 0);
    memory_queue->RetrieveErrorRecord(&record);
    CHECK(record.FailureCount == 4 && record.FirstFailure.HResult == E_DSTORAGE_INVALID_DESTINATION_TYPE);

    // After Close, enqueued commands are dropped and Close is repeatable.
    file->Close();
    CHECK(file->GetFileInformation(&file_info) == HRESULT_FROM_WIN32(ERROR_INVALID_HANDLE));
    queue->Close();
    queue->EnqueueRequest(&read1);
    queue->Submit();
    queue->Close();
    memory_queue->Close();
    return 0;
}

// Public buffer destination: a memory source written into an upload-heap buffer on
// the queue's device. Expected bytes are the source bytes.
int check_public_buffer_destination() {
    SetErrorMode(SEM_FAILCRITICALERRORS | SEM_NOGPFAULTERRORBOX);
    Com<ID3D12Device> device;
    HRESULT hr = D3D12XboxCreateDevice(nullptr, __uuidof(ID3D12Device), device.out());
    if (hr != S_OK || !device.value) {
        std::fprintf(stderr, "xcrdapi: public D3D12XboxCreateDevice hr=0x%08lx\n", static_cast<unsigned long>(hr));
        return 1;
    }
    constexpr unsigned kBytes = 16;
    Com<ID3D12Resource> upload;
    hr = create_buffer(device.value, D3D12_HEAP_TYPE_UPLOAD, D3D12_RESOURCE_STATE_GENERIC_READ, kBytes, &upload.value);
    if (hr != S_OK || !upload.value) {
        std::fprintf(stderr, "xcrdapi: public upload buffer hr=0x%08lx\n", static_cast<unsigned long>(hr));
        return 1;
    }
    if (const int filled = map_fill(upload.value, 0xEE, kBytes)) return filled;

    Com<IDStorageFactory> factory;
    CHECK(DStorageGetFactory(__uuidof(IDStorageFactory), factory.out()) == S_OK);
    DSTORAGE_QUEUE_DESC desc{};
    desc.SourceType = DSTORAGE_REQUEST_SOURCE_MEMORY;
    desc.Capacity = DSTORAGE_MIN_QUEUE_CAPACITY;
    desc.Priority = DSTORAGE_PRIORITY_NORMAL;
    desc.Device = device.value;
    Com<IDStorageQueue1> queue;
    CHECK(factory->CreateQueue(&desc, __uuidof(IDStorageQueue1), queue.out()) == S_OK);
    Com<IDStorageStatusArray> status;
    CHECK(factory->CreateStatusArray(1, nullptr, __uuidof(IDStorageStatusArray), status.out()) == S_OK);

    unsigned char source[8];
    for (unsigned i = 0; i < 8; ++i) source[i] = pattern_byte(i);
    DSTORAGE_REQUEST request{};
    request.Options.SourceType = DSTORAGE_REQUEST_SOURCE_MEMORY;
    request.Options.DestinationType = DSTORAGE_REQUEST_DESTINATION_BUFFER;
    request.Source.Memory.Source = source;
    request.Source.Memory.Size = 8;
    request.Destination.Buffer.Resource = upload.value;
    request.Destination.Buffer.Offset = 4;
    request.Destination.Buffer.Size = 8;
    Event done;
    CHECK(done.handle);
    queue->EnqueueRequest(&request);
    queue->EnqueueStatus(status.value, 0);
    queue->EnqueueSetEvent(done.handle);
    queue->Submit();
    CHECK(WaitForSingleObject(done.handle, 10000) == WAIT_OBJECT_0);
    CHECK(status->GetHResult(0) == S_OK);

    unsigned char got[kBytes];
    if (const int copied = map_copy(upload.value, got, kBytes)) return copied;
    unsigned char expect[kBytes];
    std::memset(expect, 0xEE, kBytes);
    std::memcpy(expect + 4, source, 8);
    CHECK(bytes_match(got, expect, kBytes, "public_buffer"));
    queue->Close();
    return 0;
}

int main() {
    void* unsupported = nullptr;
    CHECK(DStorageGetFactory(file_iid, &unsupported) == E_NOINTERFACE && !unsupported);
    CHECK(DStorageGetFactory(factory_iid, nullptr) == E_POINTER);
    Com<Factory> factory;
    CHECK(DStorageGetFactory(factory_iid, factory.out()) == S_OK);

    Com<IUnknown> identity;
    CHECK(factory->QueryInterface(IID_IUnknown, identity.out()) == S_OK);
    CHECK(identity.value == factory.value);
    CHECK(factory->SetStagingBufferSize(0) == S_OK);

    TemporaryFile temporary;
    WCHAR directory[MAX_PATH];
    const DWORD directory_len = GetTempPathW(MAX_PATH, directory);
    CHECK(directory_len && directory_len < MAX_PATH);
    CHECK(GetTempFileNameW(directory, L"bxs", 0, temporary.name));
    const HANDLE writer = CreateFileW(temporary.name, GENERIC_WRITE, 0, nullptr,
                                      TRUNCATE_EXISTING, FILE_ATTRIBUTE_NORMAL, nullptr);
    CHECK(writer != INVALID_HANDLE_VALUE);
    constexpr char contents[] = "ABCDEF0123456789";
    DWORD written = 0;
    const BOOL wrote = WriteFile(writer, contents, sizeof(contents), &written, nullptr);
    CloseHandle(writer);
    CHECK(wrote && written == sizeof(contents));

    Com<File> file;
    CHECK(factory->OpenFile(temporary.name, file_iid, file.out()) == S_OK);
    BY_HANDLE_FILE_INFORMATION file_info{};
    CHECK(file->GetFileInformation(&file_info) == S_OK);
    CHECK(file_info.nFileSizeHigh == 0 && file_info.nFileSizeLow == sizeof(contents));

    Com<Status> status;
    CHECK(factory->CreateStatusArray(2, status_iid, status.out()) == S_OK);
    Event event;
    CHECK(event.handle);
    std::array<char, 5> first{}, second{}, copied{};
    Com<Queue> queue;
    CHECK(factory->CreateQueue(5, queue_iid, queue.out()) == S_OK);

    Request read1{file.value, 2, nullptr, first.data(), 4, 0};
    Request read2{file.value, 10, nullptr, second.data(), 4, 0};
    Request copy{nullptr, 0, first.data(), copied.data(), 4, 0};
    CHECK(queue->EnqueueRequest(&read1) == S_OK);
    CHECK(queue->EnqueueRequest(&read2) == S_OK);
    CHECK(queue->EnqueueRequest(&copy) == S_OK);
    CHECK(queue->EnqueueStatus(status.value, 0) == S_OK);
    CHECK(queue->EnqueueSetEvent(event.handle) == S_OK);
    CHECK(queue->EnqueueRequest(&copy) == HRESULT_FROM_WIN32(ERROR_BUSY));
    CHECK(status->GetResult(0) == E_PENDING);
    CHECK(WaitForSingleObject(event.handle, 0) == WAIT_TIMEOUT);
    CHECK(first[0] == 0 && second[0] == 0 && copied[0] == 0);

    CHECK(queue->Submit() == S_OK);
    CHECK(WaitForSingleObject(event.handle, 5000) == WAIT_OBJECT_0);
    CHECK(status->GetResult(0) == S_OK);
    CHECK(std::strcmp(first.data(), "CDEF") == 0);
    CHECK(std::strcmp(second.data(), "4567") == 0);
    CHECK(std::strcmp(copied.data(), "CDEF") == 0);

    ErrorRecord error{};
    CHECK(queue->RetrieveErrorRecord(&error) == S_OK && error.failure_count == 0);
    Request past_end{file.value, sizeof(contents), nullptr, first.data(), 1, 0};
    Request compressed{nullptr, 0, first.data(), copied.data(), 4, compression_lz};
    CHECK(queue->EnqueueRequest(&past_end) == S_OK);
    CHECK(queue->EnqueueRequest(&compressed) == S_OK);
    CHECK(queue->EnqueueStatus(status.value, 0) == S_OK);
    CHECK(queue->EnqueueStatus(status.value, 1) == S_OK);
    CHECK(queue->Submit() == S_OK);
    CHECK(file->Close() == S_OK);
    CHECK(file->GetFileInformation(&file_info) == HRESULT_FROM_WIN32(ERROR_INVALID_HANDLE));
    CHECK(queue->Close() == S_OK);

    CHECK(status->GetResult(0) == HRESULT_FROM_WIN32(ERROR_HANDLE_EOF));
    CHECK(status->GetResult(1) == S_OK);
    CHECK(queue->RetrieveErrorRecord(&error) == S_OK);
    CHECK(error.failure_count == 2 && error.first_error == HRESULT_FROM_WIN32(ERROR_HANDLE_EOF));
    CHECK(queue->RetrieveErrorRecord(&error) == S_OK && error.failure_count == 0);
    CHECK(queue->EnqueueRequest(&copy) == HRESULT_FROM_WIN32(ERROR_INVALID_HANDLE));
    QueueInfo info{};
    CHECK(queue->Query(&info) == S_OK && info.capacity == 5 && info.available == 5);

    // Validation edge cases and Close()-without-Submit() drain coverage.
    Com<Queue> edge_queue;
    CHECK(factory->CreateQueue(4, queue_iid, edge_queue.out()) == S_OK);
    CHECK(edge_queue->EnqueueRequest(nullptr) == E_INVALIDARG);
    Request overflow{file.value, static_cast<UINT64>(std::numeric_limits<LONGLONG>::max()), nullptr, first.data(), 1, 0};
    CHECK(edge_queue->EnqueueRequest(&overflow) == E_INVALIDARG);
    Request ambiguous{file.value, 0, first.data(), copied.data(), 4, 0};
    CHECK(edge_queue->EnqueueRequest(&ambiguous) == E_INVALIDARG);
    ForeignStatus foreign_status;
    CHECK(edge_queue->EnqueueStatus(&foreign_status, 0) == E_INVALIDARG);
    CHECK(edge_queue->EnqueueStatus(status.value, 2) == E_INVALIDARG);
    CHECK(edge_queue->EnqueueSignal(nullptr, 1) == E_POINTER);

    std::array<char, 5> src_close{"WXYZ"};
    std::array<char, 5> dst_close{};
    Request close_copy{nullptr, 0, src_close.data(), dst_close.data(), 4, 0};
    MockFence mock_fence;
    Event close_event;
    CHECK(close_event.handle);
    CHECK(edge_queue->EnqueueRequest(&close_copy) == S_OK);
    CHECK(edge_queue->EnqueueSignal(&mock_fence, 42) == S_OK);
    CHECK(edge_queue->EnqueueStatus(status.value, 0) == S_OK);
    CHECK(edge_queue->EnqueueStatus(status.value, 0) == HRESULT_FROM_WIN32(ERROR_BUSY));
    CHECK(edge_queue->EnqueueSetEvent(close_event.handle) == S_OK);
    CHECK(edge_queue->Close() == S_OK);
    CHECK(std::strcmp(dst_close.data(), "WXYZ") == 0);
    CHECK(mock_fence.value == 42 && mock_fence.refs == 1);
    CHECK(status->GetResult(0) == S_OK);
    CHECK(WaitForSingleObject(close_event.handle, 0) == WAIT_OBJECT_0);

    if (const int public_api = check_public_interfaces(temporary.name, contents)) return public_api;
    if (const int public_buffer = check_public_buffer_destination()) return public_buffer;
    if (const int compressed = check_compressed_rejection(factory.value)) return compressed;
    if (const int batched = check_batched_buffer_transfers(factory.value)) return batched;
    if (const int unconfirmed = check_unconfirmed_staged_copy(factory.value)) return unconfirmed;

    std::puts("test_xcrdapi: passed");
}

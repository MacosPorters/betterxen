#include "synthetic_storage.h"

#include <array>
#include <cstdio>
#include <cstring>
#include <limits>

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
    Request compressed{nullptr, 0, first.data(), copied.data(), 4, 1};
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

    std::puts("test_xcrdapi: passed");
}

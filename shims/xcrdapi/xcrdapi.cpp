#include "synthetic_storage.h"

#include <cstring>
#include <limits>
#include <new>

#include "bx_log.h"

namespace {
using namespace bx::storage;

HRESULT win32_error() { return HRESULT_FROM_WIN32(GetLastError()); }

class Lock {
public:
    explicit Lock(SRWLOCK& mutex) : mutex_(mutex) { AcquireSRWLockExclusive(&mutex_); }
    ~Lock() { ReleaseSRWLockExclusive(&mutex_); }

private:
    SRWLOCK& mutex_;
};

template<class Interface, const IID& interface_id>
class Object : public Interface {
public:
    HRESULT STDMETHODCALLTYPE QueryInterface(REFIID riid, void** result) override {
        if (!result) return E_POINTER;
        *result = nullptr;
        if (!IsEqualIID(riid, interface_id) && !IsEqualIID(riid, IID_IUnknown)) {
            return E_NOINTERFACE;
        }
        *result = static_cast<Interface*>(this);
        AddRef();
        return S_OK;
    }

    ULONG STDMETHODCALLTYPE AddRef() override { return InterlockedIncrement(&references_); }

    ULONG STDMETHODCALLTYPE Release() override {
        const ULONG references = InterlockedDecrement(&references_);
        if (!references) delete this;
        return references;
    }

protected:
    virtual ~Object() = default;

private:
    LONG references_ = 1;
};

template<class Interface>
HRESULT publish(Interface* object, REFIID iid, void** result) {
    const HRESULT status = object->QueryInterface(iid, result);
    object->Release();
    return status;
}

HRESULT duplicate_handle(HANDLE handle, HANDLE* copy) {
    const HANDLE process = GetCurrentProcess();
    return DuplicateHandle(process, handle, process, copy, 0, FALSE, DUPLICATE_SAME_ACCESS)
        ? S_OK : win32_error();
}

class StorageFile final : public Object<File, file_iid> {
public:
    explicit StorageFile(HANDLE handle) : handle_(handle) {}

    HRESULT STDMETHODCALLTYPE Close() override {
        Lock lock(mutex_);
        if (handle_ == INVALID_HANDLE_VALUE) return S_OK;
        if (!CloseHandle(handle_)) return win32_error();
        handle_ = INVALID_HANDLE_VALUE;
        return S_OK;
    }

    HRESULT STDMETHODCALLTYPE GetFileInformation(BY_HANDLE_FILE_INFORMATION* info) override {
        if (!info) return E_POINTER;
        Lock lock(mutex_);
        if (handle_ == INVALID_HANDLE_VALUE) return HRESULT_FROM_WIN32(ERROR_INVALID_HANDLE);
        return GetFileInformationByHandle(handle_, info) ? S_OK : win32_error();
    }

    HRESULT STDMETHODCALLTYPE GetHandle(HANDLE* copy) override {
        if (!copy) return E_POINTER;
        *copy = nullptr;
        Lock lock(mutex_);
        if (handle_ == INVALID_HANDLE_VALUE) return HRESULT_FROM_WIN32(ERROR_INVALID_HANDLE);
        return duplicate_handle(handle_, copy);
    }

private:
    ~StorageFile() override { Close(); }

    SRWLOCK mutex_ = SRWLOCK_INIT;
    HANDLE handle_;
};

class StatusArray final : public Object<Status, status_iid> {
public:
    struct Slot {
        HRESULT result = E_PENDING;
        bool busy = false;
    };

    StatusArray(Slot* slots, UINT count) : slots_(slots), count_(count) {}

    HRESULT STDMETHODCALLTYPE GetResult(UINT index) override {
        if (index >= count_) return E_INVALIDARG;
        Lock lock(mutex_);
        return slots_[index].result;
    }

    HRESULT begin(UINT index) {
        if (index >= count_) return E_INVALIDARG;
        Lock lock(mutex_);
        if (slots_[index].busy) return HRESULT_FROM_WIN32(ERROR_BUSY);
        slots_[index] = {E_PENDING, true};
        return S_OK;
    }

    void complete(UINT index, HRESULT result) {
        Lock lock(mutex_);
        slots_[index] = {result, false};
    }

private:
    ~StatusArray() override { delete[] slots_; }

    SRWLOCK mutex_ = SRWLOCK_INIT;
    Slot* slots_;
    UINT count_;
};

enum class Kind { request, status, signal, event };

struct Entry {
    Kind kind;
    Request request{};
    HANDLE handle = nullptr;
    StatusArray* status = nullptr;
    UINT index = 0;
    ID3D12Fence* fence = nullptr;
    UINT64 value = 0;
};

HRESULT read_request(HANDLE file, const Request& request) {
    if (!request.size) return S_OK;
    if (!file) {
        std::memmove(request.destination, request.source, request.size);
        return S_OK;
    }

    OVERLAPPED operation{};
    operation.Offset = static_cast<DWORD>(request.file_offset);
    operation.OffsetHigh = static_cast<DWORD>(request.file_offset >> 32);
    operation.hEvent = CreateEventW(nullptr, TRUE, FALSE, nullptr);
    if (!operation.hEvent) return win32_error();

    DWORD bytes = 0;
    HRESULT result = S_OK;
    if (!ReadFile(file, request.destination, request.size, &bytes, &operation)) {
        const DWORD error = GetLastError();
        if (error != ERROR_IO_PENDING) {
            result = HRESULT_FROM_WIN32(error);
        } else if (!GetOverlappedResult(file, &operation, &bytes, TRUE)) {
            result = win32_error();
        }
    }
    CloseHandle(operation.hEvent);
    if (SUCCEEDED(result) && bytes != request.size) return HRESULT_FROM_WIN32(ERROR_HANDLE_EOF);
    return result;
}

// Each queue is drained serially, in FIFO order, by a single threadpool work item.
class WorkQueue final : public Object<Queue, queue_iid> {
public:
    WorkQueue(Entry* entries, UINT capacity) : entries_(entries), capacity_(capacity) {}

    bool create_work() {
        work_ = CreateThreadpoolWork(run, this, nullptr);
        return work_ != nullptr;
    }

    HRESULT STDMETHODCALLTYPE EnqueueRequest(const Request* request) override {
        constexpr UINT64 max_file_offset = std::numeric_limits<LONGLONG>::max();
        if (!request || (request->file && request->source)
            || (request->size && (!request->destination || (!request->source && !request->file)))
            || (request->file && (request->file_offset > max_file_offset - request->size))) {
            return E_INVALIDARG;
        }
        Entry entry{Kind::request, *request};
        if (request->file && !request->compression) {
            const HRESULT result = request->file->GetHandle(&entry.handle);
            if (FAILED(result)) return result;
        }
        const HRESULT result = enqueue(entry);
        if (FAILED(result) && entry.handle) CloseHandle(entry.handle);
        return result;
    }

    HRESULT STDMETHODCALLTYPE EnqueueStatus(Status* status, UINT index) override {
        auto* array = dynamic_cast<StatusArray*>(status);
        if (!array) return E_INVALIDARG;

        Lock lock(mutex_);
        const HRESULT available = check_available();
        if (FAILED(available)) return available;
        const HRESULT result = array->begin(index);
        if (FAILED(result)) return result;

        array->AddRef();
        Entry entry{Kind::status};
        entry.status = array;
        entry.index = index;
        append(entry);
        return S_OK;
    }

    HRESULT STDMETHODCALLTYPE EnqueueSignal(ID3D12Fence* fence, UINT64 value) override {
        if (!fence) return E_POINTER;
        Entry entry{Kind::signal};
        entry.fence = fence;
        entry.value = value;
        fence->AddRef();
        const HRESULT result = enqueue(entry);
        if (FAILED(result)) fence->Release();
        return result;
    }

    HRESULT STDMETHODCALLTYPE EnqueueSetEvent(HANDLE event) override {
        if (!event || event == INVALID_HANDLE_VALUE) return E_INVALIDARG;
        Entry entry{Kind::event};
        const HRESULT duplicated = duplicate_handle(event, &entry.handle);
        if (FAILED(duplicated)) return duplicated;
        const HRESULT result = enqueue(entry);
        if (FAILED(result)) CloseHandle(entry.handle);
        return result;
    }

    HRESULT STDMETHODCALLTYPE Submit() override {
        Lock lock(mutex_);
        if (closed_) return HRESULT_FROM_WIN32(ERROR_INVALID_HANDLE);
        submit();
        return S_OK;
    }

    HRESULT STDMETHODCALLTYPE RetrieveErrorRecord(ErrorRecord* out) override {
        if (!out) return E_POINTER;
        Lock lock(mutex_);
        *out = errors_;
        errors_ = {};
        return S_OK;
    }

    HRESULT STDMETHODCALLTYPE Query(QueueInfo* info) override {
        if (!info) return E_POINTER;
        Lock lock(mutex_);
        *info = {capacity_, capacity_ - size_};
        return S_OK;
    }

    HRESULT STDMETHODCALLTYPE Close() override {
        {
            Lock lock(mutex_);
            closed_ = true;
            submit();
        }
        if (work_) WaitForThreadpoolWorkCallbacks(work_, FALSE);
        return S_OK;
    }

private:
    ~WorkQueue() override {
        Close();
        if (work_) CloseThreadpoolWork(work_);
        delete[] entries_;
    }

    HRESULT check_available() const {
        if (closed_) return HRESULT_FROM_WIN32(ERROR_INVALID_HANDLE);
        return size_ == capacity_ ? HRESULT_FROM_WIN32(ERROR_BUSY) : S_OK;
    }

    void append(const Entry& entry) {
        entries_[(head_ + size_) % capacity_] = entry;
        ++size_;
    }

    HRESULT enqueue(const Entry& entry) {
        Lock lock(mutex_);
        const HRESULT result = check_available();
        if (FAILED(result)) return result;
        append(entry);
        return S_OK;
    }

    void submit() {
        submitted_ = size_;
        if (submitted_ && !active_) {
            active_ = true;
            SubmitThreadpoolWork(work_);
        }
    }

    static void CALLBACK run(PTP_CALLBACK_INSTANCE, void* context, PTP_WORK) {
        static_cast<WorkQueue*>(context)->drain();
    }

    void drain() {
        for (;;) {
            Entry entry{};
            {
                Lock lock(mutex_);
                if (!submitted_) {
                    active_ = false;
                    return;
                }
                entry = entries_[head_];
            }
            HRESULT result = S_OK;
            switch (entry.kind) {
                case Kind::request:
                    if (entry.request.compression) {
                        BX_STUB_LOG("XCRDAPI.dll", "compression");
                        result = E_NOTIMPL;
                    } else {
                        result = read_request(entry.handle, entry.request);
                    }
                    if (entry.handle) CloseHandle(entry.handle);
                    break;
                case Kind::status:
                    entry.status->complete(entry.index, status_error_);
                    entry.status->Release();
                    status_error_ = S_OK;
                    break;
                case Kind::signal:
                    result = entry.fence->Signal(entry.value);
                    entry.fence->Release();
                    break;
                case Kind::event:
                    if (!SetEvent(entry.handle)) result = win32_error();
                    CloseHandle(entry.handle);
                    break;
            }
            Lock lock(mutex_);
            if (FAILED(result)) {
                if (SUCCEEDED(status_error_)) status_error_ = result;
                if (!errors_.failure_count) errors_.first_error = result;
                ++errors_.failure_count;
            }
            head_ = (head_ + 1) % capacity_;
            --size_;
            --submitted_;
        }
    }

    SRWLOCK mutex_ = SRWLOCK_INIT;
    Entry* entries_;
    UINT capacity_;
    UINT head_ = 0;
    UINT size_ = 0;
    UINT submitted_ = 0;
    bool active_ = false;
    bool closed_ = false;
    PTP_WORK work_ = nullptr;
    HRESULT status_error_ = S_OK;
    ErrorRecord errors_{};
};

class StorageFactory final : public Object<Factory, factory_iid> {
public:
    HRESULT STDMETHODCALLTYPE OpenFile(PCWSTR path, REFIID iid, void** out) override {
        if (!out) return E_POINTER;
        *out = nullptr;
        if (!path || !*path) return E_INVALIDARG;
        HANDLE handle = CreateFileW(path, GENERIC_READ, FILE_SHARE_READ, nullptr,
                                    OPEN_EXISTING, FILE_FLAG_OVERLAPPED, nullptr);
        if (handle == INVALID_HANDLE_VALUE) return win32_error();
        auto* file = new (std::nothrow) StorageFile(handle);
        if (!file) {
            CloseHandle(handle);
            return E_OUTOFMEMORY;
        }
        return publish(file, iid, out);
    }

    HRESULT STDMETHODCALLTYPE CreateQueue(UINT capacity, REFIID iid, void** out) override {
        if (!out) return E_POINTER;
        *out = nullptr;
        if (!capacity || capacity > 65535) return E_INVALIDARG;
        auto* entries = new (std::nothrow) Entry[capacity];
        if (!entries) return E_OUTOFMEMORY;
        auto* queue = new (std::nothrow) WorkQueue(entries, capacity);
        if (!queue) {
            delete[] entries;
            return E_OUTOFMEMORY;
        }
        if (!queue->create_work()) {
            const HRESULT error = win32_error();
            queue->Release();
            return error;
        }
        return publish(queue, iid, out);
    }

    HRESULT STDMETHODCALLTYPE CreateStatusArray(UINT count, REFIID iid, void** out) override {
        if (!out) return E_POINTER;
        *out = nullptr;
        if (!count || count > 65535) return E_INVALIDARG;
        auto* slots = new (std::nothrow) StatusArray::Slot[count];
        if (!slots) return E_OUTOFMEMORY;
        auto* array = new (std::nothrow) StatusArray(slots, count);
        if (!array) {
            delete[] slots;
            return E_OUTOFMEMORY;
        }
        return publish(array, iid, out);
    }

    HRESULT STDMETHODCALLTYPE SetStagingBufferSize(UINT size) override {
        if (!size) return S_OK;
        BX_STUB_LOG("XCRDAPI.dll", "SetStagingBufferSize");
        return E_NOTIMPL;
    }
};
} // namespace

extern "C" HRESULT WINAPI DStorageGetFactory(REFIID riid, void** ppv) {
    if (!ppv) return E_POINTER;
    *ppv = nullptr;
    if (!IsEqualIID(riid, factory_iid) && !IsEqualIID(riid, IID_IUnknown)) {
        BX_STUB_LOG("XCRDAPI.dll", "DStorageGetFactory");
        return E_NOINTERFACE;
    }
    auto* factory = new (std::nothrow) StorageFactory;
    return factory ? publish(factory, riid, ppv) : E_OUTOFMEMORY;
}

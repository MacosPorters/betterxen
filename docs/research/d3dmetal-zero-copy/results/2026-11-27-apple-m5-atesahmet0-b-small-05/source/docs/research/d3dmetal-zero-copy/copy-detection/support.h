// SPDX-License-Identifier: GPL-3.0-or-later
#pragma once

#include <windows.h>
#include <d3d12.h>
#include <immintrin.h>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <stdexcept>
#include <string>
#include <utility>

template <typename T> class Com {
    T* ptr = nullptr;
public:
    Com() = default;
    ~Com() { if (ptr) ptr->Release(); }
    Com(const Com&) = delete;
    Com& operator=(const Com&) = delete;
    Com(Com&& other) noexcept : ptr(std::exchange(other.ptr, nullptr)) {}
    Com& operator=(Com&& other) noexcept {
        std::swap(ptr, other.ptr);
        return *this;
    }
    T* get() const { return ptr; }
    T* operator->() const { return ptr; }
    void** out() { return reinterpret_cast<void**>(&ptr); }
};

inline void check(HRESULT hr, const char* name) {
    if (SUCCEEDED(hr)) return;
    std::fprintf(stderr, "%s hr=%08lx\n", name, static_cast<unsigned long>(hr));
    throw std::runtime_error(name);
}

inline long long ticks() {
    LARGE_INTEGER value;
    QueryPerformanceCounter(&value);
    return value.QuadPart;
}

inline double us(long long delta) {
    static const double frequency = [] {
        LARGE_INTEGER value;
        QueryPerformanceFrequency(&value);
        return double(value.QuadPart);
    }();
    return double(delta) * 1e6 / frequency;
}

inline D3D12_RESOURCE_DESC buffer_desc(UINT64 bytes, D3D12_RESOURCE_FLAGS flags) {
    D3D12_RESOURCE_DESC desc{};
    desc.Dimension = D3D12_RESOURCE_DIMENSION_BUFFER;
    desc.Width = bytes;
    desc.Height = 1;
    desc.DepthOrArraySize = 1;
    desc.MipLevels = 1;
    desc.SampleDesc.Count = 1;
    desc.Layout = D3D12_TEXTURE_LAYOUT_ROW_MAJOR;
    desc.Flags = flags;
    return desc;
}

enum class Heap { wb, wc, upload, readback, gpu };

inline D3D12_HEAP_PROPERTIES properties(Heap kind) {
    D3D12_HEAP_PROPERTIES prop{};
    prop.CreationNodeMask = prop.VisibleNodeMask = 1;
    switch (kind) {
    case Heap::wb:
    case Heap::wc:
        prop.Type = D3D12_HEAP_TYPE_CUSTOM;
        prop.MemoryPoolPreference = D3D12_MEMORY_POOL_L0;
        prop.CPUPageProperty = kind == Heap::wb
            ? D3D12_CPU_PAGE_PROPERTY_WRITE_BACK : D3D12_CPU_PAGE_PROPERTY_WRITE_COMBINE;
        break;
    case Heap::upload: prop.Type = D3D12_HEAP_TYPE_UPLOAD; break;
    case Heap::readback: prop.Type = D3D12_HEAP_TYPE_READBACK; break;
    case Heap::gpu: prop.Type = D3D12_HEAP_TYPE_DEFAULT; break;
    }
    return prop;
}

struct Device {
    HMODULE dll = LoadLibraryW(L"d3d12.dll");
    Com<ID3D12Device> device;
    Device() {
        if (!dll || GetModuleHandleW(L"d3d12_xs.dll"))
            throw std::runtime_error("host d3d12.dll or shim gate");
        auto create = reinterpret_cast<decltype(&D3D12CreateDevice)>(
            reinterpret_cast<void*>(GetProcAddress(dll, "D3D12CreateDevice")));
        if (!create) throw std::runtime_error("D3D12CreateDevice symbol");
        check(create(nullptr, D3D_FEATURE_LEVEL_11_0, __uuidof(ID3D12Device),
                     device.out()), "D3D12CreateDevice");
        char path[MAX_PATH]{};
        GetModuleFileNameA(dll, path, MAX_PATH);
        D3D12_FEATURE_DATA_ARCHITECTURE1 arch{};
        const HRESULT result = device->CheckFeatureSupport(D3D12_FEATURE_ARCHITECTURE1,
                                                          &arch, sizeof(arch));
        std::printf("DEVICE dll=%s shim=0 architecture_hr=%08lx uma=%d coherent=%d isolated_mmu=%d\n",
                    path, static_cast<unsigned long>(result), arch.UMA,
                    arch.CacheCoherentUMA, arch.IsolatedMMU);
    }

    Com<ID3D12Heap> heap(Heap kind, UINT64 bytes) {
        D3D12_HEAP_DESC desc{};
        desc.SizeInBytes = bytes;
        desc.Alignment = D3D12_DEFAULT_RESOURCE_PLACEMENT_ALIGNMENT;
        desc.Properties = properties(kind);
        desc.Flags = D3D12_HEAP_FLAG_ALLOW_ONLY_BUFFERS;
        Com<ID3D12Heap> result;
        check(device->CreateHeap(&desc, __uuidof(ID3D12Heap), result.out()), "CreateHeap");
        return result;
    }
};

struct Buffer {
    Com<ID3D12Heap> backing;
    Com<ID3D12Resource> resource;
    void* pointer = nullptr;
    Buffer(Device& d, Heap kind, UINT64 bytes, D3D12_RESOURCE_STATES state,
           D3D12_RESOURCE_FLAGS flags = D3D12_RESOURCE_FLAG_NONE) {
        const auto desc = buffer_desc(bytes, flags);
        if (kind == Heap::wb || kind == Heap::wc) {
            const auto alloc = d.device->GetResourceAllocationInfo(0, 1, &desc);
            backing = d.heap(kind, alloc.SizeInBytes);
            check(d.device->CreatePlacedResource(backing.get(), 0, &desc, state, nullptr,
                  __uuidof(ID3D12Resource), resource.out()), "CreatePlacedResource");
        } else {
            const auto prop = properties(kind);
            check(d.device->CreateCommittedResource(&prop, D3D12_HEAP_FLAG_NONE, &desc,
                  state, nullptr, __uuidof(ID3D12Resource), resource.out()),
                  "CreateCommittedResource");
        }
    }
    ~Buffer() { if (pointer) resource->Unmap(0, nullptr); }
    uint32_t* map() {
        check(resource->Map(0, nullptr, &pointer), "Map");
        if (!pointer) throw std::runtime_error("Map returned null");
        return static_cast<uint32_t*>(pointer);
    }
};

struct Queue {
    Com<ID3D12CommandQueue> queue;
    Com<ID3D12CommandAllocator> allocator;
    Com<ID3D12GraphicsCommandList> list;
    Com<ID3D12Fence> fence;
    HANDLE event = nullptr;
    UINT64 value = 0;
    ID3D12Device* device;
    explicit Queue(Device& d) : device(d.device.get()) {
        D3D12_COMMAND_QUEUE_DESC desc{};
        desc.Type = D3D12_COMMAND_LIST_TYPE_DIRECT;
        check(device->CreateCommandQueue(&desc, __uuidof(ID3D12CommandQueue), queue.out()), "CreateCommandQueue");
        check(device->CreateCommandAllocator(desc.Type, __uuidof(ID3D12CommandAllocator), allocator.out()), "CreateCommandAllocator");
        check(device->CreateCommandList(0, desc.Type, allocator.get(), nullptr,
              __uuidof(ID3D12GraphicsCommandList), list.out()), "CreateCommandList");
        check(list->Close(), "initial Close");
        check(device->CreateFence(0, D3D12_FENCE_FLAG_NONE, __uuidof(ID3D12Fence), fence.out()), "CreateFence");
        event = CreateEventW(nullptr, FALSE, FALSE, nullptr);
        if (!event) throw std::runtime_error("CreateEvent");
    }
    ~Queue() { CloseHandle(event); }
    void begin() {
        check(allocator->Reset(), "allocator Reset");
        check(list->Reset(allocator.get(), nullptr), "list Reset");
    }
    void transition(ID3D12Resource* resource, D3D12_RESOURCE_STATES from,
                    D3D12_RESOURCE_STATES to) {
        D3D12_RESOURCE_BARRIER barrier{};
        barrier.Type = D3D12_RESOURCE_BARRIER_TYPE_TRANSITION;
        barrier.Transition = {resource, D3D12_RESOURCE_BARRIER_ALL_SUBRESOURCES, from, to};
        list->ResourceBarrier(1, &barrier);
    }
    void alias(ID3D12Resource* before, ID3D12Resource* after) {
        D3D12_RESOURCE_BARRIER barrier{};
        barrier.Type = D3D12_RESOURCE_BARRIER_TYPE_ALIASING;
        barrier.Aliasing = {before, after};
        list->ResourceBarrier(1, &barrier);
    }
    void close() { check(list->Close(), "Close command list"); }
    struct Latency { double execute, signal, wait, total; };
    Latency execute(bool submit = true) {
        ID3D12CommandList* lists[] = {list.get()};
        const auto start = ticks();
        if (submit) queue->ExecuteCommandLists(1, lists);
        const auto executed = ticks();
        // A signal/event failure leaves work in flight. Terminate the disposable
        // guest without unwinding resources that may still be referenced.
        const HRESULT signal = queue->Signal(fence.get(), ++value);
        const auto signalled = ticks();
        const HRESULT arm = SUCCEEDED(signal) ? fence->SetEventOnCompletion(value, event) : signal;
        const DWORD wait = SUCCEEDED(arm) ? WaitForSingleObject(event, 5000) : WAIT_FAILED;
        const auto complete = ticks();
        if (wait != WAIT_OBJECT_0 || fence->GetCompletedValue() < value ||
            fence->GetCompletedValue() == UINT64_MAX) {
            std::fprintf(stderr, "fence failure signal=%08lx arm=%08lx wait=%lu completed=%llu target=%llu\n",
                         static_cast<unsigned long>(signal), static_cast<unsigned long>(arm),
                         static_cast<unsigned long>(wait), fence->GetCompletedValue(), value);
            std::fflush(nullptr);
            ExitProcess(2);
        }
        check(device->GetDeviceRemovedReason(), "GetDeviceRemovedReason");
        return {us(executed - start), us(signalled - executed), us(complete - signalled),
                us(complete - start)};
    }
};

inline void receipt_pause() {
    std::puts("BACKEND_READY");
    if (std::getchar() == EOF) throw std::runtime_error("backend receipt handshake");
}

struct Timestamp {
    Com<ID3D12QueryHeap> queries;
    Buffer output;
    UINT64 hz = 0;
    const UINT64* data;
    Timestamp(Device& d, Queue& q)
        : output(d, Heap::readback, 16, D3D12_RESOURCE_STATE_COPY_DEST) {
        const D3D12_QUERY_HEAP_DESC desc{D3D12_QUERY_HEAP_TYPE_TIMESTAMP, 2, 0};
        check(d.device->CreateQueryHeap(&desc, __uuidof(ID3D12QueryHeap), queries.out()), "timestamp heap");
        check(q.queue->GetTimestampFrequency(&hz), "GetTimestampFrequency");
        if (!hz) throw std::runtime_error("zero GPU timestamp frequency");
        data = reinterpret_cast<const UINT64*>(output.map());
        std::printf("TIMESTAMP hertz=%llu\n", hz);
    }
    void start(Queue& q) { q.list->EndQuery(queries.get(), D3D12_QUERY_TYPE_TIMESTAMP, 0); }
    void end(Queue& q) {
        q.list->EndQuery(queries.get(), D3D12_QUERY_TYPE_TIMESTAMP, 1);
        q.list->ResolveQueryData(queries.get(), D3D12_QUERY_TYPE_TIMESTAMP, 0, 2, output.resource.get(), 0);
    }
    double elapsed() const {
        if (data[1] <= data[0]) return -1;
        return double(data[1] - data[0]) * 1e6 / double(hz);
    }
};


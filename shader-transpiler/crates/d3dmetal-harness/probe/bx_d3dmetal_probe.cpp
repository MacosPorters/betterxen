// SPDX-License-Identifier: GPL-3.0-or-later
//
// bx-d3dmetal-probe: x86_64 helper for the BetterXen M1-D2 D3DMetal harness.
//
// D3DMetal.framework ships as x86_64 only, so it cannot be loaded into the
// arm64 harness process. This probe runs under Rosetta 2, links
// utmapp/d3dmetal-native (which hosts D3DMetal outside Wine), and:
//
//   1. loads the D3DMetal.framework named by --framework,
//   2. creates a D3D12 device and reports its highest shader model,
//   3. builds a compute PSO from the embedded SM 6.6 DXIL kernel,
//   4. fills a UAV buffer with a sentinel, dispatches the kernel, and
//   5. prints every word read back.
//
// The probe never decides whether the words are correct: the arm64 harness
// computes the expected values independently and judges the readback.
//
// Output protocol (stdout, one record per line, prefix "BXD3DM "):
//   BXD3DM protocol 2
//   BXD3DM init_failed <call>=<code>         (D3DMetal found but not usable)
//   BXD3DM framework <path>
//   BXD3DM step <name> hr=0x<8 hex>          (every D3D call that returns HRESULT)
//   BXD3DM shader_model 0x<2 hex>            (highest supported, e.g. 0x66)
//   BXD3DM dispatch groups=<n> threads=<n> words=<n> sentinel=0x<8 hex>
//   BXD3DM readback <words * 8 lowercase hex digits>
//   BXD3DM device_removed reason=0x<8 hex>   (fence reported UINT64_MAX)
//   BXD3DM timeout fence after <n> ms
//   BXD3DM done
// Exit status: 0 readback printed, 2 a step failed or the GPU did not finish,
// 3 d3dmetal-native could not initialize the framework, 64 usage error.
// A missing framework is a SKIP decided by the harness before the probe runs;
// a framework that fails to initialize is a failure.

#ifndef WIDL_EXPLICIT_AGGREGATE_RETURNS
#error "build with -DWIDL_EXPLICIT_AGGREGATE_RETURNS=1 to match D3DMetal's D3D12 ABI"
#endif

#include <cstdint>
#include <cstdio>
#include <cstring>
#include <ctime>

#include <d3d12.h>
#include <windows.h>

#include "d3dmetal_native.h"
#include "shaders/fill_cs66.h"

namespace {

constexpr int kProtocolVersion = 2; // bump with every change to the records above
constexpr UINT kGroups = 4;
constexpr UINT kThreadsPerGroup = 64; // must match [numthreads] in fill_cs66.hlsl
constexpr UINT kWords = kGroups * kThreadsPerGroup;
constexpr UINT64 kBytes = kWords * sizeof(uint32_t);
constexpr uint32_t kSentinel = 0xA5A5A5A5u;
constexpr int kFenceTimeoutMs = 10000;
constexpr UINT kShaderModel66 = 0x66; // the vendored d3d12.h stops at D3D_SHADER_MODEL_6_0

constexpr int kExitOk = 0;
constexpr int kExitStepFailed = 2;
constexpr int kExitInitFailed = 3;
constexpr int kExitUsage = 64;
// GetCompletedValue() returns this once the device has been removed.
constexpr UINT64 kDeviceRemovedFenceValue = UINT64_MAX;

// Minimal owning COM pointer: releases on scope exit.
template <typename T>
class ComPtr {
public:
    ComPtr() = default;
    ComPtr(const ComPtr&) = delete;
    ComPtr& operator=(const ComPtr&) = delete;
    ~ComPtr() {
        if (ptr_) ptr_->Release();
    }
    T* get() const { return ptr_; }
    T* operator->() const { return ptr_; }
    T** put() { return &ptr_; }
    void** out() { return reinterpret_cast<void**>(&ptr_); }

private:
    T* ptr_ = nullptr;
};

// Prints the step record; returns true when hr succeeded.
bool step(const char* name, HRESULT hr) {
    std::printf("BXD3DM step %s hr=0x%08x\n", name, static_cast<unsigned>(hr));
    std::fflush(stdout);
    return SUCCEEDED(hr);
}

D3D12_RESOURCE_DESC buffer_desc(UINT64 bytes, D3D12_RESOURCE_FLAGS flags) {
    D3D12_RESOURCE_DESC desc = {};
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

HRESULT create_buffer(ID3D12Device* device, D3D12_HEAP_TYPE heap, D3D12_RESOURCE_STATES state,
                      D3D12_RESOURCE_FLAGS flags, ComPtr<ID3D12Resource>& out) {
    D3D12_HEAP_PROPERTIES props = {};
    props.Type = heap;
    const D3D12_RESOURCE_DESC desc = buffer_desc(kBytes, flags);
    return device->CreateCommittedResource(&props, D3D12_HEAP_FLAG_NONE, &desc, state, nullptr,
                                           __uuidof(ID3D12Resource), out.out());
}

D3D12_RESOURCE_BARRIER transition(ID3D12Resource* resource, D3D12_RESOURCE_STATES before,
                                  D3D12_RESOURCE_STATES after) {
    D3D12_RESOURCE_BARRIER barrier = {};
    barrier.Type = D3D12_RESOURCE_BARRIER_TYPE_TRANSITION;
    barrier.Transition.pResource = resource;
    barrier.Transition.Subresource = D3D12_RESOURCE_BARRIER_ALL_SUBRESOURCES;
    barrier.Transition.StateBefore = before;
    barrier.Transition.StateAfter = after;
    return barrier;
}

enum class FenceWait { Completed, DeviceRemoved, TimedOut };

// Polls the fence; D3DMetal's event handles are not Win32 events outside Wine.
// The device-removed value is checked first: it would otherwise satisfy `>= value`.
FenceWait wait_for_fence(ID3D12Fence* fence, UINT64 value) {
    for (int ms = 0; ms <= kFenceTimeoutMs; ++ms) {
        const UINT64 completed = fence->GetCompletedValue();
        if (completed == kDeviceRemovedFenceValue) return FenceWait::DeviceRemoved;
        if (completed >= value) return FenceWait::Completed;
        const timespec one_ms = {0, 1000000};
        nanosleep(&one_ms, nullptr);
    }
    return FenceWait::TimedOut;
}

int run(const char* framework_path) {
    // A neutral executable name keeps D3DMetal's per-application profiles out of the run.
    const dmn_result path_set = dmn_set_executable_path("/betterxen/bx-d3dmetal-probe.exe");
    if (path_set != DMN_SUCCESS) {
        std::printf("BXD3DM init_failed dmn_set_executable_path=%d\n", static_cast<int>(path_set));
        return kExitInitFailed;
    }
    dmn_options options = {};
    options.struct_size = sizeof(options);
    options.framework_path = framework_path;
    const dmn_result init = dmn_init(&options);
    if (init != DMN_SUCCESS) {
        std::printf("BXD3DM init_failed dmn_init=%d\n", static_cast<int>(init));
        return kExitInitFailed;
    }
    const char* loaded = dmn_framework_path();
    std::printf("BXD3DM framework %s\n", loaded ? loaded : "(unknown)");

    ComPtr<ID3D12Device> device;
    if (!step("D3D12CreateDevice", D3D12CreateDevice(nullptr, D3D_FEATURE_LEVEL_11_0,
                                                     __uuidof(ID3D12Device), device.out())))
        return kExitStepFailed;

    D3D12_FEATURE_DATA_SHADER_MODEL shader_model = {};
    shader_model.HighestShaderModel = static_cast<D3D_SHADER_MODEL>(kShaderModel66);
    if (!step("CheckFeatureSupport(SHADER_MODEL)",
              device->CheckFeatureSupport(D3D12_FEATURE_SHADER_MODEL, &shader_model,
                                          sizeof(shader_model))))
        return kExitStepFailed;
    std::printf("BXD3DM shader_model 0x%02x\n",
                static_cast<unsigned>(shader_model.HighestShaderModel));

    D3D12_ROOT_PARAMETER uav_param = {};
    uav_param.ParameterType = D3D12_ROOT_PARAMETER_TYPE_UAV;
    uav_param.Descriptor.ShaderRegister = 0;
    uav_param.ShaderVisibility = D3D12_SHADER_VISIBILITY_ALL;
    D3D12_ROOT_SIGNATURE_DESC root_desc = {};
    root_desc.NumParameters = 1;
    root_desc.pParameters = &uav_param;
    ComPtr<ID3DBlob> root_blob;
    ComPtr<ID3DBlob> root_error;
    if (!step("D3D12SerializeRootSignature",
              D3D12SerializeRootSignature(&root_desc, D3D_ROOT_SIGNATURE_VERSION_1,
                                          root_blob.put(), root_error.put())))
        return kExitStepFailed;
    ComPtr<ID3D12RootSignature> root_signature;
    if (!step("CreateRootSignature",
              device->CreateRootSignature(0, root_blob->GetBufferPointer(),
                                          root_blob->GetBufferSize(),
                                          __uuidof(ID3D12RootSignature),
                                          root_signature.out())))
        return kExitStepFailed;

    D3D12_COMPUTE_PIPELINE_STATE_DESC pso_desc = {};
    pso_desc.pRootSignature = root_signature.get();
    pso_desc.CS.pShaderBytecode = g_fill_cs66;
    pso_desc.CS.BytecodeLength = sizeof(g_fill_cs66);
    ComPtr<ID3D12PipelineState> pso;
    if (!step("CreateComputePipelineState",
              device->CreateComputePipelineState(&pso_desc, __uuidof(ID3D12PipelineState),
                                                 pso.out())))
        return kExitStepFailed;

    D3D12_COMMAND_QUEUE_DESC queue_desc = {};
    queue_desc.Type = D3D12_COMMAND_LIST_TYPE_DIRECT;
    ComPtr<ID3D12CommandQueue> queue;
    ComPtr<ID3D12CommandAllocator> allocator;
    ComPtr<ID3D12GraphicsCommandList> list;
    ComPtr<ID3D12Fence> fence;
    if (!step("CreateCommandQueue",
              device->CreateCommandQueue(&queue_desc, __uuidof(ID3D12CommandQueue),
                                         queue.out())) ||
        !step("CreateCommandAllocator",
              device->CreateCommandAllocator(D3D12_COMMAND_LIST_TYPE_DIRECT,
                                             __uuidof(ID3D12CommandAllocator),
                                             allocator.out())) ||
        !step("CreateCommandList",
              device->CreateCommandList(0, D3D12_COMMAND_LIST_TYPE_DIRECT, allocator.get(),
                                        pso.get(), __uuidof(ID3D12GraphicsCommandList),
                                        list.out())) ||
        !step("CreateFence", device->CreateFence(0, D3D12_FENCE_FLAG_NONE,
                                                 __uuidof(ID3D12Fence), fence.out())))
        return kExitStepFailed;

    ComPtr<ID3D12Resource> output;
    ComPtr<ID3D12Resource> upload;
    ComPtr<ID3D12Resource> readback;
    if (!step("CreateCommittedResource(output)",
              create_buffer(device.get(), D3D12_HEAP_TYPE_DEFAULT, D3D12_RESOURCE_STATE_COPY_DEST,
                            D3D12_RESOURCE_FLAG_ALLOW_UNORDERED_ACCESS, output)) ||
        !step("CreateCommittedResource(upload)",
              create_buffer(device.get(), D3D12_HEAP_TYPE_UPLOAD,
                            D3D12_RESOURCE_STATE_GENERIC_READ, D3D12_RESOURCE_FLAG_NONE, upload)) ||
        !step("CreateCommittedResource(readback)",
              create_buffer(device.get(), D3D12_HEAP_TYPE_READBACK,
                            D3D12_RESOURCE_STATE_COPY_DEST, D3D12_RESOURCE_FLAG_NONE, readback)))
        return kExitStepFailed;

    // The sentinel tells "kernel never wrote" apart from "kernel wrote wrong values".
    void* mapped = nullptr;
    if (!step("Map(upload)", upload->Map(0, nullptr, &mapped))) return kExitStepFailed;
    uint32_t* fill = static_cast<uint32_t*>(mapped);
    for (UINT i = 0; i < kWords; ++i) fill[i] = kSentinel;
    upload->Unmap(0, nullptr);

    list->CopyBufferRegion(output.get(), 0, upload.get(), 0, kBytes);
    const D3D12_RESOURCE_BARRIER to_uav = transition(
        output.get(), D3D12_RESOURCE_STATE_COPY_DEST, D3D12_RESOURCE_STATE_UNORDERED_ACCESS);
    list->ResourceBarrier(1, &to_uav);
    list->SetComputeRootSignature(root_signature.get());
    list->SetPipelineState(pso.get());
    list->SetComputeRootUnorderedAccessView(0, output->GetGPUVirtualAddress());
    list->Dispatch(kGroups, 1, 1);
    const D3D12_RESOURCE_BARRIER to_copy = transition(
        output.get(), D3D12_RESOURCE_STATE_UNORDERED_ACCESS, D3D12_RESOURCE_STATE_COPY_SOURCE);
    list->ResourceBarrier(1, &to_copy);
    list->CopyBufferRegion(readback.get(), 0, output.get(), 0, kBytes);
    if (!step("Close", list->Close())) return kExitStepFailed;

    ID3D12CommandList* lists[] = {list.get()};
    queue->ExecuteCommandLists(1, lists);
    if (!step("Signal", queue->Signal(fence.get(), 1))) return kExitStepFailed;
    switch (wait_for_fence(fence.get(), 1)) {
    case FenceWait::Completed:
        break;
    case FenceWait::DeviceRemoved:
        std::printf("BXD3DM device_removed reason=0x%08x\n",
                    static_cast<unsigned>(device->GetDeviceRemovedReason()));
        return kExitStepFailed;
    case FenceWait::TimedOut:
        std::printf("BXD3DM timeout fence after %d ms\n", kFenceTimeoutMs);
        return kExitStepFailed;
    }

    std::printf("BXD3DM dispatch groups=%u threads=%u words=%u sentinel=0x%08x\n", kGroups,
                kThreadsPerGroup, kWords, kSentinel);
    const D3D12_RANGE read_range = {0, static_cast<SIZE_T>(kBytes)};
    if (!step("Map(readback)", readback->Map(0, &read_range, &mapped))) return kExitStepFailed;
    const uint32_t* words = static_cast<const uint32_t*>(mapped);
    std::printf("BXD3DM readback ");
    for (UINT i = 0; i < kWords; ++i) std::printf("%08x", words[i]);
    std::printf("\n");
    const D3D12_RANGE no_write = {0, 0};
    readback->Unmap(0, &no_write);
    std::printf("BXD3DM done\n");
    return kExitOk;
}

} // namespace

int main(int argc, char** argv) {
    std::printf("BXD3DM protocol %d\n", kProtocolVersion);
    if (argc != 3 || std::strcmp(argv[1], "--framework") != 0) {
        std::fprintf(stderr, "usage: %s --framework <D3DMetal.framework>\n", argv[0]);
        return kExitUsage;
    }
    return run(argv[2]);
}

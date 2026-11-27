#pragma once
// SPDX-License-Identifier: GPL-3.0-or-later
// Supplemental original synthetic tests; baseline ../probe.cpp is unchanged.
#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include <algorithm>
#include <array>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <d3d12.h>
#include <d3dcompiler.h>
#include <dxgi1_4.h>
#include <emmintrin.h>
#include <stdexcept>
#include <string>
#include <vector>
#include <windows.h>

template <class T> struct Ref {
  T *p = nullptr;
  Ref() = default;
  Ref(const Ref &) = delete;
  Ref &operator=(const Ref &) = delete;
  ~Ref() { clear(); }
  void clear() {
    if (p) {
      p->Release();
      p = nullptr;
    }
  }
  T *operator->() const { return p; }
  void **out() { return reinterpret_cast<void **>(&p); }
};
inline void require(HRESULT h, const char *name) {
  if (FAILED(h)) {
    std::printf("FAIL %s HRESULT=0x%08lX\n", name, (unsigned long)h);
    throw std::runtime_error(name);
  }
}
inline void report(HRESULT h, const char *name) {
  std::printf("API %s HRESULT=0x%08lX\n", name, (unsigned long)h);
}
template <class T> static T symbol(HMODULE m, const char *name) {
  FARPROC p = GetProcAddress(m, name);
  if (!p)
    throw std::runtime_error(name);
  T result;
  static_assert(sizeof(result) == sizeof(p));
  std::memcpy(&result, &p, sizeof(p));
  return result;
}
inline double ticks() {
  LARGE_INTEGER t;
  QueryPerformanceCounter(&t);
  return double(t.QuadPart);
}
inline double frequency() {
  LARGE_INTEGER t;
  QueryPerformanceFrequency(&t);
  return double(t.QuadPart);
}
inline D3D12_HEAP_PROPERTIES properties(D3D12_HEAP_TYPE type) {
  D3D12_HEAP_PROPERTIES p{};
  p.Type = type;
  p.CreationNodeMask = 1;
  p.VisibleNodeMask = 1;
  return p;
}
static D3D12_RESOURCE_DESC
bufferDesc(UINT64 bytes,
           D3D12_RESOURCE_FLAGS flags = D3D12_RESOURCE_FLAG_NONE) {
  D3D12_RESOURCE_DESC r{};
  r.Dimension = D3D12_RESOURCE_DIMENSION_BUFFER;
  r.Width = bytes;
  r.Height = 1;
  r.DepthOrArraySize = 1;
  r.MipLevels = 1;
  r.SampleDesc.Count = 1;
  r.Layout = D3D12_TEXTURE_LAYOUT_ROW_MAJOR;
  r.Flags = flags;
  return r;
}
struct Context {
  HMODULE dll = nullptr;
  Ref<ID3D12Device3> device;
  Context() {
    dll = LoadLibraryA("d3d12.dll");
    if (!dll)
      throw std::runtime_error("LoadLibrary d3d12");
    char path[2048]{};
    GetModuleFileNameA(dll, path, sizeof(path));
    std::printf("D3D12_DLL %s\n", path);
    using Create =
        HRESULT(WINAPI *)(IUnknown *, D3D_FEATURE_LEVEL, REFIID, void **);
    require(symbol<Create>(dll, "D3D12CreateDevice")(
                nullptr, D3D_FEATURE_LEVEL_11_0, __uuidof(ID3D12Device3),
                device.out()),
            "CreateDevice3");
    bool shim = GetModuleHandleA("d3d12_xs.dll") != nullptr;
    std::printf("SHIM_LOADED %d\n", shim);
    if (shim)
      throw std::runtime_error("shim contamination");
  }
};
struct Queue {
  ID3D12Device3 *d;
  Ref<ID3D12CommandQueue> q;
  Ref<ID3D12CommandAllocator> allocator;
  Ref<ID3D12GraphicsCommandList> list;
  Ref<ID3D12Fence> fence;
  HANDLE event = nullptr;
  UINT64 value = 0;
  explicit Queue(ID3D12Device3 *device) : d(device) {
    D3D12_COMMAND_QUEUE_DESC desc{};
    desc.Type = D3D12_COMMAND_LIST_TYPE_DIRECT;
    require(d->CreateCommandQueue(&desc, __uuidof(ID3D12CommandQueue), q.out()),
            "CreateCommandQueue");
    require(d->CreateCommandAllocator(
                desc.Type, __uuidof(ID3D12CommandAllocator), allocator.out()),
            "CreateCommandAllocator");
    require(d->CreateCommandList(0, desc.Type, allocator.p, nullptr,
                                 __uuidof(ID3D12GraphicsCommandList),
                                 list.out()),
            "CreateCommandList");
    require(list->Close(), "initial Close");
    require(d->CreateFence(0, D3D12_FENCE_FLAG_NONE, __uuidof(ID3D12Fence),
                           fence.out()),
            "CreateFence");
    event = CreateEventW(nullptr, FALSE, FALSE, nullptr);
    if (!event)
      throw std::runtime_error("CreateEvent");
  }
  ~Queue() {
    if (event)
      CloseHandle(event);
  }
  void begin() {
    require(allocator->Reset(), "allocator Reset");
    require(list->Reset(allocator.p, nullptr), "list Reset");
  }
  void transition(ID3D12Resource *r, D3D12_RESOURCE_STATES a,
                  D3D12_RESOURCE_STATES b) {
    if (a == b)
      return;
    D3D12_RESOURCE_BARRIER x{};
    x.Type = D3D12_RESOURCE_BARRIER_TYPE_TRANSITION;
    x.Transition.pResource = r;
    x.Transition.StateBefore = a;
    x.Transition.StateAfter = b;
    x.Transition.Subresource = D3D12_RESOURCE_BARRIER_ALL_SUBRESOURCES;
    list->ResourceBarrier(1, &x);
  }
  void close() { require(list->Close(), "Close"); }
  [[noreturn]] void stopInFlight(const char *operation, HRESULT status,
                                UINT64 target, UINT64 completed,
                                DWORD wait = WAIT_FAILED) {
    std::printf("FAIL in_flight operation=%s HRESULT=0x%08lX target=%llu "
                "completed=%llu wait=%lu process_exit=2\n",
                operation, (unsigned long)status, target, completed,
                (unsigned long)wait);
    std::fflush(nullptr);
    // The disposable guest must not unwind/recycle resources without proof of
    // completion. Its supervising runner records exit 2 as a failed case.
    ExitProcess(2);
  }
  void waitFor(UINT64 target) {
    UINT64 completed = fence->GetCompletedValue();
    if (completed == ~UINT64(0))
      stopInFlight("device removed fence", E_FAIL, target, completed);
    if (completed >= target)
      return;
    HRESULT armed = fence->SetEventOnCompletion(target, event);
    if (FAILED(armed))
      stopInFlight("SetEventOnCompletion", armed, target, completed);
    DWORD waited = WaitForSingleObject(event, 10000);
    completed = fence->GetCompletedValue();
    if (waited != WAIT_OBJECT_0 || completed == ~UINT64(0) || completed < target)
      stopInFlight("fence completion", E_FAIL, target, completed, waited);
  }
  void executeClosed() {
    ID3D12CommandList *lists[] = {list.p};
    q->ExecuteCommandLists(1, lists);
    HRESULT signalled = q->Signal(fence.p, ++value);
    if (FAILED(signalled))
      stopInFlight("Signal", signalled, value, fence->GetCompletedValue());
    waitFor(value);
  }
  void finish() {
    close();
    _mm_sfence();
    executeClosed();
    require(d->GetDeviceRemovedReason(), "GetDeviceRemovedReason");
  }
};
struct Buffer {
  Ref<ID3D12Heap> heap;
  Ref<ID3D12Resource> resource;
  void *cpu = nullptr;
  UINT64 size = 0;
  void allocate(ID3D12Device3 *d, UINT64 bytes, const std::string &type,
                bool uav = false) {
    size = bytes;
    auto desc =
        bufferDesc(bytes, uav ? D3D12_RESOURCE_FLAG_ALLOW_UNORDERED_ACCESS
                              : D3D12_RESOURCE_FLAG_NONE);
    auto state =
        uav ? D3D12_RESOURCE_STATE_COMMON : D3D12_RESOURCE_STATE_GENERIC_READ;
    if (type == "upload" || type == "readback") {
      if (uav)
        throw std::runtime_error(
            "UAV on upload/readback forbidden in this probe");
      auto p = properties(type == "upload" ? D3D12_HEAP_TYPE_UPLOAD
                                           : D3D12_HEAP_TYPE_READBACK);
      if (type == "readback")
        state = D3D12_RESOURCE_STATE_COPY_DEST;
      require(d->CreateCommittedResource(&p, D3D12_HEAP_FLAG_NONE, &desc, state,
                                         nullptr, __uuidof(ID3D12Resource),
                                         resource.out()),
              "committed buffer");
    } else {
      D3D12_HEAP_DESC hd{};
      hd.SizeInBytes = (bytes + 65535) & ~UINT64(65535);
      hd.Alignment = 65536;
      hd.Properties = properties(D3D12_HEAP_TYPE_CUSTOM);
      hd.Properties.CPUPageProperty =
          type == "wc" ? D3D12_CPU_PAGE_PROPERTY_WRITE_COMBINE
                       : D3D12_CPU_PAGE_PROPERTY_WRITE_BACK;
      hd.Properties.MemoryPoolPreference = D3D12_MEMORY_POOL_L0;
      require(d->CreateHeap(&hd, __uuidof(ID3D12Heap), heap.out()),
              "custom heap");
      require(d->CreatePlacedResource(heap.p, 0, &desc, state, nullptr,
                                      __uuidof(ID3D12Resource), resource.out()),
              "placed buffer");
    }
  }
  void map() {
    require(resource->Map(0, nullptr, &cpu), "Map buffer");
    if (!cpu)
      throw std::runtime_error("null buffer pointer");
  }
  void release() {
    cpu = nullptr;
    resource.clear();
    heap.clear();
  }
};
[[maybe_unused]] inline void identity(Context &c) {
  using Factory = HRESULT(WINAPI *)(UINT, REFIID, void **);
  HMODULE dxgi = LoadLibraryA("dxgi.dll");
  if (!dxgi)
    throw std::runtime_error("dxgi load");
  Ref<IDXGIFactory4> f;
  require(symbol<Factory>(dxgi, "CreateDXGIFactory2")(
              0, __uuidof(IDXGIFactory4), f.out()),
          "factory4");
  LUID luid = c.device->GetAdapterLuid();
  Ref<IDXGIAdapter2> a;
  require(f->EnumAdapterByLuid(luid, __uuidof(IDXGIAdapter2), a.out()),
          "adapter by device LUID");
  DXGI_ADAPTER_DESC2 ad{};
  require(a->GetDesc2(&ad), "GetDesc2");
  char name[512]{};
  WideCharToMultiByte(CP_UTF8, 0, ad.Description, -1, name, sizeof(name),
                      nullptr, nullptr);
  std::printf(
      "ADAPTER description=%s vendor=0x%04X device=0x%04X subsystem=0x%08X "
      "revision=%u flags=%u dedicated_video_bytes=%llu "
      "dedicated_system_bytes=%llu shared_system_bytes=%llu luid=%08lX:%08lX\n",
      name, ad.VendorId, ad.DeviceId, ad.SubSysId, ad.Revision, ad.Flags,
      (unsigned long long)ad.DedicatedVideoMemory,
      (unsigned long long)ad.DedicatedSystemMemory,
      (unsigned long long)ad.SharedSystemMemory, (unsigned long)luid.HighPart,
      (unsigned long)luid.LowPart);
  D3D12_FEATURE_DATA_D3D12_OPTIONS opts{};
  require(c.device->CheckFeatureSupport(D3D12_FEATURE_D3D12_OPTIONS, &opts,
                                        sizeof(opts)),
          "options");
  std::printf(
      "OPTIONS ResourceHeapTier=%d CrossAdapterRowMajorTextureSupported=%d "
      "StandardSwizzle64KBSupported=%d\n",
      opts.ResourceHeapTier, opts.CrossAdapterRowMajorTextureSupported,
      opts.StandardSwizzle64KBSupported);
  D3D12_FEATURE_DATA_ARCHITECTURE1 arch{};
  require(c.device->CheckFeatureSupport(D3D12_FEATURE_ARCHITECTURE1, &arch,
                                        sizeof(arch)),
          "architecture1");
  std::printf(
      "ARCH UMA=%d CacheCoherentUMA=%d TileBasedRenderer=%d IsolatedMMU=%d\n",
      arch.UMA, arch.CacheCoherentUMA, arch.TileBasedRenderer,
      arch.IsolatedMMU);
  for (auto type : {D3D12_HEAP_TYPE_DEFAULT, D3D12_HEAP_TYPE_UPLOAD,
                    D3D12_HEAP_TYPE_READBACK}) {
    auto p = c.device->GetCustomHeapProperties(0, type);
    std::printf(
        "CUSTOM_PROPERTIES input_type=%d Type=%d CPUPageProperty=%d "
        "MemoryPoolPreference=%d CreationNodeMask=%u VisibleNodeMask=%u\n",
        type, p.Type, p.CPUPageProperty, p.MemoryPoolPreference,
        p.CreationNodeMask, p.VisibleNodeMask);
  }
  SYSTEM_INFO si{};
  GetSystemInfo(&si);
  std::printf("GUEST_PAGE_SIZE %lu ALLOCATION_GRANULARITY %lu\n", si.dwPageSize,
              si.dwAllocationGranularity);
}
struct Shader {
  Ref<ID3D12RootSignature> root;
  Ref<ID3D12PipelineState> pipeline;
  static constexpr const char *code = R"HLSL(
ByteAddressBuffer source : register(t0);
RWByteAddressBuffer result : register(u0);
cbuffer Parameters : register(b0) { uint count; uint salt; uint wordStride; };
[numthreads(64,1,1)]
void main(uint3 tid : SV_DispatchThreadID) {
    uint accum=0;
    for(uint i=tid.x; i<count; i+=256) accum ^= source.Load(i*wordStride*4)+salt;
    result.Store(tid.x*4,accum);
}
)HLSL";
  explicit Shader(Context &c) {
    HMODULE compiler = LoadLibraryA("d3dcompiler_47.dll");
    if (!compiler)
      throw std::runtime_error("d3dcompiler47 unavailable");
    char path[2048]{};
    GetModuleFileNameA(compiler, path, sizeof(path));
    std::printf("SHADER_COMPILER %s\n", path);
    using Compile = HRESULT(WINAPI *)(
        LPCVOID, SIZE_T, LPCSTR, const D3D_SHADER_MACRO *, ID3DInclude *,
        LPCSTR, LPCSTR, UINT, UINT, ID3DBlob **, ID3DBlob **);
    Ref<ID3DBlob> binary, error;
    HRESULT h = symbol<Compile>(compiler, "D3DCompile")(
        code, std::strlen(code), "original-phase2.hlsl", nullptr, nullptr,
        "main", "cs_5_0", D3DCOMPILE_OPTIMIZATION_LEVEL3, 0, &binary.p,
        &error.p);
    if (error.p)
      std::printf("COMPILER_LOG %.*s\n", (int)error->GetBufferSize(),
                  (const char *)error->GetBufferPointer());
    require(h, "D3DCompile cs5_0");
    std::printf("SHADER_BYTECODE bytes=%zu magic=%.4s\n",
                binary->GetBufferSize(), (char *)binary->GetBufferPointer());
    FILE *bytecode = std::fopen("consumption.dxbc", "wb");
    if (!bytecode ||
        std::fwrite(binary->GetBufferPointer(), 1, binary->GetBufferSize(),
                    bytecode) != binary->GetBufferSize())
      throw std::runtime_error("archive shader");
    std::fclose(bytecode);
    FILE *hlsl = std::fopen("consumption.hlsl", "wb");
    if (!hlsl)
      throw std::runtime_error("archive HLSL");
    std::fwrite(code, 1, std::strlen(code), hlsl);
    std::fclose(hlsl);
    D3D12_ROOT_PARAMETER params[3]{};
    params[0].ParameterType = D3D12_ROOT_PARAMETER_TYPE_SRV;
    params[0].Descriptor.ShaderRegister = 0;
    params[1].ParameterType = D3D12_ROOT_PARAMETER_TYPE_UAV;
    params[1].Descriptor.ShaderRegister = 0;
    params[2].ParameterType = D3D12_ROOT_PARAMETER_TYPE_32BIT_CONSTANTS;
    params[2].Constants.ShaderRegister = 0;
    params[2].Constants.Num32BitValues = 3;
    D3D12_ROOT_SIGNATURE_DESC desc{};
    desc.NumParameters = 3;
    desc.pParameters = params;
    using Serialize =
        HRESULT(WINAPI *)(const D3D12_ROOT_SIGNATURE_DESC *,
                          D3D_ROOT_SIGNATURE_VERSION, ID3DBlob **, ID3DBlob **);
    Ref<ID3DBlob> serialized, serializeError;
    require(symbol<Serialize>(c.dll, "D3D12SerializeRootSignature")(
                &desc, D3D_ROOT_SIGNATURE_VERSION_1, &serialized.p,
                &serializeError.p),
            "serialize root");
    require(c.device->CreateRootSignature(
                0, serialized->GetBufferPointer(), serialized->GetBufferSize(),
                __uuidof(ID3D12RootSignature), root.out()),
            "CreateRootSignature");
    D3D12_COMPUTE_PIPELINE_STATE_DESC pd{};
    pd.pRootSignature = root.p;
    pd.CS = {binary->GetBufferPointer(), binary->GetBufferSize()};
    require(c.device->CreateComputePipelineState(
                &pd, __uuidof(ID3D12PipelineState), pipeline.out()),
            "CreateComputePipelineState");
  }
  void dispatch(Queue &q, Buffer &src, Buffer &dst, UINT count, UINT salt,
                UINT stride) {
    q.begin();
    q.transition(dst.resource.p, D3D12_RESOURCE_STATE_COMMON,
                 D3D12_RESOURCE_STATE_UNORDERED_ACCESS);
    q.list->SetPipelineState(pipeline.p);
    q.list->SetComputeRootSignature(root.p);
    q.list->SetComputeRootShaderResourceView(
        0, src.resource->GetGPUVirtualAddress());
    q.list->SetComputeRootUnorderedAccessView(
        1, dst.resource->GetGPUVirtualAddress());
    UINT values[] = {count, salt, stride};
    q.list->SetComputeRoot32BitConstants(2, 3, values, 0);
    q.list->Dispatch(4, 1, 1);
    q.transition(dst.resource.p, D3D12_RESOURCE_STATE_UNORDERED_ACCESS,
                 D3D12_RESOURCE_STATE_COMMON);
    q.finish();
  }
};
inline uint32_t pattern(UINT64 i, UINT seed) {
  uint32_t x = uint32_t(i) ^ seed;
  x ^= x >> 16;
  x *= 0x7FEB352Du;
  x ^= x >> 15;
  x *= 0x846CA68Bu;
  return x ^ (x >> 16);
}

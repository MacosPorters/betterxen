// SPDX-License-Identifier: GPL-3.0-or-later
// Original BetterXen workload (shader wave-semantics investigation): a minimal Direct3D 12
// compute host that is independent of the vendored recompiler.
//
//   d3d12_run.exe caps
//       Prints the default adapter and the device capabilities relevant to wave operations.
//   d3d12_run.exe run <shader.dxil> <groups_x> <out_dwords>
//       Creates a root signature with one root UAV (u0), a compute PSO from the DXIL file,
//       fills a UAV buffer of <out_dwords> dwords with 0xDEADBEEF, dispatches <groups_x> groups
//       and prints every dword read back as `out <index> 0x<hex>`. A dword that stays
//       0xDEADBEEF was never written.
//
// Status: `caps` and `run` have both been compiled and run under CrossOver 26.3 on the M4.
// Exit codes: 0 ran, 2 no device, 3 pipeline creation failed, 4 other D3D12 failure.
#define COBJMACROS
#include <d3d12.h>
#include <dxgi1_4.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define CK(x, what) do { HRESULT h_ = (x); if (FAILED(h_)) { \
    printf("FAIL %s hr=0x%08lx\n", what, (unsigned long)h_); return 4; } } while (0)

static ID3D12Device *make_device(void) {
    ID3D12Device *dev = NULL;
    static const D3D_FEATURE_LEVEL levels[] = {D3D_FEATURE_LEVEL_12_1, D3D_FEATURE_LEVEL_12_0, D3D_FEATURE_LEVEL_11_1, D3D_FEATURE_LEVEL_11_0};
    for (unsigned i = 0; i < sizeof levels / sizeof *levels; i++) {
        if (SUCCEEDED(D3D12CreateDevice(NULL, levels[i], &IID_ID3D12Device, (void **)&dev))) {
            printf("device_created_at_feature_level=0x%x\n", (unsigned)levels[i]);
            return dev;
        }
    }
    return NULL;
}

static int caps(void) {
    ID3D12Device *dev = make_device();
    if (!dev) { printf("NO DEVICE\n"); return 2; }
    LUID luid; dev->lpVtbl->GetAdapterLuid(dev, &luid); // (the C macro cannot return a struct)
    IDXGIFactory4 *f = NULL;
    if (SUCCEEDED(CreateDXGIFactory1(&IID_IDXGIFactory4, (void **)&f))) {
        IDXGIAdapter1 *ad = NULL;
        if (SUCCEEDED(IDXGIFactory4_EnumAdapterByLuid(f, luid, &IID_IDXGIAdapter1, (void **)&ad))) {
            DXGI_ADAPTER_DESC1 d; IDXGIAdapter1_GetDesc1(ad, &d);
            char n[128]; size_t i = 0; for (; d.Description[i] && i < 127; i++) n[i] = (char)d.Description[i]; n[i] = 0;
            printf("adapter=%s vendor=0x%x device=0x%x vram_mb=%u shared_mb=%u\n", n, d.VendorId, d.DeviceId,
                   (unsigned)(d.DedicatedVideoMemory >> 20), (unsigned)(d.SharedSystemMemory >> 20));
            IDXGIAdapter1_Release(ad);
        }
        IDXGIFactory4_Release(f);
    }
    D3D12_FEATURE_DATA_SHADER_MODEL sm = {D3D_SHADER_MODEL_6_7};
    HRESULT h = ID3D12Device_CheckFeatureSupport(dev, D3D12_FEATURE_SHADER_MODEL, &sm, sizeof sm);
    if (FAILED(h)) { sm.HighestShaderModel = D3D_SHADER_MODEL_6_6; h = ID3D12Device_CheckFeatureSupport(dev, D3D12_FEATURE_SHADER_MODEL, &sm, sizeof sm); }
    printf("highest_shader_model=0x%x (query hr=0x%08lx)\n", (unsigned)sm.HighestShaderModel, (unsigned long)h);
    D3D12_FEATURE_DATA_D3D12_OPTIONS o = {0};
    h = ID3D12Device_CheckFeatureSupport(dev, D3D12_FEATURE_D3D12_OPTIONS, &o, sizeof o);
    printf("options: hr=0x%08lx double_ops=%d min_precision=0x%x resource_binding_tier=%d\n", (unsigned long)h, o.DoublePrecisionFloatShaderOps, (unsigned)o.MinPrecisionSupport, (int)o.ResourceBindingTier);
    D3D12_FEATURE_DATA_D3D12_OPTIONS1 o1 = {0};
    h = ID3D12Device_CheckFeatureSupport(dev, D3D12_FEATURE_D3D12_OPTIONS1, &o1, sizeof o1);
    printf("options1: hr=0x%08lx wave_ops=%d wave_lane_count_min=%u wave_lane_count_max=%u total_lane_count=%u expanded_compute_resource_states=%d int64_shader_ops=%d\n",
           (unsigned long)h, o1.WaveOps, o1.WaveLaneCountMin, o1.WaveLaneCountMax, o1.TotalLaneCount, o1.ExpandedComputeResourceStates, o1.Int64ShaderOps);
    ID3D12Device_Release(dev);
    return 0;
}

static int run(const char *path, unsigned groups, unsigned dwords) {
    FILE *fp = fopen(path, "rb");
    if (!fp) { printf("FAIL open %s\n", path); return 4; }
    if (fseek(fp, 0, SEEK_END)) { fclose(fp); return 4; }
    long n = ftell(fp);
    if (n <= 0 || n > 16 * 1024 * 1024 || fseek(fp, 0, SEEK_SET)) { fclose(fp); return 4; }
    void *code = malloc((size_t)n);
    if (!code) { fclose(fp); return 4; }
    if (fread(code, 1, (size_t)n, fp) != (size_t)n) { printf("FAIL read\n"); return 4; }
    fclose(fp);
    ID3D12Device *dev = make_device();
    if (!dev) { printf("NO DEVICE\n"); return 2; }

    D3D12_FEATURE_DATA_D3D12_OPTIONS1 actual_caps = {0};
    CK(ID3D12Device_CheckFeatureSupport(dev, D3D12_FEATURE_D3D12_OPTIONS1, &actual_caps, sizeof actual_caps), "dispatch wave capabilities");
    printf("dispatch_caps wave_ops=%d min=%u max=%u\n", actual_caps.WaveOps,
           actual_caps.WaveLaneCountMin, actual_caps.WaveLaneCountMax);

    D3D12_ROOT_PARAMETER rp = {0};
    rp.ParameterType = D3D12_ROOT_PARAMETER_TYPE_UAV;
    rp.Descriptor.ShaderRegister = 0;
    rp.ShaderVisibility = D3D12_SHADER_VISIBILITY_ALL;
    D3D12_ROOT_SIGNATURE_DESC rsd = {0};
    rsd.NumParameters = 1; rsd.pParameters = &rp;
    ID3DBlob *blob = NULL, *err = NULL;
    CK(D3D12SerializeRootSignature(&rsd, D3D_ROOT_SIGNATURE_VERSION_1, &blob, &err), "serialize root signature");
    ID3D12RootSignature *rs = NULL;
    CK(ID3D12Device_CreateRootSignature(dev, 0, ID3D10Blob_GetBufferPointer(blob), ID3D10Blob_GetBufferSize(blob), &IID_ID3D12RootSignature, (void **)&rs), "create root signature");

    D3D12_COMPUTE_PIPELINE_STATE_DESC pd = {0};
    pd.pRootSignature = rs;
    pd.CS.pShaderBytecode = code; pd.CS.BytecodeLength = (SIZE_T)n;
    ID3D12PipelineState *pso = NULL;
    HRESULT h = ID3D12Device_CreateComputePipelineState(dev, &pd, &IID_ID3D12PipelineState, (void **)&pso);
    if (FAILED(h)) { printf("PSO_FAILED hr=0x%08lx\n", (unsigned long)h); return 3; }

    UINT64 bytes = (UINT64)dwords * 4;
    D3D12_HEAP_PROPERTIES hp_def = {0}, hp_up = {0}, hp_rb = {0};
    hp_def.Type = D3D12_HEAP_TYPE_DEFAULT; hp_up.Type = D3D12_HEAP_TYPE_UPLOAD; hp_rb.Type = D3D12_HEAP_TYPE_READBACK;
    D3D12_RESOURCE_DESC rd = {0};
    rd.Dimension = D3D12_RESOURCE_DIMENSION_BUFFER; rd.Width = bytes; rd.Height = 1; rd.DepthOrArraySize = 1; rd.MipLevels = 1;
    rd.Format = DXGI_FORMAT_UNKNOWN; rd.SampleDesc.Count = 1; rd.Layout = D3D12_TEXTURE_LAYOUT_ROW_MAJOR;
    ID3D12Resource *def, *up, *rb;
    D3D12_RESOURCE_DESC rdu = rd; rdu.Flags = D3D12_RESOURCE_FLAG_ALLOW_UNORDERED_ACCESS;
    CK(ID3D12Device_CreateCommittedResource(dev, &hp_def, D3D12_HEAP_FLAG_NONE, &rdu, D3D12_RESOURCE_STATE_COPY_DEST, NULL, &IID_ID3D12Resource, (void **)&def), "create default");
    CK(ID3D12Device_CreateCommittedResource(dev, &hp_up, D3D12_HEAP_FLAG_NONE, &rd, D3D12_RESOURCE_STATE_GENERIC_READ, NULL, &IID_ID3D12Resource, (void **)&up), "create upload");
    CK(ID3D12Device_CreateCommittedResource(dev, &hp_rb, D3D12_HEAP_FLAG_NONE, &rd, D3D12_RESOURCE_STATE_COPY_DEST, NULL, &IID_ID3D12Resource, (void **)&rb), "create readback");
    unsigned *p; CK(ID3D12Resource_Map(up, 0, NULL, (void **)&p), "map upload");
    for (unsigned i = 0; i < dwords; i++) p[i] = 0xDEADBEEFu;
    ID3D12Resource_Unmap(up, 0, NULL);

    D3D12_COMMAND_QUEUE_DESC qd = {0};
    qd.Type = D3D12_COMMAND_LIST_TYPE_DIRECT;
    ID3D12CommandQueue *q; CK(ID3D12Device_CreateCommandQueue(dev, &qd, &IID_ID3D12CommandQueue, (void **)&q), "queue");
    ID3D12CommandAllocator *al; CK(ID3D12Device_CreateCommandAllocator(dev, D3D12_COMMAND_LIST_TYPE_DIRECT, &IID_ID3D12CommandAllocator, (void **)&al), "allocator");
    ID3D12GraphicsCommandList *cl; CK(ID3D12Device_CreateCommandList(dev, 0, D3D12_COMMAND_LIST_TYPE_DIRECT, al, pso, &IID_ID3D12GraphicsCommandList, (void **)&cl), "list");
    ID3D12GraphicsCommandList_CopyBufferRegion(cl, def, 0, up, 0, bytes);
    D3D12_RESOURCE_BARRIER b = {0};
    b.Type = D3D12_RESOURCE_BARRIER_TYPE_TRANSITION;
    b.Transition.pResource = def; b.Transition.Subresource = D3D12_RESOURCE_BARRIER_ALL_SUBRESOURCES;
    b.Transition.StateBefore = D3D12_RESOURCE_STATE_COPY_DEST; b.Transition.StateAfter = D3D12_RESOURCE_STATE_UNORDERED_ACCESS;
    ID3D12GraphicsCommandList_ResourceBarrier(cl, 1, &b);
    ID3D12GraphicsCommandList_SetComputeRootSignature(cl, rs);
    ID3D12GraphicsCommandList_SetComputeRootUnorderedAccessView(cl, 0, ID3D12Resource_GetGPUVirtualAddress(def));
    ID3D12GraphicsCommandList_Dispatch(cl, groups, 1, 1);
    b.Transition.StateBefore = D3D12_RESOURCE_STATE_UNORDERED_ACCESS; b.Transition.StateAfter = D3D12_RESOURCE_STATE_COPY_SOURCE;
    ID3D12GraphicsCommandList_ResourceBarrier(cl, 1, &b);
    ID3D12GraphicsCommandList_CopyBufferRegion(cl, rb, 0, def, 0, bytes);
    CK(ID3D12GraphicsCommandList_Close(cl), "close");
    ID3D12CommandList *lists[1] = {(ID3D12CommandList *)cl};
    ID3D12CommandQueue_ExecuteCommandLists(q, 1, lists);
    ID3D12Fence *fence; CK(ID3D12Device_CreateFence(dev, 0, D3D12_FENCE_FLAG_NONE, &IID_ID3D12Fence, (void **)&fence), "fence");
    HANDLE ev = CreateEventA(NULL, FALSE, FALSE, NULL);
    if (!ev) { printf("FAIL CreateEvent\n"); return 4; }
    CK(ID3D12CommandQueue_Signal(q, fence, 1), "signal");
    CK(ID3D12Fence_SetEventOnCompletion(fence, 1, ev), "set fence event");
    if (WaitForSingleObject(ev, 30000) != WAIT_OBJECT_0) { printf("FAIL timeout waiting for the GPU\n"); return 4; }
    CK(ID3D12Resource_Map(rb, 0, NULL, (void **)&p), "map readback");
    for (unsigned i = 0; i < dwords; i++) printf("out %u 0x%08x\n", i, p[i]);
    D3D12_RANGE none = {0, 0};
    ID3D12Resource_Unmap(rb, 0, &none);
    CloseHandle(ev);
    ID3D12Fence_Release(fence);
    ID3D12GraphicsCommandList_Release(cl);
    ID3D12CommandAllocator_Release(al);
    ID3D12CommandQueue_Release(q);
    ID3D12Resource_Release(rb);
    ID3D12Resource_Release(up);
    ID3D12Resource_Release(def);
    ID3D12PipelineState_Release(pso);
    ID3D12RootSignature_Release(rs);
    ID3D10Blob_Release(blob);
    if (err) ID3D10Blob_Release(err);
    ID3D12Device_Release(dev);
    free(code);
    return 0;
}

int main(int argc, char **argv) {
    if (argc == 2 && !strcmp(argv[1], "caps")) return caps();
    if (argc == 5 && !strcmp(argv[1], "run")) {
        char *end;
        unsigned long groups = strtoul(argv[3], &end, 10);
        if (*end || groups < 1 || groups > 65535) return 1;
        unsigned long words = strtoul(argv[4], &end, 10);
        if (*end || words < 1 || words > 1048576) return 1;
        return run(argv[2], (unsigned)groups, (unsigned)words);
    }
    printf("usage: d3d12_run caps | run <shader.dxil> <groups_x> <out_dwords>\n");
    return 1;
}

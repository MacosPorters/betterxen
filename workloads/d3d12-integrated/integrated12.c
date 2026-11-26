// SPDX-License-Identifier: GPL-3.0-or-later
// Original BetterXen workload: Direct3D 12 graphics and compute with presentation and
// self-verification, run through `betterxen hybrid run`. Written from Microsoft's public
// Direct3D 12 documentation.
//
// The scene is deliberately the same as workloads/d3d11-integrated, with the same
// verification schedule, so the two APIs must produce the same pixel digest:
//   pass 1  two textured quads with a depth buffer into an offscreen target; near quad B is
//           drawn first and moves with the arrow keys
//   pass 2  compute: rotate channels into a UAV and write a 16-row frame-number band
//   pass 3  full-screen triangle into the swapchain back buffer
// The back buffer is copied to a READBACK buffer and compared, with zero tolerance, with an
// image computed on the CPU. Two frames are in flight; each has its own command allocator,
// constant-buffer slice and fence value.
//
// Limits: read-back proves what was handed to Present, not what reached the screen. No debug
// layer is used, so a missing barrier that happens to work is not detected.
//
// usage: integrated12.exe [--frames N] [--duration-s N] [--verify-every N] [--vsync]
//          [--external-input] [--input-timeout-s N] [--hold-ms N] [--max-growth-mb N]
//          [--expect-absent-env NAME] [--fault color|blank|stale|input|depth|upload|crash|hang]
// Exit: 0 pass, 1 a verification failed, 2 no Direct3D 12 device, 3 an API call failed.
#define COBJMACROS
#define WIDL_C_INLINE_WRAPPERS /* struct-returning D3D12 methods in C (mingw-w64) */
#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <psapi.h>
#include <d3d12.h>
#include <d3dcompiler.h>
#include <dxgi1_4.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define W 256
#define H 256
#define BAND 16
#define STEP 16
#define SLOTS 2
#define ROW_PITCH (W * 4) /* 1024, a multiple of D3D12_TEXTURE_DATA_PITCH_ALIGNMENT */

static int failures;
static LARGE_INTEGER qpc_freq, qpc_start;

static double now_ms(void) {
    LARGE_INTEGER t;
    QueryPerformanceCounter(&t);
    return (double)(t.QuadPart - qpc_start.QuadPart) * 1000.0 / (double)qpc_freq.QuadPart;
}

static void gate(const char *name, int ok, const char *detail) {
    printf("gate %s %s %s t_ms=%.1f\n", name, ok ? "pass" : "FAIL", detail, now_ms());
    if (!ok) failures++;
}

#define API(x, what) do { HRESULT hr_ = (x); if (FAILED(hr_)) { \
    printf("FAIL api %s hr=0x%08lx\n", what, (unsigned long)hr_); exit(3); } } while (0)
#define WIN(x, what) do { if (!(x)) { \
    printf("FAIL api %s error=%lu\n", what, (unsigned long)GetLastError()); exit(3); } } while (0)

/* ---- window and input ------------------------------------------------------------------ */

static int off_x, off_y, keys_seen, quit_requested;

static LRESULT CALLBACK wndproc(HWND h, UINT m, WPARAM w, LPARAM l) {
    if (m == WM_KEYDOWN) {
        if (w == VK_ESCAPE) quit_requested = 1;
        else {
            int dx = w == VK_RIGHT ? STEP : w == VK_LEFT ? -STEP : 0;
            int dy = w == VK_DOWN ? STEP : w == VK_UP ? -STEP : 0;
            if (dx || dy) {
                keys_seen++;
                off_x += dx;
                off_y += dy;
                if (off_x < -64) off_x = -64;
                if (off_x > 32) off_x = 32;
                if (off_y < -64) off_y = -64;
                if (off_y > 32) off_y = 32;
            }
        }
        return 0;
    }
    if (m == WM_CLOSE) { quit_requested = 1; return 0; }
    return DefWindowProcA(h, m, w, l);
}

static void pump(void) {
    MSG msg;
    while (PeekMessageA(&msg, NULL, 0, 0, PM_REMOVE)) {
        TranslateMessage(&msg);
        DispatchMessageA(&msg);
    }
}

/* ---- expected image (CPU, independent of the shaders) ---------------------------------- */

static const unsigned char TEX_A[4][4] = {
    {255, 0, 0, 255}, {0, 255, 0, 255}, {0, 0, 255, 255}, {255, 255, 255, 255}};
static const unsigned char TEX_B[4][4] = {
    {255, 255, 0, 255}, {0, 255, 255, 255}, {255, 0, 255, 255}, {128, 128, 128, 255}};

static void expected_pixel(int x, int y, unsigned marker, int bx, int by, int wrong, unsigned char out[4]) {
    if (y < BAND) {
        out[0] = (unsigned char)(marker & 255);
        out[1] = (unsigned char)(marker >> 8);
        out[2] = 77;
        out[3] = 255;
        return;
    }
    const unsigned char *c = NULL;
    if (x >= bx && x < bx + 128 && y >= by && y < by + 128)
        c = TEX_B[((y - by) / 64) * 2 + (x - bx) / 64];
    else if (x >= 32 && x < 160 && y >= 32 && y < 160)
        c = TEX_A[((y - 32) / 64) * 2 + (x - 32) / 64];
    out[0] = c ? c[2] : 0;
    out[1] = c ? c[0] : 0;
    out[2] = c ? c[1] : 0;
    out[3] = 255;
    if (wrong && c == TEX_A[0]) out[0] ^= 255;
}

/* ---- Direct3D 12 ----------------------------------------------------------------------- */

static const char *HLSL =
    "cbuffer C : register(b0) { float2 offset; uint marker; uint pad; };\n"
    "Texture2D tex : register(t0); SamplerState smp : register(s0);\n"
    "struct VI { float3 p : POSITION; float2 uv : TEXCOORD; };\n"
    "struct VO { float4 p : SV_Position; float2 uv : TEXCOORD; };\n"
    "VO vs_quad(VI v) { VO o; o.p = float4(v.p.xy + offset, v.p.z, 1); o.uv = v.uv; return o; }\n"
    "float4 ps_quad(VO i) : SV_Target { return tex.Sample(smp, i.uv); }\n"
    "float4 vs_full(uint id : SV_VertexID) : SV_Position {\n"
    "  float2 uv = float2((id << 1) & 2, id & 2);\n"
    "  return float4(uv * float2(2, -2) + float2(-1, 1), 0, 1); }\n"
    "float4 ps_full(float4 p : SV_Position) : SV_Target { return tex.Load(int3(p.xy, 0)); }\n"
    "RWTexture2D<unorm float4> dst : register(u0);\n"
    "[numthreads(8, 8, 1)] void cs_main(uint3 id : SV_DispatchThreadID) {\n"
    "  if (id.y < 16) { dst[id.xy] = float4((marker & 255) / 255.0, (marker >> 8) / 255.0, 77 / 255.0, 1); return; }\n"
    "  float4 c = tex.Load(int3(id.xy, 0));\n"
    "  dst[id.xy] = float4(c.b, c.r, c.g, 1); }\n";

typedef struct { float x, y, z, u, v; } Vertex;
typedef struct { float ox, oy; unsigned marker, pad; } Constants;

static ID3D12Device *dev;
static ID3D12CommandQueue *queue;
static ID3D12Fence *fence;
static HANDLE fence_event;
static UINT64 fence_next = 1;

static ID3DBlob *compile(const char *entry, const char *target) {
    ID3DBlob *code = NULL, *err = NULL;
    HRESULT hr = D3DCompile(HLSL, strlen(HLSL), NULL, NULL, NULL, entry, target, 0, 0, &code, &err);
    if (FAILED(hr)) {
        printf("FAIL api D3DCompile %s hr=0x%08lx %s\n", entry, (unsigned long)hr,
               err ? (const char *)ID3D10Blob_GetBufferPointer(err) : "");
        exit(3);
    }
    if (err) ID3D10Blob_Release(err);
    return code;
}

static ID3D12Resource *resource(D3D12_HEAP_TYPE heap, const D3D12_RESOURCE_DESC *d,
                                D3D12_RESOURCE_STATES state, const D3D12_CLEAR_VALUE *clear) {
    D3D12_HEAP_PROPERTIES hp = {.Type = heap};
    ID3D12Resource *r = NULL;
    API(ID3D12Device_CreateCommittedResource(dev, &hp, D3D12_HEAP_FLAG_NONE, d, state, clear,
                                             &IID_ID3D12Resource, (void **)&r), "CreateCommittedResource");
    return r;
}

static ID3D12Resource *buffer(D3D12_HEAP_TYPE heap, UINT64 bytes, D3D12_RESOURCE_STATES state) {
    D3D12_RESOURCE_DESC d = {.Dimension = D3D12_RESOURCE_DIMENSION_BUFFER, .Width = bytes, .Height = 1,
                             .DepthOrArraySize = 1, .MipLevels = 1, .SampleDesc = {1, 0},
                             .Layout = D3D12_TEXTURE_LAYOUT_ROW_MAJOR};
    return resource(heap, &d, state, NULL);
}

static ID3D12Resource *texture(UINT w, UINT h, DXGI_FORMAT fmt, D3D12_RESOURCE_FLAGS flags,
                               D3D12_RESOURCE_STATES state, const D3D12_CLEAR_VALUE *clear) {
    D3D12_RESOURCE_DESC d = {.Dimension = D3D12_RESOURCE_DIMENSION_TEXTURE2D, .Width = w, .Height = h,
                             .DepthOrArraySize = 1, .MipLevels = 1, .Format = fmt, .SampleDesc = {1, 0},
                             .Flags = flags};
    return resource(D3D12_HEAP_TYPE_DEFAULT, &d, state, clear);
}

static void barrier(ID3D12GraphicsCommandList *list, ID3D12Resource *r, D3D12_RESOURCE_STATES from,
                    D3D12_RESOURCE_STATES to) {
    D3D12_RESOURCE_BARRIER b = {.Type = D3D12_RESOURCE_BARRIER_TYPE_TRANSITION};
    b.Transition.pResource = r;
    b.Transition.Subresource = D3D12_RESOURCE_BARRIER_ALL_SUBRESOURCES;
    b.Transition.StateBefore = from;
    b.Transition.StateAfter = to;
    ID3D12GraphicsCommandList_ResourceBarrier(list, 1, &b);
}

static void wait_for(UINT64 value) {
    if (ID3D12Fence_GetCompletedValue(fence) < value) {
        API(ID3D12Fence_SetEventOnCompletion(fence, value, fence_event), "SetEventOnCompletion");
        WIN(WaitForSingleObject(fence_event, 10000) == WAIT_OBJECT_0, "GPU completion within 10 s");
    }
}

static UINT64 submit(ID3D12GraphicsCommandList *list) {
    API(ID3D12GraphicsCommandList_Close(list), "Close");
    ID3D12CommandList *lists[1] = {(ID3D12CommandList *)list};
    ID3D12CommandQueue_ExecuteCommandLists(queue, 1, lists);
    const UINT64 value = fence_next++;
    API(ID3D12CommandQueue_Signal(queue, fence, value), "Signal");
    return value;
}

static void copy_texture(ID3D12GraphicsCommandList *list, ID3D12Resource *tex, ID3D12Resource *buf,
                         UINT w, UINT h, UINT pitch, int to_texture) {
    D3D12_TEXTURE_COPY_LOCATION t = {.pResource = tex, .Type = D3D12_TEXTURE_COPY_TYPE_SUBRESOURCE_INDEX};
    D3D12_TEXTURE_COPY_LOCATION b = {.pResource = buf, .Type = D3D12_TEXTURE_COPY_TYPE_PLACED_FOOTPRINT};
    b.PlacedFootprint.Footprint = (D3D12_SUBRESOURCE_FOOTPRINT){DXGI_FORMAT_R8G8B8A8_UNORM, w, h, 1, pitch};
    if (to_texture) ID3D12GraphicsCommandList_CopyTextureRegion(list, &t, 0, 0, 0, &b, NULL);
    else ID3D12GraphicsCommandList_CopyTextureRegion(list, &b, 0, 0, 0, &t, NULL);
}

static void fill(ID3D12Resource *upload, const void *data, size_t bytes) {
    void *p = NULL;
    D3D12_RANGE none = {0, 0};
    API(ID3D12Resource_Map(upload, 0, &none, &p), "Map upload");
    memcpy(p, data, bytes);
    ID3D12Resource_Unmap(upload, 0, NULL);
}

static ID3D12DescriptorHeap *heap_of(D3D12_DESCRIPTOR_HEAP_TYPE type, UINT n, int visible) {
    D3D12_DESCRIPTOR_HEAP_DESC d = {.Type = type, .NumDescriptors = n,
                                    .Flags = visible ? D3D12_DESCRIPTOR_HEAP_FLAG_SHADER_VISIBLE
                                                     : D3D12_DESCRIPTOR_HEAP_FLAG_NONE};
    ID3D12DescriptorHeap *h = NULL;
    API(ID3D12Device_CreateDescriptorHeap(dev, &d, &IID_ID3D12DescriptorHeap, (void **)&h), "descriptor heap");
    return h;
}

static ID3D12RootSignature *root_signature(int compute) {
    D3D12_DESCRIPTOR_RANGE srv = {D3D12_DESCRIPTOR_RANGE_TYPE_SRV, 1, 0, 0, 0};
    D3D12_DESCRIPTOR_RANGE other = {compute ? D3D12_DESCRIPTOR_RANGE_TYPE_UAV : D3D12_DESCRIPTOR_RANGE_TYPE_SAMPLER,
                                    1, 0, 0, 0};
    D3D12_ROOT_PARAMETER p[3];
    memset(p, 0, sizeof p);
    p[0].ParameterType = D3D12_ROOT_PARAMETER_TYPE_CBV;
    p[1].ParameterType = D3D12_ROOT_PARAMETER_TYPE_DESCRIPTOR_TABLE;
    p[1].DescriptorTable.NumDescriptorRanges = 1;
    p[1].DescriptorTable.pDescriptorRanges = &srv;
    p[2] = p[1];
    p[2].DescriptorTable.pDescriptorRanges = &other;
    D3D12_ROOT_SIGNATURE_DESC d = {.NumParameters = 3, .pParameters = p,
                                   .Flags = compute ? D3D12_ROOT_SIGNATURE_FLAG_NONE
                                                    : D3D12_ROOT_SIGNATURE_FLAG_ALLOW_INPUT_ASSEMBLER_INPUT_LAYOUT};
    ID3DBlob *blob = NULL, *err = NULL;
    API(D3D12SerializeRootSignature(&d, D3D_ROOT_SIGNATURE_VERSION_1, &blob, &err), "serialize root signature");
    ID3D12RootSignature *rs = NULL;
    API(ID3D12Device_CreateRootSignature(dev, 0, ID3D10Blob_GetBufferPointer(blob), ID3D10Blob_GetBufferSize(blob),
                                         &IID_ID3D12RootSignature, (void **)&rs), "root signature");
    ID3D10Blob_Release(blob);
    return rs;
}

static ID3D12PipelineState *graphics_pipeline(ID3D12RootSignature *rs, ID3DBlob *vs, ID3DBlob *ps,
                                              int quad, int depth_test) {
    static const D3D12_INPUT_ELEMENT_DESC ied[2] = {
        {"POSITION", 0, DXGI_FORMAT_R32G32B32_FLOAT, 0, 0, D3D12_INPUT_CLASSIFICATION_PER_VERTEX_DATA, 0},
        {"TEXCOORD", 0, DXGI_FORMAT_R32G32_FLOAT, 0, 12, D3D12_INPUT_CLASSIFICATION_PER_VERTEX_DATA, 0}};
    D3D12_GRAPHICS_PIPELINE_STATE_DESC d;
    memset(&d, 0, sizeof d);
    d.pRootSignature = rs;
    d.VS = (D3D12_SHADER_BYTECODE){ID3D10Blob_GetBufferPointer(vs), ID3D10Blob_GetBufferSize(vs)};
    d.PS = (D3D12_SHADER_BYTECODE){ID3D10Blob_GetBufferPointer(ps), ID3D10Blob_GetBufferSize(ps)};
    d.BlendState.RenderTarget[0].RenderTargetWriteMask = D3D12_COLOR_WRITE_ENABLE_ALL;
    d.SampleMask = UINT_MAX;
    d.RasterizerState.FillMode = D3D12_FILL_MODE_SOLID;
    d.RasterizerState.CullMode = D3D12_CULL_MODE_BACK;
    d.RasterizerState.DepthClipEnable = TRUE;
    if (quad) {
        d.InputLayout = (D3D12_INPUT_LAYOUT_DESC){ied, 2};
        d.DepthStencilState.DepthEnable = depth_test;
        d.DepthStencilState.DepthWriteMask = D3D12_DEPTH_WRITE_MASK_ALL;
        d.DepthStencilState.DepthFunc = D3D12_COMPARISON_FUNC_LESS;
        d.DSVFormat = DXGI_FORMAT_D32_FLOAT;
    }
    d.PrimitiveTopologyType = D3D12_PRIMITIVE_TOPOLOGY_TYPE_TRIANGLE;
    d.NumRenderTargets = 1;
    d.RTVFormats[0] = DXGI_FORMAT_R8G8B8A8_UNORM;
    d.SampleDesc.Count = 1;
    ID3D12PipelineState *p = NULL;
    API(ID3D12Device_CreateGraphicsPipelineState(dev, &d, &IID_ID3D12PipelineState, (void **)&p), "graphics PSO");
    return p;
}

static void quad_vertices(Vertex *v, int x0, int y0, float z) {
    const float l = x0 / 128.0f - 1.0f, r = (x0 + 128) / 128.0f - 1.0f;
    const float t = 1.0f - y0 / 128.0f, b = 1.0f - (y0 + 128) / 128.0f;
    v[0] = (Vertex){l, t, z, 0, 0};
    v[1] = (Vertex){r, t, z, 1, 0};
    v[2] = (Vertex){l, b, z, 0, 1};
    v[3] = (Vertex){r, b, z, 1, 1};
}

static double working_set_mb(void) {
    PROCESS_MEMORY_COUNTERS m;
    memset(&m, 0, sizeof m);
    if (!GetProcessMemoryInfo(GetCurrentProcess(), &m, sizeof m) || !m.WorkingSetSize) return -1.0;
    return (double)m.WorkingSetSize / (1024.0 * 1024.0);
}

/* Create, upload, copy back, compare and destroy a texture on the GPU; 1 if it survived. */
static int churn(ID3D12CommandAllocator *alloc, ID3D12GraphicsCommandList *list, unsigned seed) {
    static unsigned data[256 * 256];
    for (unsigned i = 0; i < 256 * 256; i++) data[i] = i * 2654435761u + seed;
    ID3D12Resource *up = buffer(D3D12_HEAP_TYPE_UPLOAD, sizeof data, D3D12_RESOURCE_STATE_GENERIC_READ);
    ID3D12Resource *down = buffer(D3D12_HEAP_TYPE_READBACK, sizeof data, D3D12_RESOURCE_STATE_COPY_DEST);
    ID3D12Resource *tex = texture(256, 256, DXGI_FORMAT_R8G8B8A8_UNORM, D3D12_RESOURCE_FLAG_NONE,
                                  D3D12_RESOURCE_STATE_COPY_DEST, NULL);
    fill(up, data, sizeof data);
    API(ID3D12CommandAllocator_Reset(alloc), "churn allocator reset");
    API(ID3D12GraphicsCommandList_Reset(list, alloc, NULL), "churn list reset");
    copy_texture(list, tex, up, 256, 256, 1024, 1);
    barrier(list, tex, D3D12_RESOURCE_STATE_COPY_DEST, D3D12_RESOURCE_STATE_COPY_SOURCE);
    copy_texture(list, tex, down, 256, 256, 1024, 0);
    wait_for(submit(list));
    void *p = NULL;
    API(ID3D12Resource_Map(down, 0, NULL, &p), "Map readback churn");
    const int ok = !memcmp(p, data, sizeof data);
    D3D12_RANGE none = {0, 0};
    ID3D12Resource_Unmap(down, 0, &none);
    ID3D12Resource_Release(tex);
    ID3D12Resource_Release(down);
    ID3D12Resource_Release(up);
    return ok;
}

int main(int argc, char **argv) {
    int frames = 600, verify_every = 30, vsync = 0, external = 0, input_timeout_s = 60;
    int duration_s = 0, hold_ms = 0, max_growth = 64;
    const char *fault = "", *absent_env = NULL;
    setvbuf(stdout, NULL, _IONBF, 0);
    QueryPerformanceFrequency(&qpc_freq);
    QueryPerformanceCounter(&qpc_start);
    for (int i = 1; i < argc; i++) {
        const char *a = argv[i], *v = i + 1 < argc ? argv[i + 1] : NULL;
        if (!strcmp(a, "--vsync")) vsync = 1;
        else if (!strcmp(a, "--external-input")) external = 1;
        else if (!strcmp(a, "--frames") && v) frames = atoi(v), i++;
        else if (!strcmp(a, "--duration-s") && v) duration_s = atoi(v), i++;
        else if (!strcmp(a, "--verify-every") && v) verify_every = atoi(v), i++;
        else if (!strcmp(a, "--input-timeout-s") && v) input_timeout_s = atoi(v), i++;
        else if (!strcmp(a, "--hold-ms") && v) hold_ms = atoi(v), i++;
        else if (!strcmp(a, "--max-growth-mb") && v) max_growth = atoi(v), i++;
        else if (!strcmp(a, "--expect-absent-env") && v) absent_env = v, i++;
        else if (!strcmp(a, "--fault") && v &&
                 (!strcmp(v, "color") || !strcmp(v, "blank") || !strcmp(v, "stale") || !strcmp(v, "input") ||
                  !strcmp(v, "depth") || !strcmp(v, "upload") || !strcmp(v, "crash") || !strcmp(v, "hang")))
            fault = v, i++;
        else { printf("FAIL usage: unknown or incomplete argument %s\n", a); return 3; }
    }
    if (frames < 64 || verify_every < 1) { printf("FAIL usage: --frames >= 64, --verify-every >= 1\n"); return 3; }

    if (absent_env) {
        char tmp[2];
        SetLastError(0);
        const DWORD n = GetEnvironmentVariableA(absent_env, tmp, sizeof tmp);
        const int absent = n == 0 && GetLastError() == ERROR_ENVVAR_NOT_FOUND;
        char msg[128];
        snprintf(msg, sizeof msg, "%.64s %s", absent_env, absent ? "is not visible" : "is visible");
        gate("runtime.env", absent, msg);
    }

    WNDCLASSA wc = {.lpfnWndProc = wndproc, .hInstance = GetModuleHandleA(NULL),
                    .lpszClassName = "BetterXenIntegrated12"};
    WIN(RegisterClassA(&wc), "RegisterClass");
    RECT r = {0, 0, W, H};
    AdjustWindowRect(&r, WS_OVERLAPPEDWINDOW, FALSE);
    HWND hwnd = CreateWindowA(wc.lpszClassName, "BetterXen D3D12 workload", WS_OVERLAPPEDWINDOW | WS_VISIBLE,
                              64, 64, r.right - r.left, r.bottom - r.top, NULL, NULL, wc.hInstance, NULL);
    WIN(hwnd, "CreateWindow");
    pump();

    HRESULT hr = D3D12CreateDevice(NULL, D3D_FEATURE_LEVEL_11_0, &IID_ID3D12Device, (void **)&dev);
    if (FAILED(hr)) { printf("NO DEVICE hr=0x%08lx\n", (unsigned long)hr); return 2; }
    IDXGIFactory4 *factory = NULL;
    API(CreateDXGIFactory1(&IID_IDXGIFactory4, (void **)&factory), "factory");
    D3D12_COMMAND_QUEUE_DESC qd = {.Type = D3D12_COMMAND_LIST_TYPE_DIRECT};
    API(ID3D12Device_CreateCommandQueue(dev, &qd, &IID_ID3D12CommandQueue, (void **)&queue), "queue");
    {
        IDXGIAdapter1 *ad = NULL;
        DXGI_ADAPTER_DESC1 desc;
        for (UINT i = 0; i < 1 && SUCCEEDED(IDXGIFactory4_EnumAdapters1(factory, i, &ad)); i++) {
            if (SUCCEEDED(IDXGIAdapter1_GetDesc1(ad, &desc))) {
                char name[128];
                size_t k = 0;
                for (; k < 127 && desc.Description[k]; k++) name[k] = (char)desc.Description[k];
                name[k] = 0;
                printf("adapter=%s vendor=0x%x\n", name, desc.VendorId);
            }
            IDXGIAdapter1_Release(ad);
        }
    }
    API(ID3D12Device_CreateFence(dev, 0, D3D12_FENCE_FLAG_NONE, &IID_ID3D12Fence, (void **)&fence), "fence");
    fence_event = CreateEventA(NULL, FALSE, FALSE, NULL);
    WIN(fence_event, "CreateEvent");
    DXGI_SWAP_CHAIN_DESC1 sd = {.Width = W, .Height = H, .Format = DXGI_FORMAT_R8G8B8A8_UNORM,
                                .SampleDesc = {1, 0}, .BufferUsage = DXGI_USAGE_RENDER_TARGET_OUTPUT,
                                .BufferCount = 2, .SwapEffect = DXGI_SWAP_EFFECT_FLIP_DISCARD};
    IDXGISwapChain1 *swap1 = NULL;
    IDXGISwapChain3 *swap = NULL;
    API(IDXGIFactory4_CreateSwapChainForHwnd(factory, (IUnknown *)queue, hwnd, &sd, NULL, NULL, &swap1), "swapchain");
    API(IDXGISwapChain1_QueryInterface(swap1, &IID_IDXGISwapChain3, (void **)&swap), "swapchain3");

    ID3D12CommandAllocator *alloc[SLOTS], *aux_alloc = NULL;
    ID3D12GraphicsCommandList *list = NULL, *aux = NULL;
    UINT64 slot_value[SLOTS] = {0};
    for (int i = 0; i < SLOTS; i++)
        API(ID3D12Device_CreateCommandAllocator(dev, D3D12_COMMAND_LIST_TYPE_DIRECT, &IID_ID3D12CommandAllocator,
                                                (void **)&alloc[i]), "allocator");
    API(ID3D12Device_CreateCommandAllocator(dev, D3D12_COMMAND_LIST_TYPE_DIRECT, &IID_ID3D12CommandAllocator,
                                            (void **)&aux_alloc), "aux allocator");
    API(ID3D12Device_CreateCommandList(dev, 0, D3D12_COMMAND_LIST_TYPE_DIRECT, alloc[0], NULL,
                                       &IID_ID3D12GraphicsCommandList, (void **)&list), "list");
    API(ID3D12Device_CreateCommandList(dev, 0, D3D12_COMMAND_LIST_TYPE_DIRECT, aux_alloc, NULL,
                                       &IID_ID3D12GraphicsCommandList, (void **)&aux), "aux list");
    API(ID3D12GraphicsCommandList_Close(aux), "close aux");

    /* resources */
    const D3D12_CLEAR_VALUE black_clear = {.Format = DXGI_FORMAT_R8G8B8A8_UNORM, .Color = {0, 0, 0, 1}};
    const D3D12_CLEAR_VALUE depth_clear = {.Format = DXGI_FORMAT_D32_FLOAT, .DepthStencil = {1.0f, 0}};
    ID3D12Resource *back[2];
    for (UINT i = 0; i < 2; i++)
        API(IDXGISwapChain3_GetBuffer(swap, i, &IID_ID3D12Resource, (void **)&back[i]), "GetBuffer");
    ID3D12Resource *off = texture(W, H, DXGI_FORMAT_R8G8B8A8_UNORM, D3D12_RESOURCE_FLAG_ALLOW_RENDER_TARGET,
                                  D3D12_RESOURCE_STATE_NON_PIXEL_SHADER_RESOURCE, &black_clear);
    ID3D12Resource *post = texture(W, H, DXGI_FORMAT_R8G8B8A8_UNORM, D3D12_RESOURCE_FLAG_ALLOW_UNORDERED_ACCESS,
                                   D3D12_RESOURCE_STATE_PIXEL_SHADER_RESOURCE, NULL);
    ID3D12Resource *depth = texture(W, H, DXGI_FORMAT_D32_FLOAT, D3D12_RESOURCE_FLAG_ALLOW_DEPTH_STENCIL,
                                    D3D12_RESOURCE_STATE_DEPTH_WRITE, &depth_clear);
    ID3D12Resource *ta = texture(2, 2, DXGI_FORMAT_R8G8B8A8_UNORM, D3D12_RESOURCE_FLAG_NONE,
                                 D3D12_RESOURCE_STATE_COPY_DEST, NULL);
    ID3D12Resource *tb = texture(2, 2, DXGI_FORMAT_R8G8B8A8_UNORM, D3D12_RESOURCE_FLAG_NONE,
                                 D3D12_RESOURCE_STATE_COPY_DEST, NULL);

    Vertex verts[8];
    quad_vertices(verts, 96, 96, 0.25f);    /* B, near */
    quad_vertices(verts + 4, 32, 32, 0.5f); /* A, far */
    const unsigned short indices[6] = {0, 1, 2, 2, 1, 3};
    ID3D12Resource *vb = buffer(D3D12_HEAP_TYPE_DEFAULT, sizeof verts, D3D12_RESOURCE_STATE_COPY_DEST);
    ID3D12Resource *ib = buffer(D3D12_HEAP_TYPE_DEFAULT, sizeof indices, D3D12_RESOURCE_STATE_COPY_DEST);

    /* one upload buffer: vertices, indices, then the two textures at 512-byte offsets */
    unsigned char staging[2048];
    memset(staging, 0, sizeof staging);
    memcpy(staging, verts, sizeof verts);
    memcpy(staging + 256, indices, sizeof indices);
    for (int row = 0; row < 2; row++) {
        memcpy(staging + 512 + row * 256, TEX_A[row * 2], 8);
        memcpy(staging + 1024 + row * 256, TEX_B[row * 2], 8);
    }
    if (!strcmp(fault, "upload")) staging[512] ^= 255; /* wrong texel data reaches the GPU */
    ID3D12Resource *up = buffer(D3D12_HEAP_TYPE_UPLOAD, sizeof staging, D3D12_RESOURCE_STATE_GENERIC_READ);
    fill(up, staging, sizeof staging);
    ID3D12GraphicsCommandList_CopyBufferRegion(list, vb, 0, up, 0, sizeof verts);
    ID3D12GraphicsCommandList_CopyBufferRegion(list, ib, 0, up, 256, sizeof indices);
    for (int t = 0; t < 2; t++) {
        D3D12_TEXTURE_COPY_LOCATION dst = {.pResource = t ? tb : ta,
                                           .Type = D3D12_TEXTURE_COPY_TYPE_SUBRESOURCE_INDEX};
        D3D12_TEXTURE_COPY_LOCATION src = {.pResource = up, .Type = D3D12_TEXTURE_COPY_TYPE_PLACED_FOOTPRINT};
        src.PlacedFootprint.Offset = 512 + 512 * (UINT64)t;
        src.PlacedFootprint.Footprint = (D3D12_SUBRESOURCE_FOOTPRINT){DXGI_FORMAT_R8G8B8A8_UNORM, 2, 2, 1, 256};
        ID3D12GraphicsCommandList_CopyTextureRegion(list, &dst, 0, 0, 0, &src, NULL);
        barrier(list, t ? tb : ta, D3D12_RESOURCE_STATE_COPY_DEST, D3D12_RESOURCE_STATE_PIXEL_SHADER_RESOURCE);
    }
    barrier(list, vb, D3D12_RESOURCE_STATE_COPY_DEST, D3D12_RESOURCE_STATE_VERTEX_AND_CONSTANT_BUFFER);
    barrier(list, ib, D3D12_RESOURCE_STATE_COPY_DEST, D3D12_RESOURCE_STATE_INDEX_BUFFER);
    wait_for(submit(list));
    ID3D12Resource_Release(up);

    /* constant buffer: per slot, one 256-byte entry for quad B and one for quad A */
    ID3D12Resource *cb = buffer(D3D12_HEAP_TYPE_UPLOAD, SLOTS * 2 * 256, D3D12_RESOURCE_STATE_GENERIC_READ);
    unsigned char *cb_ptr = NULL;
    {
        D3D12_RANGE none = {0, 0};
        API(ID3D12Resource_Map(cb, 0, &none, (void **)&cb_ptr), "map constants");
    }
    ID3D12Resource *readback = buffer(D3D12_HEAP_TYPE_READBACK, ROW_PITCH * H, D3D12_RESOURCE_STATE_COPY_DEST);

    /* descriptors */
    ID3D12DescriptorHeap *rtv_heap = heap_of(D3D12_DESCRIPTOR_HEAP_TYPE_RTV, 3, 0);
    ID3D12DescriptorHeap *dsv_heap = heap_of(D3D12_DESCRIPTOR_HEAP_TYPE_DSV, 1, 0);
    ID3D12DescriptorHeap *srv_heap = heap_of(D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV, 5, 1);
    ID3D12DescriptorHeap *smp_heap = heap_of(D3D12_DESCRIPTOR_HEAP_TYPE_SAMPLER, 1, 1);
    const UINT rtv_step = ID3D12Device_GetDescriptorHandleIncrementSize(dev, D3D12_DESCRIPTOR_HEAP_TYPE_RTV);
    const UINT srv_step = ID3D12Device_GetDescriptorHandleIncrementSize(dev, D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV);
    D3D12_CPU_DESCRIPTOR_HANDLE rtv0 = ID3D12DescriptorHeap_GetCPUDescriptorHandleForHeapStart(rtv_heap);
    D3D12_CPU_DESCRIPTOR_HANDLE dsv = ID3D12DescriptorHeap_GetCPUDescriptorHandleForHeapStart(dsv_heap);
    D3D12_CPU_DESCRIPTOR_HANDLE srv0 = ID3D12DescriptorHeap_GetCPUDescriptorHandleForHeapStart(srv_heap);
    D3D12_GPU_DESCRIPTOR_HANDLE gpu0 = ID3D12DescriptorHeap_GetGPUDescriptorHandleForHeapStart(srv_heap);
    D3D12_CPU_DESCRIPTOR_HANDLE rtv[3];
    for (UINT i = 0; i < 3; i++) {
        rtv[i].ptr = rtv0.ptr + (SIZE_T)i * rtv_step;
        ID3D12Device_CreateRenderTargetView(dev, i < 2 ? back[i] : off, NULL, rtv[i]);
    }
    ID3D12Device_CreateDepthStencilView(dev, depth, NULL, dsv);
    ID3D12Resource *srv_res[5] = {ta, tb, off, NULL, post};
    D3D12_GPU_DESCRIPTOR_HANDLE table[5];
    for (UINT i = 0; i < 5; i++) {
        D3D12_CPU_DESCRIPTOR_HANDLE h = {srv0.ptr + (SIZE_T)i * srv_step};
        table[i].ptr = gpu0.ptr + (UINT64)i * srv_step;
        if (srv_res[i]) ID3D12Device_CreateShaderResourceView(dev, srv_res[i], NULL, h);
        else ID3D12Device_CreateUnorderedAccessView(dev, post, NULL, NULL, h);
    }
    D3D12_SAMPLER_DESC smp = {.Filter = D3D12_FILTER_MIN_MAG_MIP_POINT,
                              .AddressU = D3D12_TEXTURE_ADDRESS_MODE_CLAMP,
                              .AddressV = D3D12_TEXTURE_ADDRESS_MODE_CLAMP,
                              .AddressW = D3D12_TEXTURE_ADDRESS_MODE_CLAMP,
                              .ComparisonFunc = D3D12_COMPARISON_FUNC_NEVER, .MaxLOD = D3D12_FLOAT32_MAX};
    ID3D12Device_CreateSampler(dev, &smp, ID3D12DescriptorHeap_GetCPUDescriptorHandleForHeapStart(smp_heap));
    D3D12_GPU_DESCRIPTOR_HANDLE smp_table = ID3D12DescriptorHeap_GetGPUDescriptorHandleForHeapStart(smp_heap);
    gate("graphics.init", 1, "device queue allocators lists fence swapchain resources descriptor heaps");

    ID3DBlob *vsq = compile("vs_quad", "vs_5_0"), *psq = compile("ps_quad", "ps_5_0");
    ID3DBlob *vsf = compile("vs_full", "vs_5_0"), *psf = compile("ps_full", "ps_5_0");
    ID3DBlob *csb = compile("cs_main", "cs_5_0");
    ID3D12RootSignature *rs_gfx = root_signature(0), *rs_cs = root_signature(1);
    ID3D12PipelineState *pso_quad = graphics_pipeline(rs_gfx, vsq, psq, 1, strcmp(fault, "depth") != 0);
    ID3D12PipelineState *pso_full = graphics_pipeline(rs_gfx, vsf, psf, 0, 0);
    ID3D12PipelineState *pso_cs = NULL;
    D3D12_COMPUTE_PIPELINE_STATE_DESC cd = {.pRootSignature = rs_cs,
        .CS = {ID3D10Blob_GetBufferPointer(csb), ID3D10Blob_GetBufferSize(csb)}};
    API(ID3D12Device_CreateComputePipelineState(dev, &cd, &IID_ID3D12PipelineState, (void **)&pso_cs), "compute PSO");
    ID3D10Blob_Release(vsq);
    ID3D10Blob_Release(psq);
    ID3D10Blob_Release(vsf);
    ID3D10Blob_Release(psf);
    ID3D10Blob_Release(csb);
    gate("shader.compile", 1, "root signatures, two graphics and one compute pipeline (shader model 5.0)");

    static const WPARAM script[5] = {VK_RIGHT, VK_RIGHT, VK_DOWN, VK_LEFT, VK_DOWN};
    int injected = 0, verified = 0, bad_frames = 0, presented = 0, churns = 0, churn_ok = 1;
    unsigned long long digest = 1469598103934665603ULL;
    double mem_warm = -1.0, mem_peak = 0.0, prev = now_ms(), next_sample = 60000.0;
    int last_keys = 0, after_keys = 0;
    const int warmup = frames / 5;
    const D3D12_VIEWPORT vp = {0, 0, W, H, 0, 1};
    const D3D12_RECT scissor = {0, 0, W, H};
    const float black[4] = {0, 0, 0, 1};
    const D3D12_VERTEX_BUFFER_VIEW vbv = {ID3D12Resource_GetGPUVirtualAddress(vb), sizeof verts, sizeof(Vertex)};
    const D3D12_INDEX_BUFFER_VIEW ibv = {ID3D12Resource_GetGPUVirtualAddress(ib), sizeof indices,
                                         DXGI_FORMAT_R16_UINT};
    ID3D12DescriptorHeap *heaps[2] = {srv_heap, smp_heap};
    const double loop_start = now_ms();
    if (external) printf("ready for input: right right down left down\n");

    int f = 0, last = 0;
    for (; !last && !quit_requested; f++) {
        if (!external && strcmp(fault, "input") && injected < 5 && f == (injected + 1) * (frames / 8)) {
            WIN(PostMessageA(hwnd, WM_KEYDOWN, script[injected], 0), "PostMessage");
            injected++;
        }
        pump();
        const int keys_changed = keys_seen != last_keys;
        last_keys = keys_seen;
        if (external) {
            if (keys_seen >= 5) after_keys++;
            last = after_keys >= 30 || now_ms() - loop_start > input_timeout_s * 1000.0;
        } else if (duration_s) {
            last = f >= frames - 1 && now_ms() - loop_start >= duration_s * 1000.0;
        } else {
            last = f == frames - 1;
        }
        const unsigned marker = (unsigned)(f + 1) & 0xffff;
        const int bx = 96 + off_x, by = 96 + off_y;
        const int slot = f % SLOTS;
        wait_for(slot_value[slot]);
        Constants moved = {off_x / 128.0f, -off_y / 128.0f, strcmp(fault, "stale") ? marker : 1u, 0};
        Constants fixed = {0.0f, 0.0f, moved.marker, 0};
        memcpy(cb_ptr + slot * 512, &moved, sizeof moved);
        memcpy(cb_ptr + slot * 512 + 256, &fixed, sizeof fixed);
        const D3D12_GPU_VIRTUAL_ADDRESS cb_va = ID3D12Resource_GetGPUVirtualAddress(cb) + (UINT64)slot * 512;
        const UINT bb = IDXGISwapChain3_GetCurrentBackBufferIndex(swap);
        const int check = f == 0 || last || f % verify_every == 0 ||
                          (injected && f == injected * (frames / 8) + 1) || (external && keys_changed);

        API(ID3D12CommandAllocator_Reset(alloc[slot]), "allocator reset");
        API(ID3D12GraphicsCommandList_Reset(list, alloc[slot], NULL), "list reset");
        ID3D12GraphicsCommandList_SetDescriptorHeaps(list, 2, heaps);
        ID3D12GraphicsCommandList_RSSetViewports(list, 1, &vp);
        ID3D12GraphicsCommandList_RSSetScissorRects(list, 1, &scissor);

        /* pass 1 */
        barrier(list, off, D3D12_RESOURCE_STATE_NON_PIXEL_SHADER_RESOURCE, D3D12_RESOURCE_STATE_RENDER_TARGET);
        ID3D12GraphicsCommandList_OMSetRenderTargets(list, 1, &rtv[2], FALSE, &dsv);
        ID3D12GraphicsCommandList_ClearRenderTargetView(list, rtv[2], black, 0, NULL);
        ID3D12GraphicsCommandList_ClearDepthStencilView(list, dsv, D3D12_CLEAR_FLAG_DEPTH, 1.0f, 0, 0, NULL);
        ID3D12GraphicsCommandList_SetGraphicsRootSignature(list, rs_gfx);
        ID3D12GraphicsCommandList_SetPipelineState(list, pso_quad);
        ID3D12GraphicsCommandList_IASetPrimitiveTopology(list, D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
        ID3D12GraphicsCommandList_IASetVertexBuffers(list, 0, 1, &vbv);
        ID3D12GraphicsCommandList_IASetIndexBuffer(list, &ibv);
        ID3D12GraphicsCommandList_SetGraphicsRootDescriptorTable(list, 2, smp_table);
        if (strcmp(fault, "blank")) {
            ID3D12GraphicsCommandList_SetGraphicsRootConstantBufferView(list, 0, cb_va);
            ID3D12GraphicsCommandList_SetGraphicsRootDescriptorTable(list, 1, table[1]);
            ID3D12GraphicsCommandList_DrawIndexedInstanced(list, 6, 1, 0, 0, 0);
            ID3D12GraphicsCommandList_SetGraphicsRootConstantBufferView(list, 0, cb_va + 256);
            ID3D12GraphicsCommandList_SetGraphicsRootDescriptorTable(list, 1, table[0]);
            ID3D12GraphicsCommandList_DrawIndexedInstanced(list, 6, 1, 0, 4, 0);
        }
        barrier(list, off, D3D12_RESOURCE_STATE_RENDER_TARGET, D3D12_RESOURCE_STATE_NON_PIXEL_SHADER_RESOURCE);

        /* pass 2 */
        barrier(list, post, D3D12_RESOURCE_STATE_PIXEL_SHADER_RESOURCE, D3D12_RESOURCE_STATE_UNORDERED_ACCESS);
        ID3D12GraphicsCommandList_SetComputeRootSignature(list, rs_cs);
        ID3D12GraphicsCommandList_SetPipelineState(list, pso_cs);
        ID3D12GraphicsCommandList_SetComputeRootConstantBufferView(list, 0, cb_va);
        ID3D12GraphicsCommandList_SetComputeRootDescriptorTable(list, 1, table[2]);
        ID3D12GraphicsCommandList_SetComputeRootDescriptorTable(list, 2, table[3]);
        ID3D12GraphicsCommandList_Dispatch(list, W / 8, H / 8, 1);
        barrier(list, post, D3D12_RESOURCE_STATE_UNORDERED_ACCESS, D3D12_RESOURCE_STATE_PIXEL_SHADER_RESOURCE);

        /* pass 3 */
        barrier(list, back[bb], D3D12_RESOURCE_STATE_PRESENT, D3D12_RESOURCE_STATE_RENDER_TARGET);
        ID3D12GraphicsCommandList_OMSetRenderTargets(list, 1, &rtv[bb], FALSE, NULL);
        ID3D12GraphicsCommandList_SetGraphicsRootSignature(list, rs_gfx);
        ID3D12GraphicsCommandList_SetPipelineState(list, pso_full);
        ID3D12GraphicsCommandList_SetGraphicsRootConstantBufferView(list, 0, cb_va);
        ID3D12GraphicsCommandList_SetGraphicsRootDescriptorTable(list, 1, table[4]);
        ID3D12GraphicsCommandList_SetGraphicsRootDescriptorTable(list, 2, smp_table);
        ID3D12GraphicsCommandList_DrawInstanced(list, 3, 1, 0, 0);
        if (check) {
            barrier(list, back[bb], D3D12_RESOURCE_STATE_RENDER_TARGET, D3D12_RESOURCE_STATE_COPY_SOURCE);
            copy_texture(list, back[bb], readback, W, H, ROW_PITCH, 0);
            barrier(list, back[bb], D3D12_RESOURCE_STATE_COPY_SOURCE, D3D12_RESOURCE_STATE_PRESENT);
        } else {
            barrier(list, back[bb], D3D12_RESOURCE_STATE_RENDER_TARGET, D3D12_RESOURCE_STATE_PRESENT);
        }
        slot_value[slot] = submit(list);

        if (check) {
            wait_for(slot_value[slot]);
            const unsigned char *mapped = NULL;
            API(ID3D12Resource_Map(readback, 0, NULL, (void **)&mapped), "map readback");
            int mismatches = 0;
            for (int y = 0; y < H; y++) {
                const unsigned char *row = mapped + (size_t)y * ROW_PITCH;
                for (int x = 0; x < W; x++) {
                    unsigned char e[4];
                    expected_pixel(x, y, marker, bx, by, !strcmp(fault, "color"), e);
                    if (memcmp(row + x * 4, e, 4) && !mismatches++)
                        printf("mismatch frame=%d x=%d y=%d got=%u,%u,%u,%u want=%u,%u,%u,%u\n", f, x, y,
                               row[x * 4], row[x * 4 + 1], row[x * 4 + 2], row[x * 4 + 3], e[0], e[1], e[2], e[3]);
                    for (int c = 0; c < 4; c++) digest = (digest ^ row[x * 4 + c]) * 1099511628211ULL;
                }
            }
            D3D12_RANGE none = {0, 0};
            ID3D12Resource_Unmap(readback, 0, &none);
            verified++;
            if (mismatches) bad_frames++;
        }
        API(IDXGISwapChain3_Present(swap, (UINT)vsync, 0), "Present");
        presented++;
        if (f == 0) {
            char msg[96];
            snprintf(msg, sizeof msg, "pixels=%d mismatched_frames=%d present=ok", W * H, bad_frames);
            gate("frame.first", bad_frames == 0, msg);
            if (!strcmp(fault, "crash")) *(volatile int *)0 = 1;
            if (!strcmp(fault, "hang")) Sleep(INFINITE);
        }
        if (f % 100 == 50) {
            churn_ok = churn(aux_alloc, aux, (unsigned)f) && churn_ok;
            churns++;
        }
        const double t = now_ms();
        printf("frame_ms %.4f\n", t - prev);
        prev = t;
        if (f == warmup) mem_warm = working_set_mb();
        if (f % 50 == 0) {
            const double mb = working_set_mb();
            if (mb > mem_peak) mem_peak = mb;
        }
        if (t - loop_start >= next_sample) {
            printf("sample t_s=%.0f frame=%d working_set_mb=%.1f verified=%d mismatched=%d\n",
                   (t - loop_start) / 1000.0, f, working_set_mb(), verified, bad_frames);
            next_sample += 60000.0;
        }
    }
    /* signal after the last Present so the final wait covers it */
    const UINT64 idle = fence_next++;
    API(ID3D12CommandQueue_Signal(queue, fence, idle), "final signal");
    wait_for(idle);

    char msg[200];
    snprintf(msg, sizeof msg, "presented=%d verified=%d mismatched=%d digest=%016llx", presented, verified,
             bad_frames, digest);
    gate("frame.continuous", presented == f && bad_frames == 0 && (external || duration_s || presented == frames), msg);
    snprintf(msg, sizeof msg, "cycles=%d", churns);
    gate("resources.churn", churn_ok && (churns > 0 || presented <= 50), msg);
    if (external) {
        snprintf(msg, sizeof msg, "keys_seen=%d offset=%d,%d want=16,32 (delivered from outside)", keys_seen,
                 off_x, off_y);
        gate("input", keys_seen == 5 && off_x == 16 && off_y == 32 && bad_frames == 0, msg);
    } else {
        snprintf(msg, sizeof msg, "posted=%d received=%d offset=%d,%d want=16,32 (PostMessage)", injected,
                 keys_seen, off_x, off_y);
        gate("input", injected == 5 && keys_seen == 5 && off_x == 16 && off_y == 32 && bad_frames == 0, msg);
    }
    const double mem_end = working_set_mb();
    if (mem_end > mem_peak) mem_peak = mem_end;
    if (mem_warm < 0 && f <= warmup) mem_warm = mem_end; /* run ended before warm-up */
    snprintf(msg, sizeof msg, "working_set_mb_after_warmup=%.1f end=%.1f peak_sampled=%.1f bound_growth=%d",
             mem_warm, mem_end, mem_peak, max_growth);
    gate("memory.bounded", mem_warm > 0 && mem_end > 0 && mem_end - mem_warm <= max_growth, msg);
    const HRESULT removed = ID3D12Device_GetDeviceRemovedReason(dev);
    snprintf(msg, sizeof msg, "device_removed_reason=0x%08lx", (unsigned long)removed);
    gate("device.healthy", removed == S_OK, msg);

    if (hold_ms > 0) {
        /* the last presented image stays in the window; keep handling messages */
        printf("holding hold_ms=%d\n", hold_ms);
        const double until = now_ms() + hold_ms;
        while (now_ms() < until && !quit_requested) {
            pump();
            Sleep(10);
        }
    }

    ID3D12Resource_Unmap(cb, 0, NULL);
#define DROP(p, vt) vt##_Release(p)
    DROP(pso_cs, ID3D12PipelineState);
    DROP(pso_full, ID3D12PipelineState);
    DROP(pso_quad, ID3D12PipelineState);
    DROP(rs_cs, ID3D12RootSignature);
    DROP(rs_gfx, ID3D12RootSignature);
    DROP(smp_heap, ID3D12DescriptorHeap);
    DROP(srv_heap, ID3D12DescriptorHeap);
    DROP(dsv_heap, ID3D12DescriptorHeap);
    DROP(rtv_heap, ID3D12DescriptorHeap);
    ID3D12Resource *owned[] = {readback, cb, ib, vb, tb, ta, depth, post, off, back[0], back[1]};
    for (size_t i = 0; i < sizeof owned / sizeof owned[0]; i++) ID3D12Resource_Release(owned[i]);
    DROP(aux, ID3D12GraphicsCommandList);
    DROP(list, ID3D12GraphicsCommandList);
    DROP(aux_alloc, ID3D12CommandAllocator);
    for (int i = 0; i < SLOTS; i++) DROP(alloc[i], ID3D12CommandAllocator);
    DROP(swap, IDXGISwapChain3);
    DROP(swap1, IDXGISwapChain1);
    DROP(factory, IDXGIFactory4);
    DROP(fence, ID3D12Fence);
    DROP(queue, ID3D12CommandQueue);
    const ULONG refs = ID3D12Device_Release(dev);
    int clean = CloseHandle(fence_event) != 0;
    clean = DestroyWindow(hwnd) && clean;
    pump();
    snprintf(msg, sizeof msg, "window_destroyed device_refs_after_release=%lu", (unsigned long)refs);
    gate("shutdown", clean && refs == 0, msg);

    if (failures) {
        printf("FAIL integrated12 failed_gates=%d\n", failures);
        return 1;
    }
    printf("PASS integrated12 frames=%d verified=%d\n", presented, verified);
    return 0;
}

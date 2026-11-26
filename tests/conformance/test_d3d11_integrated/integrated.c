// SPDX-License-Identifier: GPL-3.0-or-later
// Original BetterXen workload (issue #365): one Windows program that exercises runtime
// services and a presenting Direct3D 11 pipeline together, and verifies its own output.
//
// It is meant to be launched through `betterxen-hybrid run`. Every stage prints one line
//   gate <name> pass|FAIL <detail>
// and the final line is `PASS integrated ...` only if every stage passed.
//
// Rendering, per frame:
//   pass 1  two textured quads into an offscreen target with a depth buffer. The near quad B
//           is drawn FIRST, so the far quad A is visible in the overlap only if depth is
//           ignored. Arrow keys move quad B in 16-pixel steps.
//   pass 2  a compute shader rotates the colour channels (r,g,b) -> (b,r,g) into a UAV and
//           writes a 16-row band that encodes the frame number produced by a second thread.
//   pass 3  a full-screen triangle copies that texture to the swapchain back buffer.
// The back buffer is read back before Present and compared, pixel by pixel and with zero
// tolerance, with an image computed here on the CPU from the authored texels and rectangles.
// All colour channels are 0, 128, 255 or an integer/255 value, so no rounding is involved.
//
// Limits of what this shows: the read-back proves the content handed to Present, and Present
// returning success proves the call was accepted. Neither proves what reached the screen.
// Input is delivered with PostMessage inside the Windows environment; it does not test the
// macOS keyboard path unless a person runs `--interactive`.
//
// usage: integrated.exe [--frames N] [--verify-every N] [--vsync] [--interactive]
//                       [--hold-ms N] [--max-growth-mb N]
//                       [--fault color|blank|stale|input|depth|crash|hang]
// crash and hang are for testing the supervisor: they happen after the first verified frame.
// Exit: 0 pass, 1 a verification failed, 2 no graphics device, 3 an API call failed.
#define COBJMACROS
#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <psapi.h>
#include <d3d11.h>
#include <d3dcompiler.h>
#include <dxgi.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define W 256
#define H 256
#define BAND 16
#define STEP 16
#define WORKERS 4
#define RELEASE(p, vt) do { if (p) { vt##_Release(p); (p) = NULL; } } while (0)

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

/* ---- runtime services ------------------------------------------------------------------ */

static DWORD tls_slot;
static CRITICAL_SECTION lock;
static volatile LONG interlocked_total;
static unsigned long long locked_total;

static DWORD WINAPI worker(LPVOID arg) {
    const unsigned id = (unsigned)(uintptr_t)arg;
    TlsSetValue(tls_slot, (LPVOID)(uintptr_t)(id + 1));
    unsigned long long local = 0;
    for (unsigned i = 1; i <= 10000; i++) {
        local += i;
        InterlockedIncrement(&interlocked_total);
        Sleep(0);
    }
    EnterCriticalSection(&lock);
    locked_total += local * (uintptr_t)TlsGetValue(tls_slot);
    LeaveCriticalSection(&lock);
    return (uintptr_t)TlsGetValue(tls_slot) == id + 1 ? 0 : 1;
}

static void check_threads(void) {
    HANDLE t[WORKERS];
    InitializeCriticalSection(&lock);
    tls_slot = TlsAlloc();
    WIN(tls_slot != TLS_OUT_OF_INDEXES, "TlsAlloc");
    for (unsigned i = 0; i < WORKERS; i++) {
        t[i] = CreateThread(NULL, 0, worker, (LPVOID)(uintptr_t)i, 0, NULL);
        WIN(t[i], "CreateThread");
    }
    int ok = WaitForMultipleObjects(WORKERS, t, TRUE, 60000) == WAIT_OBJECT_0;
    for (unsigned i = 0; i < WORKERS; i++) {
        DWORD code = 99;
        ok = ok && GetExitCodeThread(t[i], &code) && code == 0;
        CloseHandle(t[i]);
    }
    /* each worker sums 1..10000 = 50005000, weighted by id+1: 1+2+3+4 = 10 */
    ok = ok && locked_total == 500050000ULL && interlocked_total == WORKERS * 10000;
    TlsFree(tls_slot);
    DeleteCriticalSection(&lock);
    gate("runtime.threads", ok, "workers=4 tls critical_section interlocked wait");
}

static volatile LONG faults_seen;
static unsigned char *fault_page;

static LONG CALLBACK on_fault(EXCEPTION_POINTERS *e) {
    const EXCEPTION_RECORD *r = e->ExceptionRecord;
    if (r->ExceptionCode == EXCEPTION_ACCESS_VIOLATION && r->NumberParameters >= 2 &&
        r->ExceptionInformation[0] == 1 && r->ExceptionInformation[1] == (ULONG_PTR)fault_page) {
        DWORD old;
        InterlockedIncrement(&faults_seen);
        if (VirtualProtect(fault_page, 4096, PAGE_READWRITE, &old)) return EXCEPTION_CONTINUE_EXECUTION;
    }
    return EXCEPTION_CONTINUE_SEARCH;
}

static void check_memory(void) {
    const size_t n = 16u << 20;
    unsigned char *p = VirtualAlloc(NULL, n, MEM_RESERVE | MEM_COMMIT, PAGE_READWRITE);
    WIN(p, "VirtualAlloc");
    for (size_t i = 0; i < n; i += 997) p[i] = (unsigned char)(i * 31);
    int ok = 1;
    for (size_t i = 0; i < n; i += 997) ok = ok && p[i] == (unsigned char)(i * 31);
    MEMORY_BASIC_INFORMATION mi;
    DWORD old = 0;
    ok = ok && VirtualProtect(p, 4096, PAGE_READONLY, &old) && old == PAGE_READWRITE;
    ok = ok && VirtualQuery(p, &mi, sizeof mi) == sizeof mi && mi.Protect == PAGE_READONLY;
    gate("runtime.memory", ok, "alloc=16MiB protect query");

    fault_page = p;
    PVOID h = AddVectoredExceptionHandler(1, on_fault);
    WIN(h, "AddVectoredExceptionHandler");
    *(volatile unsigned char *)p = 0x5a; /* faults once, handler makes the page writable */
    const int caught = faults_seen == 1 && p[0] == 0x5a;
    RemoveVectoredExceptionHandler(h);
    gate("runtime.exceptions", caught, "write to read-only page handled and resumed");
    VirtualFree(p, 0, MEM_RELEASE);
}

static void check_time(void) {
    const double a = now_ms();
    const ULONGLONG ta = GetTickCount64();
    Sleep(50);
    const double d = now_ms() - a;
    const ULONGLONG td = GetTickCount64() - ta;
    char msg[96];
    snprintf(msg, sizeof msg, "sleep=50ms qpc=%.1fms tick=%llums", d, td);
    /* timer granularity may shorten by a tick; a loaded host may lengthen a lot */
    gate("runtime.time", d >= 45.0 && d < 5000.0 && td >= 30, msg);
}

static void check_files(void) {
    char tmp[MAX_PATH], dir[MAX_PATH + 64], a[MAX_PATH + 72], b[MAX_PATH + 72];
    DWORD length = GetTempPathA(sizeof tmp, tmp);
    WIN(length > 0 && length < sizeof tmp, "GetTempPath within buffer");
    int written = snprintf(dir, sizeof dir, "%sbx-integrated-%lu", tmp, (unsigned long)GetCurrentProcessId());
    WIN(written > 0 && (size_t)written < sizeof dir, "temporary directory path within buffer");
    WIN(CreateDirectoryA(dir, NULL), "CreateDirectory");
    snprintf(a, sizeof a, "%s\\a.bin", dir);
    snprintf(b, sizeof b, "%s\\b.bin", dir);
    static unsigned char out[65536], in[65536];
    for (size_t i = 0; i < sizeof out; i++) out[i] = (unsigned char)(i ^ (i >> 7));
    DWORD n = 0;
    HANDLE f = CreateFileA(a, GENERIC_WRITE, 0, NULL, CREATE_NEW, FILE_ATTRIBUTE_NORMAL, NULL);
    WIN(f != INVALID_HANDLE_VALUE, "CreateFile write");
    int ok = WriteFile(f, out, sizeof out, &n, NULL) && n == sizeof out && FlushFileBuffers(f);
    CloseHandle(f);
    ok = ok && MoveFileA(a, b) && GetFileAttributesA(a) == INVALID_FILE_ATTRIBUTES;
    f = CreateFileA(b, GENERIC_READ, FILE_SHARE_READ, NULL, OPEN_EXISTING, 0, NULL);
    ok = ok && f != INVALID_HANDLE_VALUE;
    if (f != INVALID_HANDLE_VALUE) {
        LARGE_INTEGER pos = {.QuadPart = 4096};
        ok = ok && ReadFile(f, in, sizeof in, &n, NULL) && n == sizeof in && !memcmp(in, out, sizeof in);
        ok = ok && SetFilePointerEx(f, pos, NULL, FILE_BEGIN) && ReadFile(f, in, 16, &n, NULL) &&
             n == 16 && !memcmp(in, out + 4096, 16);
        CloseHandle(f);
    }
    ok = DeleteFileA(b) && ok;
    ok = RemoveDirectoryA(dir) && ok;
    gate("runtime.files", ok, "create write flush rename read seek delete in GetTempPath");
}

/* ---- window, input and the frame producer thread --------------------------------------- */

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

static HANDLE ev_request, ev_ready;
static volatile LONG producer_stop;
static unsigned requested_frame, produced_value;

static DWORD WINAPI producer(LPVOID unused) {
    (void)unused;
    for (;;) {
        if (WaitForSingleObject(ev_request, INFINITE) != WAIT_OBJECT_0) return 1;
        if (producer_stop) return 0;
        produced_value = (requested_frame + 1) & 0xffff; /* never 0, so a stale 0 is visible */
        SetEvent(ev_ready);
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
    /* quad B is nearer, so it wins wherever it covers the pixel */
    if (x >= bx && x < bx + 128 && y >= by && y < by + 128)
        c = TEX_B[((y - by) / 64) * 2 + (x - bx) / 64];
    else if (x >= 32 && x < 160 && y >= 32 && y < 160)
        c = TEX_A[((y - 32) / 64) * 2 + (x - 32) / 64];
    out[0] = c ? c[2] : 0;
    out[1] = c ? c[0] : 0;
    out[2] = c ? c[1] : 0;
    out[3] = 255;
    if (wrong && c == TEX_A[0]) out[0] ^= 255; /* negative control: one texel region */
}

/* ---- Direct3D -------------------------------------------------------------------------- */

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

typedef struct { float x, y, z, u, v; } Vertex;
typedef struct { float ox, oy; unsigned marker, pad; } Constants;

static void quad(Vertex *v, int x0, int y0, float z) {
    const float l = x0 / 128.0f - 1.0f, r = (x0 + 128) / 128.0f - 1.0f;
    const float t = 1.0f - y0 / 128.0f, b = 1.0f - (y0 + 128) / 128.0f;
    v[0] = (Vertex){l, t, z, 0, 0};
    v[1] = (Vertex){r, t, z, 1, 0};
    v[2] = (Vertex){l, b, z, 0, 1};
    v[3] = (Vertex){r, b, z, 1, 1};
}

/* Working set in MiB. Wine on macOS (CrossOver 26.3) returns PrivateUsage = 0 and an implausible
 * PagefileUsage, so only the working set is used; a zero reading is "unavailable", never "no growth". */
static double working_set_mb(void) {
    PROCESS_MEMORY_COUNTERS m;
    memset(&m, 0, sizeof m);
    if (!GetProcessMemoryInfo(GetCurrentProcess(), &m, sizeof m) || !m.WorkingSetSize) return -1.0;
    return (double)m.WorkingSetSize / (1024.0 * 1024.0);
}

static ID3D11Texture2D *texture(ID3D11Device *dev, UINT w, UINT h, DXGI_FORMAT fmt, UINT bind,
                                D3D11_USAGE usage, UINT cpu, const void *init) {
    D3D11_TEXTURE2D_DESC d = {.Width = w, .Height = h, .MipLevels = 1, .ArraySize = 1, .Format = fmt,
                              .SampleDesc = {1, 0}, .Usage = usage, .BindFlags = bind,
                              .CPUAccessFlags = cpu};
    D3D11_SUBRESOURCE_DATA s = {.pSysMem = init, .SysMemPitch = w * 4};
    ID3D11Texture2D *t = NULL;
    API(ID3D11Device_CreateTexture2D(dev, &d, init ? &s : NULL, &t), "CreateTexture2D");
    return t;
}

/* Four independent BC1 blocks: endpoint 0 is red, green, blue, white in RGB565;
   all sixteen indices are zero. Thus an 8x8 point-sampled texture has precisely
   the same quadrant colours as the original uncompressed 2x2 TEX_A. No encoder
   or decoder supplies these expectations. Only exact endpoints are tested.
   https://learn.microsoft.com/windows/win32/direct3d10/d3d10-graphics-programming-guide-resources-block-compression */
static ID3D11Texture2D *bc1_texture(ID3D11Device *dev, int corrupt) {
    unsigned char blocks[32] = {
        0x00,0xf8,0,0,0,0,0,0, 0xe0,0x07,0,0,0,0,0,0,
        0x1f,0x00,0,0,0,0,0,0, 0xff,0xff,0,0,0,0,0,0};
    if (corrupt) { blocks[0]=0xe0; blocks[1]=0x07; } /* red becomes green */
    D3D11_TEXTURE2D_DESC d = {.Width=8,.Height=8,.MipLevels=1,.ArraySize=1,.Format=DXGI_FORMAT_BC1_UNORM,
        .SampleDesc={1,0},.Usage=D3D11_USAGE_IMMUTABLE,.BindFlags=D3D11_BIND_SHADER_RESOURCE};
    D3D11_SUBRESOURCE_DATA initial = {.pSysMem=blocks,.SysMemPitch=16,.SysMemSlicePitch=32};
    ID3D11Texture2D *result=NULL;
    API(ID3D11Device_CreateTexture2D(dev,&d,&initial,&result),"BC1 texture");
    return result;
}

/* Create, fill, copy, read back and release resources; returns 1 if the contents survived. */
static int churn(ID3D11Device *dev, ID3D11DeviceContext *ctx, unsigned seed) {
    static unsigned data[256 * 256];
    for (unsigned i = 0; i < 256 * 256; i++) data[i] = i * 2654435761u + seed;
    ID3D11Texture2D *src = texture(dev, 256, 256, DXGI_FORMAT_R8G8B8A8_UNORM, D3D11_BIND_SHADER_RESOURCE,
                                   D3D11_USAGE_DEFAULT, 0, data);
    ID3D11Texture2D *dst = texture(dev, 256, 256, DXGI_FORMAT_R8G8B8A8_UNORM, 0, D3D11_USAGE_STAGING,
                                   D3D11_CPU_ACCESS_READ, NULL);
    ID3D11DeviceContext_CopyResource(ctx, (ID3D11Resource *)dst, (ID3D11Resource *)src);
    D3D11_MAPPED_SUBRESOURCE m;
    API(ID3D11DeviceContext_Map(ctx, (ID3D11Resource *)dst, 0, D3D11_MAP_READ, 0, &m), "Map churn");
    int ok = 1;
    for (unsigned y = 0; y < 256; y++)
        ok = ok && !memcmp((const char *)m.pData + (size_t)y * m.RowPitch, data + y * 256, 1024);
    ID3D11DeviceContext_Unmap(ctx, (ID3D11Resource *)dst, 0);
    ID3D11Texture2D_Release(src);
    ID3D11Texture2D_Release(dst);
    return ok;
}

int main(int argc, char **argv) {
    int frames = 600, verify_every = 30, vsync = 0, interactive = 0, hold_ms = 0, max_growth = 64, bc1 = 0, hold_first_ms = 0;
    const char *fault = "";
    const char *window_title = "BetterXen integrated workload";
    setvbuf(stdout, NULL, _IONBF, 0); /* the supervisor detects stalls from output growth */
    QueryPerformanceFrequency(&qpc_freq);
    QueryPerformanceCounter(&qpc_start);
    for (int i = 1; i < argc; i++) {
        const char *a = argv[i], *v = i + 1 < argc ? argv[i + 1] : NULL;
        if (!strcmp(a, "--vsync")) vsync = 1;
        else if (!strcmp(a, "--interactive")) interactive = 1;
        else if (!strcmp(a, "--bc1")) bc1 = 1;
        else if (!strcmp(a, "--frames") && v) frames = atoi(v), i++;
        else if (!strcmp(a, "--verify-every") && v) verify_every = atoi(v), i++;
        else if (!strcmp(a, "--hold-ms") && v) hold_ms = atoi(v), i++;
        else if (!strcmp(a, "--hold-first-ms") && v) hold_first_ms = atoi(v), i++;
        else if (!strcmp(a, "--window-title") && v) window_title = v, i++;
        else if (!strcmp(a, "--max-growth-mb") && v) max_growth = atoi(v), i++;
        else if (!strcmp(a, "--fault") && v &&
                 (!strcmp(v, "color") || !strcmp(v, "blank") || !strcmp(v, "stale") ||
                  !strcmp(v, "input") || !strcmp(v, "depth") || !strcmp(v, "crash") || !strcmp(v, "hang") || !strcmp(v,"bc1")))
            fault = v, i++;
        else { printf("FAIL usage: unknown or incomplete argument %s\n", a); return 3; }
    }
    if (frames < 64 || verify_every < 1) { printf("FAIL usage: --frames >= 64, --verify-every >= 1\n"); return 3; }

    check_threads();
    check_memory();
    check_time();
    check_files();

    WNDCLASSA wc = {.lpfnWndProc = wndproc, .hInstance = GetModuleHandleA(NULL),
                    .lpszClassName = "BetterXenIntegrated"};
    WIN(RegisterClassA(&wc), "RegisterClass");
    RECT r = {0, 0, W, H};
    AdjustWindowRect(&r, WS_OVERLAPPEDWINDOW, FALSE);
    HWND hwnd = CreateWindowA(wc.lpszClassName, window_title,
                              WS_OVERLAPPEDWINDOW | WS_VISIBLE, 64, 64, r.right - r.left,
                              r.bottom - r.top, NULL, NULL, wc.hInstance, NULL);
    WIN(hwnd, "CreateWindow");
    pump();

    DXGI_SWAP_CHAIN_DESC sd = {.BufferDesc = {.Width = W, .Height = H, .Format = DXGI_FORMAT_R8G8B8A8_UNORM},
                               .SampleDesc = {1, 0}, .BufferUsage = DXGI_USAGE_RENDER_TARGET_OUTPUT,
                               .BufferCount = 2, .OutputWindow = hwnd, .Windowed = TRUE,
                               .SwapEffect = DXGI_SWAP_EFFECT_DISCARD};
    ID3D11Device *dev = NULL;
    ID3D11DeviceContext *ctx = NULL;
    IDXGISwapChain *swap = NULL;
    D3D_FEATURE_LEVEL fl, want = D3D_FEATURE_LEVEL_11_0;
    HRESULT hr = D3D11CreateDeviceAndSwapChain(NULL, D3D_DRIVER_TYPE_HARDWARE, NULL, 0, &want, 1,
                                               D3D11_SDK_VERSION, &sd, &swap, &dev, &fl, &ctx);
    if (FAILED(hr)) { printf("NO DEVICE hr=0x%08lx\n", (unsigned long)hr); return 2; }
    {
        IDXGIDevice *dd = NULL;
        IDXGIAdapter *ad = NULL;
        DXGI_ADAPTER_DESC desc;
        if (SUCCEEDED(ID3D11Device_QueryInterface(dev, &IID_IDXGIDevice, (void **)&dd)) &&
            SUCCEEDED(IDXGIDevice_GetAdapter(dd, &ad)) && SUCCEEDED(IDXGIAdapter_GetDesc(ad, &desc))) {
            char name[128];
            size_t i = 0;
            for (; i < 127 && desc.Description[i]; i++) name[i] = (char)desc.Description[i];
            name[i] = 0;
            printf("adapter=%s vendor=0x%x\n", name, desc.VendorId);
        }
        RELEASE(ad, IDXGIAdapter);
        RELEASE(dd, IDXGIDevice);
    }

    ID3D11Texture2D *back = NULL;
    API(IDXGISwapChain_GetBuffer(swap, 0, &IID_ID3D11Texture2D, (void **)&back), "GetBuffer");
    ID3D11RenderTargetView *back_rtv = NULL, *off_rtv = NULL;
    API(ID3D11Device_CreateRenderTargetView(dev, (ID3D11Resource *)back, NULL, &back_rtv), "back rtv");
    ID3D11Texture2D *off = texture(dev, W, H, DXGI_FORMAT_R8G8B8A8_UNORM,
                                   D3D11_BIND_RENDER_TARGET | D3D11_BIND_SHADER_RESOURCE,
                                   D3D11_USAGE_DEFAULT, 0, NULL);
    ID3D11Texture2D *post = texture(dev, W, H, DXGI_FORMAT_R8G8B8A8_UNORM,
                                    D3D11_BIND_UNORDERED_ACCESS | D3D11_BIND_SHADER_RESOURCE,
                                    D3D11_USAGE_DEFAULT, 0, NULL);
    ID3D11Texture2D *depth = texture(dev, W, H, DXGI_FORMAT_D32_FLOAT, D3D11_BIND_DEPTH_STENCIL,
                                     D3D11_USAGE_DEFAULT, 0, NULL);
    ID3D11Texture2D *staging = texture(dev, W, H, DXGI_FORMAT_R8G8B8A8_UNORM, 0, D3D11_USAGE_STAGING,
                                       D3D11_CPU_ACCESS_READ, NULL);
    ID3D11Texture2D *ta = bc1 ? bc1_texture(dev,!strcmp(fault,"bc1")) : texture(dev, 2, 2, DXGI_FORMAT_R8G8B8A8_UNORM, D3D11_BIND_SHADER_RESOURCE,
                                  D3D11_USAGE_IMMUTABLE, 0, TEX_A);
    ID3D11Texture2D *tb = texture(dev, 2, 2, DXGI_FORMAT_R8G8B8A8_UNORM, D3D11_BIND_SHADER_RESOURCE,
                                  D3D11_USAGE_IMMUTABLE, 0, TEX_B);
    ID3D11DepthStencilView *dsv = NULL;
    ID3D11ShaderResourceView *off_srv, *post_srv, *ta_srv, *tb_srv;
    ID3D11UnorderedAccessView *post_uav = NULL;
    API(ID3D11Device_CreateRenderTargetView(dev, (ID3D11Resource *)off, NULL, &off_rtv), "off rtv");
    API(ID3D11Device_CreateDepthStencilView(dev, (ID3D11Resource *)depth, NULL, &dsv), "dsv");
    API(ID3D11Device_CreateShaderResourceView(dev, (ID3D11Resource *)off, NULL, &off_srv), "off srv");
    API(ID3D11Device_CreateShaderResourceView(dev, (ID3D11Resource *)post, NULL, &post_srv), "post srv");
    API(ID3D11Device_CreateShaderResourceView(dev, (ID3D11Resource *)ta, NULL, &ta_srv), "a srv");
    API(ID3D11Device_CreateShaderResourceView(dev, (ID3D11Resource *)tb, NULL, &tb_srv), "b srv");
    API(ID3D11Device_CreateUnorderedAccessView(dev, (ID3D11Resource *)post, NULL, &post_uav), "uav");

    Vertex verts[8];
    quad(verts, 96, 96, 0.25f);    /* B, near, vertices 0..3 */
    quad(verts + 4, 32, 32, 0.5f); /* A, far, vertices 4..7 */
    const unsigned short indices[6] = {0, 1, 2, 2, 1, 3};
    ID3D11Buffer *vb, *ib, *cb;
    D3D11_BUFFER_DESC bd = {.ByteWidth = sizeof verts, .BindFlags = D3D11_BIND_VERTEX_BUFFER};
    D3D11_SUBRESOURCE_DATA init = {.pSysMem = verts};
    API(ID3D11Device_CreateBuffer(dev, &bd, &init, &vb), "vb");
    bd.ByteWidth = sizeof indices;
    bd.BindFlags = D3D11_BIND_INDEX_BUFFER;
    init.pSysMem = indices;
    API(ID3D11Device_CreateBuffer(dev, &bd, &init, &ib), "ib");
    bd.ByteWidth = sizeof(Constants);
    bd.BindFlags = D3D11_BIND_CONSTANT_BUFFER;
    API(ID3D11Device_CreateBuffer(dev, &bd, NULL, &cb), "cb");

    D3D11_SAMPLER_DESC smpd = {.Filter = D3D11_FILTER_MIN_MAG_MIP_POINT,
                               .AddressU = D3D11_TEXTURE_ADDRESS_CLAMP,
                               .AddressV = D3D11_TEXTURE_ADDRESS_CLAMP,
                               .AddressW = D3D11_TEXTURE_ADDRESS_CLAMP,
                               .ComparisonFunc = D3D11_COMPARISON_NEVER, .MaxLOD = D3D11_FLOAT32_MAX};
    ID3D11SamplerState *sampler;
    API(ID3D11Device_CreateSamplerState(dev, &smpd, &sampler), "sampler");
    D3D11_DEPTH_STENCIL_DESC dsd = {.DepthEnable = strcmp(fault, "depth") != 0,
                                    .DepthWriteMask = D3D11_DEPTH_WRITE_MASK_ALL,
                                    .DepthFunc = D3D11_COMPARISON_LESS};
    ID3D11DepthStencilState *depth_state;
    API(ID3D11Device_CreateDepthStencilState(dev, &dsd, &depth_state), "depth state");
    D3D11_QUERY_DESC qd = {D3D11_QUERY_EVENT, 0};
    ID3D11Query *query;
    API(ID3D11Device_CreateQuery(dev, &qd, &query), "query");
    gate("graphics.init", 1, "window swapchain targets depth textures buffers views states");

    ID3DBlob *b_vsq = compile("vs_quad", "vs_5_0"), *b_psq = compile("ps_quad", "ps_5_0");
    ID3DBlob *b_vsf = compile("vs_full", "vs_5_0"), *b_psf = compile("ps_full", "ps_5_0");
    ID3DBlob *b_cs = compile("cs_main", "cs_5_0");
#define PTR(b) ID3D10Blob_GetBufferPointer(b)
#define LEN(b) ID3D10Blob_GetBufferSize(b)
    ID3D11VertexShader *vsq, *vsf;
    ID3D11PixelShader *psq, *psf;
    ID3D11ComputeShader *cs;
    ID3D11InputLayout *layout;
    API(ID3D11Device_CreateVertexShader(dev, PTR(b_vsq), LEN(b_vsq), NULL, &vsq), "vs quad");
    API(ID3D11Device_CreatePixelShader(dev, PTR(b_psq), LEN(b_psq), NULL, &psq), "ps quad");
    API(ID3D11Device_CreateVertexShader(dev, PTR(b_vsf), LEN(b_vsf), NULL, &vsf), "vs full");
    API(ID3D11Device_CreatePixelShader(dev, PTR(b_psf), LEN(b_psf), NULL, &psf), "ps full");
    API(ID3D11Device_CreateComputeShader(dev, PTR(b_cs), LEN(b_cs), NULL, &cs), "cs");
    const D3D11_INPUT_ELEMENT_DESC ied[2] = {
        {"POSITION", 0, DXGI_FORMAT_R32G32B32_FLOAT, 0, 0, D3D11_INPUT_PER_VERTEX_DATA, 0},
        {"TEXCOORD", 0, DXGI_FORMAT_R32G32_FLOAT, 0, 12, D3D11_INPUT_PER_VERTEX_DATA, 0}};
    API(ID3D11Device_CreateInputLayout(dev, ied, 2, PTR(b_vsq), LEN(b_vsq), &layout), "input layout");
    ID3D10Blob_Release(b_vsq);
    ID3D10Blob_Release(b_psq);
    ID3D10Blob_Release(b_vsf);
    ID3D10Blob_Release(b_psf);
    ID3D10Blob_Release(b_cs);
    gate("shader.compile", 1, "vs ps cs compiled and created (shader model 5.0)");

    ev_request = CreateEventA(NULL, FALSE, FALSE, NULL);
    ev_ready = CreateEventA(NULL, FALSE, FALSE, NULL);
    WIN(ev_request && ev_ready, "CreateEvent");
    HANDLE prod = CreateThread(NULL, 0, producer, NULL, 0, NULL);
    WIN(prod, "CreateThread producer");

    /* Scripted input: right, right, down, left, down, delivered at fixed frames. */
    static const WPARAM script[5] = {VK_RIGHT, VK_RIGHT, VK_DOWN, VK_LEFT, VK_DOWN};
    int injected = 0, verified = 0, bad_frames = 0, presented = 0, churns = 0, churn_ok = 1;
    unsigned long long digest = 1469598103934665603ULL; /* FNV-1a over verified pixels */
    double mem_warm = -1.0, mem_peak = 0.0, prev = now_ms();
    const int warmup = frames / 5;
    const D3D11_VIEWPORT vp = {0, 0, W, H, 0, 1};
    const float black[4] = {0, 0, 0, 1};
    const UINT stride = sizeof(Vertex), zero = 0;
    ID3D11ShaderResourceView *no_srv = NULL;
    ID3D11UnorderedAccessView *no_uav = NULL;
    ID3D11RenderTargetView *no_rtv = NULL;

    int f = 0;
    for (; f < frames && !quit_requested; f++) {
        if (!interactive && strcmp(fault, "input") && injected < 5 && f == (injected + 1) * (frames / 8)) {
            WIN(PostMessageA(hwnd, WM_KEYDOWN, script[injected], 0), "PostMessage");
            injected++;
        }
        pump();
        requested_frame = (unsigned)f;
        SetEvent(ev_request);
        WIN(WaitForSingleObject(ev_ready, 10000) == WAIT_OBJECT_0, "producer handoff");
        const unsigned marker = produced_value;
        const int bx = 96 + off_x, by = 96 + off_y;

        Constants k = {off_x / 128.0f, -off_y / 128.0f, strcmp(fault, "stale") ? marker : 1u, 0};
        ID3D11DeviceContext_UpdateSubresource(ctx, (ID3D11Resource *)cb, 0, NULL, &k, 0, 0);

        /* pass 1 */
        ID3D11DeviceContext_OMSetRenderTargets(ctx, 1, &off_rtv, dsv);
        ID3D11DeviceContext_OMSetDepthStencilState(ctx, depth_state, 0);
        ID3D11DeviceContext_ClearRenderTargetView(ctx, off_rtv, black);
        ID3D11DeviceContext_ClearDepthStencilView(ctx, dsv, D3D11_CLEAR_DEPTH, 1.0f, 0);
        ID3D11DeviceContext_RSSetViewports(ctx, 1, &vp);
        ID3D11DeviceContext_IASetInputLayout(ctx, layout);
        ID3D11DeviceContext_IASetPrimitiveTopology(ctx, D3D11_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
        ID3D11DeviceContext_IASetVertexBuffers(ctx, 0, 1, &vb, &stride, &zero);
        ID3D11DeviceContext_IASetIndexBuffer(ctx, ib, DXGI_FORMAT_R16_UINT, 0);
        ID3D11DeviceContext_VSSetShader(ctx, vsq, NULL, 0);
        ID3D11DeviceContext_VSSetConstantBuffers(ctx, 0, 1, &cb);
        ID3D11DeviceContext_PSSetShader(ctx, psq, NULL, 0);
        ID3D11DeviceContext_PSSetSamplers(ctx, 0, 1, &sampler);
        if (strcmp(fault, "blank")) {
            ID3D11DeviceContext_PSSetShaderResources(ctx, 0, 1, &tb_srv);
            ID3D11DeviceContext_DrawIndexed(ctx, 6, 0, 0); /* B first, with the input offset */
            Constants fixed = k;
            fixed.ox = fixed.oy = 0.0f;
            ID3D11DeviceContext_UpdateSubresource(ctx, (ID3D11Resource *)cb, 0, NULL, &fixed, 0, 0);
            ID3D11DeviceContext_PSSetShaderResources(ctx, 0, 1, &ta_srv);
            ID3D11DeviceContext_DrawIndexed(ctx, 6, 0, 4);
        }

        /* pass 2 */
        ID3D11DeviceContext_OMSetRenderTargets(ctx, 1, &no_rtv, NULL);
        ID3D11DeviceContext_CSSetShader(ctx, cs, NULL, 0);
        ID3D11DeviceContext_CSSetConstantBuffers(ctx, 0, 1, &cb);
        ID3D11DeviceContext_CSSetShaderResources(ctx, 0, 1, &off_srv);
        ID3D11DeviceContext_CSSetUnorderedAccessViews(ctx, 0, 1, &post_uav, NULL);
        ID3D11DeviceContext_Dispatch(ctx, W / 8, H / 8, 1);
        ID3D11DeviceContext_CSSetUnorderedAccessViews(ctx, 0, 1, &no_uav, NULL);
        ID3D11DeviceContext_CSSetShaderResources(ctx, 0, 1, &no_srv);

        /* pass 3 */
        ID3D11DeviceContext_OMSetRenderTargets(ctx, 1, &back_rtv, NULL);
        ID3D11DeviceContext_IASetInputLayout(ctx, NULL);
        ID3D11DeviceContext_VSSetShader(ctx, vsf, NULL, 0);
        ID3D11DeviceContext_PSSetShader(ctx, psf, NULL, 0);
        ID3D11DeviceContext_PSSetShaderResources(ctx, 0, 1, &post_srv);
        ID3D11DeviceContext_Draw(ctx, 3, 0);
        ID3D11DeviceContext_PSSetShaderResources(ctx, 0, 1, &no_srv);

        const int check = f == 0 || f == frames - 1 || f % verify_every == 0 ||
                          (injected && f == injected * (frames / 8) + 1);
        if (check) {
            ID3D11DeviceContext_CopyResource(ctx, (ID3D11Resource *)staging, (ID3D11Resource *)back);
            D3D11_MAPPED_SUBRESOURCE m;
            API(ID3D11DeviceContext_Map(ctx, (ID3D11Resource *)staging, 0, D3D11_MAP_READ, 0, &m), "Map");
            int mismatches = 0;
            for (int y = 0; y < H; y++) {
                const unsigned char *row = (const unsigned char *)m.pData + (size_t)y * m.RowPitch;
                for (int x = 0; x < W; x++) {
                    unsigned char e[4];
                    expected_pixel(x, y, marker, bx, by, !strcmp(fault, "color"), e);
                    if (memcmp(row + x * 4, e, 4)) {
                        if (!mismatches++)
                            printf("mismatch frame=%d x=%d y=%d got=%u,%u,%u,%u want=%u,%u,%u,%u\n", f, x, y,
                                   row[x * 4], row[x * 4 + 1], row[x * 4 + 2], row[x * 4 + 3], e[0], e[1], e[2], e[3]);
                    }
                    for (int c = 0; c < 4; c++) digest = (digest ^ row[x * 4 + c]) * 1099511628211ULL;
                }
            }
            ID3D11DeviceContext_Unmap(ctx, (ID3D11Resource *)staging, 0);
            verified++;
            if (mismatches) bad_frames++;
        }

        ID3D11DeviceContext_End(ctx, (ID3D11Asynchronous *)query);
        API(IDXGISwapChain_Present(swap, (UINT)vsync, 0), "Present");
        presented++;
        BOOL done = FALSE;
        const double wait_start = now_ms();
        for (;;) {
            HRESULT status=ID3D11DeviceContext_GetData(ctx,(ID3D11Asynchronous *)query,&done,sizeof done,0);
            API(status,"GPU event query");
            if (status==S_OK && done) break;
            WIN(now_ms() - wait_start < 10000.0, "GPU completion within 10 s");
            Sleep(0);
        }
        if (f == 0) {
            char msg[96];
            snprintf(msg, sizeof msg, "pixels=%d mismatched_frames=%d present=ok", W * H, bad_frames);
            gate("frame.first", bad_frames == 0, msg);
            if (!strcmp(fault, "crash")) *(volatile int *)0 = 1;
            if (!strcmp(fault, "hang")) Sleep(INFINITE);
            const double until=now_ms()+hold_first_ms;
            while (now_ms()<until && !quit_requested) { pump(); Sleep(10); }
        }
        if (f % 100 == 50) {
            churn_ok = churn(dev, ctx, (unsigned)f) && churn_ok;
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
    }

    char msg[200];
    snprintf(msg, sizeof msg, "presented=%d verified=%d mismatched=%d digest=%016llx", presented, verified,
             bad_frames, digest);
    gate("frame.continuous", presented == f && (interactive || presented == frames) && bad_frames == 0, msg);
    if (bc1) gate("textures.bc1", verified > 0 && bad_frames == 0, "RGB565 endpoints sampled through graphics, compute and presentation; exact RGBA readback");
    snprintf(msg, sizeof msg, "cycles=%d", churns);
    gate("resources.churn", churn_ok && (interactive || churns > 0 || frames <= 50), msg);

    if (interactive) {
        snprintf(msg, sizeof msg, "keys_seen=%d offset=%d,%d (person-driven, not scripted)", keys_seen, off_x, off_y);
        gate("input", keys_seen > 0 && bad_frames == 0, msg);
    } else {
        /* by hand: right,right,down,left,down = (+16, +32) */
        snprintf(msg, sizeof msg, "posted=%d received=%d offset=%d,%d want=16,32 (PostMessage)", injected,
                 keys_seen, off_x, off_y);
        gate("input", injected == 5 && keys_seen == 5 && off_x == 16 && off_y == 32 && bad_frames == 0, msg);
    }

    const double mem_end = working_set_mb();
    if (mem_end > mem_peak) mem_peak = mem_end;
    snprintf(msg, sizeof msg, "working_set_mb_after_warmup=%.1f end=%.1f peak_sampled=%.1f bound_growth=%d", mem_warm,
             mem_end, mem_peak, max_growth);
    gate("memory.bounded", mem_warm > 0 && mem_end > 0 && mem_end - mem_warm <= max_growth, msg);

    /* keep presenting the last image so an outside observer can capture the window */
    if (hold_ms > 0) {
        printf("holding hold_ms=%d\n", hold_ms);
        const double until = now_ms() + hold_ms;
        while (now_ms() < until && !quit_requested) {
            pump();
            ID3D11DeviceContext_OMSetRenderTargets(ctx, 1, &back_rtv, NULL);
            ID3D11DeviceContext_PSSetShaderResources(ctx, 0, 1, &post_srv);
            ID3D11DeviceContext_Draw(ctx, 3, 0);
            API(IDXGISwapChain_Present(swap, 1, 0), "Present hold");
            Sleep(10);
        }
    }

    producer_stop = 1;
    SetEvent(ev_request);
    int clean = WaitForSingleObject(prod, 10000) == WAIT_OBJECT_0;
    CloseHandle(prod);
    CloseHandle(ev_request);
    CloseHandle(ev_ready);

    ID3D11DeviceContext_ClearState(ctx);
    ID3D11DeviceContext_Flush(ctx);
    RELEASE(query, ID3D11Query);
    RELEASE(depth_state, ID3D11DepthStencilState);
    RELEASE(sampler, ID3D11SamplerState);
    RELEASE(layout, ID3D11InputLayout);
    RELEASE(cs, ID3D11ComputeShader);
    RELEASE(psf, ID3D11PixelShader);
    RELEASE(vsf, ID3D11VertexShader);
    RELEASE(psq, ID3D11PixelShader);
    RELEASE(vsq, ID3D11VertexShader);
    RELEASE(cb, ID3D11Buffer);
    RELEASE(ib, ID3D11Buffer);
    RELEASE(vb, ID3D11Buffer);
    RELEASE(post_uav, ID3D11UnorderedAccessView);
    RELEASE(tb_srv, ID3D11ShaderResourceView);
    RELEASE(ta_srv, ID3D11ShaderResourceView);
    RELEASE(post_srv, ID3D11ShaderResourceView);
    RELEASE(off_srv, ID3D11ShaderResourceView);
    RELEASE(dsv, ID3D11DepthStencilView);
    RELEASE(off_rtv, ID3D11RenderTargetView);
    RELEASE(back_rtv, ID3D11RenderTargetView);
    RELEASE(tb, ID3D11Texture2D);
    RELEASE(ta, ID3D11Texture2D);
    RELEASE(staging, ID3D11Texture2D);
    RELEASE(depth, ID3D11Texture2D);
    RELEASE(post, ID3D11Texture2D);
    RELEASE(off, ID3D11Texture2D);
    RELEASE(back, ID3D11Texture2D);
    RELEASE(swap, IDXGISwapChain);
    RELEASE(ctx, ID3D11DeviceContext);
    const ULONG refs = ID3D11Device_Release(dev);
    clean = DestroyWindow(hwnd) && clean;
    pump();
    snprintf(msg, sizeof msg, "producer_joined window_destroyed device_refs_after_release=%lu", (unsigned long)refs);
    gate("shutdown", clean && refs == 0, msg);

    if (failures) {
        printf("FAIL integrated failed_gates=%d\n", failures);
        return 1;
    }
    printf("PASS integrated frames=%d verified=%d\n", presented, verified);
    return 0;
}

// SPDX-License-Identifier: GPL-3.0-or-later
// Original BetterXen workload (issue #365, #145): Direct3D 11 indexed triangle, shaders,
// offscreen render target and a sustained frame loop with per-frame GPU-completion timing.
//
// Pipeline: HLSL vertex and pixel shaders compiled at run time with D3DCompile, a vertex
// buffer (3 vertices), an index buffer (3 indices), a 640x360 RGBA8 render target, no window
// and no presentation. Each frame clears, draws, issues an event query and waits for it to
// complete; the interval between consecutive completions is printed as `frame_ms <value>`,
// one line per frame, so `betterxen-hybrid frames` can summarise them. This measures
// submit-to-completion of a trivial draw plus synchronisation, NOT presentation or realistic
// rendering cost.
//
// Correctness (read back once after the first frame): pixel (0,0) lies outside the triangle
// and must be the clear colour (0,0,0,255) exactly; the centre pixel must be the triangle
// colour (0.2, 0.4, 0.8, 1.0) within 0.6 of c*255 (the Direct3D float-to-UNORM tolerance).
// Usage: frameloop.exe [--negative-control] [frames]  (default 300). Exit 0 pass, 1 wrong pixel, 2 no device,
// 3 other API failure.
#define COBJMACROS
#include <d3d11.h>
#include <d3dcompiler.h>
#include <dxgi.h>
#include <math.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static const char *VS =
    "struct V { float2 p : POSITION; };\n"
    "float4 main(V v) : SV_Position { return float4(v.p, 0, 1); }\n";
static const char *PS =
    "float4 main() : SV_Target { return float4(0.2, 0.4, 0.8, 1.0); }\n";

#define CHECK(x, what) do { HRESULT hr_ = (x); if (FAILED(hr_)) { \
    printf("FAIL %s hr=0x%08lx\n", what, (unsigned long)hr_); return 3; } } while (0)

static ID3DBlob *compile(const char *src, const char *target) {
    ID3DBlob *code = NULL, *err = NULL;
    HRESULT hr = D3DCompile(src, strlen(src), NULL, NULL, NULL, "main", target, 0, 0, &code, &err);
    if (FAILED(hr)) {
        printf("FAIL D3DCompile %s hr=0x%08lx %s\n", target, (unsigned long)hr,
               err ? (const char *)ID3D10Blob_GetBufferPointer(err) : "");
        return NULL;
    }
    return code;
}

int main(int argc, char **argv) {
    // `--negative-control [frames]` expects a deliberately wrong centre colour and must exit 1.
    const int negative = argc > 1 && !strcmp(argv[1], "--negative-control");
    int frames = argc > 1 + negative ? atoi(argv[1 + negative]) : 300;
    if (frames < 1) frames = 1;
    ID3D11Device *dev = NULL;
    ID3D11DeviceContext *ctx = NULL;
    D3D_FEATURE_LEVEL fl;
    HRESULT hr = D3D11CreateDevice(NULL, D3D_DRIVER_TYPE_HARDWARE, NULL, 0, NULL, 0,
                                   D3D11_SDK_VERSION, &dev, &fl, &ctx);
    if (FAILED(hr)) { printf("NO DEVICE hr=0x%08lx\n", (unsigned long)hr); return 2; }
    printf("device ok feature_level=0x%x\n", (unsigned)fl);
    {
        IDXGIDevice *dd = NULL; IDXGIAdapter *ad = NULL; DXGI_ADAPTER_DESC desc;
        if (SUCCEEDED(ID3D11Device_QueryInterface(dev, &IID_IDXGIDevice, (void **)&dd)) &&
            SUCCEEDED(IDXGIDevice_GetAdapter(dd, &ad)) && SUCCEEDED(IDXGIAdapter_GetDesc(ad, &desc))) {
            char name[128]; size_t i = 0;
            for (; desc.Description[i] && i < 127; i++) name[i] = (char)desc.Description[i];
            name[i] = 0;
            printf("adapter=%s vendor=0x%x dedicated_vram_mb=%u\n", name, desc.VendorId,
                   (unsigned)(desc.DedicatedVideoMemory >> 20));
        }
    }

    ID3DBlob *vsb = compile(VS, "vs_4_0"), *psb = compile(PS, "ps_4_0");
    if (!vsb || !psb) return 3;
    ID3D11VertexShader *vs; ID3D11PixelShader *ps; ID3D11InputLayout *il;
    CHECK(ID3D11Device_CreateVertexShader(dev, ID3D10Blob_GetBufferPointer(vsb), ID3D10Blob_GetBufferSize(vsb), NULL, &vs), "vs");
    CHECK(ID3D11Device_CreatePixelShader(dev, ID3D10Blob_GetBufferPointer(psb), ID3D10Blob_GetBufferSize(psb), NULL, &ps), "ps");
    D3D11_INPUT_ELEMENT_DESC ied = {"POSITION", 0, DXGI_FORMAT_R32G32_FLOAT, 0, 0, D3D11_INPUT_PER_VERTEX_DATA, 0};
    CHECK(ID3D11Device_CreateInputLayout(dev, &ied, 1, ID3D10Blob_GetBufferPointer(vsb), ID3D10Blob_GetBufferSize(vsb), &il), "layout");

    // A triangle that covers the centre of the target and not the (0,0) corner pixel.
    const float verts[6] = {-0.5f, -0.5f, 0.0f, 0.6f, 0.5f, -0.5f};
    const unsigned short idx[3] = {0, 1, 2}; // clockwise in clip space (y up) = front face; D3D11 culls back faces by default
    ID3D11Buffer *vb, *ib;
    D3D11_BUFFER_DESC bd = {0}; D3D11_SUBRESOURCE_DATA sd = {0};
    bd.ByteWidth = sizeof verts; bd.BindFlags = D3D11_BIND_VERTEX_BUFFER; sd.pSysMem = verts;
    CHECK(ID3D11Device_CreateBuffer(dev, &bd, &sd, &vb), "vb");
    bd.ByteWidth = sizeof idx; bd.BindFlags = D3D11_BIND_INDEX_BUFFER; sd.pSysMem = idx;
    CHECK(ID3D11Device_CreateBuffer(dev, &bd, &sd, &ib), "ib");

    const UINT W = 640, H = 360;
    D3D11_TEXTURE2D_DESC td = {0};
    td.Width = W; td.Height = H; td.MipLevels = 1; td.ArraySize = 1;
    td.Format = DXGI_FORMAT_R8G8B8A8_UNORM; td.SampleDesc.Count = 1;
    td.Usage = D3D11_USAGE_DEFAULT; td.BindFlags = D3D11_BIND_RENDER_TARGET;
    ID3D11Texture2D *rt, *st; ID3D11RenderTargetView *rtv;
    CHECK(ID3D11Device_CreateTexture2D(dev, &td, NULL, &rt), "rt");
    td.Usage = D3D11_USAGE_STAGING; td.BindFlags = 0; td.CPUAccessFlags = D3D11_CPU_ACCESS_READ;
    CHECK(ID3D11Device_CreateTexture2D(dev, &td, NULL, &st), "staging");
    CHECK(ID3D11Device_CreateRenderTargetView(dev, (ID3D11Resource *)rt, NULL, &rtv), "rtv");

    D3D11_QUERY_DESC qd = {D3D11_QUERY_EVENT, 0};
    ID3D11Query *q; CHECK(ID3D11Device_CreateQuery(dev, &qd, &q), "query");

    D3D11_VIEWPORT vp = {0, 0, (float)W, (float)H, 0, 1};
    const float clear[4] = {0, 0, 0, 1};
    UINT stride = 8, off = 0;
    LARGE_INTEGER freq, prev, now; QueryPerformanceFrequency(&freq);
    QueryPerformanceCounter(&prev);
    for (int f = 0; f < frames; f++) {
        ID3D11DeviceContext_OMSetRenderTargets(ctx, 1, &rtv, NULL);
        ID3D11DeviceContext_RSSetViewports(ctx, 1, &vp);
        ID3D11DeviceContext_ClearRenderTargetView(ctx, rtv, clear);
        ID3D11DeviceContext_IASetInputLayout(ctx, il);
        ID3D11DeviceContext_IASetPrimitiveTopology(ctx, D3D11_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
        ID3D11DeviceContext_IASetVertexBuffers(ctx, 0, 1, &vb, &stride, &off);
        ID3D11DeviceContext_IASetIndexBuffer(ctx, ib, DXGI_FORMAT_R16_UINT, 0);
        ID3D11DeviceContext_VSSetShader(ctx, vs, NULL, 0);
        ID3D11DeviceContext_PSSetShader(ctx, ps, NULL, 0);
        ID3D11DeviceContext_DrawIndexed(ctx, 3, 0, 0);
        ID3D11DeviceContext_End(ctx, (ID3D11Asynchronous *)q);
        ID3D11DeviceContext_Flush(ctx);
        BOOL done = FALSE;
        while (ID3D11DeviceContext_GetData(ctx, (ID3D11Asynchronous *)q, &done, sizeof done, 0) != S_OK || !done) { }
        QueryPerformanceCounter(&now);
        printf("frame_ms %.4f\n", (double)(now.QuadPart - prev.QuadPart) * 1000.0 / (double)freq.QuadPart);
        prev = now;
        if (f == 0) {
            ID3D11DeviceContext_CopyResource(ctx, (ID3D11Resource *)st, (ID3D11Resource *)rt);
            D3D11_MAPPED_SUBRESOURCE m;
            CHECK(ID3D11DeviceContext_Map(ctx, (ID3D11Resource *)st, 0, D3D11_MAP_READ, 0, &m), "map");
            const unsigned char *c = (const unsigned char *)m.pData;
            const unsigned char *k = c + (size_t)(H / 2) * m.RowPitch + (W / 2) * 4;
            const float want[4] = {0.2f, 0.4f, 0.8f, 1.0f};
            int bad = 0;
            if (c[0] != 0 || c[1] != 0 || c[2] != 0 || c[3] != 255) {
                printf("FAIL corner pixel = %u %u %u %u, want 0 0 0 255\n", c[0], c[1], c[2], c[3]); bad = 1;
            }
            for (int ch = 0; ch < 4; ch++)
                if (fabsf(want[ch] * 255.0f + (negative ? 100.0f : 0.0f) - (float)k[ch]) > 0.6f) bad = 1;
            printf("centre pixel = %u %u %u %u\n", k[0], k[1], k[2], k[3]);
            ID3D11DeviceContext_Unmap(ctx, (ID3D11Resource *)st, 0);
            if (bad) { printf("FAIL pixels\n"); return 1; }
            QueryPerformanceCounter(&prev);
        }
    }
    printf("PASS frames=%d\n", frames);
    return 0;
}

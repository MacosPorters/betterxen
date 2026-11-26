// SPDX-License-Identifier: GPL-3.0-or-later
// Original BetterXen workload (issue #365, first step): Direct3D 11 clear and readback.
// Creates a hardware device, clears a 64x64 RGBA8 render target to (0.25, 0.5, 0.75, 1.0),
// copies it to a staging texture, maps it, and checks every pixel. Expected bytes by the ideal
// FLOAT->UNORM rule (c*255, add 0.5, drop the fraction): 64, 128, 191, 255. The Direct3D data
// conversion rules also permit a tolerance of 0.6 unit-last-place on the integer side
// (D3D11/D3D12_FLOAT32_TO_INTEGER_TOLERANCE_IN_ULP = 0.6f), so a byte passes when it is
// within 0.6 of c*255. 0.5*255 = 127.5 is an exact tie, so both 127 and 128 are permitted.
// The program prints whether the result matched the ideal exactly. This corrects an earlier
// version that required exact equality, which was stricter than the specification.
// Exit codes: 0 pass, 1 wrong pixel, 2 no device (reported, not a pass), 3 other API failure.
#define COBJMACROS
#include <d3d11.h>
#include <math.h>
#include <stdio.h>
#include <string.h>

// `--negative-control` expects a deliberately wrong colour; the program must then exit 1.
int main(int argc, char **argv) {
    const int negative = argc > 1 && !strcmp(argv[1], "--negative-control");
    ID3D11Device *dev = NULL;
    ID3D11DeviceContext *ctx = NULL;
    D3D_FEATURE_LEVEL got;
    HRESULT hr = D3D11CreateDevice(NULL, D3D_DRIVER_TYPE_HARDWARE, NULL, 0, NULL, 0,
                                   D3D11_SDK_VERSION, &dev, &got, &ctx);
    if (FAILED(hr)) { printf("NO DEVICE hr=0x%08lx\n", (unsigned long)hr); return 2; }
    printf("device ok feature_level=0x%x\n", (unsigned)got);

    D3D11_TEXTURE2D_DESC d = {0};
    d.Width = 64; d.Height = 64; d.MipLevels = 1; d.ArraySize = 1;
    d.Format = DXGI_FORMAT_R8G8B8A8_UNORM; d.SampleDesc.Count = 1;
    d.Usage = D3D11_USAGE_DEFAULT; d.BindFlags = D3D11_BIND_RENDER_TARGET;
    ID3D11Texture2D *rt = NULL, *st = NULL;
    if (FAILED(ID3D11Device_CreateTexture2D(dev, &d, NULL, &rt))) { printf("FAIL create rt\n"); return 3; }
    d.Usage = D3D11_USAGE_STAGING; d.BindFlags = 0; d.CPUAccessFlags = D3D11_CPU_ACCESS_READ;
    if (FAILED(ID3D11Device_CreateTexture2D(dev, &d, NULL, &st))) { printf("FAIL create staging\n"); return 3; }
    ID3D11RenderTargetView *rtv = NULL;
    if (FAILED(ID3D11Device_CreateRenderTargetView(dev, (ID3D11Resource *)rt, NULL, &rtv))) {
        printf("FAIL create rtv\n"); return 3;
    }
    const float color[4] = {0.25f, 0.5f, 0.75f, 1.0f};
    ID3D11DeviceContext_ClearRenderTargetView(ctx, rtv, color);
    ID3D11DeviceContext_CopyResource(ctx, (ID3D11Resource *)st, (ID3D11Resource *)rt);
    D3D11_MAPPED_SUBRESOURCE m;
    if (FAILED(ID3D11DeviceContext_Map(ctx, (ID3D11Resource *)st, 0, D3D11_MAP_READ, 0, &m))) {
        printf("FAIL map\n"); return 3;
    }
    const unsigned char ideal[4] = {64, 128, 191, 255};
    const float tol = 0.6f;
    int bad = 0, inexact = 0;
    for (UINT y = 0; y < 64; y++) {
        const unsigned char *row = (const unsigned char *)m.pData + (size_t)y * m.RowPitch;
        for (UINT x = 0; x < 64; x++) {
            const unsigned char *p = row + x * 4;
            int off_ideal = 0, off_tol = 0;
            for (int ch = 0; ch < 4; ch++) {
                if (p[ch] != ideal[ch] + (negative ? 100 : 0)) off_ideal = 1;
                if (fabsf(color[ch] * 255.0f + (negative ? 100.0f : 0.0f) - (float)p[ch]) > tol) off_tol = 1;
            }
            if (off_ideal) inexact++;
            if (off_tol) {
                if (bad < 3) printf("pixel (%u,%u) = %u %u %u %u\n", x, y, p[0], p[1], p[2], p[3]);
                bad++;
            }
        }
    }
    ID3D11DeviceContext_Unmap(ctx, (ID3D11Resource *)st, 0);
    if (bad) { printf("FAIL %d wrong pixels\n", bad); return 1; }
    printf("PASS 4096 pixels within tolerance; %d differ from the ideal rounding\n", inexact);
    return 0;
}

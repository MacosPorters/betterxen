// SPDX-License-Identifier: GPL-3.0-or-later
//
// Original Direct3D 11 texture sample. Draws a triangle that covers an 8x8
// offscreen target, point-samples one of two synthetic textures, and reads
// the target back. Expected pixels come from texture_ref.c. That file does
// not call Direct3D. --negative-control adds one texel to the shader U
// coordinate, so a draw that follows the shader no longer matches the
// unbiased reference.

#define COBJMACROS
#define WIN32_LEAN_AND_MEAN
#define NOMINMAX

#include "texture_ref.h"

#include <d3d11.h>
#include <d3dcompiler.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "sample_hlsl.h"

struct Gpu {
    ID3D11Device *device;
    ID3D11DeviceContext *context;
    ID3D11Texture2D *target;
    ID3D11RenderTargetView *target_view;
    ID3D11Texture2D *staging;
    ID3D11VertexShader *vertex_shader;
    ID3D11PixelShader *pixel_shader;
    ID3D11SamplerState *sampler;
    ID3D11Buffer *bias;
    ID3D11RasterizerState *raster;
    ID3D11BlendState *blend;
};

static void fail_setup(const char *what, HRESULT hr)
{
    printf("d3d11-texture: ERROR %s hr=0x%08lx\n", what, (unsigned long)hr);
    printf("d3d11-texture: FAIL\n");
    fflush(stdout);
    exit(3);
}

static void fail_setup_msg(const char *what)
{
    printf("d3d11-texture: ERROR %s\n", what);
    printf("d3d11-texture: FAIL\n");
    fflush(stdout);
    exit(3);
}

static void format_pixel(const uint8_t *pixel, char out[9])
{
    sprintf(out, "%02x%02x%02x%02x", pixel[0], pixel[1], pixel[2], pixel[3]);
}

static int count_matches(const uint8_t *expected, const uint8_t *actual)
{
    int matched = 0;
    for (int i = 0; i < TEX_PIXELS; i++) {
        if (memcmp(expected + i * 4, actual + i * 4, 4) == 0)
            matched++;
    }
    return matched;
}

static void print_mismatches(const char *name, const uint8_t *expected, const uint8_t *actual)
{
    int shown = 0;
    for (int y = 0; y < TEX_H && shown < 8; y++) {
        for (int x = 0; x < TEX_W && shown < 8; x++) {
            const uint8_t *want = expected + (y * TEX_W + x) * 4;
            const uint8_t *got = actual + (y * TEX_W + x) * 4;
            if (memcmp(want, got, 4) == 0)
                continue;
            char want_text[9];
            char got_text[9];
            format_pixel(want, want_text);
            format_pixel(got, got_text);
            printf("d3d11-texture: mismatch pattern=%s x=%d y=%d expected=%s actual=%s\n", name, x, y, want_text,
                   got_text);
            shown++;
        }
    }
}

static const char *level_name(D3D_FEATURE_LEVEL level)
{
    switch (level) {
    case D3D_FEATURE_LEVEL_11_0:
        return "11_0";
    case D3D_FEATURE_LEVEL_10_1:
        return "10_1";
    case D3D_FEATURE_LEVEL_10_0:
        return "10_0";
    default:
        return "other";
    }
}

static int create_device(struct Gpu *gpu, const char **driver_name, const char **feature_name)
{
    static const D3D_FEATURE_LEVEL levels[] = {
        D3D_FEATURE_LEVEL_11_0,
        D3D_FEATURE_LEVEL_10_1,
        D3D_FEATURE_LEVEL_10_0,
    };
    static const D3D_DRIVER_TYPE drivers[] = {
        D3D_DRIVER_TYPE_HARDWARE,
        D3D_DRIVER_TYPE_WARP,
    };
    static const char *names[] = {"hardware", "warp"};
    HRESULT hrs[2];

    for (int i = 0; i < 2; i++) {
        D3D_FEATURE_LEVEL got = D3D_FEATURE_LEVEL_10_0;
        gpu->device = NULL;
        gpu->context = NULL;
        hrs[i] = D3D11CreateDevice(NULL, drivers[i], NULL, 0, levels, 3, D3D11_SDK_VERSION, &gpu->device, &got,
                                   &gpu->context);
        if (SUCCEEDED(hrs[i])) {
            *driver_name = names[i];
            *feature_name = level_name(got);
            return 1;
        }
    }
    printf("d3d11-texture: ERROR device hardware hr=0x%08lx warp hr=0x%08lx\n", (unsigned long)hrs[0],
           (unsigned long)hrs[1]);
    printf("d3d11-texture: FAIL\n");
    fflush(stdout);
    return 0;
}

static ID3DBlob *compile_stage(const char *entry, const char *profile)
{
    ID3DBlob *code = NULL;
    ID3DBlob *errors = NULL;
    HRESULT hr = D3DCompile(k_shader_source, strlen(k_shader_source), "sample.hlsl", NULL, NULL, entry, profile,
                            D3DCOMPILE_OPTIMIZATION_LEVEL1, 0, &code, &errors);
    if (FAILED(hr)) {
        if (errors) {
            fwrite(ID3D10Blob_GetBufferPointer(errors), 1, ID3D10Blob_GetBufferSize(errors), stdout);
            fputc('\n', stdout);
            ID3D10Blob_Release(errors);
        }
        if (code)
            ID3D10Blob_Release(code);
        fail_setup("D3DCompile", hr);
    }
    if (errors)
        ID3D10Blob_Release(errors);
    return code;
}

static void create_pipeline(struct Gpu *gpu, float u_bias)
{
    ID3DBlob *vs = compile_stage("vs_main", "vs_4_0");
    ID3DBlob *ps = compile_stage("ps_main", "ps_4_0");
    HRESULT hr = ID3D11Device_CreateVertexShader(gpu->device, ID3D10Blob_GetBufferPointer(vs),
                                                 ID3D10Blob_GetBufferSize(vs), NULL, &gpu->vertex_shader);
    if (FAILED(hr)) {
        ID3D10Blob_Release(vs);
        ID3D10Blob_Release(ps);
        fail_setup("CreateVertexShader", hr);
    }
    hr = ID3D11Device_CreatePixelShader(gpu->device, ID3D10Blob_GetBufferPointer(ps), ID3D10Blob_GetBufferSize(ps),
                                        NULL, &gpu->pixel_shader);
    ID3D10Blob_Release(vs);
    ID3D10Blob_Release(ps);
    if (FAILED(hr))
        fail_setup("CreatePixelShader", hr);

    D3D11_SAMPLER_DESC sampler_desc;
    memset(&sampler_desc, 0, sizeof sampler_desc);
    sampler_desc.Filter = D3D11_FILTER_MIN_MAG_MIP_POINT;
    sampler_desc.AddressU = D3D11_TEXTURE_ADDRESS_CLAMP;
    sampler_desc.AddressV = D3D11_TEXTURE_ADDRESS_CLAMP;
    sampler_desc.AddressW = D3D11_TEXTURE_ADDRESS_CLAMP;
    sampler_desc.MaxAnisotropy = 1;
    sampler_desc.ComparisonFunc = D3D11_COMPARISON_NEVER;
    sampler_desc.MinLOD = 0.0f;
    sampler_desc.MaxLOD = 0.0f;
    hr = ID3D11Device_CreateSamplerState(gpu->device, &sampler_desc, &gpu->sampler);
    if (FAILED(hr))
        fail_setup("CreateSamplerState", hr);

    float bias[4] = {u_bias, 0.0f, 0.0f, 0.0f};
    D3D11_BUFFER_DESC buffer_desc;
    memset(&buffer_desc, 0, sizeof buffer_desc);
    buffer_desc.ByteWidth = sizeof bias;
    buffer_desc.Usage = D3D11_USAGE_IMMUTABLE;
    buffer_desc.BindFlags = D3D11_BIND_CONSTANT_BUFFER;
    D3D11_SUBRESOURCE_DATA bias_data;
    bias_data.pSysMem = bias;
    bias_data.SysMemPitch = 0;
    bias_data.SysMemSlicePitch = 0;
    hr = ID3D11Device_CreateBuffer(gpu->device, &buffer_desc, &bias_data, &gpu->bias);
    if (FAILED(hr))
        fail_setup("CreateBuffer", hr);

    D3D11_RASTERIZER_DESC raster_desc;
    memset(&raster_desc, 0, sizeof raster_desc);
    raster_desc.FillMode = D3D11_FILL_SOLID;
    raster_desc.CullMode = D3D11_CULL_NONE;
    raster_desc.DepthClipEnable = TRUE;
    hr = ID3D11Device_CreateRasterizerState(gpu->device, &raster_desc, &gpu->raster);
    if (FAILED(hr))
        fail_setup("CreateRasterizerState", hr);

    D3D11_BLEND_DESC blend_desc;
    memset(&blend_desc, 0, sizeof blend_desc);
    blend_desc.RenderTarget[0].RenderTargetWriteMask = D3D11_COLOR_WRITE_ENABLE_ALL;
    hr = ID3D11Device_CreateBlendState(gpu->device, &blend_desc, &gpu->blend);
    if (FAILED(hr))
        fail_setup("CreateBlendState", hr);

    D3D11_TEXTURE2D_DESC texture_desc;
    memset(&texture_desc, 0, sizeof texture_desc);
    texture_desc.Width = TEX_W;
    texture_desc.Height = TEX_H;
    texture_desc.MipLevels = 1;
    texture_desc.ArraySize = 1;
    texture_desc.Format = DXGI_FORMAT_R8G8B8A8_UNORM;
    texture_desc.SampleDesc.Count = 1;
    texture_desc.Usage = D3D11_USAGE_DEFAULT;
    texture_desc.BindFlags = D3D11_BIND_RENDER_TARGET;
    hr = ID3D11Device_CreateTexture2D(gpu->device, &texture_desc, NULL, &gpu->target);
    if (FAILED(hr))
        fail_setup("CreateTexture2D render target", hr);
    hr = ID3D11Device_CreateRenderTargetView(gpu->device, (ID3D11Resource *)gpu->target, NULL, &gpu->target_view);
    if (FAILED(hr))
        fail_setup("CreateRenderTargetView", hr);

    texture_desc.Usage = D3D11_USAGE_STAGING;
    texture_desc.BindFlags = 0;
    texture_desc.CPUAccessFlags = D3D11_CPU_ACCESS_READ;
    hr = ID3D11Device_CreateTexture2D(gpu->device, &texture_desc, NULL, &gpu->staging);
    if (FAILED(hr))
        fail_setup("CreateTexture2D staging", hr);
}

static void bind_pipeline(struct Gpu *gpu)
{
    D3D11_VIEWPORT viewport;
    viewport.TopLeftX = 0.0f;
    viewport.TopLeftY = 0.0f;
    viewport.Width = (float)TEX_W;
    viewport.Height = (float)TEX_H;
    viewport.MinDepth = 0.0f;
    viewport.MaxDepth = 1.0f;

    ID3D11DeviceContext_IASetInputLayout(gpu->context, NULL);
    ID3D11DeviceContext_IASetPrimitiveTopology(gpu->context, D3D11_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
    ID3D11DeviceContext_VSSetShader(gpu->context, gpu->vertex_shader, NULL, 0);
    ID3D11DeviceContext_PSSetShader(gpu->context, gpu->pixel_shader, NULL, 0);
    ID3D11DeviceContext_PSSetSamplers(gpu->context, 0, 1, &gpu->sampler);
    ID3D11DeviceContext_PSSetConstantBuffers(gpu->context, 0, 1, &gpu->bias);
    ID3D11DeviceContext_RSSetState(gpu->context, gpu->raster);
    ID3D11DeviceContext_RSSetViewports(gpu->context, 1, &viewport);
    ID3D11DeviceContext_OMSetBlendState(gpu->context, gpu->blend, NULL, 0xffffffffu);
    ID3D11DeviceContext_OMSetRenderTargets(gpu->context, 1, &gpu->target_view, NULL);
}

static void read_pattern(struct Gpu *gpu, const uint8_t *texture, uint8_t *readback)
{
    D3D11_TEXTURE2D_DESC desc;
    memset(&desc, 0, sizeof desc);
    desc.Width = TEX_W;
    desc.Height = TEX_H;
    desc.MipLevels = 1;
    desc.ArraySize = 1;
    desc.Format = DXGI_FORMAT_R8G8B8A8_UNORM;
    desc.SampleDesc.Count = 1;
    desc.Usage = D3D11_USAGE_IMMUTABLE;
    desc.BindFlags = D3D11_BIND_SHADER_RESOURCE;

    D3D11_SUBRESOURCE_DATA initial;
    initial.pSysMem = texture;
    initial.SysMemPitch = TEX_W * 4;
    initial.SysMemSlicePitch = 0;

    ID3D11Texture2D *pixels = NULL;
    ID3D11ShaderResourceView *view = NULL;
    HRESULT hr = ID3D11Device_CreateTexture2D(gpu->device, &desc, &initial, &pixels);
    if (FAILED(hr))
        fail_setup("CreateTexture2D source", hr);
    hr = ID3D11Device_CreateShaderResourceView(gpu->device, (ID3D11Resource *)pixels, NULL, &view);
    if (FAILED(hr)) {
        ID3D11Texture2D_Release(pixels);
        fail_setup("CreateShaderResourceView", hr);
    }

    // Magenta is not a texel in either pattern, so a pixel the triangle
    // misses cannot match the reference.
    float clear_color[4] = {1.0f, 0.0f, 1.0f, 1.0f};
    ID3D11DeviceContext_ClearRenderTargetView(gpu->context, gpu->target_view, clear_color);
    ID3D11DeviceContext_PSSetShaderResources(gpu->context, 0, 1, &view);
    ID3D11DeviceContext_Draw(gpu->context, 3, 0);
    ID3D11DeviceContext_CopyResource(gpu->context, (ID3D11Resource *)gpu->staging, (ID3D11Resource *)gpu->target);

    D3D11_MAPPED_SUBRESOURCE mapped;
    memset(&mapped, 0, sizeof mapped);
    hr = ID3D11DeviceContext_Map(gpu->context, (ID3D11Resource *)gpu->staging, 0, D3D11_MAP_READ, 0, &mapped);
    if (FAILED(hr)) {
        ID3D11ShaderResourceView_Release(view);
        ID3D11Texture2D_Release(pixels);
        fail_setup("Map", hr);
    }
    if (mapped.pData == NULL || mapped.RowPitch < TEX_W * 4) {
        ID3D11DeviceContext_Unmap(gpu->context, (ID3D11Resource *)gpu->staging, 0);
        ID3D11ShaderResourceView_Release(view);
        ID3D11Texture2D_Release(pixels);
        fail_setup_msg("staging row");
    }
    const uint8_t *rows = mapped.pData;
    for (int y = 0; y < TEX_H; y++)
        memcpy(readback + y * TEX_W * 4, rows + (size_t)y * mapped.RowPitch, TEX_W * 4);
    ID3D11DeviceContext_Unmap(gpu->context, (ID3D11Resource *)gpu->staging, 0);

    ID3D11ShaderResourceView_Release(view);
    ID3D11Texture2D_Release(pixels);
}

static void release_gpu(struct Gpu *gpu)
{
    if (gpu->blend)
        ID3D11BlendState_Release(gpu->blend);
    if (gpu->raster)
        ID3D11RasterizerState_Release(gpu->raster);
    if (gpu->bias)
        ID3D11Buffer_Release(gpu->bias);
    if (gpu->sampler)
        ID3D11SamplerState_Release(gpu->sampler);
    if (gpu->pixel_shader)
        ID3D11PixelShader_Release(gpu->pixel_shader);
    if (gpu->vertex_shader)
        ID3D11VertexShader_Release(gpu->vertex_shader);
    if (gpu->staging)
        ID3D11Texture2D_Release(gpu->staging);
    if (gpu->target_view)
        ID3D11RenderTargetView_Release(gpu->target_view);
    if (gpu->target)
        ID3D11Texture2D_Release(gpu->target);
    if (gpu->context)
        ID3D11DeviceContext_Release(gpu->context);
    if (gpu->device)
        ID3D11Device_Release(gpu->device);
}

int main(int argc, char **argv)
{
    int negative = 0;
    for (int i = 1; i < argc; i++) {
        if (strcmp(argv[i], "--negative-control") == 0) {
            negative = 1;
            continue;
        }
        fprintf(stderr, "usage: d3d11-texture [--negative-control]\n");
        return 2;
    }

    if (negative)
        printf("d3d11-texture: negative-control: pixel shader adds one texel to U\n");

    struct Gpu gpu;
    memset(&gpu, 0, sizeof gpu);
    const char *driver_name = NULL;
    const char *feature_name = NULL;
    if (!create_device(&gpu, &driver_name, &feature_name))
        return 3;
    printf("d3d11-texture: device=%s feature_level=%s\n", driver_name, feature_name);

    float u_bias = negative ? TEXTURE_TEXEL_U : 0.0f;
    create_pipeline(&gpu, u_bias);
    bind_pipeline(&gpu);

    uint8_t combined[TEX_BYTES * PATTERN_COUNT];
    int failed = 0;
    for (int pattern = 0; pattern < PATTERN_COUNT; pattern++) {
        uint8_t texture[TEX_BYTES];
        uint8_t expected[TEX_BYTES];
        uint8_t readback[TEX_BYTES];
        texture_fill(pattern, texture);
        // The reference stays unbiased. The shader bias is the deliberate mismatch.
        texture_expect_point(texture, 0.0f, 0.0f, expected);
        read_pattern(&gpu, texture, readback);

        const char *name = texture_pattern_name(pattern);
        int matched = count_matches(expected, readback);
        printf("d3d11-texture: pattern=%s matched=%d/%d digest=%016llx\n", name, matched, TEX_PIXELS,
               (unsigned long long)texture_digest(readback, TEX_BYTES));
        print_mismatches(name, expected, readback);
        if (matched != TEX_PIXELS)
            failed = 1;
        memcpy(combined + pattern * TEX_BYTES, readback, TEX_BYTES);
    }

    if (negative && !failed) {
        printf("d3d11-texture: negative-control matched the unbiased reference\n");
        failed = 1;
    }

    printf("d3d11-texture: digest=%016llx\n", (unsigned long long)texture_digest(combined, sizeof combined));
    printf("d3d11-texture: %s\n", failed ? "FAIL" : "PASS");
    fflush(stdout);
    release_gpu(&gpu);
    return failed ? 1 : 0;
}

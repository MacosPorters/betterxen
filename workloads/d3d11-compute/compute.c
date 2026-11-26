// SPDX-License-Identifier: GPL-3.0-or-later
#define COBJMACROS
#include <d3d11.h>
#include <inttypes.h>
#include <stdio.h>
#include <string.h>

static const unsigned char shader_bytecode[] = {
#include "compute_dxbc.inc"
};

enum { GROUP_WIDTH = 64, CAPACITY = 3 * GROUP_WIDTH, PATTERNS = 2 };
static const uint32_t sentinel = UINT32_C(0xcafef00d);

// Authored as base + residue. The second row crosses the 32-bit wrap boundary.
static const uint32_t inputs[PATTERNS][16] = {
    {0x1000, 0x1001, 0x1002, 0x1003, 0x1004, 0x1005, 0x1006, 0x1007,
     0x1008, 0x1009, 0x100a, 0x100b, 0x100c, 0x100d, 0x100e, 0x100f},
    {0xfffffff0, 0xfffffff1, 0xfffffff2, 0xfffffff3,
     0xfffffff4, 0xfffffff5, 0xfffffff6, 0xfffffff7,
     0xfffffff8, 0xfffffff9, 0xfffffffa, 0xfffffffb,
     0xfffffffc, 0xfffffffd, 0xfffffffe, 0xffffffff},
};

static const uint32_t expected[PATTERNS][16] = {
    {0x1001, 0x1003, 0x1005, 0x1007, 0x1009, 0x100b, 0x100d, 0x100f,
     0x1011, 0x1013, 0x1015, 0x1017, 0x1019, 0x101b, 0x101d, 0x101f},
    {0xfffffff1, 0xfffffff3, 0xfffffff5, 0xfffffff7, 0xfffffff9, 0xfffffffb,
     0xfffffffd, 0xffffffff, 1, 3, 5, 7, 9, 11, 13, 15},
};

#define CHECK(operation) do { \
    HRESULT result = (operation); \
    if (FAILED(result)) { \
        printf("API_ERROR operation=%s hr=0x%08lx\n", #operation, (unsigned long)result); \
        goto cleanup; \
    } \
} while (0)

static int run_case(ID3D11Device *device, ID3D11DeviceContext *context,
                    UINT pattern, UINT count, int negative, uint32_t *digest,
                    UINT *mismatches) {
    uint32_t input[CAPACITY], initial[CAPACITY];
    const uint32_t params[4] = {count, 0, 0, 0};
    ID3D11Buffer *source = NULL, *destination = NULL, *staging = NULL, *constants = NULL;
    ID3D11ShaderResourceView *srv = NULL;
    ID3D11UnorderedAccessView *uav = NULL;
    int status = 3;
    for (UINT i = 0; i < CAPACITY; ++i) {
        input[i] = inputs[pattern][i % 16] ^ ((i / 16) << 16);
        initial[i] = sentinel;
    }

    D3D11_BUFFER_DESC desc = {
        .ByteWidth = sizeof input, .Usage = D3D11_USAGE_DEFAULT,
        .BindFlags = D3D11_BIND_SHADER_RESOURCE,
        .MiscFlags = D3D11_RESOURCE_MISC_BUFFER_STRUCTURED,
        .StructureByteStride = sizeof(uint32_t),
    };
    D3D11_SUBRESOURCE_DATA data = {.pSysMem = input};
    CHECK(ID3D11Device_CreateBuffer(device, &desc, &data, &source));
    desc.BindFlags = D3D11_BIND_UNORDERED_ACCESS;
    data.pSysMem = initial;
    CHECK(ID3D11Device_CreateBuffer(device, &desc, &data, &destination));

    desc = (D3D11_BUFFER_DESC){
        .ByteWidth = sizeof initial, .Usage = D3D11_USAGE_STAGING,
        .CPUAccessFlags = D3D11_CPU_ACCESS_READ,
    };
    CHECK(ID3D11Device_CreateBuffer(device, &desc, NULL, &staging));
    desc = (D3D11_BUFFER_DESC){
        .ByteWidth = sizeof params, .Usage = D3D11_USAGE_IMMUTABLE,
        .BindFlags = D3D11_BIND_CONSTANT_BUFFER,
    };
    data.pSysMem = params;
    CHECK(ID3D11Device_CreateBuffer(device, &desc, &data, &constants));

    D3D11_SHADER_RESOURCE_VIEW_DESC read_view = {
        .Format = DXGI_FORMAT_UNKNOWN, .ViewDimension = D3D11_SRV_DIMENSION_BUFFER,
        .Buffer = {.NumElements = CAPACITY},
    };
    D3D11_UNORDERED_ACCESS_VIEW_DESC write_view = {
        .Format = DXGI_FORMAT_UNKNOWN, .ViewDimension = D3D11_UAV_DIMENSION_BUFFER,
        .Buffer = {.NumElements = CAPACITY},
    };
    CHECK(ID3D11Device_CreateShaderResourceView(device, (ID3D11Resource *)source, &read_view, &srv));
    CHECK(ID3D11Device_CreateUnorderedAccessView(device, (ID3D11Resource *)destination, &write_view, &uav));

    if (count) {
        ID3D11DeviceContext_CSSetShaderResources(context, 0, 1, &srv);
        ID3D11DeviceContext_CSSetUnorderedAccessViews(context, 0, 1, &uav, NULL);
        ID3D11DeviceContext_CSSetConstantBuffers(context, 0, 1, &constants);
        ID3D11DeviceContext_Dispatch(context, (count + GROUP_WIDTH - 1) / GROUP_WIDTH, 1, 1);
        ID3D11ShaderResourceView *no_srv = NULL;
        ID3D11UnorderedAccessView *no_uav = NULL;
        ID3D11Buffer *no_buffer = NULL;
        ID3D11DeviceContext_CSSetShaderResources(context, 0, 1, &no_srv);
        ID3D11DeviceContext_CSSetUnorderedAccessViews(context, 0, 1, &no_uav, NULL);
        ID3D11DeviceContext_CSSetConstantBuffers(context, 0, 1, &no_buffer);
    }
    ID3D11DeviceContext_CopyResource(context, (ID3D11Resource *)staging, (ID3D11Resource *)destination);
    D3D11_MAPPED_SUBRESOURCE map;
    CHECK(ID3D11DeviceContext_Map(context, (ID3D11Resource *)staging, 0, D3D11_MAP_READ, 0, &map));
    const uint32_t *words = map.pData;
    for (UINT i = 0; i < CAPACITY; ++i) {
        uint32_t want = sentinel;
        if (i < count) {
            want = expected[pattern][i % 16];
            // Input tags increase pattern 0 and decrease pattern 1 by whole 65536s.
            want += pattern == 0 ? (i / 16) << 16 : 0u - ((i / 16) << 16);
            if (negative && pattern == 0 && count == GROUP_WIDTH + 1 && i == GROUP_WIDTH)
                want ^= 1u;
        }
        if (words[i] != want) {
            if (*mismatches < 8)
                printf("MISMATCH pattern=%u count=%u index=%u expected=%08" PRIx32
                       " actual=%08" PRIx32 "\n", pattern, count, i, want, words[i]);
            ++*mismatches;
        }
        for (unsigned shift = 0; shift < 32; shift += 8) {
            *digest ^= (words[i] >> shift) & 0xffu;
            *digest *= UINT32_C(16777619);
        }
    }
    ID3D11DeviceContext_Unmap(context, (ID3D11Resource *)staging, 0);
    status = 0;

cleanup:
    if (uav) ID3D11UnorderedAccessView_Release(uav);
    if (srv) ID3D11ShaderResourceView_Release(srv);
    if (constants) ID3D11Buffer_Release(constants);
    if (staging) ID3D11Buffer_Release(staging);
    if (destination) ID3D11Buffer_Release(destination);
    if (source) ID3D11Buffer_Release(source);
    return status;
}

static void report_adapter(ID3D11Device *device) {
    IDXGIDevice *dxgi = NULL;
    IDXGIAdapter *adapter = NULL;
    DXGI_ADAPTER_DESC desc;
    if (SUCCEEDED(ID3D11Device_QueryInterface(device, &IID_IDXGIDevice, (void **)&dxgi))) {
        if (SUCCEEDED(IDXGIDevice_GetAdapter(dxgi, &adapter))) {
            if (SUCCEEDED(IDXGIAdapter_GetDesc(adapter, &desc)))
                printf("adapter=%.127ls vendor=0x%x\n", desc.Description, desc.VendorId);
            IDXGIAdapter_Release(adapter);
        }
        IDXGIDevice_Release(dxgi);
    }
}

int main(int argc, char **argv) {
    const int negative = argc == 2 && strcmp(argv[1], "--negative-control") == 0;
    if (argc != 1 && !negative) {
        puts("USAGE d3d11-compute.exe [--negative-control]");
        return 4;
    }

    ID3D11Device *device = NULL;
    ID3D11DeviceContext *context = NULL;
    const D3D_FEATURE_LEVEL level = D3D_FEATURE_LEVEL_11_0;
    HRESULT hr = D3D11CreateDevice(NULL, D3D_DRIVER_TYPE_HARDWARE, NULL, 0, &level, 1,
                                   D3D11_SDK_VERSION, &device, NULL, &context);
    if (FAILED(hr)) {
        printf("NO_DEVICE hr=0x%08lx\n", (unsigned long)hr);
        return 2;
    }

    report_adapter(device);

    ID3D11ComputeShader *shader = NULL;
    int status = 3;
    CHECK(ID3D11Device_CreateComputeShader(device, shader_bytecode,
                                          sizeof shader_bytecode, NULL, &shader));
    ID3D11DeviceContext_CSSetShader(context, shader, NULL, 0);

    const UINT counts[] = {0, 1, 63, 64, 65, 127, 128, 129, 191};
    const UINT count_cases = sizeof counts / sizeof counts[0];
    uint32_t digest = UINT32_C(2166136261);
    UINT mismatches = 0;
    for (UINT pattern = 0; pattern < PATTERNS; ++pattern) {
        for (UINT i = 0; i < count_cases; ++i) {
            status = run_case(device, context, pattern, counts[i], negative, &digest, &mismatches);
            if (status) goto cleanup;
        }
    }
    printf("%s cases=%u words=%u mismatches=%u digest=%08" PRIx32 "\n",
           mismatches ? "FAIL" : "PASS", PATTERNS * count_cases,
           PATTERNS * count_cases * CAPACITY, mismatches, digest);
    status = mismatches ? 1 : 0;

cleanup:
    ID3D11DeviceContext_ClearState(context);
    if (shader) ID3D11ComputeShader_Release(shader);
    ID3D11DeviceContext_Release(context);
    ID3D11Device_Release(device);
    return status;
}

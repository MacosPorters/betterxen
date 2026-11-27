#include "proxy.h"
#include "vmem.h"

#include <cstdio>
#include <mutex>

/*
 * Track C (M1-C1, #414): Xbox Series D3D12 entry points over the host's d3d12.dll
 * (Wine's d3d12.dll, which CrossOver backs with D3DMetal or another translation layer).
 *
 * D3D12XboxCreateDevice creates a host device with D3D12CreateDevice and returns an
 * auto-logging COM proxy around it (see proxy.h). Root signature serialization and
 * deserialization forward directly to the host d3d12.dll.
 */

namespace {

HMODULE get_host_d3d12() {
    static HMODULE mod = LoadLibraryA("d3d12.dll");
    return mod;
}

void initialize_trace_flag() {
    static std::once_flag initialized;
    std::call_once(initialized, [] {
        const bx_runtime_config_t* cfg = bx_get_runtime_config();
        g_bx_trace_d3d = (cfg && cfg->trace_d3d) ? 1u : 0u;
    });
}

} // namespace

extern "C" {

/*
 * `params` is the console creation-parameter block. Its layout is not defined in abi/ and is not
 * interpreted here: the host device is created on the default adapter at the lowest feature level
 * D3D12 accepts, and that fact is logged. `riid` must name an interface the proxy models (or one
 * the host device answers); anything else fails with E_NOINTERFACE and a logged IID.
 */
HRESULT WINAPI D3D12XboxCreateDevice(PVOID params, REFIID riid, void** ppv) {
    if (!ppv) {
        return E_POINTER;
    }
    *ppv = nullptr;
    initialize_trace_flag();

    using CreateFn = HRESULT(WINAPI*)(IUnknown*, D3D_FEATURE_LEVEL, REFIID, void**);
    HMODULE d3d12 = get_host_d3d12();
    auto create = d3d12
        ? reinterpret_cast<CreateFn>(reinterpret_cast<void*>(GetProcAddress(d3d12, "D3D12CreateDevice")))
        : nullptr;
    if (!create) {
        bx_log_emit(BX_LOG_ERROR, "d3d12_xs", "host_d3d12_missing", "d3d12.dll!D3D12CreateDevice not available");
        return E_NOINTERFACE;
    }

    ID3D12Device* host = nullptr;
    char reuse[8] = {};
    /* Test hook. While the backing cache holds a host heap, that heap keeps the host
       device alive, and a real runtime then returns the same ID3D12Device. Set the
       variable so the next proxy wraps that device even if this host does not. */
    const bool reuse_backing = GetEnvironmentVariableA("BX_D3D12_REUSE_BACKING_HOST", reuse, sizeof(reuse)) > 0 &&
                               reuse[0] == '1' && reuse[1] == '\0';
    if (reuse_backing) {
        host = static_cast<ID3D12Device*>(bx_vmem_addref_backing_host());
    }
    HRESULT hr = S_OK;
    if (!host) {
        hr = create(nullptr, D3D_FEATURE_LEVEL_11_0, __uuidof(ID3D12Device), reinterpret_cast<void**>(&host));
        if (FAILED(hr) || !host) {
            char detail[96];
            std::snprintf(detail, sizeof(detail), "D3D12CreateDevice hr=0x%08lx", static_cast<unsigned long>(hr));
            bx_log_emit(BX_LOG_ERROR, "d3d12_xs", "host_device_failed", detail);
            return FAILED(hr) ? hr : E_FAIL;
        }
    }

    BxDeviceProxy* proxy = BxDeviceProxy::create(host);
    host->Release();
    if (!proxy) {
        return E_OUTOFMEMORY;
    }
    if (!bx_vmem_install_device_hooks(proxy)) {
        proxy->Release();
        return E_FAIL;
    }

    /* Record how far up the ID3D12DeviceN chain the host goes: QueryInterface on the proxy stops there. */
    int highest = 0;
    ID3D12Device* const levels[] = {
        proxy->h_ID3D12Device1, proxy->h_ID3D12Device2, proxy->h_ID3D12Device3, proxy->h_ID3D12Device4,
        proxy->h_ID3D12Device5, proxy->h_ID3D12Device6, proxy->h_ID3D12Device7, proxy->h_ID3D12Device8,
        proxy->h_ID3D12Device9, proxy->h_ID3D12Device10,
    };
    for (int i = 0; i < 10; ++i) {
        if (levels[i]) highest = i + 1;
    }
    char detail[256];
    std::snprintf(detail, sizeof(detail),
                  "host_device=%p highest_host_interface=ID3D12Device%d feature_level=11_0 params=%s (not interpreted) trace_d3d=%u",
                  static_cast<void*>(proxy->h_ID3D12Device), highest, params ? "non-null" : "null", g_bx_trace_d3d);
    bx_log_emit(BX_LOG_INFO, "d3d12_xs", "D3D12XboxCreateDevice", detail);

    hr = proxy->QueryInterface(riid, ppv);
    proxy->Release();
    return hr;
}

HRESULT WINAPI D3D12ConfigureVirtualMemoryX(PVOID config) {
    return bx_d3d12_configure_virtual_memory(config);
}

HRESULT WINAPI D3D12SerializeVersionedRootSignature(
    const D3D12_VERSIONED_ROOT_SIGNATURE_DESC* desc,
    ID3DBlob** blob,
    ID3DBlob** error_blob
) {
    using Fn = HRESULT(WINAPI*)(const D3D12_VERSIONED_ROOT_SIGNATURE_DESC*, ID3DBlob**, ID3DBlob**);
    HMODULE d3d12 = get_host_d3d12();
    if (d3d12) {
        auto fn = reinterpret_cast<Fn>(reinterpret_cast<void*>(
            GetProcAddress(d3d12, "D3D12SerializeVersionedRootSignature")));
        if (fn) {
            return fn(desc, blob, error_blob);
        }
    }
    BX_STUB_LOG("d3d12_xs.dll", "D3D12SerializeVersionedRootSignature");
    return E_NOTIMPL;
}

HRESULT WINAPI D3D12CreateRootSignatureDeserializer(
    LPCVOID src_data,
    SIZE_T src_size,
    REFIID riid,
    void** out_deserializer
) {
    using Fn = HRESULT(WINAPI*)(LPCVOID, SIZE_T, REFIID, void**);
    HMODULE d3d12 = get_host_d3d12();
    if (d3d12) {
        auto fn = reinterpret_cast<Fn>(reinterpret_cast<void*>(
            GetProcAddress(d3d12, "D3D12CreateRootSignatureDeserializer")));
        if (fn) {
            return fn(src_data, src_size, riid, out_deserializer);
        }
    }
    BX_STUB_LOG("d3d12_xs.dll", "D3D12CreateRootSignatureDeserializer");
    return E_NOTIMPL;
}

} // extern "C"

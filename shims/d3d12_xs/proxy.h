#ifndef BETTERXEN_D3D12_XS_PROXY_H
#define BETTERXEN_D3D12_XS_PROXY_H

/*
 * Track C (M1-C1, #414): auto-logging COM proxies over the host d3d12.dll.
 *
 * The pass-through methods live in proxy_generated.{h,cpp} (see tools/gen_proxy.py);
 * this header adds the hand-written state, identity and wrap/unwrap logic.
 *
 * WIDL_EXPLICIT_AGGREGATE_RETURNS makes <d3d12.h> spell out the hidden return pointer of
 * methods that return a struct, which is how those methods are really called on x64 Windows.
 */
#ifndef WIDL_EXPLICIT_AGGREGATE_RETURNS
#define WIDL_EXPLICIT_AGGREGATE_RETURNS
#endif
#include <windows.h>
#include <unknwn.h>
#include <d3d12.h>

#include <atomic>
#include <cstdint>

#include "bx_config.h"
#include "bx_log.h"

/* Private IID: a proxy answers it with its host object (no AddRef). Host objects do not know it. */
extern const GUID IID_BxProxyHost;

/* Non-zero when BX_TRACE_D3D=1: every proxied call is then written to the JSONL log. */
extern uint32_t g_bx_trace_d3d;
void bx_d3d_trace_call(const char* iface, const char* method);
#define BX_D3D_TRACE(iface, method) \
    do { if (g_bx_trace_d3d) bx_d3d_trace_call((iface), (method)); } while (0)

/* Formats a GUID as xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx into `out` (needs 37 bytes). */
void bx_format_guid(REFGUID guid, char* out, size_t out_len);

/* Returns the host object behind `obj` when it is one of our proxies, otherwise `obj` itself. */
void* bx_unwrap_raw(IUnknown* obj);
template <typename T>
T* bx_unwrap(T* obj) {
    return static_cast<T*>(bx_unwrap_raw(obj));
}

#include "proxy_generated.h"

class BxDeviceProxy final : public BxDeviceProxyGen {
public:
    /* Wraps `host` (takes its own references). Returns nullptr on allocation failure. */
    static BxDeviceProxy* create(ID3D12Device* host);

    HRESULT STDMETHODCALLTYPE QueryInterface(REFIID riid, void** ppv) override;
    ULONG STDMETHODCALLTYPE AddRef() override;
    ULONG STDMETHODCALLTYPE Release() override;

    HRESULT STDMETHODCALLTYPE CreateCommandQueue(const D3D12_COMMAND_QUEUE_DESC* desc, REFIID riid, void** ppv) override;
    HRESULT STDMETHODCALLTYPE CreateCommandQueue1(const D3D12_COMMAND_QUEUE_DESC* desc, REFIID creator_id, REFIID riid, void** ppv) override;
    HRESULT STDMETHODCALLTYPE CreateCommandAllocator(D3D12_COMMAND_LIST_TYPE type, REFIID riid, void** ppv) override;
    HRESULT STDMETHODCALLTYPE CreateCommandList(UINT node_mask, D3D12_COMMAND_LIST_TYPE type, ID3D12CommandAllocator* allocator,
                                                ID3D12PipelineState* initial_state, REFIID riid, void** ppv) override;
    HRESULT STDMETHODCALLTYPE CreateCommandList1(UINT node_mask, D3D12_COMMAND_LIST_TYPE type, D3D12_COMMAND_LIST_FLAGS flags,
                                                 REFIID riid, void** ppv) override;
    HRESULT STDMETHODCALLTYPE CreateGraphicsPipelineState(const D3D12_GRAPHICS_PIPELINE_STATE_DESC* desc, REFIID riid, void** ppv) override;
    HRESULT STDMETHODCALLTYPE CreateComputePipelineState(const D3D12_COMPUTE_PIPELINE_STATE_DESC* desc, REFIID riid, void** ppv) override;
    HRESULT STDMETHODCALLTYPE CreatePipelineState(const D3D12_PIPELINE_STATE_STREAM_DESC* desc, REFIID riid, void** ppv) override;
    HRESULT STDMETHODCALLTYPE MakeResident(UINT count, ID3D12Pageable* const* objects) override;
    HRESULT STDMETHODCALLTYPE Evict(UINT count, ID3D12Pageable* const* objects) override;
    HRESULT STDMETHODCALLTYPE EnqueueMakeResident(D3D12_RESIDENCY_FLAGS flags, UINT count, ID3D12Pageable* const* objects,
                                                  ID3D12Fence* fence, UINT64 fence_value) override;
    HRESULT STDMETHODCALLTYPE SetResidencyPriority(UINT count, ID3D12Pageable* const* objects,
                                                   const D3D12_RESIDENCY_PRIORITY* priorities) override;

private:
    BxDeviceProxy() = default;
    ~BxDeviceProxy() = default;
    /* Wraps a freshly created host queue/allocator/list and hands the caller `riid` on the wrapper. */
    HRESULT wrap_child(IUnknown* host_child, const char* what, REFIID riid, void** ppv);
    std::atomic<ULONG> refs_{1};
};

/* Shared identity/lifetime code for the three device-child proxies. */
template <typename Gen>
class BxChildProxy final : public Gen {
public:
    static BxChildProxy* create(IUnknown* host, BxDeviceProxy* device);

    HRESULT STDMETHODCALLTYPE QueryInterface(REFIID riid, void** ppv) override;
    ULONG STDMETHODCALLTYPE AddRef() override;
    ULONG STDMETHODCALLTYPE Release() override;
    HRESULT STDMETHODCALLTYPE GetDevice(REFIID riid, void** ppv) override;

private:
    BxChildProxy() = default;
    ~BxChildProxy() = default;
    std::atomic<ULONG> refs_{1};
    BxDeviceProxy* device_ = nullptr;
    IUnknown* host_identity_ = nullptr;
};

/* The command queue additionally unwraps the command-list array it is handed. */
class BxCommandQueueProxyImpl : public BxCommandQueueProxyGen {
public:
    void STDMETHODCALLTYPE ExecuteCommandLists(UINT count, ID3D12CommandList* const* lists) override;

protected:
    ~BxCommandQueueProxyImpl() = default;
};

using BxCommandQueueProxy = BxChildProxy<BxCommandQueueProxyImpl>;
using BxCommandAllocatorProxy = BxChildProxy<BxCommandAllocatorProxyGen>;
using BxCommandListProxy = BxChildProxy<BxCommandListProxyGen>;

#endif /* BETTERXEN_D3D12_XS_PROXY_H */

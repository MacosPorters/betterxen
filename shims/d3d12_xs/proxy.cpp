#include "proxy.h"
#include "vmem.h"

#include <cstdio>
#include <cstring>
#include <new>

#include "shader_gate.h"

/* {6B1D5C0E-2F4A-4E0B-9A57-3C0E5D7B1A42}: private to this DLL, never handed to a host object. */
const GUID IID_BxProxyHost = {0x6b1d5c0e, 0x2f4a, 0x4e0b, {0x9a, 0x57, 0x3c, 0x0e, 0x5d, 0x7b, 0x1a, 0x42}};

uint32_t g_bx_trace_d3d = 0;

void bx_d3d_trace_call(const char* iface, const char* method) {
    char detail[160];
    std::snprintf(detail, sizeof(detail), "%s::%s", iface, method);
    bx_log_emit(BX_LOG_INFO, "d3d12_xs", "call", detail);
}

void bx_format_guid(REFGUID guid, char* out, size_t out_len) {
    std::snprintf(out, out_len, "%08lx-%04x-%04x-%02x%02x-%02x%02x%02x%02x%02x%02x",
                  static_cast<unsigned long>(guid.Data1), guid.Data2, guid.Data3,
                  guid.Data4[0], guid.Data4[1], guid.Data4[2], guid.Data4[3],
                  guid.Data4[4], guid.Data4[5], guid.Data4[6], guid.Data4[7]);
}

void* bx_unwrap_raw(IUnknown* obj) {
    if (!obj) {
        return nullptr;
    }
    void* host = nullptr;
    if (obj->QueryInterface(IID_BxProxyHost, &host) == S_OK && host) {
        return host; /* our proxy: by contract this answer carries no reference */
    }
    return obj;
}

namespace {

/*
 * Shared tail of QueryInterface for an IID the proxy does not model. The host is asked so that
 * host-internal and newer interfaces keep working, but the pointer it returns is NOT proxied:
 * that is logged every time so the gap is visible in `bx-cli trace`.
 */
HRESULT query_host_unproxied(IUnknown* host, const char* leaf, REFIID riid, void** ppv) {
    char guid[40];
    bx_format_guid(riid, guid, sizeof(guid));
    char detail[160];
    HRESULT hr = host ? host->QueryInterface(riid, ppv) : E_NOINTERFACE;
    if (hr == S_OK && *ppv) {
        std::snprintf(detail, sizeof(detail), "%s iid=%s served by host object without proxy", leaf, guid);
        bx_log_emit(BX_LOG_WARN, "d3d12_xs", "qi_unproxied", detail);
        return S_OK;
    }
    *ppv = nullptr;
    std::snprintf(detail, sizeof(detail), "%s iid=%s hr=0x%08lx", leaf, guid, static_cast<unsigned long>(hr));
    bx_log_emit(BX_LOG_WARN, "d3d12_xs", "qi_unsupported", detail);
    return FAILED(hr) ? hr : E_NOINTERFACE;
}

/* Copies `count` object pointers into `out`, replacing our proxies with their host objects. */
template <typename T>
T** unwrap_array(UINT count, T* const* in, T** stack, UINT stack_cap) {
    T** out = stack;
    if (count > stack_cap) {
        out = static_cast<T**>(HeapAlloc(GetProcessHeap(), 0, sizeof(T*) * count));
        if (!out) {
            return nullptr;
        }
    }
    for (UINT i = 0; i < count; ++i) {
        out[i] = bx_unwrap(in[i]);
    }
    return out;
}

template <typename T>
void free_unwrapped(T** arr, T** stack) {
    if (arr && arr != stack) {
        HeapFree(GetProcessHeap(), 0, arr);
    }
}

constexpr UINT kStackObjects = 64;

/* ---- Shader container metadata (public DXBC container layout) ------------------------------ */

uint32_t read_u32(const uint8_t* p) {
    return static_cast<uint32_t>(p[0]) | (static_cast<uint32_t>(p[1]) << 8) |
           (static_cast<uint32_t>(p[2]) << 16) | (static_cast<uint32_t>(p[3]) << 24);
}

void put_fourcc(char* dst, const uint8_t* src) {
    for (int i = 0; i < 4; ++i) {
        uint8_t ch = src[i];
        bool ok = (ch >= '0' && ch <= '9') || (ch >= 'A' && ch <= 'Z') || (ch >= 'a' && ch <= 'z') || ch == '_';
        dst[i] = ok ? static_cast<char>(ch) : '?';
    }
    dst[4] = '\0';
}

/*
 * Logs what kind of container a shader stage carries: size, magic and (for DXBC) the chunk FourCCs,
 * so the M2 shader-format gate can tell DXIL from other payloads. Nothing is modified or copied.
 *
 * Returns false when a non-empty DXBC container is malformed or has no shader program chunk
 * (DXIL/SHEX/SHDR), so we return E_INVALIDARG instead of forwarding a container that crashes
 * D3DMetal 3.0.
 */
bool log_shader(const char* pso_kind, const char* stage, const D3D12_SHADER_BYTECODE& code) {
    if (!code.pShaderBytecode || code.BytecodeLength == 0) {
        return true;
    }
    auto* bytes = static_cast<const uint8_t*>(code.pShaderBytecode);
    size_t len = code.BytecodeLength;
    char magic[5] = "none";
    if (len >= 4) {
        put_fourcc(magic, bytes);
    }
    char chunks[128] = "";
    uint32_t chunk_count = 0;
    bool well_formed = false;
    bool is_dxbc = len >= 4 && std::memcmp(bytes, "DXBC", 4) == 0;
    bool has_shader_chunk = false;
    /* DXBC header: magic[4] digest[16] version[4] total_size[4] chunk_count[4] chunk_offsets[4*n]. */
    if (is_dxbc && len >= 32) {
        uint32_t declared = read_u32(bytes + 24);
        chunk_count = read_u32(bytes + 28);
        well_formed = declared >= 32 && declared <= len && chunk_count <= (declared - 32) / 4;
        size_t used = 0;
        for (uint32_t i = 0; well_formed && i < chunk_count; ++i) {
            uint32_t off = read_u32(bytes + 32 + 4 * static_cast<size_t>(i));
            if (off > declared || declared - off < 8) {
                well_formed = false;
                break;
            }
            /* ACCC counts too: the shader format gate (#466) decides what happens to it. */
            if (std::memcmp(bytes + off, "DXIL", 4) == 0 ||
                std::memcmp(bytes + off, "SHEX", 4) == 0 ||
                std::memcmp(bytes + off, "SHDR", 4) == 0 ||
                std::memcmp(bytes + off, "ACCC", 4) == 0) {
                has_shader_chunk = true;
            }
            if (i < 16) {
                char cc[5];
                put_fourcc(cc, bytes + off);
                int n = std::snprintf(chunks + used, sizeof(chunks) - used, "%s%s", used ? "," : "", cc);
                if (n > 0 && static_cast<size_t>(n) < sizeof(chunks) - used) {
                    used += static_cast<size_t>(n);
                }
            }
        }
    }
    char detail[320];
    std::snprintf(detail, sizeof(detail), "pso=%s stage=%s bytes=%llu magic=%s well_formed=%d chunk_count=%lu chunks=%s",
                  pso_kind, stage, static_cast<unsigned long long>(len), magic, well_formed ? 1 : 0,
                  static_cast<unsigned long>(chunk_count), chunks);
    bx_log_emit(BX_LOG_INFO, "d3d12_xs", "pso_shader", detail);
    return !is_dxbc || (well_formed && has_shader_chunk);
}

void log_created(const char* what, HRESULT hr) {
    char detail[96];
    std::snprintf(detail, sizeof(detail), "%s hr=0x%08lx", what, static_cast<unsigned long>(hr));
    bx_log_emit(FAILED(hr) ? BX_LOG_WARN : BX_LOG_INFO, "d3d12_xs", "create", detail);
}

} // namespace

/* ---- Device ----------------------------------------------------------------------------------- */

BxDeviceProxy* BxDeviceProxy::create(ID3D12Device* host) {
    auto* self = new (std::nothrow) BxDeviceProxy();
    if (!self) {
        return nullptr;
    }
    self->bx_acquire_hosts(host);
    if (!self->h_ID3D12Device) {
        self->bx_release_hosts();
        delete self;
        return nullptr;
    }
    return self;
}

HRESULT STDMETHODCALLTYPE BxDeviceProxy::QueryInterface(REFIID riid, void** ppv) {
    if (!ppv) {
        return E_POINTER;
    }
    if (IsEqualGUID(riid, IID_BxProxyHost)) {
        *ppv = h_ID3D12Device;
        return S_OK;
    }
    if (bx_match_iid(riid)) {
        AddRef();
        *ppv = static_cast<ID3D12Device10*>(this);
        return S_OK;
    }
    return query_host_unproxied(h_ID3D12Device, kLeafName, riid, ppv);
}

ULONG STDMETHODCALLTYPE BxDeviceProxy::AddRef() {
    return refs_.fetch_add(1, std::memory_order_relaxed) + 1;
}

bool BxDeviceProxy::bx_try_acquire_if_nonzero() {
    ULONG cur = refs_.load(std::memory_order_acquire);
    while (cur != 0) {
        if (refs_.compare_exchange_weak(cur, cur + 1, std::memory_order_acq_rel, std::memory_order_acquire)) {
            return true;
        }
    }
    return false;
}

bool BxDeviceProxy::bx_try_release_last() {
    ULONG expected = 1;
    return refs_.compare_exchange_strong(expected, 0, std::memory_order_acq_rel, std::memory_order_acquire);
}

ULONG STDMETHODCALLTYPE BxDeviceProxy::Release() {
    /* The 1->0 transition and a weak GetDevice acquire share g_section.
       GetDevice adds a reference only when the count is non-zero, so a proxy
       at zero is never brought back. Exactly one Release performs final cleanup. */
    ULONG cur = refs_.load(std::memory_order_acquire);
    for (;;) {
        if (cur == 0) {
            return 0;
        }
        if (cur > 1) {
            if (refs_.compare_exchange_weak(cur, cur - 1, std::memory_order_acq_rel, std::memory_order_acquire)) {
                return cur - 1;
            }
            continue;
        }
        if (bx_vmem_final_release(this)) {
            bx_release_hosts();
            delete this;
            return 0;
        }
        cur = refs_.load(std::memory_order_acquire);
    }
}

HRESULT BxDeviceProxy::wrap_child(IUnknown* host_child, const char* what, REFIID riid, void** ppv) {
    HRESULT hr = E_OUTOFMEMORY;
    IUnknown* wrapper = nullptr;
    void* probe = nullptr;
    if (host_child->QueryInterface(__uuidof(ID3D12CommandQueue), &probe) == S_OK) {
        static_cast<IUnknown*>(probe)->Release();
        wrapper = static_cast<ID3D12CommandQueue*>(BxCommandQueueProxy::create(host_child, this));
    } else if (host_child->QueryInterface(__uuidof(ID3D12CommandAllocator), &probe) == S_OK) {
        static_cast<IUnknown*>(probe)->Release();
        wrapper = static_cast<ID3D12CommandAllocator*>(BxCommandAllocatorProxy::create(host_child, this));
    } else if (host_child->QueryInterface(__uuidof(ID3D12GraphicsCommandList), &probe) == S_OK) {
        static_cast<IUnknown*>(probe)->Release();
        wrapper = static_cast<ID3D12GraphicsCommandList*>(BxCommandListProxy::create(host_child, this));
    } else {
        /* For example a video or other non-graphics command list: hand the host object out, visibly. */
        char guid[40];
        bx_format_guid(riid, guid, sizeof(guid));
        char detail[160];
        std::snprintf(detail, sizeof(detail), "%s iid=%s is not a proxied interface family; host object returned", what, guid);
        bx_log_emit(BX_LOG_WARN, "d3d12_xs", "child_unproxied", detail);
        return host_child->QueryInterface(riid, ppv);
    }
    if (wrapper) {
        hr = wrapper->QueryInterface(riid, ppv);
        wrapper->Release();
    }
    return hr;
}

HRESULT STDMETHODCALLTYPE BxDeviceProxy::CreateCommandQueue(const D3D12_COMMAND_QUEUE_DESC* desc, REFIID riid, void** ppv) {
    BX_D3D_TRACE("ID3D12Device", "CreateCommandQueue");
    if (!ppv) {
        return h_ID3D12Device->CreateCommandQueue(desc, riid, nullptr);
    }
    *ppv = nullptr;
    IUnknown* host_child = nullptr;
    HRESULT hr = h_ID3D12Device->CreateCommandQueue(desc, __uuidof(IUnknown), reinterpret_cast<void**>(&host_child));
    if (hr == S_OK && host_child) {
        hr = wrap_child(host_child, "CreateCommandQueue", riid, ppv);
        host_child->Release();
    }
    log_created("ID3D12CommandQueue", hr);
    return hr;
}

HRESULT STDMETHODCALLTYPE BxDeviceProxy::CreateCommandQueue1(const D3D12_COMMAND_QUEUE_DESC* desc, REFIID creator_id, REFIID riid, void** ppv) {
    BX_D3D_TRACE("ID3D12Device9", "CreateCommandQueue1");
    if (!h_ID3D12Device9) {
        BX_VTABLE_STUB_LOG("ID3D12Device9", 75u, "CreateCommandQueue1");
        if (ppv) *ppv = nullptr;
        return E_NOTIMPL;
    }
    if (!ppv) {
        return h_ID3D12Device9->CreateCommandQueue1(desc, creator_id, riid, nullptr);
    }
    *ppv = nullptr;
    IUnknown* host_child = nullptr;
    HRESULT hr = h_ID3D12Device9->CreateCommandQueue1(desc, creator_id, __uuidof(IUnknown), reinterpret_cast<void**>(&host_child));
    if (hr == S_OK && host_child) {
        hr = wrap_child(host_child, "CreateCommandQueue1", riid, ppv);
        host_child->Release();
    }
    log_created("ID3D12CommandQueue (CreateCommandQueue1)", hr);
    return hr;
}

HRESULT STDMETHODCALLTYPE BxDeviceProxy::CreateCommandAllocator(D3D12_COMMAND_LIST_TYPE type, REFIID riid, void** ppv) {
    BX_D3D_TRACE("ID3D12Device", "CreateCommandAllocator");
    if (!ppv) {
        return h_ID3D12Device->CreateCommandAllocator(type, riid, nullptr);
    }
    *ppv = nullptr;
    IUnknown* host_child = nullptr;
    HRESULT hr = h_ID3D12Device->CreateCommandAllocator(type, __uuidof(IUnknown), reinterpret_cast<void**>(&host_child));
    if (hr == S_OK && host_child) {
        hr = wrap_child(host_child, "CreateCommandAllocator", riid, ppv);
        host_child->Release();
    }
    log_created("ID3D12CommandAllocator", hr);
    return hr;
}

HRESULT STDMETHODCALLTYPE BxDeviceProxy::CreateCommandList(UINT node_mask, D3D12_COMMAND_LIST_TYPE type, ID3D12CommandAllocator* allocator,
                                                           ID3D12PipelineState* initial_state, REFIID riid, void** ppv) {
    BX_D3D_TRACE("ID3D12Device", "CreateCommandList");
    if (!ppv) {
        return h_ID3D12Device->CreateCommandList(node_mask, type, bx_unwrap(allocator), initial_state, riid, nullptr);
    }
    *ppv = nullptr;
    IUnknown* host_child = nullptr;
    HRESULT hr = h_ID3D12Device->CreateCommandList(node_mask, type, bx_unwrap(allocator), initial_state,
                                                   __uuidof(IUnknown), reinterpret_cast<void**>(&host_child));
    if (hr == S_OK && host_child) {
        hr = wrap_child(host_child, "CreateCommandList", riid, ppv);
        host_child->Release();
    }
    log_created("ID3D12GraphicsCommandList", hr);
    return hr;
}

HRESULT STDMETHODCALLTYPE BxDeviceProxy::CreateCommandList1(UINT node_mask, D3D12_COMMAND_LIST_TYPE type, D3D12_COMMAND_LIST_FLAGS flags,
                                                            REFIID riid, void** ppv) {
    BX_D3D_TRACE("ID3D12Device4", "CreateCommandList1");
    if (!h_ID3D12Device4) {
        BX_VTABLE_STUB_LOG("ID3D12Device4", 51u, "CreateCommandList1");
        if (ppv) *ppv = nullptr;
        return E_NOTIMPL;
    }
    if (!ppv) {
        return h_ID3D12Device4->CreateCommandList1(node_mask, type, flags, riid, nullptr);
    }
    *ppv = nullptr;
    IUnknown* host_child = nullptr;
    HRESULT hr = h_ID3D12Device4->CreateCommandList1(node_mask, type, flags, __uuidof(IUnknown), reinterpret_cast<void**>(&host_child));
    if (hr == S_OK && host_child) {
        hr = wrap_child(host_child, "CreateCommandList1", riid, ppv);
        host_child->Release();
    }
    log_created("ID3D12GraphicsCommandList (CreateCommandList1)", hr);
    return hr;
}

HRESULT STDMETHODCALLTYPE BxDeviceProxy::CreateGraphicsPipelineState(const D3D12_GRAPHICS_PIPELINE_STATE_DESC* desc, REFIID riid, void** ppv) {
    BX_D3D_TRACE("ID3D12Device", "CreateGraphicsPipelineState");
    bool shaders_ok = true;
    if (desc) {
        shaders_ok &= log_shader("graphics", "VS", desc->VS);
        shaders_ok &= log_shader("graphics", "PS", desc->PS);
        shaders_ok &= log_shader("graphics", "DS", desc->DS);
        shaders_ok &= log_shader("graphics", "HS", desc->HS);
        shaders_ok &= log_shader("graphics", "GS", desc->GS);
    }
    if (!shaders_ok) {
        if (ppv) *ppv = nullptr;
        log_created("graphics pipeline state", E_INVALIDARG);
        return E_INVALIDARG;
    }
    if (!desc) {
        HRESULT hr = h_ID3D12Device->CreateGraphicsPipelineState(desc, riid, ppv);
        log_created("graphics pipeline state", hr);
        return hr;
    }
    /* M2 shader format gate (#466): every stage is classified; an ACCC-only stage fails the PSO. */
    BxShaderGate gate;
    D3D12_GRAPHICS_PIPELINE_STATE_DESC admitted = *desc;
    HRESULT gated = gate.admit("graphics", "VS", desc->VS, admitted.VS);
    if (SUCCEEDED(gated)) gated = gate.admit("graphics", "PS", desc->PS, admitted.PS);
    if (SUCCEEDED(gated)) gated = gate.admit("graphics", "DS", desc->DS, admitted.DS);
    if (SUCCEEDED(gated)) gated = gate.admit("graphics", "HS", desc->HS, admitted.HS);
    if (SUCCEEDED(gated)) gated = gate.admit("graphics", "GS", desc->GS, admitted.GS);
    if (FAILED(gated)) {
        if (ppv) *ppv = nullptr;
        log_created("graphics pipeline state", gated);
        return gated;
    }
    HRESULT hr = h_ID3D12Device->CreateGraphicsPipelineState(&admitted, riid, ppv);
    log_created("graphics pipeline state", hr);
    return hr;
}

HRESULT STDMETHODCALLTYPE BxDeviceProxy::CreateComputePipelineState(const D3D12_COMPUTE_PIPELINE_STATE_DESC* desc, REFIID riid, void** ppv) {
    BX_D3D_TRACE("ID3D12Device", "CreateComputePipelineState");
    if (desc && !log_shader("compute", "CS", desc->CS)) {
        if (ppv) *ppv = nullptr;
        log_created("compute pipeline state", E_INVALIDARG);
        return E_INVALIDARG;
    }
    if (!desc) {
        HRESULT hr = h_ID3D12Device->CreateComputePipelineState(desc, riid, ppv);
        log_created("compute pipeline state", hr);
        return hr;
    }
    /* M2 shader format gate (#466): DXIL is passed on, an ACCC-only container is refused. */
    BxShaderGate gate;
    D3D12_COMPUTE_PIPELINE_STATE_DESC admitted = *desc;
    const HRESULT gated = gate.admit("compute", "CS", desc->CS, admitted.CS);
    if (FAILED(gated)) {
        if (ppv) *ppv = nullptr;
        log_created("compute pipeline state", gated);
        return gated;
    }
    HRESULT hr = h_ID3D12Device->CreateComputePipelineState(&admitted, riid, ppv);
    log_created("compute pipeline state", hr);
    return hr;
}

HRESULT STDMETHODCALLTYPE BxDeviceProxy::CreatePipelineState(const D3D12_PIPELINE_STATE_STREAM_DESC* desc, REFIID riid, void** ppv) {
    BX_D3D_TRACE("ID3D12Device2", "CreatePipelineState");
    if (!h_ID3D12Device2) {
        BX_VTABLE_STUB_LOG("ID3D12Device2", 47u, "CreatePipelineState");
        if (ppv) *ppv = nullptr;
        return E_NOTIMPL;
    }
    /* The stream form is forwarded unparsed: only its size is recorded (sub-object walking is M2 work). */
    char detail[96];
    std::snprintf(detail, sizeof(detail), "pso=stream bytes=%llu shaders_not_parsed=1",
                  static_cast<unsigned long long>(desc ? desc->SizeInBytes : 0));
    bx_log_emit(BX_LOG_INFO, "d3d12_xs", "pso_stream", detail);
    HRESULT hr = h_ID3D12Device2->CreatePipelineState(desc, riid, ppv);
    log_created("stream pipeline state", hr);
    return hr;
}

HRESULT STDMETHODCALLTYPE BxDeviceProxy::MakeResident(UINT count, ID3D12Pageable* const* objects) {
    BX_D3D_TRACE("ID3D12Device", "MakeResident");
    ID3D12Pageable* stack[kStackObjects];
    ID3D12Pageable** hosts = objects ? unwrap_array(count, objects, stack, kStackObjects) : nullptr;
    if (objects && !hosts) return E_OUTOFMEMORY;
    HRESULT hr = h_ID3D12Device->MakeResident(count, hosts);
    free_unwrapped(hosts, stack);
    return hr;
}

HRESULT STDMETHODCALLTYPE BxDeviceProxy::Evict(UINT count, ID3D12Pageable* const* objects) {
    BX_D3D_TRACE("ID3D12Device", "Evict");
    ID3D12Pageable* stack[kStackObjects];
    ID3D12Pageable** hosts = objects ? unwrap_array(count, objects, stack, kStackObjects) : nullptr;
    if (objects && !hosts) return E_OUTOFMEMORY;
    HRESULT hr = h_ID3D12Device->Evict(count, hosts);
    free_unwrapped(hosts, stack);
    return hr;
}

HRESULT STDMETHODCALLTYPE BxDeviceProxy::EnqueueMakeResident(D3D12_RESIDENCY_FLAGS flags, UINT count, ID3D12Pageable* const* objects,
                                                             ID3D12Fence* fence, UINT64 fence_value) {
    BX_D3D_TRACE("ID3D12Device3", "EnqueueMakeResident");
    if (!h_ID3D12Device3) {
        BX_VTABLE_STUB_LOG("ID3D12Device3", 50u, "EnqueueMakeResident");
        return E_NOTIMPL;
    }
    ID3D12Pageable* stack[kStackObjects];
    ID3D12Pageable** hosts = objects ? unwrap_array(count, objects, stack, kStackObjects) : nullptr;
    if (objects && !hosts) return E_OUTOFMEMORY;
    HRESULT hr = h_ID3D12Device3->EnqueueMakeResident(flags, count, hosts, fence, fence_value);
    free_unwrapped(hosts, stack);
    return hr;
}

HRESULT STDMETHODCALLTYPE BxDeviceProxy::SetResidencyPriority(UINT count, ID3D12Pageable* const* objects,
                                                              const D3D12_RESIDENCY_PRIORITY* priorities) {
    BX_D3D_TRACE("ID3D12Device1", "SetResidencyPriority");
    if (!h_ID3D12Device1) {
        BX_VTABLE_STUB_LOG("ID3D12Device1", 46u, "SetResidencyPriority");
        return E_NOTIMPL;
    }
    ID3D12Pageable* stack[kStackObjects];
    ID3D12Pageable** hosts = objects ? unwrap_array(count, objects, stack, kStackObjects) : nullptr;
    if (objects && !hosts) return E_OUTOFMEMORY;
    HRESULT hr = h_ID3D12Device1->SetResidencyPriority(count, hosts, priorities);
    free_unwrapped(hosts, stack);
    return hr;
}

/* ---- Device children (queue, allocator, list) ------------------------------------------------- */

template <typename Gen>
BxChildProxy<Gen>* BxChildProxy<Gen>::create(IUnknown* host, BxDeviceProxy* device) {
    auto* self = new (std::nothrow) BxChildProxy<Gen>();
    if (!self) {
        return nullptr;
    }
    self->bx_acquire_hosts(host);
    host->QueryInterface(__uuidof(IUnknown), reinterpret_cast<void**>(&self->host_identity_));
    self->device_ = device;
    device->AddRef();
    return self;
}

template <typename Gen>
HRESULT STDMETHODCALLTYPE BxChildProxy<Gen>::QueryInterface(REFIID riid, void** ppv) {
    if (!ppv) {
        return E_POINTER;
    }
    if (IsEqualGUID(riid, IID_BxProxyHost)) {
        *ppv = host_identity_;
        return S_OK;
    }
    if (this->bx_match_iid(riid)) {
        AddRef();
        *ppv = static_cast<Gen*>(this);
        return S_OK;
    }
    return query_host_unproxied(host_identity_, Gen::kLeafName, riid, ppv);
}

template <typename Gen>
ULONG STDMETHODCALLTYPE BxChildProxy<Gen>::AddRef() {
    return refs_.fetch_add(1, std::memory_order_relaxed) + 1;
}

template <typename Gen>
ULONG STDMETHODCALLTYPE BxChildProxy<Gen>::Release() {
    ULONG left = refs_.fetch_sub(1, std::memory_order_acq_rel) - 1;
    if (left == 0) {
        this->bx_release_hosts();
        if (host_identity_) host_identity_->Release();
        BxDeviceProxy* device = device_;
        delete this;
        if (device) device->Release();
    }
    return left;
}

template <typename Gen>
HRESULT STDMETHODCALLTYPE BxChildProxy<Gen>::GetDevice(REFIID riid, void** ppv) {
    BX_D3D_TRACE("ID3D12DeviceChild", "GetDevice");
    if (!ppv) {
        return E_POINTER;
    }
    return device_->QueryInterface(riid, ppv); /* the proxy device, never the host device */
}

void STDMETHODCALLTYPE BxCommandQueueProxyImpl::ExecuteCommandLists(UINT count, ID3D12CommandList* const* lists) {
    BX_D3D_TRACE("ID3D12CommandQueue", "ExecuteCommandLists");
    ID3D12CommandList* stack[kStackObjects];
    ID3D12CommandList** hosts = lists ? unwrap_array(count, lists, stack, kStackObjects) : nullptr;
    if (lists && !hosts) {
        bx_log_emit(BX_LOG_ERROR, "d3d12_xs", "execute_command_lists_dropped", "out of memory unwrapping command lists");
        return;
    }
    h_ID3D12CommandQueue->ExecuteCommandLists(count, hosts);
    free_unwrapped(hosts, stack);
}

template class BxChildProxy<BxCommandQueueProxyImpl>;
template class BxChildProxy<BxCommandAllocatorProxyGen>;
template class BxChildProxy<BxCommandListProxyGen>;

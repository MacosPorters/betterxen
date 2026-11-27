#include "proxy.h"
#include "vmem.h"

#include "bx_xmem_internal.h"

#include <cstdio>
#include <cstring>
#include <new>

/*
 * The configured region is backed by one CUSTOM WRITE_BACK / L0 heap and a buffer
 * that stays mapped. That heap is created with D3D12_HEAP_FLAG_NONE, 64 KiB
 * alignment, and node masks 0. CreateHeap returns it only for the explicit
 * opt-in desc documented in vmem.h. In #473, CPU and GPU reads agreed after
 * a fence on tested L0 heaps. Both OpenExistingHeap methods returned E_NOTIMPL.
 * This file does not request L1 and does not treat an xmem address as a GPU pointer.
 *
 * Any other CreateHeap forwards the caller's D3D12_HEAP_DESC unmodified.
 * The wrapper charges the heap on create and credits it on final Release.
 * Resources created by CreatePlacedResource retain the wrapper.
 * GetDevice on every wrapper returns the BxDeviceProxy. An ordinary heap AddRefs
 * that proxy and Releases it after the wrapper is destroyed, matching
 * BxChildProxy, so the caller may release the device and keep the heap.
 * The cached backing heap does not AddRef: this record would keep the device
 * and its budget charge alive. That pointer is weak. It is cleared when the
 * proxy is destroyed and set to the proxy that next receives the cached heap.
 * The cache is keyed by the host ID3D12Device and is released when that device
 * changes.
 * A null out pointer is forwarded unchanged to the host.
 * vkd3d on CrossOver 26.2 faults on that pointer with or without this shim.
 * https://learn.microsoft.com/en-us/windows/win32/api/d3d12/nf-d3d12-id3d12device-createheap
 *
 * CreatePlacedResource forwards the caller's parameters to the host.
 * On the backing heap the offset must be a multiple of the alignment
 * GetResourceAllocationInfo reports, the resource
 * must fit, and an upload or readback heap (not this CUSTOM heap) must use the
 * initial state Microsoft Learn requires:
 * https://learn.microsoft.com/en-us/windows/win32/api/d3d12/nf-d3d12-id3d12device-createplacedresource
 * https://learn.microsoft.com/en-us/windows/win32/api/d3d12/nf-d3d12-id3d12device-getresourceallocationinfo
 * Heap alignment values 0, 64 KiB and 4 MiB, and ID3D12Heap1:
 * https://learn.microsoft.com/en-us/windows/win32/api/d3d12/ns-d3d12-d3d12_heap_desc
 * https://learn.microsoft.com/en-us/windows/win32/api/d3d12/nn-d3d12-id3d12heap1
 * Non-custom heap properties use UNKNOWN CPU page and memory pool:
 * https://learn.microsoft.com/en-us/windows/win32/api/d3d12/ns-d3d12-d3d12_heap_properties
 */

namespace {

constexpr uint64_t k64K = 64ull * 1024ull;
constexpr uint32_t kConfigVersion = 1;
constexpr GUID kPlacedResourceHeap = {0x3a652cbb, 0xb5b3, 0x4e09, {0xb3, 0x6c, 0xc0, 0x2c, 0xa9, 0x83, 0xae, 0x1a}};

struct Record {
    bool configured = false;
    bool backing_opt_in = false;
    bx_hardware_profile_t profile = BX_PROFILE_LOCKHART;
    uint64_t title_limit = 0;
    bool have_region = false;
    uint64_t config_base = 0;
    uint64_t config_size = 0;
    uint64_t config_generation = 0;
    bx_xmem_region_info_t region{};
    /* Bytes of host heaps still alive. D3DMetal keeps freed memory in its own
       pool, so this counts live heaps only: charge on create, credit on the
       final Release. It does not assume the OS reclaimed the pages. */
    uint64_t charged_private_bytes = 0;
    uint64_t cpu_pointer = 0;
    class BxChargedHeap* backing = nullptr;
    ID3D12Resource* backing_buffer = nullptr;
    /* Host device that created `backing`. Not an owning reference. A later
       CreateHeap whose host device pointer differs releases the cache. */
    ID3D12Device* backing_host = nullptr;
};

struct Section {
    CRITICAL_SECTION cs;
    Section() { InitializeCriticalSection(&cs); }
    ~Section() { DeleteCriticalSection(&cs); }
};

Section g_section;
Record g_record;

struct Guard {
    explicit Guard(Section& section) : section(section) { EnterCriticalSection(&section.cs); }
    ~Guard() { LeaveCriticalSection(&section.cs); }
    Section& section;
};

uint64_t title_limit() {
    const bx_runtime_config_t* cfg = bx_get_runtime_config();
    if (cfg && cfg->title_memory_limit_bytes) {
        return cfg->title_memory_limit_bytes;
    }
    /* Same 8 GiB Lockhart default bx_runtime_veh stores when no profile is set. */
    return 8ull << 30;
}

uint64_t active_cap(const bx_xmem_stats_t& stats) {
    uint64_t cap = title_limit();
    if (stats.title_limit_bytes != 0 && stats.title_limit_bytes < cap) {
        cap = stats.title_limit_bytes;
    }
    return cap;
}

const char* profile_name(bx_hardware_profile_t profile) {
    return profile == BX_PROFILE_ANACONDA ? "anaconda" : "lockhart";
}

/* Private host heaps count against the same Lockhart/Anaconda cap as xmem. */
bool try_charge(uint64_t bytes) {
    Guard guard(g_section);
    bx_xmem_stats_t stats{};
    bx_xmem_get_internal_stats(&stats);
    const uint64_t cap = active_cap(stats);
    const uint64_t xmem_used = stats.committed_virtual_bytes + stats.allocated_physical_bytes;
    if (bytes > cap || xmem_used > cap || g_record.charged_private_bytes > cap - xmem_used) {
        return false;
    }
    const uint64_t room = cap - xmem_used - g_record.charged_private_bytes;
    if (bytes > room) {
        return false;
    }
    g_record.charged_private_bytes += bytes;
    return true;
}

void release_charge(uint64_t bytes) {
    Guard guard(g_section);
    if (bytes > g_record.charged_private_bytes) {
        bx_log_emit(BX_LOG_ERROR, "d3d12_xs", "heap_budget", "GPU heap charge underflow on release");
        g_record.charged_private_bytes = 0;
        return;
    }
    g_record.charged_private_bytes -= bytes;
}

/*
 * Holds the budget charge for one private host heap and drops it on the last Release.
 * QueryInterface for ID3D12Heap, and for ID3D12Heap1 when the host heap answers it,
 * returns this object. bx_unwrap sees IID_BxProxyHost and receives the host heap.
 * GetProtectedResourceSession forwards to the host ID3D12Heap1.
 * GetDevice returns the BxDeviceProxy, matching BxChildProxy::GetDevice.
 * Ordinary heaps (`owns_device_`) AddRef the proxy in the constructor and
 * Release it after `delete this`, the same order as BxChildProxy::Release.
 * The process-wide backing heap does not. g_record holds that heap for the
 * life of the region, so a strong reference would pin the device and the
 * budget charge after the caller dropped the proxy. `device_` on that heap is
 * weak. It is cleared when the heap leaves the cache and when its proxy's
 * reference count goes from 1 to 0. hand_out_backing_locked points it at the
 * proxy that receives the cache. Those writes happen under g_section, together
 * with the 1->0 transition, so GetDevice cannot resurrect a proxy at zero.
 * https://learn.microsoft.com/en-us/windows/win32/api/d3d12/nn-d3d12-id3d12heap1
 * https://learn.microsoft.com/en-us/windows/win32/api/d3d12/nf-d3d12-id3d12devicechild-getdevice
 */
class BxChargedHeap final : public ID3D12Heap1 {
public:
    static BxChargedHeap* create(ID3D12Heap* host, ID3D12Heap1* host1, uint64_t bytes, BxDeviceProxy* device,
                                 bool own_device) {
        return new (std::nothrow) BxChargedHeap(host, host1, bytes, device, own_device);
    }

    /* Weak backing heap only. Called under g_section. */
    void set_device(BxDeviceProxy* device) {
        if (!owns_device_) {
            device_ = device;
        }
    }
    BxDeviceProxy* device() const { return device_; }

    HRESULT STDMETHODCALLTYPE QueryInterface(REFIID riid, void** ppv) override {
        if (!ppv) {
            return E_POINTER;
        }
        *ppv = nullptr;
        if (riid == __uuidof(IUnknown) || riid == __uuidof(ID3D12Object) || riid == __uuidof(ID3D12DeviceChild) ||
            riid == __uuidof(ID3D12Pageable) || riid == __uuidof(ID3D12Heap)) {
            *ppv = static_cast<ID3D12Heap*>(this);
            AddRef();
            return S_OK;
        }
        if (riid == __uuidof(ID3D12Heap1)) {
            if (!host1_) {
                return E_NOINTERFACE;
            }
            *ppv = static_cast<ID3D12Heap1*>(this);
            AddRef();
            return S_OK;
        }
        if (riid == IID_BxProxyHost) {
            *ppv = host_;
            return S_OK;
        }
        return E_NOINTERFACE;
    }

    ULONG STDMETHODCALLTYPE AddRef() override { return InterlockedIncrement(&refs_); }

    ULONG STDMETHODCALLTYPE Release() override {
        const ULONG left = InterlockedDecrement(&refs_);
        if (left == 0) {
            /* Final Release of a live heap. D3DMetal may pool the pages after this,
               so the budget credit is the live-heap count, not a physical-memory drop. */
            if (host1_) {
                host1_->Release();
            }
            if (host_) {
                host_->Release();
            }
            release_charge(bytes_);
            /* Same order as BxChildProxy::Release: destroy the wrapper, then Release the device. */
            BxDeviceProxy* owned = owns_device_ ? device_ : nullptr;
            device_ = nullptr;
            delete this;
            if (owned) {
                owned->Release();
            }
        }
        return left;
    }

    HRESULT STDMETHODCALLTYPE GetPrivateData(REFGUID guid, UINT* size, void* data) override {
        return host_->GetPrivateData(guid, size, data);
    }
    HRESULT STDMETHODCALLTYPE SetPrivateData(REFGUID guid, UINT size, const void* data) override {
        return host_->SetPrivateData(guid, size, data);
    }
    HRESULT STDMETHODCALLTYPE SetPrivateDataInterface(REFGUID guid, const IUnknown* data) override {
        return host_->SetPrivateDataInterface(guid, data);
    }
    HRESULT STDMETHODCALLTYPE SetName(LPCWSTR name) override { return host_->SetName(name); }
    HRESULT STDMETHODCALLTYPE GetDevice(REFIID riid, void** ppv) override {
        BX_D3D_TRACE("ID3D12DeviceChild", "GetDevice");
        if (!ppv) {
            return E_POINTER;
        }
        /* The proxy, never the host device.
           An ordinary heap owns its proxy and nothing else writes device_, so that
           path stays lock-free. The backing heap is weak. Its acquire and the
           proxy's 1->0 Release share g_section. The acquire is a compare-exchange
           that refuses zero, so it cannot resurrect a proxy that Release has
           already claimed. QueryInterface below adds one more reference from a
           non-zero count and does not enter g_section; the compare-exchange
           reference is then dropped. Guard's critical section is recursive, and
           this path does not re-enter it. */
        if (owns_device_) {
            if (!device_) {
                *ppv = nullptr;
                return E_FAIL;
            }
            return device_->QueryInterface(riid, ppv);
        }
        BxDeviceProxy* proxy = nullptr;
        {
            Guard guard(g_section);
            proxy = device_;
            if (!proxy || !proxy->bx_try_acquire_if_nonzero()) {
                *ppv = nullptr;
                return E_FAIL;
            }
        }
        /* The compare-exchange reference keeps the proxy alive. QueryInterface
           adds the caller's reference from a non-zero count, then the
           compare-exchange reference is dropped. Release is outside g_section
           so a 1->0 transition cannot try to take that lock on this thread. */
        const HRESULT hr = proxy->QueryInterface(riid, ppv);
        proxy->Release();
        return hr;
    }
    D3D12_HEAP_DESC* STDMETHODCALLTYPE GetDesc(D3D12_HEAP_DESC* ret) override { return host_->GetDesc(ret); }
    HRESULT STDMETHODCALLTYPE GetProtectedResourceSession(REFIID riid, void** session) override {
        if (!session) {
            return E_POINTER;
        }
        *session = nullptr;
        if (!host1_) {
            return E_NOINTERFACE;
        }
        return host1_->GetProtectedResourceSession(riid, session);
    }

private:
    BxChargedHeap(ID3D12Heap* host, ID3D12Heap1* host1, uint64_t bytes, BxDeviceProxy* device, bool own_device)
        : host_(host), host1_(host1), bytes_(bytes), device_(device), owns_device_(own_device) {
        if (host_) {
            host_->AddRef();
        }
        if (host1_) {
            host1_->AddRef();
        }
        if (owns_device_ && device_) {
            device_->AddRef();
        }
    }
    ~BxChargedHeap() = default;

    ID3D12Heap* host_;
    ID3D12Heap1* host1_;
    uint64_t bytes_;
    BxDeviceProxy* device_;
    bool owns_device_; /* ordinary heap: strong, like BxChildProxy. Backing cache: weak. */
    LONG refs_ = 1;
};

struct RegionSnap {
    bool eligible = false;
    uint64_t base = 0;
    uint64_t size = 0;
    uint64_t generation = 0;
};

RegionSnap region_snap() {
    Guard guard(g_section);
    RegionSnap snap;
    snap.eligible = g_record.configured && g_record.backing_opt_in && g_record.have_region && g_record.config_size != 0;
    snap.base = g_record.region.base_va;
    snap.size = g_record.config_size;
    snap.generation = g_record.config_generation;
    return snap;
}

bool region_still_mapped(const RegionSnap& snap) {
    if (!snap.eligible || snap.size == 0) {
        return false;
    }
    bx_xmem_region_info_t live{};
    if (!bx_xmem_query_region(snap.base, &live) || live.base_va != snap.base || live.size_bytes == 0 ||
        snap.size % live.size_bytes != 0) {
        return false;
    }
    const uint64_t page = live.size_bytes;
    for (uint64_t offset = 0; offset < snap.size; offset += page) {
        bx_xmem_region_info_t part{};
        if (!bx_xmem_query_region(snap.base + offset, &part) || part.base_va != snap.base + offset ||
            part.size_bytes != page) {
            return false;
        }
    }
    return true;
}

/* True only for the explicit backing-heap request documented in vmem.h. */
bool backing_request(const D3D12_HEAP_DESC* desc, RegionSnap& snap) {
    if (!desc) {
        return false;
    }
    if (desc->Properties.Type != D3D12_HEAP_TYPE_CUSTOM ||
        desc->Properties.CPUPageProperty != D3D12_CPU_PAGE_PROPERTY_WRITE_BACK ||
        desc->Properties.MemoryPoolPreference != D3D12_MEMORY_POOL_L0) {
        return false;
    }
    if (desc->Alignment != 0 && desc->Alignment != k64K) {
        return false;
    }
    if (desc->Flags != D3D12_HEAP_FLAG_NONE) {
        return false;
    }
    if (desc->Properties.CreationNodeMask > 1 || desc->Properties.VisibleNodeMask > 1) {
        return false;
    }
    snap = region_snap();
    if (!snap.eligible || desc->SizeInBytes != snap.size) {
        return false;
    }
    return region_still_mapped(snap);
}

HRESULT host_private_heap(BxDeviceProxy* self, const D3D12_HEAP_DESC* desc, REFIID riid, void** out, bool own_device);

/* Test hook. One CreateHeap treats the cached host as foreign even when the
   runtime returns the same ID3D12Device, so the replacement path can be checked
   on a machine that keeps a single host device alive. */
bool force_backing_replace() {
    char buf[8] = {};
    return GetEnvironmentVariableA("BX_D3D12_FORCE_BACKING_REPLACE", buf, sizeof(buf)) > 0 && buf[0] == '1' &&
           buf[1] == '\0';
}

/* Caller holds g_section. */
bool cache_matches_locked(ID3D12Device* host, uint64_t bytes) {
    if (force_backing_replace()) {
        return false;
    }
    return g_record.backing && g_record.backing_host == host && g_record.configured && g_record.backing_opt_in &&
           g_record.have_region && g_record.config_size == bytes;
}

/* Caller holds g_section. A heap that leaves the cache loses its weak proxy
   pointer before the cache drops it, so a caller who still holds the heap
   gets E_FAIL from GetDevice instead of a freed proxy. */
void take_cached_heap_locked(BxChargedHeap** heap, ID3D12Resource** buffer) {
    *heap = g_record.backing;
    *buffer = g_record.backing_buffer;
    if (*heap) {
        (*heap)->set_device(nullptr);
    }
    g_record.backing = nullptr;
    g_record.backing_buffer = nullptr;
    g_record.backing_host = nullptr;
    g_record.cpu_pointer = 0;
}

/* Caller holds g_section. Host and size were just checked. Retarget and take
   the caller's reference before the lock is released. */
HRESULT hand_out_backing_locked(BxDeviceProxy* self, REFIID riid, void** out) {
    g_record.backing->set_device(self);
    g_record.backing->AddRef();
    const HRESULT hr = g_record.backing->QueryInterface(riid, out);
    g_record.backing->Release();
    return hr;
}

void release_backing_pair(BxChargedHeap* heap, ID3D12Resource* buffer) {
    if (buffer) {
        buffer->Unmap(0, nullptr);
        buffer->Release();
    }
    if (heap) {
        heap->Release();
    }
}

HRESULT ensure_backing(BxDeviceProxy* self, const RegionSnap& snap, REFIID riid, void** out) {
    const uint64_t bytes = snap.size;
    BxChargedHeap* stale_heap = nullptr;
    ID3D12Resource* stale_buffer = nullptr;
    {
        /* One section covers the host and config check, the heap choice, and the
           reference handed to the caller. A second device cannot swap the cache
           in the middle of that. */
        Guard guard(g_section);
        if (snap.generation != g_record.config_generation) {
            return HRESULT_FROM_WIN32(ERROR_RETRY);
        }
        if (cache_matches_locked(self->h_ID3D12Device, bytes)) {
            return hand_out_backing_locked(self, riid, out);
        }
        if (g_record.backing) {
            take_cached_heap_locked(&stale_heap, &stale_buffer);
        }
    }
    release_backing_pair(stale_heap, stale_buffer);
    stale_heap = nullptr;
    stale_buffer = nullptr;

    D3D12_HEAP_DESC desc = {};
    desc.SizeInBytes = bytes;
    desc.Alignment = k64K;
    desc.Properties.Type = D3D12_HEAP_TYPE_CUSTOM;
    desc.Properties.CPUPageProperty = D3D12_CPU_PAGE_PROPERTY_WRITE_BACK;
    desc.Properties.MemoryPoolPreference = D3D12_MEMORY_POOL_L0;
    desc.Flags = D3D12_HEAP_FLAG_NONE;
    void* created = nullptr;
    HRESULT hr = host_private_heap(self, &desc, __uuidof(ID3D12Heap), &created, false);
    if (FAILED(hr) || !created) {
        return FAILED(hr) ? hr : E_FAIL;
    }
    auto* wrap = static_cast<BxChargedHeap*>(created);
    ID3D12Heap* host_heap = bx_unwrap(wrap);

    D3D12_RESOURCE_DESC buffer = {};
    buffer.Dimension = D3D12_RESOURCE_DIMENSION_BUFFER;
    buffer.Width = bytes;
    buffer.Height = 1;
    buffer.DepthOrArraySize = 1;
    buffer.MipLevels = 1;
    buffer.Format = DXGI_FORMAT_UNKNOWN;
    buffer.SampleDesc.Count = 1;
    buffer.Layout = D3D12_TEXTURE_LAYOUT_ROW_MAJOR;
    ID3D12Resource* resource = nullptr;
    hr = self->h_ID3D12Device->CreatePlacedResource(host_heap, 0, &buffer, D3D12_RESOURCE_STATE_GENERIC_READ, nullptr,
                                                    __uuidof(ID3D12Resource), reinterpret_cast<void**>(&resource));
    if (FAILED(hr) || !resource) {
        wrap->Release();
        return FAILED(hr) ? hr : E_FAIL;
    }
    void* mapped = nullptr;
    hr = resource->Map(0, nullptr, &mapped);
    if (FAILED(hr) || !mapped) {
        resource->Release();
        wrap->Release();
        return FAILED(hr) ? hr : E_FAIL;
    }

    HRESULT published = E_FAIL;
    bool keep_new = false;
    {
        /* The host allocation may have raced with a config change or another
           creator. Publish only for the configuration this request validated. */
        Guard guard(g_section);
        if (snap.generation != g_record.config_generation) {
            published = HRESULT_FROM_WIN32(ERROR_RETRY);
        } else if (cache_matches_locked(self->h_ID3D12Device, bytes)) {
            published = hand_out_backing_locked(self, riid, out);
        } else {
            if (g_record.backing) {
                take_cached_heap_locked(&stale_heap, &stale_buffer);
            }
            keep_new = true;
            g_record.backing = wrap;
            g_record.backing_buffer = resource;
            g_record.backing_host = self->h_ID3D12Device;
            /* Remember the map. The caller's config block is not stored and is not written here. */
            g_record.cpu_pointer = reinterpret_cast<uint64_t>(mapped);
            published = hand_out_backing_locked(self, riid, out);
        }
    }
    if (!keep_new) {
        release_backing_pair(wrap, resource);
    }
    release_backing_pair(stale_heap, stale_buffer);
    return published;
}

HRESULT host_private_heap(BxDeviceProxy* self, const D3D12_HEAP_DESC* desc, REFIID riid, void** out, bool own_device) {
    /* desc is forwarded unmodified. The charge follows the wrapper, including ID3D12Heap1. */
    if (!try_charge(desc->SizeInBytes)) {
        bx_log_emit(BX_LOG_WARN, "d3d12_xs", "heap_budget",
                    "CreateHeap exceeds the Lockhart/Anaconda xmem title budget");
        return E_OUTOFMEMORY;
    }
    void* raw = nullptr;
    HRESULT hr = self->h_ID3D12Device->CreateHeap(desc, riid, &raw);
    if (FAILED(hr) || !raw) {
        release_charge(desc->SizeInBytes);
        return FAILED(hr) ? hr : E_FAIL;
    }
    auto* unknown = static_cast<IUnknown*>(raw);
    ID3D12Heap* host = nullptr;
    const HRESULT heap_hr = unknown->QueryInterface(__uuidof(ID3D12Heap), reinterpret_cast<void**>(&host));
    if (FAILED(heap_hr) || !host) {
        unknown->Release();
        release_charge(desc->SizeInBytes);
        return FAILED(heap_hr) ? heap_hr : E_NOINTERFACE;
    }
    ID3D12Heap1* host1 = nullptr;
    void* probed = nullptr;
    if (host->QueryInterface(__uuidof(ID3D12Heap1), &probed) == S_OK && probed) {
        host1 = static_cast<ID3D12Heap1*>(probed);
    }
    BxChargedHeap* wrap = BxChargedHeap::create(host, host1, desc->SizeInBytes, self, own_device);
    if (host1) {
        host1->Release();
    }
    host->Release();
    unknown->Release();
    if (!wrap) {
        release_charge(desc->SizeInBytes);
        return E_OUTOFMEMORY;
    }
    hr = wrap->QueryInterface(riid, out);
    wrap->Release();
    if (FAILED(hr) && out) {
        *out = nullptr;
    }
    return hr;
}

bool is_backing_heap(ID3D12Heap* heap) {
    if (!heap) {
        return false;
    }
    BxChargedHeap* backing = nullptr;
    {
        Guard guard(g_section);
        backing = g_record.backing;
        if (!backing) {
            return false;
        }
        backing->AddRef();
    }
    IUnknown* want = nullptr;
    IUnknown* got = nullptr;
    const HRESULT want_hr = backing->QueryInterface(IID_IUnknown, reinterpret_cast<void**>(&want));
    const HRESULT got_hr = heap->QueryInterface(IID_IUnknown, reinterpret_cast<void**>(&got));
    const bool same = SUCCEEDED(want_hr) && SUCCEEDED(got_hr) && want == got;
    if (want) {
        want->Release();
    }
    if (got) {
        got->Release();
    }
    backing->Release();
    return same;
}

void log_placed(UINT64 offset, HRESULT hr) {
    char detail[160];
    std::snprintf(detail, sizeof(detail), "CreatePlacedResource offset=%llu hr=0x%08lx",
                  static_cast<unsigned long long>(offset), static_cast<unsigned long>(hr));
    bx_log_emit(FAILED(hr) ? BX_LOG_WARN : BX_LOG_INFO, "d3d12_xs", "placed_resource", detail);
}

HRESULT STDMETHODCALLTYPE create_heap(BxDeviceProxy* self, const D3D12_HEAP_DESC* desc, REFIID riid, void** out) {
    BX_D3D_TRACE("ID3D12Device", "CreateHeap");
    if (!self || !self->h_ID3D12Device) {
        if (out) {
            *out = nullptr;
        }
        BX_VTABLE_STUB_LOG("ID3D12Device", 28u, "CreateHeap");
        return E_NOTIMPL;
    }
    if (!desc) {
        if (out) {
            *out = nullptr;
        }
        return E_INVALIDARG;
    }
    if (!out) {
        /* A null out pointer is forwarded unchanged to the host.
         * vkd3d on CrossOver 26.2 faults on that pointer with or without this shim.
         */
        return self->h_ID3D12Device->CreateHeap(desc, riid, nullptr);
    }
    *out = nullptr;
    RegionSnap snap;
    if (backing_request(desc, snap)) {
        return ensure_backing(self, snap, riid, out);
    }
    return host_private_heap(self, desc, riid, out, true);
}

HRESULT validate_backing_placement(ID3D12Device* device, ID3D12Heap* heap, UINT64 offset, const D3D12_RESOURCE_DESC* desc,
                                   D3D12_RESOURCE_STATES state, const D3D12_CLEAR_VALUE* clear) {
    if (!heap || !desc) {
        return E_INVALIDARG;
    }
    if (desc->Dimension == D3D12_RESOURCE_DIMENSION_BUFFER && clear) {
        return E_INVALIDARG;
    }
    D3D12_HEAP_DESC heap_desc = heap->GetDesc();
    if (heap_desc.Properties.Type == D3D12_HEAP_TYPE_UPLOAD && state != D3D12_RESOURCE_STATE_GENERIC_READ) {
        return E_INVALIDARG;
    }
    if (heap_desc.Properties.Type == D3D12_HEAP_TYPE_READBACK && state != D3D12_RESOURCE_STATE_COPY_DEST) {
        return E_INVALIDARG;
    }
    D3D12_RESOURCE_ALLOCATION_INFO info = device->GetResourceAllocationInfo(0, 1, desc);
    if (info.SizeInBytes == 0 || info.SizeInBytes == ~0ull) {
        return E_INVALIDARG;
    }
    if (info.Alignment != 0 && (offset % info.Alignment) != 0) {
        return E_INVALIDARG;
    }
    if (offset > heap_desc.SizeInBytes || info.SizeInBytes > heap_desc.SizeInBytes - offset) {
        return E_INVALIDARG;
    }
    return S_OK;
}

HRESULT STDMETHODCALLTYPE create_placed(BxDeviceProxy* self, ID3D12Heap* heap, UINT64 offset,
                                       const D3D12_RESOURCE_DESC* desc, D3D12_RESOURCE_STATES state,
                                       const D3D12_CLEAR_VALUE* clear, REFIID riid, void** out) {
    BX_D3D_TRACE("ID3D12Device", "CreatePlacedResource");
    if (!self || !self->h_ID3D12Device) {
        if (out) {
            *out = nullptr;
        }
        BX_VTABLE_STUB_LOG("ID3D12Device", 29u, "CreatePlacedResource");
        return E_NOTIMPL;
    }
    if (out) {
        *out = nullptr;
    }
    if (is_backing_heap(heap)) {
        const HRESULT check_hr = validate_backing_placement(self->h_ID3D12Device, heap, offset, desc, state, clear);
        if (FAILED(check_hr)) {
            log_placed(offset, check_hr);
            return check_hr;
        }
    }
    ID3D12Heap* host_heap = bx_unwrap(heap);
    HRESULT hr = self->h_ID3D12Device->CreatePlacedResource(host_heap, offset, desc, state, clear, riid, out);
    if (SUCCEEDED(hr) && out && *out && host_heap != heap) {
        /* The resource owns a reference to the shim heap through its host
           object's private data, including across QueryInterface calls. */
        auto* created = static_cast<IUnknown*>(*out);
        ID3D12Object* object = nullptr;
        HRESULT retain_hr = created->QueryInterface(__uuidof(ID3D12Object), reinterpret_cast<void**>(&object));
        if (SUCCEEDED(retain_hr)) {
            retain_hr = object->SetPrivateDataInterface(kPlacedResourceHeap, heap);
            object->Release();
        }
        if (FAILED(retain_hr)) {
            created->Release();
            *out = nullptr;
            hr = retain_hr;
        }
    }
    log_placed(offset, hr);
    return hr;
}

struct ConfigHeader {
    uint32_t magic;
    uint32_t struct_size;
};
static_assert(sizeof(ConfigHeader) == 8, "config probe is the 8-byte header");

void write_cpu_pointer(void* config, uint64_t pointer) {
    std::memcpy(static_cast<char*>(config) + offsetof(BxD3d12VirtualMemoryConfig, cpu_pointer), &pointer, sizeof(pointer));
}

} // namespace

extern "C" HRESULT bx_d3d12_configure_virtual_memory(void* config) {
    if (!config) {
        /* Titles call this before reserving memory and require S_OK. */
        BX_STUB_LOG("d3d12_xs.dll", "D3D12ConfigureVirtualMemoryX");
        return S_OK;
    }
    /*
     * PVOID has no size. Read 8 bytes, and only then the rest of a block that
     * names this struct. A mismatch is left untouched.
     */
    ConfigHeader header{};
    std::memcpy(&header, config, sizeof(header));
    if (header.magic != BX_D3D12_VMEM_MAGIC || header.struct_size != sizeof(BxD3d12VirtualMemoryConfig)) {
        BX_STUB_LOG("d3d12_xs.dll", "D3D12ConfigureVirtualMemoryX");
        return S_OK;
    }

    BxD3d12VirtualMemoryConfig in{};
    std::memcpy(&in, config, sizeof(in));
    const uint32_t unknown_flags = in.flags & ~BX_D3D12_VMEM_FLAG_BACKING_HEAP;
    if (in.version != kConfigVersion || unknown_flags != 0 || in.reserved != 0) {
        /* A console parameter block is not this struct. Leave those bytes alone. */
        BX_STUB_LOG("d3d12_xs.dll", "D3D12ConfigureVirtualMemoryX");
        return S_OK;
    }

    const bx_runtime_config_t* cfg = bx_get_runtime_config();
    const uint64_t cap = title_limit();
    const auto profile = (cfg && cfg->profile == BX_PROFILE_ANACONDA) ? BX_PROFILE_ANACONDA : BX_PROFILE_LOCKHART;
    if (in.size_bytes != 0 && (in.size_bytes % k64K != 0 || in.size_bytes > cap)) {
        return E_INVALIDARG;
    }

    bx_xmem_stats_t stats{};
    bx_xmem_get_internal_stats(&stats);
    if (in.size_bytes != 0 && stats.title_limit_bytes != 0 && in.size_bytes > stats.title_limit_bytes) {
        return E_INVALIDARG;
    }

    bx_xmem_region_info_t region{};
    bool have_region = false;
    if (in.base_va != 0) {
        bx_xmem_region_info_t first{};
        if (!bx_xmem_query_region(in.base_va, &first) || first.size_bytes == 0 || first.base_va != in.base_va ||
            first.base_va % k64K != 0 || first.size_bytes % k64K != 0) {
            return E_INVALIDARG;
        }
        const uint64_t span = in.size_bytes != 0 ? in.size_bytes : first.size_bytes;
        if (span < first.size_bytes || span % first.size_bytes != 0) {
            return E_INVALIDARG;
        }
        for (uint64_t offset = 0; offset < span; offset += first.size_bytes) {
            bx_xmem_region_info_t page{};
            if (!bx_xmem_query_region(first.base_va + offset, &page) || page.base_va != first.base_va + offset ||
                page.size_bytes != first.size_bytes) {
                return E_INVALIDARG;
            }
        }
        region = first;
        region.size_bytes = span;
        have_region = true;
    }

    const uint64_t new_base = have_region ? region.base_va : in.base_va;
    const uint64_t new_size = have_region ? region.size_bytes : in.size_bytes;
    const bool backing_opt_in = (in.flags & BX_D3D12_VMEM_FLAG_BACKING_HEAP) != 0;
    BxChargedHeap* old_heap = nullptr;
    ID3D12Resource* old_buffer = nullptr;
    uint64_t published = 0;
    {
        Guard guard(g_section);
        const bool same = g_record.configured && g_record.config_base == new_base && g_record.config_size == new_size;
        if (!same) {
            take_cached_heap_locked(&old_heap, &old_buffer);
        }
        if (!same || g_record.backing_opt_in != backing_opt_in || g_record.have_region != have_region) {
            ++g_record.config_generation;
        }
        g_record.configured = true;
        g_record.backing_opt_in = backing_opt_in;
        g_record.profile = profile;
        g_record.title_limit = cap;
        g_record.have_region = have_region;
        g_record.config_base = new_base;
        g_record.config_size = new_size;
        g_record.region = have_region ? region : bx_xmem_region_info_t{};
        published = g_record.cpu_pointer;
    }
    /* The only write to caller memory, and it happens before this call returns. */
    write_cpu_pointer(config, published);
    if (old_buffer) {
        old_buffer->Unmap(0, nullptr);
        old_buffer->Release();
    }
    if (old_heap) {
        old_heap->Release();
    }
    uint64_t charged = 0;
    {
        Guard guard(g_section);
        charged = g_record.charged_private_bytes;
    }

    char detail[384];
    std::snprintf(detail, sizeof(detail),
                  "profile=%s title_limit=%llu xmem_used=%llu xmem_region=%d base=0x%llx region_bytes=%llu gpu_heap_bytes=%llu",
                  profile_name(profile), static_cast<unsigned long long>(cap),
                  static_cast<unsigned long long>(stats.committed_virtual_bytes + stats.allocated_physical_bytes),
                  have_region ? 1 : 0, static_cast<unsigned long long>(have_region ? region.base_va : 0),
                  static_cast<unsigned long long>(have_region ? region.size_bytes : 0),
                  static_cast<unsigned long long>(charged));
    bx_log_emit(BX_LOG_INFO, "d3d12_xs", "D3D12ConfigureVirtualMemoryX", detail);
    return S_OK;
}

bool bx_vmem_final_release(void* device_proxy) {
    Guard guard(g_section);
    auto* proxy = static_cast<BxDeviceProxy*>(device_proxy);
    if (!proxy->bx_try_release_last()) {
        return false;
    }
    /* The only weak pointer is the cached backing heap. Heaps that already left
       the cache were cleared in take_cached_heap_locked. */
    if (g_record.backing && g_record.backing->device() == proxy) {
        g_record.backing->set_device(nullptr);
    }
    return true;
}

void* bx_vmem_addref_backing_host() {
    Guard guard(g_section);
    if (!g_record.backing_host) {
        return nullptr;
    }
    g_record.backing_host->AddRef();
    return g_record.backing_host;
}

bool bx_vmem_install_device_hooks(void* device) {
    /*
     * CreateHeap is slot 28 and CreatePlacedResource is slot 29 on ID3D12Device.
     * BxDeviceProxy inherits both methods from the generated forwarders.
     * The proxy vtable is shared static data: every BxDeviceProxy instance points at
     * the same function table. Patching it once covers every device this DLL creates.
     * That table is in a read-only section, so VirtualProtect opens the two slots for
     * the store and then puts the previous protection back.
     * No other hook in this DLL replaces those methods without editing the proxy sources.
     */
    /* No caller may publish a proxy until both shared slots are ready. */
    static bool installed = false;
    Guard guard(g_section);
    if (installed) {
        return true;
    }
    auto* proxy = static_cast<BxDeviceProxy*>(device);
    void** vtable = *reinterpret_cast<void***>(proxy);
    DWORD old = 0;
    if (!VirtualProtect(vtable + 28, sizeof(void*) * 2, PAGE_READWRITE, &old)) {
        bx_log_emit(BX_LOG_ERROR, "d3d12_xs", "vmem_hook", "VirtualProtect on the device vtable failed");
        BX_VTABLE_STUB_LOG("ID3D12Device", 28u, "CreateHeap");
        return false;
    }
    vtable[28] = reinterpret_cast<void*>(&create_heap);
    vtable[29] = reinterpret_cast<void*>(&create_placed);
    DWORD ignored = 0;
    VirtualProtect(vtable + 28, sizeof(void*) * 2, old, &ignored);
    installed = true;
    bx_log_emit(BX_LOG_INFO, "d3d12_xs", "vmem_hook", "CreateHeap and CreatePlacedResource installed");
    return true;
}

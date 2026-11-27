/*
 * M1-C1 (#414): clean-room synthetic conformance test for d3d12_xs.dll.
 *
 * Creates a device through D3D12XboxCreateDevice and drives the proxied device, command queue,
 * command allocator and command list through the publicly documented D3D12 contract: creation,
 * COM identity, an (empty) command list executed and confirmed by a fence, allocator/list reset,
 * and an empty root signature serialized, created and deserialized.
 *
 * Expected values come from the public Direct3D 12 API contract, not from the shim's source.
 * "Is this object the shim's proxy?" is answered by checking which module owns its vtable.
 */
#ifndef WIDL_EXPLICIT_AGGREGATE_RETURNS
#define WIDL_EXPLICIT_AGGREGATE_RETURNS /* call struct-returning COM methods with the real x64 ABI */
#endif
#include <windows.h>
#include <unknwn.h>
#include <d3d12.h>

#include <cstdarg>
#include <cstdint>
#include <cstdio>
#include <cstring>

#include "bx_log.h"

extern "C" __declspec(dllimport) HRESULT WINAPI D3D12XboxCreateDevice(PVOID params, REFIID riid, void** ppv);

namespace {

int g_checks = 0;
int g_failures = 0;
int g_skipped = 0;

__attribute__((format(gnu_printf, 2, 3)))
bool check(bool ok, const char* fmt, ...) {
    g_checks++;
    if (!ok) {
        g_failures++;
        char msg[256];
        va_list ap;
        va_start(ap, fmt);
        std::vsnprintf(msg, sizeof(msg), fmt, ap);
        va_end(ap);
        std::fprintf(stderr, "FAIL: %s\n", msg);
        std::fflush(stderr);
    }
    return ok;
}

void skip(const char* what) {
    g_skipped++;
    std::printf("SKIP: %s\n", what);
}

unsigned long ul(HRESULT hr) { return static_cast<unsigned long>(hr); }

/* True when the object's vtable lives inside d3d12_xs.dll, i.e. the caller holds the shim's proxy. */
bool vtable_in_shim(IUnknown* obj) {
    HMODULE owner = nullptr;
    void* vtable = *reinterpret_cast<void**>(obj);
    if (!GetModuleHandleExA(GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
                            static_cast<LPCSTR>(vtable), &owner)) {
        return false;
    }
    return owner != nullptr && owner == GetModuleHandleA("d3d12_xs.dll");
}

/* An IID that no interface uses (generated for this test). */
const GUID kUnusedIid = {0x4f0c2f6b, 0x91d3, 0x4d7e, {0xb2, 0x1a, 0x7e, 0x55, 0x03, 0xc9, 0x6a, 0x18}};

/*
 * #547: the generator (shims/d3d12_xs/tools/gen_proxy.py, PAD_SLOTS = 32) appends
 * 32 `BxBeyondPublicVtableNN` stubs after each generated proxy's public vtable so
 * that a caller indexing past the interface it was handed lands on a logged
 * E_NOTIMPL stub. The first padded slot sits at index `kPublicSlotCount`
 * (gen_proxy.py emits `slot + k` for k in [0, 32)); the last is
 * `kPublicSlotCount + 31`. The expected indices below are derived from the
 * public D3D12 interface inheritance chains (IUnknown + every level up to the
 * leaf), which is exactly what the generator walks; they match the
 * `kPublicSlotCount` constants in proxy_generated.h.
 *
 * Each stub is `HRESULT STDMETHODCALLTYPE fn()` with no arguments, so it is
 * callable through the vtable as `HRESULT (WINAPI*)()`. The stub logs
 * BX_VTABLE_STUB_LOG("<leaf>", <slot>u, "beyond_public_vtable"), which
 * bx_log_unimpl_vtable writes as
 *   {"sub":"com_vtable","event":"unimpl_vtable","detail":"<leaf>[<slot>]::beyond_public_vtable"}
 */
struct PaddedVtableExpectation {
    const char* what;    /* fixture-facing name for messages */
    const char* leaf;    /* generator's leaf interface name, used in the log detail */
    unsigned first_slot; /* index of BxBeyondPublicVtable00 */
};

/* PAD_SLOTS in gen_proxy.py. */
constexpr unsigned kPaddedSlotCount = 32;

HRESULT call_vtable_slot(IUnknown* obj, unsigned index) {
    void** vtbl = *reinterpret_cast<void***>(obj);
    auto fn = reinterpret_cast<HRESULT(WINAPI*)()>(vtbl[index]);
    return fn();
}

bool file_contains(const char* path, const char* needle_a, const char* needle_b) {
    HANDLE f = CreateFileA(path, GENERIC_READ, FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE, nullptr,
                           OPEN_EXISTING, FILE_ATTRIBUTE_NORMAL, nullptr);
    if (f == INVALID_HANDLE_VALUE) {
        return false;
    }
    bool found = false;
    LARGE_INTEGER size = {};
    if (GetFileSizeEx(f, &size) && size.QuadPart > 0 && size.QuadPart < (64ll << 20)) {
        auto* buf = static_cast<char*>(HeapAlloc(GetProcessHeap(), 0, static_cast<SIZE_T>(size.QuadPart) + 1));
        DWORD got = 0;
        if (buf && ReadFile(f, buf, static_cast<DWORD>(size.QuadPart), &got, nullptr)) {
            buf[got] = '\0';
            /* Both needles must sit on the same JSONL line. */
            for (char* line = buf; line && *line && !found;) {
                char* end = std::strchr(line, '\n');
                if (end) *end = '\0';
                found = std::strstr(line, needle_a) && std::strstr(line, needle_b);
                line = end ? end + 1 : nullptr;
            }
        }
        if (buf) HeapFree(GetProcessHeap(), 0, buf);
    }
    CloseHandle(f);
    return found;
}

} // namespace

int main() {
    SetErrorMode(SEM_FAILCRITICALERRORS | SEM_NOGPFAULTERRORBOX);

    /* --- 1. Argument contract ------------------------------------------------------------------ */
    check(D3D12XboxCreateDevice(nullptr, __uuidof(ID3D12Device), nullptr) == E_POINTER,
          "D3D12XboxCreateDevice(ppv=null) must return E_POINTER");
    void* bogus = reinterpret_cast<void*>(static_cast<uintptr_t>(0xDEAD));
    HRESULT hr = D3D12XboxCreateDevice(nullptr, kUnusedIid, &bogus);
    check(hr == E_NOINTERFACE && bogus == nullptr,
          "unknown riid must yield E_NOINTERFACE and a null object (hr=0x%08lx obj=%p)", ul(hr), bogus);

    /* --- 2. Device creation and identity ------------------------------------------------------- */
    ID3D12Device* device = nullptr;
    hr = D3D12XboxCreateDevice(nullptr, __uuidof(ID3D12Device), reinterpret_cast<void**>(&device));
    if (!check(hr == S_OK && device, "D3D12XboxCreateDevice hr=0x%08lx device=%p", ul(hr), static_cast<void*>(device))) {
        std::fprintf(stderr, "FAIL: test_d3d12_xs (no device; %d of %d checks failed)\n", g_failures, g_checks);
        return 1;
    }
    check(vtable_in_shim(device), "device must be the d3d12_xs proxy (vtable outside d3d12_xs.dll)");

    IUnknown* unk_a = nullptr;
    IUnknown* unk_b = nullptr;
    ID3D12Object* as_object = nullptr;
    check(device->QueryInterface(__uuidof(IUnknown), reinterpret_cast<void**>(&unk_a)) == S_OK &&
          device->QueryInterface(__uuidof(IUnknown), reinterpret_cast<void**>(&unk_b)) == S_OK && unk_a && unk_a == unk_b,
          "QueryInterface(IUnknown) must return one stable identity");
    check(device->QueryInterface(__uuidof(ID3D12Object), reinterpret_cast<void**>(&as_object)) == S_OK && as_object,
          "device must answer ID3D12Object");
    void* none = reinterpret_cast<void*>(static_cast<uintptr_t>(0xDEAD));
    hr = device->QueryInterface(kUnusedIid, &none);
    check(hr == E_NOINTERFACE && none == nullptr, "device QI(unused IID) hr=0x%08lx obj=%p", ul(hr), none);
    if (unk_a) unk_a->Release();
    if (unk_b) unk_b->Release();
    if (as_object) as_object->Release();

    check(device->GetNodeCount() >= 1, "GetNodeCount must be at least 1");
    LUID luid_a = device->GetAdapterLuid();
    LUID luid_b = device->GetAdapterLuid();
    check(luid_a.LowPart == luid_b.LowPart && luid_a.HighPart == luid_b.HighPart, "GetAdapterLuid must be stable");

    /* --- 3. Command queue ---------------------------------------------------------------------- */
    D3D12_COMMAND_QUEUE_DESC queue_desc = {};
    queue_desc.Type = D3D12_COMMAND_LIST_TYPE_DIRECT;
    ID3D12CommandQueue* queue = nullptr;
    hr = device->CreateCommandQueue(&queue_desc, __uuidof(ID3D12CommandQueue), reinterpret_cast<void**>(&queue));
    check(hr == S_OK && queue, "CreateCommandQueue hr=0x%08lx", ul(hr));
    if (queue) {
        check(vtable_in_shim(queue), "command queue must be the d3d12_xs proxy");
        D3D12_COMMAND_QUEUE_DESC got = queue->GetDesc();
        check(got.Type == D3D12_COMMAND_LIST_TYPE_DIRECT, "queue GetDesc().Type=%d", static_cast<int>(got.Type));
        ID3D12Device* parent = nullptr;
        hr = queue->GetDevice(__uuidof(ID3D12Device), reinterpret_cast<void**>(&parent));
        check(hr == S_OK && parent == device, "queue GetDevice must return the same proxy device");
        if (parent) parent->Release();
    }

    /* --- 4. Command allocator and list --------------------------------------------------------- */
    ID3D12CommandAllocator* allocator = nullptr;
    hr = device->CreateCommandAllocator(D3D12_COMMAND_LIST_TYPE_DIRECT, __uuidof(ID3D12CommandAllocator),
                                        reinterpret_cast<void**>(&allocator));
    check(hr == S_OK && allocator, "CreateCommandAllocator hr=0x%08lx", ul(hr));
    if (allocator) check(vtable_in_shim(allocator), "command allocator must be the d3d12_xs proxy");

    ID3D12GraphicsCommandList* list = nullptr;
    if (allocator) {
        /* The allocator handed back in is the proxy: the shim has to translate it for the host. */
        hr = device->CreateCommandList(0, D3D12_COMMAND_LIST_TYPE_DIRECT, allocator, nullptr,
                                       __uuidof(ID3D12GraphicsCommandList), reinterpret_cast<void**>(&list));
        check(hr == S_OK && list, "CreateCommandList hr=0x%08lx", ul(hr));
    }
    if (list) {
        check(vtable_in_shim(list), "command list must be the d3d12_xs proxy");
        check(list->GetType() == D3D12_COMMAND_LIST_TYPE_DIRECT, "list GetType=%d", static_cast<int>(list->GetType()));
        ID3D12Device* parent = nullptr;
        hr = list->GetDevice(__uuidof(ID3D12Device), reinterpret_cast<void**>(&parent));
        check(hr == S_OK && parent == device, "list GetDevice must return the same proxy device");
        if (parent) parent->Release();
        ID3D12CommandList* base = nullptr;
        check(list->QueryInterface(__uuidof(ID3D12CommandList), reinterpret_cast<void**>(&base)) == S_OK && base,
              "list must answer ID3D12CommandList");
        if (base) base->Release();
    }

    /* --- 5. Execute the (empty) list and confirm with a fence ---------------------------------- */
    ID3D12Fence* fence = nullptr;
    hr = device->CreateFence(0, D3D12_FENCE_FLAG_NONE, __uuidof(ID3D12Fence), reinterpret_cast<void**>(&fence));
    check(hr == S_OK && fence, "CreateFence hr=0x%08lx", ul(hr));
    if (queue && list && fence) {
        hr = list->Close();
        check(hr == S_OK, "list Close hr=0x%08lx", ul(hr));
        ID3D12CommandList* const lists[] = {list};
        queue->ExecuteCommandLists(1, lists);
        hr = queue->Signal(fence, 1);
        check(hr == S_OK, "queue Signal hr=0x%08lx", ul(hr));
        HANDLE done = CreateEventA(nullptr, FALSE, FALSE, nullptr);
        hr = fence->SetEventOnCompletion(1, done);
        check(hr == S_OK, "fence SetEventOnCompletion hr=0x%08lx", ul(hr));
        DWORD waited = WaitForSingleObject(done, 10000);
        check(waited == WAIT_OBJECT_0 && fence->GetCompletedValue() >= 1,
              "fence must reach 1 after ExecuteCommandLists+Signal (wait=%lu completed=%llu)", waited,
              static_cast<unsigned long long>(fence->GetCompletedValue()));
        CloseHandle(done);

        /* --- 6. Reset allocator and list (proxy allocator passed back again) ------------------- */
        hr = allocator->Reset();
        check(hr == S_OK, "allocator Reset hr=0x%08lx", ul(hr));
        hr = list->Reset(allocator, nullptr);
        check(hr == S_OK, "list Reset hr=0x%08lx", ul(hr));
        hr = list->Close();
        check(hr == S_OK, "list Close after Reset hr=0x%08lx", ul(hr));
    }

    /* --- 7. Optional newer interface: CreateCommandList1 (ID3D12Device4) ------------------------ */
    ID3D12Device4* device4 = nullptr;
    hr = device->QueryInterface(__uuidof(ID3D12Device4), reinterpret_cast<void**>(&device4));
    if (hr == S_OK && device4) {
        check(static_cast<ID3D12Device*>(device4) == device, "ID3D12Device4 must be the same proxy object");
        ID3D12GraphicsCommandList* closed_list = nullptr;
        hr = device4->CreateCommandList1(0, D3D12_COMMAND_LIST_TYPE_DIRECT, D3D12_COMMAND_LIST_FLAG_NONE,
                                         __uuidof(ID3D12GraphicsCommandList), reinterpret_cast<void**>(&closed_list));
        check(hr == S_OK && closed_list && vtable_in_shim(closed_list), "CreateCommandList1 hr=0x%08lx", ul(hr));
        if (closed_list) closed_list->Release();
        device4->Release();
    } else {
        check(hr == E_NOINTERFACE, "QI(ID3D12Device4) must be S_OK or E_NOINTERFACE (hr=0x%08lx)", ul(hr));
        skip("host device does not implement ID3D12Device4; CreateCommandList1 not exercised");
    }

    /* --- 8. Empty root signature: serialize, create, deserialize -------------------------------- */
    D3D12_VERSIONED_ROOT_SIGNATURE_DESC rs_desc = {};
    rs_desc.Version = D3D_ROOT_SIGNATURE_VERSION_1_0;
    ID3DBlob* rs_blob = nullptr;
    ID3DBlob* rs_error = nullptr;
    hr = D3D12SerializeVersionedRootSignature(&rs_desc, &rs_blob, &rs_error);
    check(hr == S_OK && rs_blob && rs_blob->GetBufferSize() >= 4 &&
          std::memcmp(rs_blob->GetBufferPointer(), "DXBC", 4) == 0,
          "D3D12SerializeVersionedRootSignature hr=0x%08lx", ul(hr));
    ID3D12RootSignature* root_signature = nullptr;
    if (rs_blob) {
        hr = device->CreateRootSignature(0, rs_blob->GetBufferPointer(), rs_blob->GetBufferSize(),
                                         __uuidof(ID3D12RootSignature), reinterpret_cast<void**>(&root_signature));
        check(hr == S_OK && root_signature, "CreateRootSignature hr=0x%08lx", ul(hr));
        ID3D12RootSignatureDeserializer* deserializer = nullptr;
        hr = D3D12CreateRootSignatureDeserializer(rs_blob->GetBufferPointer(), rs_blob->GetBufferSize(),
                                                  __uuidof(ID3D12RootSignatureDeserializer),
                                                  reinterpret_cast<void**>(&deserializer));
        check(hr == S_OK && deserializer, "D3D12CreateRootSignatureDeserializer hr=0x%08lx", ul(hr));
        if (deserializer) {
            const D3D12_ROOT_SIGNATURE_DESC* round_trip = deserializer->GetRootSignatureDesc();
            check(round_trip && round_trip->NumParameters == 0 && round_trip->NumStaticSamplers == 0,
                  "deserialized root signature must be empty");
            deserializer->Release();
        }
    }

    /* --- 9. Pipeline-state interception --------------------------------------------------------- */
    /*
     * A syntactically valid DXBC container that holds one chunk with a per-run FourCC and no shader.
     * The host must refuse it; the shim must forward that refusal and log the container's metadata.
     */
    char fourcc[5] = "B___";
    DWORD seed = GetTickCount() ^ (GetCurrentProcessId() * 2654435761u);
    for (int i = 1; i < 4; ++i) {
        fourcc[i] = static_cast<char>('A' + (seed % 26u));
        seed /= 26u;
    }
    uint8_t container[32 + 4 + 8 + 16] = {};
    const uint32_t total = sizeof(container);
    const uint32_t one = 1;
    const uint32_t chunk_offset = 36;
    const uint32_t chunk_size = 16;
    std::memcpy(container, "DXBC", 4);
    std::memcpy(container + 20, &one, 4);          /* version */
    std::memcpy(container + 24, &total, 4);        /* total size */
    std::memcpy(container + 28, &one, 4);          /* chunk count */
    std::memcpy(container + 32, &chunk_offset, 4); /* chunk offset table */
    std::memcpy(container + 36, fourcc, 4);
    std::memcpy(container + 40, &chunk_size, 4);

    D3D12_COMPUTE_PIPELINE_STATE_DESC pso_desc = {};
    pso_desc.pRootSignature = root_signature;
    pso_desc.CS.pShaderBytecode = container;
    pso_desc.CS.BytecodeLength = sizeof(container);
    ID3D12PipelineState* pso = nullptr;
    hr = device->CreateComputePipelineState(&pso_desc, __uuidof(ID3D12PipelineState), reinterpret_cast<void**>(&pso));
    check(FAILED(hr) && pso == nullptr, "a shaderless container must be rejected (hr=0x%08lx pso=%p)", ul(hr),
          static_cast<void*>(pso));
    if (pso) pso->Release();

    char log_path[MAX_PATH];
    DWORD log_len = GetEnvironmentVariableA("BX_LOG_FILE", log_path, sizeof(log_path));
    if (log_len > 0 && log_len < sizeof(log_path)) {
        char needle[32];
        std::snprintf(needle, sizeof(needle), "chunks=%s", fourcc);
        check(file_contains(log_path, "\"event\":\"pso_shader\"", needle),
              "BX_LOG_FILE must contain a pso_shader record with %s", needle);
    } else {
        skip("BX_LOG_FILE is not set; pso_shader log record not verified");
    }

    /* --- 10. Padded vtable slots (#547) ---------------------------------------------------------- */
    /*
     * First padded slot = 3 (IUnknown) + every public method in the leaf's
     * inheritance chain, which is how gen_proxy.py counts `slot`. Derived
     * natively from the toolchain's d3d12.h with the generator's own
     * chain_of()/parse_interfaces(); matches kPublicSlotCount in
     * proxy_generated.h (79/19/9/81).
     */
    const PaddedVtableExpectation padded[] = {
        {"device", "ID3D12Device10", 79u},
        {"command queue", "ID3D12CommandQueue", 19u},
        {"command allocator", "ID3D12CommandAllocator", 9u},
        {"command list", "ID3D12GraphicsCommandList7", 81u},
    };
    IUnknown* const padded_objs[] = {device, queue, allocator, list};
    for (unsigned i = 0; i < 4; ++i) {
        IUnknown* obj = padded_objs[i];
        if (!obj) {
            check(false, "%s proxy missing; padded vtable slots not callable", padded[i].what);
            continue;
        }
        const unsigned first = padded[i].first_slot;
        const unsigned last = first + kPaddedSlotCount - 1;
        hr = call_vtable_slot(obj, first);
        check(hr == E_NOTIMPL, "%s padded vtable slot %u (BxBeyondPublicVtable00) hr=0x%08lx, want E_NOTIMPL",
              padded[i].what, first, ul(hr));
        hr = call_vtable_slot(obj, last);
        check(hr == E_NOTIMPL, "%s padded vtable slot %u (BxBeyondPublicVtable%02u, the %u-th) hr=0x%08lx, want E_NOTIMPL",
              padded[i].what, last, kPaddedSlotCount - 1, kPaddedSlotCount, ul(hr));
    }

    if (log_len > 0 && log_len < sizeof(log_path)) {
        for (unsigned i = 0; i < 4; ++i) {
            if (!padded_objs[i]) continue;
            char needle[96];
            std::snprintf(needle, sizeof(needle), "%s[%u]::beyond_public_vtable", padded[i].leaf,
                          padded[i].first_slot);
            check(file_contains(log_path, "\"event\":\"unimpl_vtable\"", needle),
                  "BX_LOG_FILE must contain an unimpl_vtable record with %s", needle);
        }
    } else {
        skip("BX_LOG_FILE is not set; beyond_public_vtable log records not verified");
    }

    /* --- 11. Teardown: every reference must come back ------------------------------------------- */
    if (rs_error) rs_error->Release();
    if (rs_blob) rs_blob->Release();
    if (root_signature) root_signature->Release();
    if (fence) fence->Release();
    if (list) check(list->Release() == 0, "command list leaked references");
    if (allocator) check(allocator->Release() == 0, "command allocator leaked references");
    if (queue) check(queue->Release() == 0, "command queue leaked references");
    check(device->Release() == 0, "device proxy leaked references");

    if (g_failures > 0) {
        std::fprintf(stderr, "FAIL: test_d3d12_xs (%d of %d checks failed, %d skipped)\n", g_failures, g_checks, g_skipped);
        return 1;
    }
    std::printf("PASS: test_d3d12_xs (%d checks passed, %d skipped)\n", g_checks, g_skipped);
    return 0;
}

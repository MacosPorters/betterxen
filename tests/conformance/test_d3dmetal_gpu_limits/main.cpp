/*
 * #453: synthetic D3D12 compute fixtures for three GPU limits seen under D3DMetal.
 *
 *   1. wave64      A [WaveSize(64)] compute shader (SM 6.6). Observed: is it rejected
 *                  at pipeline creation, created but not run, or run, and at which
 *                  wave width? A shader without WaveSize is the control and must run.
 *   2. atomic64    SM 6.6 64-bit atomics (add with carry, max) on a typed UAV
 *                  (RWTexture2D<uint64_t> over R32G32_UINT). Observed: correct or
 *                  wrong values. The same operations on a raw buffer are run as a
 *                  control and observed the same way. A plain store beside the
 *                  atomics shows whether each kernel ran at all.
 *   3. lock64      The same two operations emulated with a per-texel spin lock over
 *                  32-bit words and a bounded retry cap (64 and 1024 tries). Required:
 *                  with the 1024 cap no update is dropped and every value is exact.
 *
 * Talks to the host d3d12.dll directly (no BetterXen shim in the path), so under
 * CrossOver/Wine with D3DMetal it measures D3DMetal. Expected values come from the
 * public HLSL/D3D12 semantics and are computed here on the CPU, never by the GPU.
 *
 * Output: one "OBSERVED: <check> <outcome>" line per observation, "FAIL:" lines for
 * required checks, and a final summary. Exit 0 when every required check passed,
 * 1 otherwise, 77 when no D3D12 device can be created or it lacks shader model 6.6.
 */
#ifndef WIDL_EXPLICIT_AGGREGATE_RETURNS
#define WIDL_EXPLICIT_AGGREGATE_RETURNS /* call struct-returning COM methods with the real x64 ABI */
#endif
#include <windows.h>
#include <d3d12.h>

#include <cstdarg>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <vector>

#include "shaders/atomic64_buffer.h"
#include "shaders/atomic64_image.h"
#include "shaders/lock64_image.h"
#include "shaders/wave64_lanes.h"
#include "shaders/wave_lanes.h"

namespace {

constexpr uint32_t kSentinel = 0xA5A5A5A5u;
constexpr DWORD kGpuTimeoutMs = 10000;
constexpr UINT kThreads = 256; /* 4 groups of 64 for the atomic kernels */
constexpr UINT kTexels = 8;

int g_required = 0;
int g_failures = 0;

__attribute__((format(gnu_printf, 2, 3)))
bool require(bool ok, const char* fmt, ...) {
    g_required++;
    if (!ok) {
        g_failures++;
        char msg[320];
        va_list ap;
        va_start(ap, fmt);
        std::vsnprintf(msg, sizeof(msg), fmt, ap);
        va_end(ap);
        std::fprintf(stderr, "FAIL: %s\n", msg);
        std::fflush(stderr);
    }
    return ok;
}

__attribute__((format(gnu_printf, 1, 2)))
void observed(const char* fmt, ...) {
    char msg[320];
    va_list ap;
    va_start(ap, fmt);
    std::vsnprintf(msg, sizeof(msg), fmt, ap);
    va_end(ap);
    std::printf("OBSERVED: %s\n", msg);
    std::fflush(stdout);
}

unsigned long ul(HRESULT hr) { return static_cast<unsigned long>(hr); }

template <typename T>
void release(T*& p) {
    if (p) {
        p->Release();
        p = nullptr;
    }
}

/* ---- CPU reference values (independent of every shader) ---- */

uint64_t add_operand(uint32_t i) { return (uint64_t{0x10000000u} << 32) | uint64_t{0xF0000000u + i}; }
uint64_t max_operand(uint32_t i) { return (uint64_t{i} << 40) | uint64_t{0xFFFFFFFFu - i}; }

void expected_texels(uint64_t add[kTexels], uint64_t mx[kTexels]) {
    for (UINT p = 0; p < kTexels; ++p) {
        add[p] = 0;
        mx[p] = 0;
    }
    for (uint32_t i = 0; i < kThreads; ++i) {
        add[i & 7u] += add_operand(i); /* uint64_t wraps, as the HLSL add does */
        if (max_operand(i) > mx[i & 7u]) mx[i & 7u] = max_operand(i);
    }
}

/* ---- D3D12 plumbing ---- */

using PFN_CreateDevice = HRESULT(WINAPI*)(IUnknown*, D3D_FEATURE_LEVEL, REFIID, void**);
using PFN_Serialize = HRESULT(WINAPI*)(const D3D12_ROOT_SIGNATURE_DESC*, D3D_ROOT_SIGNATURE_VERSION, ID3DBlob**,
                                       ID3DBlob**);

struct Gpu {
    ID3D12Device* dev = nullptr;
    ID3D12CommandQueue* queue = nullptr;
    ID3D12CommandAllocator* alloc = nullptr;
    ID3D12GraphicsCommandList* list = nullptr;
    ID3D12Fence* fence = nullptr;
    UINT64 fence_value = 0;
    HANDLE event = nullptr;
    PFN_Serialize serialize = nullptr;
    bool lost = false; /* a submission timed out or the device was removed */
};

/* Close, submit, wait (bounded) and reopen the list. False on timeout or device removal. */
bool submit(Gpu& g, const char* what) {
    if (g.lost) return false;
    HRESULT hr = g.list->Close();
    if (FAILED(hr)) {
        observed("%s command list Close failed hr=0x%08lx", what, ul(hr));
        return false;
    }
    ID3D12CommandList* lists[] = {g.list};
    g.queue->ExecuteCommandLists(1, lists);
    hr = g.queue->Signal(g.fence, ++g.fence_value);
    if (FAILED(hr)) {
        observed("%s queue Signal failed hr=0x%08lx", what, ul(hr));
        g.lost = true;
        return false;
    }
    if (g.fence->GetCompletedValue() < g.fence_value) {
        hr = g.fence->SetEventOnCompletion(g.fence_value, g.event);
        if (FAILED(hr) || WaitForSingleObject(g.event, kGpuTimeoutMs) != WAIT_OBJECT_0) {
            observed("%s GPU work did not finish within %lu ms (queue considered hung; later checks skipped)", what,
                     static_cast<unsigned long>(kGpuTimeoutMs));
            g.lost = true;
            return false;
        }
    }
    if (g.fence->GetCompletedValue() == UINT64_MAX) {
        observed("%s device removed, reason hr=0x%08lx", what, ul(g.dev->GetDeviceRemovedReason()));
        g.lost = true;
        return false;
    }
    hr = g.alloc->Reset();
    if (SUCCEEDED(hr)) hr = g.list->Reset(g.alloc, nullptr);
    if (FAILED(hr)) {
        observed("%s allocator or command list Reset failed hr=0x%08lx", what, ul(hr));
        g.lost = true;
        return false;
    }
    return true;
}

/* Discards whatever is recorded in the open list without executing it (used when a
 * check fails partway through recording), so the next check starts from an empty list. */
void abandon(Gpu& g) {
    if (g.lost) return;
    g.list->Close();
    if (FAILED(g.alloc->Reset()) || FAILED(g.list->Reset(g.alloc, nullptr))) g.lost = true;
}

/* Releases a resource the GPU may have used. After a hung or removed queue the GPU may
 * still reference it, so it is deliberately leaked instead. */
template <typename T>
void drop(const Gpu& g, T*& p) {
    if (g.lost) {
        p = nullptr;
        return;
    }
    release(p);
}

ID3D12Resource* make_buffer(Gpu& g, D3D12_HEAP_TYPE heap, UINT64 size, D3D12_RESOURCE_STATES state,
                            D3D12_RESOURCE_FLAGS flags = D3D12_RESOURCE_FLAG_NONE) {
    D3D12_HEAP_PROPERTIES hp = {};
    hp.Type = heap;
    D3D12_RESOURCE_DESC rd = {};
    rd.Dimension = D3D12_RESOURCE_DIMENSION_BUFFER;
    rd.Width = size;
    rd.Height = 1;
    rd.DepthOrArraySize = 1;
    rd.MipLevels = 1;
    rd.SampleDesc.Count = 1;
    rd.Layout = D3D12_TEXTURE_LAYOUT_ROW_MAJOR;
    rd.Flags = flags;
    ID3D12Resource* r = nullptr;
    HRESULT hr = g.dev->CreateCommittedResource(&hp, D3D12_HEAP_FLAG_NONE, &rd, state, nullptr,
                                                __uuidof(ID3D12Resource), reinterpret_cast<void**>(&r));
    return SUCCEEDED(hr) ? r : nullptr;
}

ID3D12Resource* make_texture(Gpu& g, UINT width, UINT height, DXGI_FORMAT format) {
    D3D12_HEAP_PROPERTIES hp = {};
    hp.Type = D3D12_HEAP_TYPE_DEFAULT;
    D3D12_RESOURCE_DESC rd = {};
    rd.Dimension = D3D12_RESOURCE_DIMENSION_TEXTURE2D;
    rd.Width = width;
    rd.Height = height;
    rd.DepthOrArraySize = 1;
    rd.MipLevels = 1;
    rd.Format = format;
    rd.SampleDesc.Count = 1;
    rd.Flags = D3D12_RESOURCE_FLAG_ALLOW_UNORDERED_ACCESS;
    ID3D12Resource* r = nullptr;
    HRESULT hr = g.dev->CreateCommittedResource(&hp, D3D12_HEAP_FLAG_NONE, &rd, D3D12_RESOURCE_STATE_COPY_DEST,
                                                nullptr, __uuidof(ID3D12Resource), reinterpret_cast<void**>(&r));
    return SUCCEEDED(hr) ? r : nullptr;
}

void transition(Gpu& g, ID3D12Resource* r, D3D12_RESOURCE_STATES before, D3D12_RESOURCE_STATES after) {
    D3D12_RESOURCE_BARRIER b = {};
    b.Type = D3D12_RESOURCE_BARRIER_TYPE_TRANSITION;
    b.Transition.pResource = r;
    b.Transition.Subresource = D3D12_RESOURCE_BARRIER_ALL_SUBRESOURCES;
    b.Transition.StateBefore = before;
    b.Transition.StateAfter = after;
    g.list->ResourceBarrier(1, &b);
}

void uav_barrier(Gpu& g) {
    D3D12_RESOURCE_BARRIER b = {};
    b.Type = D3D12_RESOURCE_BARRIER_TYPE_UAV;
    g.list->ResourceBarrier(1, &b);
}

bool map_write(ID3D12Resource* r, const void* src, size_t n) {
    void* p = nullptr;
    if (FAILED(r->Map(0, nullptr, &p)) || !p) return false;
    std::memcpy(p, src, n);
    r->Unmap(0, nullptr);
    return true;
}

bool map_read(ID3D12Resource* r, void* dst, size_t n) {
    void* p = nullptr;
    D3D12_RANGE range = {0, n};
    if (FAILED(r->Map(0, &range, &p)) || !p) return false;
    std::memcpy(dst, p, n);
    D3D12_RANGE none = {0, 0};
    r->Unmap(0, &none);
    return true;
}

/* Root signature from parameters; logs and returns null on failure. */
ID3D12RootSignature* make_root(Gpu& g, const char* what, const D3D12_ROOT_PARAMETER* params, UINT count) {
    D3D12_ROOT_SIGNATURE_DESC desc = {};
    desc.NumParameters = count;
    desc.pParameters = params;
    ID3DBlob* blob = nullptr;
    ID3DBlob* err = nullptr;
    HRESULT hr = g.serialize(&desc, D3D_ROOT_SIGNATURE_VERSION_1, &blob, &err);
    release(err);
    if (FAILED(hr)) {
        observed("%s root signature serialization failed hr=0x%08lx", what, ul(hr));
        return nullptr;
    }
    ID3D12RootSignature* rs = nullptr;
    hr = g.dev->CreateRootSignature(0, blob->GetBufferPointer(), blob->GetBufferSize(), __uuidof(ID3D12RootSignature),
                                    reinterpret_cast<void**>(&rs));
    release(blob);
    if (FAILED(hr)) {
        observed("%s CreateRootSignature failed hr=0x%08lx", what, ul(hr));
        return nullptr;
    }
    return rs;
}

ID3D12PipelineState* make_pipeline(Gpu& g, ID3D12RootSignature* rs, const unsigned char* code, size_t size,
                                   HRESULT* out_hr) {
    D3D12_COMPUTE_PIPELINE_STATE_DESC pd = {};
    pd.pRootSignature = rs;
    pd.CS.pShaderBytecode = code;
    pd.CS.BytecodeLength = size;
    ID3D12PipelineState* pso = nullptr;
    *out_hr = g.dev->CreateComputePipelineState(&pd, __uuidof(ID3D12PipelineState), reinterpret_cast<void**>(&pso));
    return SUCCEEDED(*out_hr) ? pso : nullptr;
}

D3D12_ROOT_PARAMETER root_uav(UINT reg) {
    D3D12_ROOT_PARAMETER p = {};
    p.ParameterType = D3D12_ROOT_PARAMETER_TYPE_UAV;
    p.Descriptor.ShaderRegister = reg;
    p.ShaderVisibility = D3D12_SHADER_VISIBILITY_ALL;
    return p;
}

/* ---- buffer kernels: run one dispatch over a root-UAV buffer, return its words ---- */

bool run_buffer_kernel(Gpu& g, const char* what, ID3D12PipelineState* pso, ID3D12RootSignature* rs,
                       const std::vector<uint32_t>& init, UINT groups, std::vector<uint32_t>& out) {
    const UINT64 bytes = init.size() * sizeof(uint32_t);
    ID3D12Resource* up = make_buffer(g, D3D12_HEAP_TYPE_UPLOAD, bytes, D3D12_RESOURCE_STATE_GENERIC_READ);
    ID3D12Resource* buf = make_buffer(g, D3D12_HEAP_TYPE_DEFAULT, bytes, D3D12_RESOURCE_STATE_COMMON,
                                      D3D12_RESOURCE_FLAG_ALLOW_UNORDERED_ACCESS);
    ID3D12Resource* rb = make_buffer(g, D3D12_HEAP_TYPE_READBACK, bytes, D3D12_RESOURCE_STATE_COPY_DEST);
    bool ok = up && buf && rb && map_write(up, init.data(), bytes);
    if (ok) {
        g.list->CopyBufferRegion(buf, 0, up, 0, bytes);
        transition(g, buf, D3D12_RESOURCE_STATE_COPY_DEST, D3D12_RESOURCE_STATE_UNORDERED_ACCESS);
        g.list->SetComputeRootSignature(rs);
        g.list->SetPipelineState(pso);
        g.list->SetComputeRootUnorderedAccessView(0, buf->GetGPUVirtualAddress());
        g.list->Dispatch(groups, 1, 1);
        transition(g, buf, D3D12_RESOURCE_STATE_UNORDERED_ACCESS, D3D12_RESOURCE_STATE_COPY_SOURCE);
        g.list->CopyBufferRegion(rb, 0, buf, 0, bytes);
        ok = submit(g, what);
        out.assign(init.size(), 0);
        ok = ok && map_read(rb, out.data(), bytes);
    } else {
        observed("%s buffer creation or upload failed", what);
    }
    drop(g, up);
    drop(g, buf);
    drop(g, rb);
    return ok;
}

/* ---- 1. wave64 ---- */

void check_wave64(Gpu& g) {
    D3D12_FEATURE_DATA_D3D12_OPTIONS1 o1 = {};
    if (SUCCEEDED(g.dev->CheckFeatureSupport(D3D12_FEATURE_D3D12_OPTIONS1, &o1, sizeof(o1)))) {
        observed("wave.reported WaveOps=%d WaveLaneCountMin=%u WaveLaneCountMax=%u", o1.WaveOps ? 1 : 0,
                 o1.WaveLaneCountMin, o1.WaveLaneCountMax);
    }
    D3D12_ROOT_PARAMETER params[] = {root_uav(0)};
    ID3D12RootSignature* rs = make_root(g, "wave", params, 1);
    if (!require(rs != nullptr, "wave root signature")) return;

    /* Two words per thread: (wave width << 16) | lane index, then the dispatch index of the
     * wave's first active lane, which names the wave the thread ran in. */
    const std::vector<uint32_t> init(128, kSentinel);
    struct Case {
        const char* name;
        const unsigned char* code;
        size_t size;
        bool control;
    } cases[] = {
        {"wave.control", g_wave_lanes_cs, sizeof(g_wave_lanes_cs), true},
        {"wave.wavesize64", g_wave64_lanes_cs, sizeof(g_wave64_lanes_cs), false},
    };
    for (const Case& c : cases) {
        HRESULT hr = S_OK;
        ID3D12PipelineState* pso = make_pipeline(g, rs, c.code, c.size, &hr);
        if (!pso) {
            observed("%s rejected at CreateComputePipelineState hr=0x%08lx", c.name, ul(hr));
            if (c.control) require(false, "%s: control pipeline must be created", c.name);
            continue;
        }
        std::vector<uint32_t> out;
        bool ran = run_buffer_kernel(g, c.name, pso, rs, init, 1, out);
        release(pso);
        if (!ran) {
            if (c.control) require(false, "%s: control dispatch must complete", c.name);
            continue;
        }
        bool all_sentinel = true;
        for (uint32_t w : out) all_sentinel = all_sentinel && w == kSentinel;
        if (all_sentinel) {
            observed("%s created but not run (output still sentinel)", c.name);
            if (c.control) require(false, "%s: control kernel must write its output", c.name);
            continue;
        }
        /* What D3D12 guarantees: every thread reports the same width, and within each wave the
         * lane indices are a permutation of 0..W-1. Threads are grouped into waves by the
         * dispatch index of the wave's first lane; no particular thread-to-lane order is assumed. */
        const uint32_t width = out[0] >> 16;
        bool layout_ok = width >= 1 && width <= 64 && 64 % width == 0;
        std::vector<std::vector<uint32_t>> waves(64);
        for (uint32_t i = 0; layout_ok && i < 64; ++i) {
            const uint32_t lanes = out[2 * i], leader = out[2 * i + 1];
            layout_ok = (lanes >> 16) == width && leader < 64;
            if (layout_ok) waves[leader].push_back(lanes & 0xFFFFu);
        }
        uint32_t wave_count = 0;
        for (auto& w : waves) {
            if (!layout_ok || w.empty()) continue;
            wave_count++;
            std::vector<bool> seen(width, false);
            layout_ok = w.size() == width;
            for (uint32_t lane : w) {
                layout_ok = layout_ok && lane < width && !seen[lane];
                if (layout_ok) seen[lane] = true;
            }
        }
        layout_ok = layout_ok && wave_count * width == 64;
        observed("%s ran with wave width %u, lane layout %s", c.name, width, layout_ok ? "consistent" : "INCONSISTENT");
        if (c.control) require(layout_ok, "%s: each wave's lane indices must be a permutation of 0..W-1", c.name);
        if (!c.control && layout_ok && width != 64) {
            observed("%s WaveSize(64) not honoured: ran at width %u instead of being rejected", c.name, width);
        }
    }
    release(rs);
}

/* ---- 2. 64-bit atomics on a typed UAV, with a raw-buffer control ---- */

/* Plain-store marker the atomic kernels write next to their atomics: proves the kernel ran. */
uint64_t marker(UINT p) { return (uint64_t{0xC0DEu} << 32) | p; }

void report_marker(const char* name, const uint64_t got[kTexels]) {
    int bad = 0;
    for (UINT p = 0; p < kTexels; ++p) bad += got[p] != marker(p);
    observed("%s plain-store marker %s (%d/8 match)", name, bad == 0 ? "written: kernel ran" : "WRONG or missing",
             8 - bad);
}

void report_texels(const char* name, const uint64_t got_add[kTexels], const uint64_t got_max[kTexels]) {
    uint64_t want_add[kTexels], want_max[kTexels];
    expected_texels(want_add, want_max);
    int bad = 0;
    for (UINT p = 0; p < kTexels; ++p) {
        bad += got_add[p] != want_add[p];
        bad += got_max[p] != want_max[p];
    }
    if (bad == 0) {
        observed("%s correct (16/16 values)", name);
    } else {
        observed("%s WRONG values: %d/16 differ; texel 0 add got 0x%016llx want 0x%016llx, max got 0x%016llx want "
                 "0x%016llx",
                 name, bad, static_cast<unsigned long long>(got_add[0]), static_cast<unsigned long long>(want_add[0]),
                 static_cast<unsigned long long>(got_max[0]), static_cast<unsigned long long>(want_max[0]));
    }
}

void check_atomic64_buffer(Gpu& g) {
    D3D12_ROOT_PARAMETER params[] = {root_uav(0)};
    ID3D12RootSignature* rs = make_root(g, "atomic64.buffer", params, 1);
    if (!rs) return;
    HRESULT hr = S_OK;
    ID3D12PipelineState* pso = make_pipeline(g, rs, g_atomic64_buffer_cs, sizeof(g_atomic64_buffer_cs), &hr);
    if (!pso) {
        observed("atomic64.buffer rejected at CreateComputePipelineState hr=0x%08lx", ul(hr));
        release(rs);
        return;
    }
    std::vector<uint32_t> out;
    if (run_buffer_kernel(g, "atomic64.buffer", pso, rs, std::vector<uint32_t>(48, 0), kThreads / 64, out)) {
        uint64_t add[kTexels], mx[kTexels], mark[kTexels];
        for (UINT p = 0; p < kTexels; ++p) {
            add[p] = (uint64_t{out[2 * p + 1]} << 32) | out[2 * p];
            mx[p] = (uint64_t{out[16 + 2 * p + 1]} << 32) | out[16 + 2 * p];
            mark[p] = (uint64_t{out[32 + 2 * p + 1]} << 32) | out[32 + 2 * p];
        }
        report_marker("atomic64.buffer", mark);
        report_texels("atomic64.buffer", add, mx);
    }
    release(pso);
    release(rs);
}

/* Texture helpers: upload zeros into / read back a tightly described 2D texture. */
struct TexCopy {
    D3D12_PLACED_SUBRESOURCE_FOOTPRINT fp = {};
    UINT64 total = 0;
};

TexCopy footprint(Gpu& g, ID3D12Resource* tex) {
    TexCopy t;
    D3D12_RESOURCE_DESC rd;
    tex->GetDesc(&rd);
    UINT rows = 0;
    UINT64 row_bytes = 0;
    g.dev->GetCopyableFootprints(&rd, 0, 1, 0, &t.fp, &rows, &row_bytes, &t.total);
    return t;
}

void copy_to_texture(Gpu& g, ID3D12Resource* tex, ID3D12Resource* up, const TexCopy& t) {
    D3D12_TEXTURE_COPY_LOCATION dst = {};
    dst.pResource = tex;
    dst.Type = D3D12_TEXTURE_COPY_TYPE_SUBRESOURCE_INDEX;
    D3D12_TEXTURE_COPY_LOCATION src = {};
    src.pResource = up;
    src.Type = D3D12_TEXTURE_COPY_TYPE_PLACED_FOOTPRINT;
    src.PlacedFootprint = t.fp;
    g.list->CopyTextureRegion(&dst, 0, 0, 0, &src, nullptr);
}

void copy_from_texture(Gpu& g, ID3D12Resource* tex, ID3D12Resource* rb, const TexCopy& t) {
    D3D12_TEXTURE_COPY_LOCATION dst = {};
    dst.pResource = rb;
    dst.Type = D3D12_TEXTURE_COPY_TYPE_PLACED_FOOTPRINT;
    dst.PlacedFootprint = t.fp;
    D3D12_TEXTURE_COPY_LOCATION src = {};
    src.pResource = tex;
    src.Type = D3D12_TEXTURE_COPY_TYPE_SUBRESOURCE_INDEX;
    g.list->CopyTextureRegion(&dst, 0, 0, 0, &src, nullptr);
}

/* Reads texel (x, y), `words` 32-bit words per texel, out of a readback copy. */
uint32_t texel_word(const std::vector<uint8_t>& data, const TexCopy& t, UINT x, UINT y, UINT words, UINT w) {
    uint32_t v = 0;
    std::memcpy(&v, data.data() + t.fp.Offset + y * t.fp.Footprint.RowPitch + (x * words + w) * 4, 4);
    return v;
}

struct ImageSet {
    explicit ImageSet(const Gpu& gpu) : g(gpu) {}
    const Gpu& g;
    ID3D12DescriptorHeap* heap = nullptr;
    std::vector<ID3D12Resource*> tex;
    std::vector<TexCopy> copy;
    std::vector<ID3D12Resource*> rb;
    ~ImageSet() {
        drop(g, heap);
        for (auto*& r : tex) drop(g, r);
        for (auto*& r : rb) drop(g, r);
    }
};

/* Creates zero-filled UAV textures with descriptors in one shader-visible heap, ready in UAV state. */
bool make_images(Gpu& g, const char* what, ImageSet& s, const UINT (*dims)[2], const DXGI_FORMAT* formats, UINT count) {
    D3D12_DESCRIPTOR_HEAP_DESC hd = {};
    hd.Type = D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV;
    hd.NumDescriptors = count;
    hd.Flags = D3D12_DESCRIPTOR_HEAP_FLAG_SHADER_VISIBLE;
    if (FAILED(g.dev->CreateDescriptorHeap(&hd, __uuidof(ID3D12DescriptorHeap), reinterpret_cast<void**>(&s.heap)))) {
        observed("%s descriptor heap creation failed", what);
        return false;
    }
    D3D12_CPU_DESCRIPTOR_HANDLE cpu;
    s.heap->GetCPUDescriptorHandleForHeapStart(&cpu);
    const UINT inc = g.dev->GetDescriptorHandleIncrementSize(D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV);
    std::vector<ID3D12Resource*> uploads;
    bool ok = true;
    for (UINT i = 0; i < count && ok; ++i) {
        ID3D12Resource* t = make_texture(g, dims[i][0], dims[i][1], formats[i]);
        if (!t) {
            observed("%s texture %u creation failed", what, i);
            ok = false;
            break;
        }
        s.tex.push_back(t);
        s.copy.push_back(footprint(g, t));
        const TexCopy& c = s.copy.back();
        ID3D12Resource* up = make_buffer(g, D3D12_HEAP_TYPE_UPLOAD, c.total, D3D12_RESOURCE_STATE_GENERIC_READ);
        ID3D12Resource* rb = make_buffer(g, D3D12_HEAP_TYPE_READBACK, c.total, D3D12_RESOURCE_STATE_COPY_DEST);
        s.rb.push_back(rb);
        uploads.push_back(up);
        std::vector<uint8_t> zeros(static_cast<size_t>(c.total), 0);
        ok = up && rb && map_write(up, zeros.data(), zeros.size());
        if (!ok) break;
        copy_to_texture(g, t, up, c);
        transition(g, t, D3D12_RESOURCE_STATE_COPY_DEST, D3D12_RESOURCE_STATE_UNORDERED_ACCESS);
        D3D12_UNORDERED_ACCESS_VIEW_DESC ud = {};
        ud.Format = formats[i];
        ud.ViewDimension = D3D12_UAV_DIMENSION_TEXTURE2D;
        D3D12_CPU_DESCRIPTOR_HANDLE h = {cpu.ptr + static_cast<SIZE_T>(i) * inc};
        g.dev->CreateUnorderedAccessView(t, nullptr, &ud, h);
    }
    if (ok) {
        ok = submit(g, what);
    } else {
        abandon(g);
    }
    for (auto*& u : uploads) drop(g, u);
    return ok;
}

bool read_images(Gpu& g, const char* what, ImageSet& s, std::vector<std::vector<uint8_t>>& data) {
    for (size_t i = 0; i < s.tex.size(); ++i) {
        transition(g, s.tex[i], D3D12_RESOURCE_STATE_UNORDERED_ACCESS, D3D12_RESOURCE_STATE_COPY_SOURCE);
        copy_from_texture(g, s.tex[i], s.rb[i], s.copy[i]);
    }
    if (!submit(g, what)) return false;
    data.resize(s.tex.size());
    for (size_t i = 0; i < s.tex.size(); ++i) {
        data[i].resize(static_cast<size_t>(s.copy[i].total));
        if (!map_read(s.rb[i], data[i].data(), data[i].size())) return false;
    }
    return true;
}

void bind_table(Gpu& g, ImageSet& s) {
    ID3D12DescriptorHeap* heaps[] = {s.heap};
    g.list->SetDescriptorHeaps(1, heaps);
    D3D12_GPU_DESCRIPTOR_HANDLE gpu;
    s.heap->GetGPUDescriptorHandleForHeapStart(&gpu);
    g.list->SetComputeRootDescriptorTable(0, gpu);
}

void check_atomic64_image(Gpu& g) {
    D3D12_FEATURE_DATA_D3D12_OPTIONS9 o9 = {};
    D3D12_FEATURE_DATA_D3D12_OPTIONS11 o11 = {};
    const bool have9 = SUCCEEDED(g.dev->CheckFeatureSupport(D3D12_FEATURE_D3D12_OPTIONS9, &o9, sizeof(o9)));
    const bool have11 = SUCCEEDED(g.dev->CheckFeatureSupport(D3D12_FEATURE_D3D12_OPTIONS11, &o11, sizeof(o11)));
    observed("atomic64.reported AtomicInt64OnTypedResourceSupported=%s AtomicInt64OnDescriptorHeapResourceSupported=%s",
             have9 ? (o9.AtomicInt64OnTypedResourceSupported ? "1" : "0") : "query-failed",
             have11 ? (o11.AtomicInt64OnDescriptorHeapResourceSupported ? "1" : "0") : "query-failed");

    D3D12_DESCRIPTOR_RANGE range = {D3D12_DESCRIPTOR_RANGE_TYPE_UAV, 1, 0, 0, 0};
    D3D12_ROOT_PARAMETER table = {};
    table.ParameterType = D3D12_ROOT_PARAMETER_TYPE_DESCRIPTOR_TABLE;
    table.DescriptorTable.NumDescriptorRanges = 1;
    table.DescriptorTable.pDescriptorRanges = &range;
    ID3D12RootSignature* rs = make_root(g, "atomic64.image", &table, 1);
    if (!rs) return;
    HRESULT hr = S_OK;
    ID3D12PipelineState* pso = make_pipeline(g, rs, g_atomic64_image_cs, sizeof(g_atomic64_image_cs), &hr);
    if (!pso) {
        observed("atomic64.image rejected at CreateComputePipelineState hr=0x%08lx", ul(hr));
        release(rs);
        return;
    }
    {
        ImageSet s(g);
        const UINT dims[][2] = {{kTexels, 3}};
        const DXGI_FORMAT fmt[] = {DXGI_FORMAT_R32G32_UINT};
        std::vector<std::vector<uint8_t>> data;
        if (make_images(g, "atomic64.image", s, dims, fmt, 1)) {
            g.list->SetComputeRootSignature(rs);
            g.list->SetPipelineState(pso);
            bind_table(g, s);
            g.list->Dispatch(kThreads / 64, 1, 1);
            if (read_images(g, "atomic64.image", s, data)) {
                uint64_t add[kTexels], mx[kTexels], mark[kTexels];
                for (UINT p = 0; p < kTexels; ++p) {
                    mark[p] = (uint64_t{texel_word(data[0], s.copy[0], p, 2, 2, 1)} << 32) |
                              texel_word(data[0], s.copy[0], p, 2, 2, 0);
                    add[p] = (uint64_t{texel_word(data[0], s.copy[0], p, 0, 2, 1)} << 32) |
                             texel_word(data[0], s.copy[0], p, 0, 2, 0);
                    mx[p] = (uint64_t{texel_word(data[0], s.copy[0], p, 1, 2, 1)} << 32) |
                            texel_word(data[0], s.copy[0], p, 1, 2, 0);
                }
                report_marker("atomic64.image", mark);
                report_texels("atomic64.image", add, mx);
            }
        }
    }
    release(pso);
    release(rs);
}

/* ---- 3. lock-based emulation with a bounded retry cap ---- */

void check_lock64(Gpu& g, UINT cap, bool required) {
    char name[48];
    std::snprintf(name, sizeof(name), "lock64.cap%u", cap);

    D3D12_DESCRIPTOR_RANGE range = {D3D12_DESCRIPTOR_RANGE_TYPE_UAV, 2, 0, 0, 0};
    D3D12_ROOT_PARAMETER params[3] = {};
    params[0].ParameterType = D3D12_ROOT_PARAMETER_TYPE_DESCRIPTOR_TABLE;
    params[0].DescriptorTable.NumDescriptorRanges = 1;
    params[0].DescriptorTable.pDescriptorRanges = &range;
    params[1] = root_uav(2);
    params[2].ParameterType = D3D12_ROOT_PARAMETER_TYPE_32BIT_CONSTANTS;
    params[2].Constants.ShaderRegister = 0;
    params[2].Constants.Num32BitValues = 1;
    ID3D12RootSignature* rs = make_root(g, name, params, 3);
    if (!rs) {
        if (required) require(false, "%s: root signature", name);
        return;
    }
    HRESULT hr = S_OK;
    ID3D12PipelineState* pso = make_pipeline(g, rs, g_lock64_image_cs, sizeof(g_lock64_image_cs), &hr);
    if (!pso) {
        observed("%s rejected at CreateComputePipelineState hr=0x%08lx", name, ul(hr));
        if (required) require(false, "%s: lock emulation pipeline must be created", name);
        release(rs);
        return;
    }
    {
        ImageSet s(g);
        const UINT dims[][2] = {{2 * kTexels, 2}, {kTexels, 2}};
        const DXGI_FORMAT fmt[] = {DXGI_FORMAT_R32_UINT, DXGI_FORMAT_R32_UINT};
        ID3D12Resource* stats = nullptr;
        ID3D12Resource* stats_rb = nullptr;
        ID3D12Resource* up = nullptr; /* kept alive until the list that copies from it has run */
        bool ok = make_images(g, name, s, dims, fmt, 2);
        if (ok) {
            const uint32_t zeros[2] = {0, 0};
            up = make_buffer(g, D3D12_HEAP_TYPE_UPLOAD, 8, D3D12_RESOURCE_STATE_GENERIC_READ);
            stats = make_buffer(g, D3D12_HEAP_TYPE_DEFAULT, 256, D3D12_RESOURCE_STATE_COMMON,
                                D3D12_RESOURCE_FLAG_ALLOW_UNORDERED_ACCESS);
            stats_rb = make_buffer(g, D3D12_HEAP_TYPE_READBACK, 8, D3D12_RESOURCE_STATE_COPY_DEST);
            ok = up && stats && stats_rb && map_write(up, zeros, sizeof(zeros));
            if (ok) {
                g.list->CopyBufferRegion(stats, 0, up, 0, 8);
                transition(g, stats, D3D12_RESOURCE_STATE_COPY_DEST, D3D12_RESOURCE_STATE_UNORDERED_ACCESS);
                g.list->SetComputeRootSignature(rs);
                g.list->SetPipelineState(pso);
                bind_table(g, s);
                g.list->SetComputeRootUnorderedAccessView(1, stats->GetGPUVirtualAddress());
                g.list->SetComputeRoot32BitConstant(2, cap, 0);
                g.list->Dispatch(kThreads / 64, 1, 1);
                uav_barrier(g);
                transition(g, stats, D3D12_RESOURCE_STATE_UNORDERED_ACCESS, D3D12_RESOURCE_STATE_COPY_SOURCE);
                g.list->CopyBufferRegion(stats_rb, 0, stats, 0, 8);
            } else {
                abandon(g);
            }
        }
        std::vector<std::vector<uint8_t>> data;
        ok = ok && read_images(g, name, s, data);
        drop(g, up);
        uint32_t st[2] = {0, 0};
        ok = ok && map_read(stats_rb, st, sizeof(st));
        if (ok) {
            uint64_t add[kTexels], mx[kTexels];
            for (UINT p = 0; p < kTexels; ++p) {
                add[p] = (uint64_t{texel_word(data[0], s.copy[0], 2 * p + 1, 0, 1, 0)} << 32) |
                         texel_word(data[0], s.copy[0], 2 * p, 0, 1, 0);
                mx[p] = (uint64_t{texel_word(data[0], s.copy[0], 2 * p + 1, 1, 1, 0)} << 32) |
                        texel_word(data[0], s.copy[0], 2 * p, 1, 1, 0);
            }
            observed("%s dropped %u of %u updates, %u tries in all", name, st[0], 2 * kThreads, st[1]);
            if (st[0] == 0) {
                report_texels(name, add, mx);
            } else {
                observed("%s values not compared (updates were dropped)", name);
            }
            if (required) {
                uint64_t want_add[kTexels], want_max[kTexels];
                expected_texels(want_add, want_max);
                bool exact = st[0] == 0;
                for (UINT p = 0; exact && p < kTexels; ++p) exact = add[p] == want_add[p] && mx[p] == want_max[p];
                require(exact, "%s: no update dropped and every value exact (dropped %u)", name, st[0]);
            }
        } else if (required) {
            require(false, "%s: dispatch and readback must complete", name);
        }
        drop(g, stats);
        drop(g, stats_rb);
    }
    release(pso);
    release(rs);
}

}  // namespace

int main() {
    std::setvbuf(stdout, nullptr, _IONBF, 0);
    HMODULE d3d12 = LoadLibraryA("d3d12.dll");
    auto create = d3d12 ? reinterpret_cast<PFN_CreateDevice>(reinterpret_cast<void*>(GetProcAddress(d3d12, "D3D12CreateDevice"))) : nullptr;
    Gpu g;
    g.serialize = d3d12 ? reinterpret_cast<PFN_Serialize>(
                                    reinterpret_cast<void*>(GetProcAddress(d3d12, "D3D12SerializeRootSignature"))) : nullptr;
    if (!create || !g.serialize) {
        std::printf("SKIP: d3d12.dll or its exports are unavailable\n");
        return 77;
    }
    HRESULT hr = create(nullptr, D3D_FEATURE_LEVEL_12_0, __uuidof(ID3D12Device), reinterpret_cast<void**>(&g.dev));
    if (FAILED(hr) || !g.dev) {
        std::printf("SKIP: D3D12CreateDevice failed hr=0x%08lx\n", ul(hr));
        return 77;
    }
    /* Every kernel is cs_6_6: below that (or when the query fails) the fixture cannot say
     * anything about these limits, so it skips instead of reporting a failure. */
    D3D12_FEATURE_DATA_SHADER_MODEL sm = {D3D_SHADER_MODEL_6_6};
    if (FAILED(g.dev->CheckFeatureSupport(D3D12_FEATURE_SHADER_MODEL, &sm, sizeof(sm))) ||
        sm.HighestShaderModel < D3D_SHADER_MODEL_6_6) {
        std::printf("SKIP: shader model 6.6 is not available\n");
        release(g.dev);
        return 77;
    }
    observed("device.shader_model 0x%x", static_cast<unsigned>(sm.HighestShaderModel));

    D3D12_COMMAND_QUEUE_DESC qd = {};
    qd.Type = D3D12_COMMAND_LIST_TYPE_DIRECT;
    bool setup = SUCCEEDED(g.dev->CreateCommandQueue(&qd, __uuidof(ID3D12CommandQueue),
                                                     reinterpret_cast<void**>(&g.queue))) &&
                 SUCCEEDED(g.dev->CreateCommandAllocator(D3D12_COMMAND_LIST_TYPE_DIRECT,
                                                         __uuidof(ID3D12CommandAllocator),
                                                         reinterpret_cast<void**>(&g.alloc))) &&
                 SUCCEEDED(g.dev->CreateCommandList(0, D3D12_COMMAND_LIST_TYPE_DIRECT, g.alloc, nullptr,
                                                    __uuidof(ID3D12GraphicsCommandList),
                                                    reinterpret_cast<void**>(&g.list))) &&
                 SUCCEEDED(g.dev->CreateFence(0, D3D12_FENCE_FLAG_NONE, __uuidof(ID3D12Fence),
                                              reinterpret_cast<void**>(&g.fence)));
    g.event = CreateEventA(nullptr, FALSE, FALSE, nullptr);
    if (!require(setup && g.event, "queue, allocator, command list and fence creation")) return 1;

    check_wave64(g);
    check_atomic64_buffer(g);
    check_atomic64_image(g);
    check_lock64(g, 64, false);
    check_lock64(g, 1024, true);
    if (g.lost) require(false, "GPU work must finish and the device must stay alive");

    std::printf("SUMMARY: %d required checks, %d failed\n", g_required, g_failures);
    if (g.lost) return 1; /* leave the device and queue alone: the GPU may still be using them */
    release(g.fence);
    release(g.list);
    release(g.alloc);
    release(g.queue);
    release(g.dev);
    if (g.event) CloseHandle(g.event);
    return g_failures == 0 ? 0 : 1;
}

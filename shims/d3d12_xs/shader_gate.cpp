#include "shader_gate.h"

#include <cstdio>
#include <cstring>
#include <cwchar>
#include <iterator>

#include "bx_log.h"
#include "bx_shader_bridge.h"

/*
 * The bridge is loaded on first use, only by full path from this DLL's own directory, where
 * build-shims.sh stages it with the shims (#484). A bare-name load would search the guest EXE's
 * directory and PATH first and could pick up a different copy. It is optional at load time, so a
 * missing bridge is a logged condition rather than a failure to load d3d12_xs.dll. Without it,
 * shaders are not classified, but a container that has an ACCC chunk and no DXIL chunk is still
 * refused, from a bounds-checked chunk scan, so it never reaches the host device.
 *
 * Constraint flags come from two public DXIL container parts, read here with bounds checks:
 *  - SFI0 is one uint64 of feature flags (DXC DxilContainer.h, DxilShaderFeatureInfo). The bit
 *    values are D3D_SHADER_FEATURE_* from the public DirectX-Headers d3dcommon.h (MIT):
 *    WAVE_OPS 0x4000, INT64_OPS 0x8000, ATOMIC_INT64_ON_TYPED_RESOURCE 0x400000,
 *    ATOMIC_INT64_ON_GROUP_SHARED 0x800000, ATOMIC_INT64_ON_DESCRIPTOR_HEAP_RESOURCE 0x10000000.
 *  - PSV0 starts with a uint32 PSVRuntimeInfo size, then PSVRuntimeInfo0, whose 16-byte stage
 *    union is followed by MinimumExpectedWaveLaneCount and MaximumExpectedWaveLaneCount (DXC
 *    DxilPipelineStateValidation.h). DXC 1.9.2609 writes 64/64 there for [WaveSize(64)] and
 *    0/0xFFFFFFFF without it (checked on the #453 blobs).
 * 64-bit atomics on raw buffers set only INT64_OPS, so they cannot be told apart from other
 * 64-bit integer use here; see ADR 0008.
 */

namespace {

using InspectFn = bx_shader_status_t (*)(const uint8_t*, size_t, bx_shader_inspection_t*);
using PrepareFn = bx_shader_status_t (*)(const uint8_t*, size_t, uint8_t**, size_t*);
using FreeFn = void (*)(uint8_t*, size_t);

struct Bridge {
    InspectFn inspect = nullptr;
    PrepareFn prepare = nullptr;
    FreeFn release = nullptr;
};

Bridge g_bridge;
INIT_ONCE g_bridge_once = INIT_ONCE_STATIC_INIT;

template <typename Fn>
Fn proc(HMODULE module, const char* name) {
    return reinterpret_cast<Fn>(reinterpret_cast<void*>(GetProcAddress(module, name)));
}

BOOL CALLBACK load_bridge(PINIT_ONCE, PVOID, PVOID*) {
    HMODULE self = nullptr;
    HMODULE module = nullptr;
    wchar_t path[MAX_PATH];
    if (GetModuleHandleExW(GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
                           reinterpret_cast<LPCWSTR>(&load_bridge), &self)) {
        const DWORD length = GetModuleFileNameW(self, path, MAX_PATH);
        wchar_t* slash = length && length < MAX_PATH ? std::wcsrchr(path, L'\\') : nullptr;
        constexpr wchar_t kName[] = L"bx_shader_bridge.dll";
        if (slash && static_cast<size_t>(slash + 1 - path) + std::size(kName) <= MAX_PATH) {
            std::memcpy(slash + 1, kName, sizeof(kName));
            module = LoadLibraryExW(path, nullptr, LOAD_WITH_ALTERED_SEARCH_PATH);
        }
    }
    if (module) {
        g_bridge.inspect = proc<InspectFn>(module, "bx_shader_inspect");
        g_bridge.prepare = proc<PrepareFn>(module, "bx_shader_prepare_for_d3dmetal");
        g_bridge.release = proc<FreeFn>(module, "bx_shader_free_buffer");
    }
    if (!g_bridge.inspect || !g_bridge.prepare || !g_bridge.release) {
        g_bridge = {};
        bx_log_emit(BX_LOG_WARN, "d3d12_xs", "shader_bridge_missing",
                    module ? "bx_shader_bridge.dll lacks an entry point; shaders pass unclassified, ACCC-only refused"
                           : "bx_shader_bridge.dll not found beside d3d12_xs.dll; shaders pass unclassified, "
                             "ACCC-only refused");
    }
    return TRUE;
}

const Bridge* bridge() {
    InitOnceExecuteOnce(&g_bridge_once, load_bridge, nullptr, nullptr);
    return g_bridge.inspect ? &g_bridge : nullptr;
}

uint32_t read_u32(const uint8_t* at) {
    uint32_t value;
    std::memcpy(&value, at, sizeof(value));
    return value;
}

/* The payload of the first chunk named `fourcc`, or null. Every read is bounds-checked, so this
 * is safe on a container nobody has validated (the no-bridge path relies on that). */
const uint8_t* find_chunk(const uint8_t* bytes, size_t len, const char* fourcc, uint32_t* size) {
    if (len < 32) return nullptr;
    const uint32_t count = read_u32(bytes + 28);
    for (uint32_t i = 0; i < count && 32 + 4 * static_cast<size_t>(i) + 4 <= len; ++i) {
        const uint32_t offset = read_u32(bytes + 32 + 4 * static_cast<size_t>(i));
        if (offset > len || len - offset < 8) continue;
        if (std::memcmp(bytes + offset, fourcc, 4) != 0) continue;
        const uint32_t chunk = read_u32(bytes + offset + 4);
        if (chunk > len - offset - 8) return nullptr;
        *size = chunk;
        return bytes + offset + 8;
    }
    return nullptr;
}

/* True when any chunk header in the table is tagged `fourcc`, whatever its declared size. */
bool has_chunk_tag(const uint8_t* bytes, size_t len, const char* fourcc) {
    if (len < 32) return false;
    const uint32_t count = read_u32(bytes + 28);
    for (uint32_t i = 0; i < count && 32 + 4 * static_cast<size_t>(i) + 4 <= len; ++i) {
        const uint32_t offset = read_u32(bytes + 32 + 4 * static_cast<size_t>(i));
        if (offset <= len && len - offset >= 4 && std::memcmp(bytes + offset, fourcc, 4) == 0) return true;
    }
    return false;
}

struct Constraints {
    bool has_flags = false;
    uint64_t flags = 0;
    bool has_lanes = false;
    uint32_t lanes_min = 0;
    uint32_t lanes_max = 0;
};

Constraints read_constraints(const uint8_t* bytes, size_t len) {
    Constraints c;
    uint32_t size = 0;
    if (const uint8_t* sfi0 = find_chunk(bytes, len, "SFI0", &size); sfi0 && size >= 8) {
        std::memcpy(&c.flags, sfi0, sizeof(c.flags));
        c.has_flags = true;
    }
    if (const uint8_t* psv0 = find_chunk(bytes, len, "PSV0", &size); psv0 && size >= 4) {
        const uint32_t info_size = read_u32(psv0);
        if (info_size >= 24 && size >= 4u + 24u) {
            c.lanes_min = read_u32(psv0 + 4 + 16);
            c.lanes_max = read_u32(psv0 + 4 + 20);
            c.has_lanes = true;
        }
    }
    return c;
}

constexpr uint64_t kWaveOps = 0x4000;
constexpr uint64_t kInt64Ops = 0x8000;
constexpr uint64_t kAtomic64Typed = 0x400000;
constexpr uint64_t kAtomic64GroupShared = 0x800000;
constexpr uint64_t kAtomic64Heap = 0x10000000;

/* The [WaveSize(64)] case on its own (#591). A wider-than-32 wave is refused at PSO creation
 * rather than handed to a host that creates the pipeline and never runs it: the caller then
 * learns from a failed call instead of from a silently absent draw. Lowering 64 lanes to 32 is
 * a semantic transform (reductions, lane indices, 64-bit masks, cross-half permutes, barriers)
 * and is deferred. 64-bit atomics stay warn-only until their detection is complete (#589). */
bool wave_wider_than_32(const Constraints& c) {
    return c.has_lanes && c.lanes_min > 32;
}

/* Measured on D3DMetal (docs/research/d3dmetal-gpu-limits.md): pipelines that require a wave
 * wider than 32 lanes, or 64-bit atomics, are created but never run. */
bool known_d3dmetal_gap(const Constraints& c) {
    return wave_wider_than_32(c)
        || (c.flags & (kAtomic64Typed | kAtomic64GroupShared | kAtomic64Heap));
}
const char* kind_name(bx_shader_kind_t kind) {
    switch (kind) {
        case BX_SHADER_DXBC_DXIL: return "dxil";
        case BX_SHADER_DXBC_LEGACY: return "legacy_dxbc";
        case BX_SHADER_DXBC_ACCC: return "accc_only";
        case BX_SHADER_DXBC_DUAL: return "dual";
        default: return "unknown";
    }
}

void log_gate(const char* pso_kind, const char* stage, size_t len, const bx_shader_inspection_t* info,
              const Constraints* c, const char* decision, HRESULT hr) {
    char lanes[32] = "none";
    if (c && c->has_lanes) {
        std::snprintf(lanes, sizeof(lanes), "%lu-%lu", static_cast<unsigned long>(c->lanes_min),
                      static_cast<unsigned long>(c->lanes_max));
    }
    char detail[400];
    if (info) {
        std::snprintf(detail, sizeof(detail),
                      "pso=%s stage=%s decision=%s hr=0x%08lx kind=%s shader_stage=%d sm=%lu.%lu bytes=%llu "
                      "dxil=%lu accc=%lu sfi0=0x%llx wave_ops=%d int64_ops=%d atomic64=%s%s%s wave_lanes=%s",
                      pso_kind, stage, decision, static_cast<unsigned long>(hr), kind_name(info->kind),
                      static_cast<int>(info->stage), static_cast<unsigned long>(info->shader_model_major),
                      static_cast<unsigned long>(info->shader_model_minor), static_cast<unsigned long long>(len),
                      static_cast<unsigned long>(info->dxil_chunk_size),
                      static_cast<unsigned long>(info->accc_chunk_size),
                      static_cast<unsigned long long>(c ? c->flags : 0), c && (c->flags & kWaveOps) ? 1 : 0,
                      c && (c->flags & kInt64Ops) ? 1 : 0,
                      c && (c->flags & kAtomic64Typed) ? "typed," : "",
                      c && (c->flags & kAtomic64GroupShared) ? "groupshared," : "",
                      c && (c->flags & kAtomic64Heap) ? "heap," : "", lanes);
    } else {
        std::snprintf(detail, sizeof(detail), "pso=%s stage=%s decision=%s hr=0x%08lx bytes=%llu", pso_kind, stage,
                      decision, static_cast<unsigned long>(hr), static_cast<unsigned long long>(len));
    }
    bx_log_emit(FAILED(hr) ? BX_LOG_WARN : BX_LOG_INFO, "d3d12_xs", "shader_gate", detail);
}

} // namespace

BxShaderGate::~BxShaderGate() {
    const Bridge* b = copy_count_ ? bridge() : nullptr;
    for (size_t i = 0; i < copy_count_; ++i) {
        if (b) b->release(copies_[i], copy_sizes_[i]);
    }
}

HRESULT BxShaderGate::admit(const char* pso_kind, const char* stage, const D3D12_SHADER_BYTECODE& in,
                            D3D12_SHADER_BYTECODE& out) {
    out = in;
    if (!in.pShaderBytecode || in.BytecodeLength == 0) return S_OK;
    const auto* bytes = static_cast<const uint8_t*>(in.pShaderBytecode);
    const size_t len = in.BytecodeLength;

    const Bridge* b = bridge();
    if (!b) {
        /* No classifier: still keep ACCC-only containers away from the host (#478 review). */
        /* Any ACCC tag counts; only a DXIL chunk that fits counts as DXIL, so doubt refuses. */
        uint32_t size = 0;
        const bool dxbc = len >= 4 && std::memcmp(bytes, "DXBC", 4) == 0;
        if (dxbc && has_chunk_tag(bytes, len, "ACCC") && !find_chunk(bytes, len, "DXIL", &size)) {
            log_gate(pso_kind, stage, len, nullptr, nullptr, "reject_accc_only_unclassified", E_NOTIMPL);
            return E_NOTIMPL;
        }
        log_gate(pso_kind, stage, len, nullptr, nullptr, "pass_unclassified", S_OK);
        return S_OK;
    }
    bx_shader_inspection_t info{};
    if (b->inspect(bytes, len, &info) != BX_SHADER_OK) {
        /* Not a container the bridge understands; the host keeps its own validation. */
        log_gate(pso_kind, stage, len, nullptr, nullptr, "pass_uninspected", S_OK);
        return S_OK;
    }
    const Constraints constraints = read_constraints(bytes, len);
    switch (info.kind) {
        case BX_SHADER_DXBC_ACCC:
            log_gate(pso_kind, stage, len, &info, &constraints, "reject_accc_only", E_NOTIMPL);
            return E_NOTIMPL;
        case BX_SHADER_DXBC_DXIL:
        case BX_SHADER_DXBC_DUAL: {
            if (copy_count_ == kMaxCopies) {
                log_gate(pso_kind, stage, len, &info, &constraints, "copy_limit", E_UNEXPECTED);
                return E_UNEXPECTED;
            }
            uint8_t* copy = nullptr;
            size_t copy_len = 0;
            const bx_shader_status_t status = b->prepare(bytes, len, &copy, &copy_len);
            if (status != BX_SHADER_OK) {
                const HRESULT hr = status == BX_SHADER_ERR_OOM ? E_OUTOFMEMORY : E_INVALIDARG;
                log_gate(pso_kind, stage, len, &info, &constraints, "prepare_failed", hr);
                return hr;
            }
            copies_[copy_count_] = copy;
            copy_sizes_[copy_count_] = copy_len;
            ++copy_count_;
            out.pShaderBytecode = copy;
            out.BytecodeLength = copy_len;
            /* A wave wider than 32 lanes is refused here (#591): D3DMetal creates such a
             * pipeline and never runs it, so returning S_OK would leave the caller with a
             * silent no-op. The stage keeps its copy alive via `gate` until the caller's
             * PSO call returns. */
            if (wave_wider_than_32(constraints)) {
                log_gate(pso_kind, stage, len, &info, &constraints, "reject_wave_wide", E_NOTIMPL);
                char detail[160];
                std::snprintf(detail, sizeof(detail),
                              "pso=%s stage=%s wave_lanes_min=%lu: a wave wider than 32 lanes is created "
                              "but never run on D3DMetal (#453); refused at PSO creation (#591)",
                              pso_kind, stage, static_cast<unsigned long>(constraints.lanes_min));
                bx_log_emit(BX_LOG_WARN, "d3d12_xs", "shader_d3dmetal_gap", detail);
                return E_NOTIMPL;
            }
            log_gate(pso_kind, stage, len, &info, &constraints, "pass_dxil", S_OK);
            if (known_d3dmetal_gap(constraints)) {
                char detail[160];
                std::snprintf(detail, sizeof(detail),
                              "pso=%s stage=%s wave_lanes_min=%lu atomic64_flags=0x%llx: D3DMetal creates such "
                              "pipelines but does not run them (#453)",
                              pso_kind, stage, static_cast<unsigned long>(constraints.lanes_min),
                              static_cast<unsigned long long>(
                                  constraints.flags & (kAtomic64Typed | kAtomic64GroupShared | kAtomic64Heap)));
                bx_log_emit(BX_LOG_WARN, "d3d12_xs", "shader_d3dmetal_gap", detail);
            }
            return S_OK;
        }
        default:
            log_gate(pso_kind, stage, len, &info, &constraints, "pass_original", S_OK);
            return S_OK;
    }
}

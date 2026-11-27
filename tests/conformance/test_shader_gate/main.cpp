/*
 * M2 Shader Format Gate (#466): synthetic conformance test for the PSO hook in d3d12_xs.dll.
 *
 * Through D3D12XboxCreateDevice it:
 *  1. creates a compute PSO from a real DXC 1.9.2609 SM 6.6 DXIL blob (fill_cs66, from
 *     d3dmetal-harness), dispatches it and checks all 256 output words on the CPU;
 *  2. submits a synthetic ACCC-only container (BetterXen FSPX v1, compute) to
 *     CreateComputePipelineState and to CreateGraphicsPipelineState and requires E_NOTIMPL
 *     with no object and no call reaching the host device;
 *  3. creates PSOs from the #453 [WaveSize(64)] and 64-bit typed-atomic blobs and requires the
 *     gate to log their constraints;
 *  4. checks the structured `shader_gate` JSONL records.
 *
 * It then runs a copy of itself with --no-bridge from a directory holding d3d12_xs.dll but not
 * the bridge, which must still run DXIL and refuse ACCC-only with E_NOTIMPL.
 *
 * Expected values come from the D3D12 contract, the DXBC layout and the documented kernel, not
 * from the shim. Needs bx_shader_bridge.dll next to the shims and BX_LOG_FILE (bx-cli --log).
 * It also checks that the bridge was loaded from d3d12_xs.dll's directory; run it with the EXE
 * in a different directory from the shims to exercise that lookup.
 */
#ifndef WIDL_EXPLICIT_AGGREGATE_RETURNS
#define WIDL_EXPLICIT_AGGREGATE_RETURNS
#endif
#include <windows.h>
#include <unknwn.h>
#include <d3d12.h>

#include <climits>
#include <cstdarg>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <cwchar>
#include <string>
#include <vector>

#include "atomic64_image.h"  // g_atomic64_image_cs, #453
#include "fill_cs66.h"       // g_fill_cs66, d3dmetal-harness
#include "wave64_lanes.h"    // g_wave64_lanes_cs, #453

extern "C" __declspec(dllimport) HRESULT WINAPI D3D12XboxCreateDevice(PVOID params, REFIID riid, void** ppv);
extern "C" __declspec(dllimport) HRESULT WINAPI D3D12SerializeVersionedRootSignature(
    const D3D12_VERSIONED_ROOT_SIGNATURE_DESC* desc, ID3DBlob** blob, ID3DBlob** error);

namespace {

int g_checks = 0;
int g_failures = 0;

__attribute__((format(gnu_printf, 2, 3)))
bool check(bool ok, const char* fmt, ...) {
    ++g_checks;
    if (!ok) {
        ++g_failures;
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

unsigned long ul(HRESULT hr) { return static_cast<unsigned long>(hr); }

int finish() {
    if (g_failures) {
        std::fprintf(stderr, "FAIL: test_shader_gate (%d of %d checks failed)\n", g_failures, g_checks);
        return 1;
    }
    std::printf("PASS: test_shader_gate (%d checks)\n", g_checks);
    return 0;
}

/* The kernel in fill_cs66.hlsl: Out[i] = hash(i) for 4 groups of 64 threads. */
constexpr uint32_t kWords = 256;
uint32_t expected_word(uint32_t i) {
    uint32_t v = i * 0x9E3779B1u;
    v ^= v >> 15;
    return v + 0x01234567u;
}

void put32(std::vector<uint8_t>& out, size_t at, uint32_t value) { std::memcpy(out.data() + at, &value, 4); }

/* A DXBC container (version 1) holding one ACCC chunk with a BetterXen FSPX v1 table whose only
 * section is the stage configuration: compute, shader model 6.0. No DXIL. */
std::vector<uint8_t> accc_only_container() {
    std::vector<uint8_t> fspx(40, 0);
    std::memcpy(fspx.data(), "FSPX", 4);
    put32(fspx, 4, 1);   // version
    put32(fspx, 8, 1);   // section count
    put32(fspx, 16, 1);  // section id 1: stage configuration
    put32(fspx, 20, 28); // offset
    put32(fspx, 24, 12); // size
    put32(fspx, 28, 6);  // stage: compute
    put32(fspx, 32, 6);  // shader model 6.0
    put32(fspx, 36, 0);
    const uint32_t total = 32 + 4 + 8 + static_cast<uint32_t>(fspx.size());
    std::vector<uint8_t> out(total, 0);
    std::memcpy(out.data(), "DXBC", 4);
    put32(out, 20, 1);
    put32(out, 24, total);
    put32(out, 28, 1);
    put32(out, 32, 36);
    std::memcpy(out.data() + 36, "ACCC", 4);
    put32(out, 40, static_cast<uint32_t>(fspx.size()));
    std::memcpy(out.data() + 44, fspx.data(), fspx.size());
    return out;
}

std::string read_log() {
    char path[MAX_PATH];
    const DWORD n = GetEnvironmentVariableA("BX_LOG_FILE", path, sizeof(path));
    if (n == 0 || n >= sizeof(path)) return {};
    std::string text;
    if (FILE* file = std::fopen(path, "rb")) {
        char buffer[4096];
        size_t got;
        while ((got = std::fread(buffer, 1, sizeof(buffer), file)) > 0) text.append(buffer, got);
        std::fclose(file);
    }
    return text;
}

/* True when one JSONL line holds every needle. */
bool log_has(const std::string& log, std::initializer_list<const char*> needles) {
    size_t start = 0;
    while (start < log.size()) {
        size_t end = log.find('\n', start);
        if (end == std::string::npos) end = log.size();
        const std::string line = log.substr(start, end - start);
        bool all = true;
        for (const char* needle : needles) all = all && line.find(needle) != std::string::npos;
        if (all) return true;
        start = end + 1;
    }
    return false;
}

ID3D12RootSignature* make_root(ID3D12Device* device, const D3D12_ROOT_PARAMETER& param) {
    D3D12_VERSIONED_ROOT_SIGNATURE_DESC desc = {};
    desc.Version = D3D_ROOT_SIGNATURE_VERSION_1_0;
    desc.Desc_1_0.NumParameters = 1;
    desc.Desc_1_0.pParameters = &param;
    ID3DBlob* blob = nullptr;
    ID3DBlob* error = nullptr;
    HRESULT hr = D3D12SerializeVersionedRootSignature(&desc, &blob, &error);
    if (error) error->Release();
    if (!check(SUCCEEDED(hr) && blob, "D3D12SerializeVersionedRootSignature hr=0x%08lx", ul(hr))) return nullptr;
    ID3D12RootSignature* root = nullptr;
    hr = device->CreateRootSignature(0, blob->GetBufferPointer(), blob->GetBufferSize(), __uuidof(ID3D12RootSignature),
                                     reinterpret_cast<void**>(&root));
    blob->Release();
    check(SUCCEEDED(hr) && root, "CreateRootSignature hr=0x%08lx", ul(hr));
    return root;
}

ID3D12Resource* make_buffer(ID3D12Device* device, D3D12_HEAP_TYPE heap, D3D12_RESOURCE_FLAGS flags,
                            D3D12_RESOURCE_STATES state) {
    D3D12_HEAP_PROPERTIES props = {};
    props.Type = heap;
    D3D12_RESOURCE_DESC desc = {};
    desc.Dimension = D3D12_RESOURCE_DIMENSION_BUFFER;
    desc.Width = kWords * 4;
    desc.Height = 1;
    desc.DepthOrArraySize = 1;
    desc.MipLevels = 1;
    desc.SampleDesc.Count = 1;
    desc.Layout = D3D12_TEXTURE_LAYOUT_ROW_MAJOR;
    desc.Flags = flags;
    ID3D12Resource* buffer = nullptr;
    const HRESULT hr = device->CreateCommittedResource(&props, D3D12_HEAP_FLAG_NONE, &desc, state, nullptr,
                                                       __uuidof(ID3D12Resource), reinterpret_cast<void**>(&buffer));
    check(SUCCEEDED(hr) && buffer, "CreateCommittedResource heap=%d hr=0x%08lx", static_cast<int>(heap), ul(hr));
    return buffer;
}

HRESULT create_compute(ID3D12Device* device, ID3D12RootSignature* root, const void* code, size_t size,
                       ID3D12PipelineState** pso) {
    D3D12_COMPUTE_PIPELINE_STATE_DESC desc = {};
    desc.pRootSignature = root;
    desc.CS.pShaderBytecode = code;
    desc.CS.BytecodeLength = size;
    *pso = nullptr;
    return device->CreateComputePipelineState(&desc, __uuidof(ID3D12PipelineState), reinterpret_cast<void**>(pso));
}

/* Runs fill_cs66 and checks every output word. */
void run_dxil(ID3D12Device* device, ID3D12RootSignature* root, ID3D12PipelineState* pso) {
    D3D12_COMMAND_QUEUE_DESC queue_desc = {};
    queue_desc.Type = D3D12_COMMAND_LIST_TYPE_DIRECT;
    ID3D12CommandQueue* queue = nullptr;
    ID3D12CommandAllocator* allocator = nullptr;
    ID3D12GraphicsCommandList* list = nullptr;
    ID3D12Fence* fence = nullptr;
    HRESULT hr = device->CreateCommandQueue(&queue_desc, __uuidof(ID3D12CommandQueue), reinterpret_cast<void**>(&queue));
    if (!check(SUCCEEDED(hr), "CreateCommandQueue hr=0x%08lx", ul(hr))) return;
    hr = device->CreateCommandAllocator(D3D12_COMMAND_LIST_TYPE_DIRECT, __uuidof(ID3D12CommandAllocator),
                                        reinterpret_cast<void**>(&allocator));
    if (check(SUCCEEDED(hr), "CreateCommandAllocator hr=0x%08lx", ul(hr))) {
        hr = device->CreateCommandList(0, D3D12_COMMAND_LIST_TYPE_DIRECT, allocator, pso,
                                       __uuidof(ID3D12GraphicsCommandList), reinterpret_cast<void**>(&list));
        check(SUCCEEDED(hr), "CreateCommandList hr=0x%08lx", ul(hr));
    }
    hr = device->CreateFence(0, D3D12_FENCE_FLAG_NONE, __uuidof(ID3D12Fence), reinterpret_cast<void**>(&fence));
    check(SUCCEEDED(hr), "CreateFence hr=0x%08lx", ul(hr));
    ID3D12Resource* out = make_buffer(device, D3D12_HEAP_TYPE_DEFAULT, D3D12_RESOURCE_FLAG_ALLOW_UNORDERED_ACCESS,
                                      D3D12_RESOURCE_STATE_UNORDERED_ACCESS);
    ID3D12Resource* readback =
        make_buffer(device, D3D12_HEAP_TYPE_READBACK, D3D12_RESOURCE_FLAG_NONE, D3D12_RESOURCE_STATE_COPY_DEST);
    if (list && fence && out && readback) {
        list->SetComputeRootSignature(root);
        list->SetComputeRootUnorderedAccessView(0, out->GetGPUVirtualAddress());
        list->Dispatch(kWords / 64, 1, 1);
        D3D12_RESOURCE_BARRIER barrier = {};
        barrier.Type = D3D12_RESOURCE_BARRIER_TYPE_TRANSITION;
        barrier.Transition.pResource = out;
        barrier.Transition.Subresource = D3D12_RESOURCE_BARRIER_ALL_SUBRESOURCES;
        barrier.Transition.StateBefore = D3D12_RESOURCE_STATE_UNORDERED_ACCESS;
        barrier.Transition.StateAfter = D3D12_RESOURCE_STATE_COPY_SOURCE;
        list->ResourceBarrier(1, &barrier);
        list->CopyResource(readback, out);
        hr = list->Close();
        check(SUCCEEDED(hr), "Close hr=0x%08lx", ul(hr));
        ID3D12CommandList* lists[] = {list};
        queue->ExecuteCommandLists(1, lists);
        HANDLE done = CreateEventW(nullptr, FALSE, FALSE, nullptr);
        check(SUCCEEDED(queue->Signal(fence, 1)), "Signal");
        if (fence->GetCompletedValue() < 1) {
            check(SUCCEEDED(fence->SetEventOnCompletion(1, done)), "SetEventOnCompletion");
            check(WaitForSingleObject(done, 10000) == WAIT_OBJECT_0, "fence did not complete within 10 s");
        }
        CloseHandle(done);
        check(fence->GetCompletedValue() != UINT64_MAX, "device removed (fence value UINT64_MAX)");

        void* mapped = nullptr;
        D3D12_RANGE range = {0, kWords * 4};
        hr = readback->Map(0, &range, &mapped);
        if (check(SUCCEEDED(hr) && mapped, "Map hr=0x%08lx", ul(hr))) {
            const auto* words = static_cast<const uint32_t*>(mapped);
            uint32_t mismatched = 0, first = kWords;
            for (uint32_t i = 0; i < kWords; ++i) {
                if (words[i] != expected_word(i)) {
                    if (!mismatched) first = i;
                    ++mismatched;
                }
            }
            check(mismatched == 0, "DXIL readback: %u of %u words wrong (first %u: got 0x%08x want 0x%08x)",
                  mismatched, kWords, first, first < kWords ? words[first] : 0u,
                  first < kWords ? expected_word(first) : 0u);
            D3D12_RANGE none = {0, 0};
            readback->Unmap(0, &none);
            if (!mismatched) std::printf("dxil: %u/%u words verified\n", kWords, kWords);
        }
    }
    if (readback) readback->Release();
    if (out) out->Release();
    if (fence) fence->Release();
    if (list) list->Release();
    if (allocator) allocator->Release();
    if (queue) queue->Release();
}

std::wstring directory_of(const wchar_t* path) {
    const std::wstring text(path);
    return text.substr(0, text.find_last_of(L'\\'));
}

/* --no-bridge: run from a directory that holds d3d12_xs.dll and its BetterXen imports but no
 * bx_shader_bridge.dll. DXIL must still run and ACCC-only must still be refused. */
int no_bridge_main() {
    ID3D12Device* device = nullptr;
    HRESULT hr = D3D12XboxCreateDevice(nullptr, __uuidof(ID3D12Device), reinterpret_cast<void**>(&device));
    if (!check(SUCCEEDED(hr) && device, "no-bridge: D3D12XboxCreateDevice hr=0x%08lx", ul(hr))) return finish();
    D3D12_ROOT_PARAMETER uav = {};
    uav.ParameterType = D3D12_ROOT_PARAMETER_TYPE_UAV;
    uav.ShaderVisibility = D3D12_SHADER_VISIBILITY_ALL;
    ID3D12RootSignature* root = make_root(device, uav);
    if (!root) return finish();
    ID3D12PipelineState* pso = nullptr;
    hr = create_compute(device, root, g_fill_cs66, sizeof(g_fill_cs66), &pso);
    check(SUCCEEDED(hr) && pso, "no-bridge: DXIL CreateComputePipelineState hr=0x%08lx", ul(hr));
    if (pso) {
        run_dxil(device, root, pso);
        pso->Release();
    }
    check(GetModuleHandleW(L"bx_shader_bridge.dll") == nullptr, "no-bridge: a bx_shader_bridge.dll was loaded anyway");
    const std::vector<uint8_t> accc = accc_only_container();
    hr = create_compute(device, root, accc.data(), accc.size(), &pso);
    check(hr == E_NOTIMPL && !pso, "no-bridge: ACCC-only compute hr=0x%08lx (want E_NOTIMPL)", ul(hr));
    if (pso) pso->Release();
    D3D12_GRAPHICS_PIPELINE_STATE_DESC graphics = {};
    graphics.pRootSignature = root;
    graphics.VS.pShaderBytecode = accc.data();
    graphics.VS.BytecodeLength = accc.size();
    graphics.SampleMask = UINT_MAX;
    graphics.PrimitiveTopologyType = D3D12_PRIMITIVE_TOPOLOGY_TYPE_TRIANGLE;
    graphics.SampleDesc.Count = 1;
    hr = device->CreateGraphicsPipelineState(&graphics, __uuidof(ID3D12PipelineState), reinterpret_cast<void**>(&pso));
    check(hr == E_NOTIMPL && !pso, "no-bridge: ACCC-only graphics VS hr=0x%08lx (want E_NOTIMPL)", ul(hr));
    if (pso) pso->Release();
    root->Release();
    device->Release();
    return finish();
}

/* Copies this EXE and d3d12_xs.dll's directory minus the bridge into a scratch directory, runs
 * the copy with --no-bridge, and requires it to pass. */
void run_no_bridge_child() {
    wchar_t exe[MAX_PATH] = {}, shim[MAX_PATH] = {}, temp[MAX_PATH] = {};
    GetModuleFileNameW(nullptr, exe, MAX_PATH);
    GetModuleFileNameW(GetModuleHandleW(L"d3d12_xs.dll"), shim, MAX_PATH);
    GetTempPathW(MAX_PATH, temp);
    wchar_t dir[MAX_PATH];
    std::swprintf(dir, MAX_PATH, L"%lsbx_shader_gate_nobridge_%lu", temp, GetCurrentProcessId());
    if (!check(CreateDirectoryW(dir, nullptr) || GetLastError() == ERROR_ALREADY_EXISTS,
               "no-bridge: cannot create %ls", dir)) {
        return;
    }
    const std::wstring from = directory_of(shim);
    const std::wstring to(dir);
    const wchar_t* files[] = {L"d3d12_xs.dll", L"xmem.dll", L"bx_runtime_veh.dll"};
    bool copied = CopyFileW(exe, (to + L"\\test_shader_gate.exe").c_str(), FALSE) != 0;
    for (const wchar_t* name : files) {
        copied = copied && CopyFileW((from + L"\\" + name).c_str(), (to + L"\\" + name).c_str(), FALSE) != 0;
    }
    if (check(copied, "no-bridge: copying the shims to %ls failed (error %lu)", dir, GetLastError())) {
        std::wstring command = L"\"" + to + L"\\test_shader_gate.exe\" --no-bridge";
        STARTUPINFOW startup = {};
        startup.cb = sizeof(startup);
        PROCESS_INFORMATION child = {};
        if (check(CreateProcessW(nullptr, command.data(), nullptr, nullptr, FALSE, 0, nullptr, dir, &startup, &child),
                  "no-bridge: CreateProcessW error %lu", GetLastError())) {
            DWORD code = 99;
            check(WaitForSingleObject(child.hProcess, 60000) == WAIT_OBJECT_0, "no-bridge: child did not finish in 60 s");
            GetExitCodeProcess(child.hProcess, &code);
            check(code == 0, "no-bridge: child exited %lu (a crash shows as an exception code)", code);
            CloseHandle(child.hThread);
            CloseHandle(child.hProcess);
        }
    }
    DeleteFileW((to + L"\\test_shader_gate.exe").c_str());
    for (const wchar_t* name : files) DeleteFileW((to + L"\\" + name).c_str());
    RemoveDirectoryW(dir);
}

} // namespace

int main() {
    if (std::wcsstr(GetCommandLineW(), L" --no-bridge")) return no_bridge_main();

    // Self-check of the reference against values computed independently for d3dmetal-harness.
    check(expected_word(0) == 0x01234567u && expected_word(1) == 0x9f598b46u && expected_word(255) == 0x9a664b32u,
          "expected_word reference values");

    if (!check(GetEnvironmentVariableA("BX_LOG_FILE", nullptr, 0) > 0,
               "BX_LOG_FILE is not set; run through bx-cli run --log")) {
        return finish();
    }

    ID3D12Device* device = nullptr;
    HRESULT hr = D3D12XboxCreateDevice(nullptr, __uuidof(ID3D12Device), reinterpret_cast<void**>(&device));
    if (!check(SUCCEEDED(hr) && device, "D3D12XboxCreateDevice hr=0x%08lx", ul(hr))) return finish();

    D3D12_ROOT_PARAMETER uav = {};
    uav.ParameterType = D3D12_ROOT_PARAMETER_TYPE_UAV;
    uav.ShaderVisibility = D3D12_SHADER_VISIBILITY_ALL;
    ID3D12RootSignature* root_uav = make_root(device, uav);
    D3D12_DESCRIPTOR_RANGE range = {};
    range.RangeType = D3D12_DESCRIPTOR_RANGE_TYPE_UAV;
    range.NumDescriptors = 1;
    D3D12_ROOT_PARAMETER table = {};
    table.ParameterType = D3D12_ROOT_PARAMETER_TYPE_DESCRIPTOR_TABLE;
    table.DescriptorTable.NumDescriptorRanges = 1;
    table.DescriptorTable.pDescriptorRanges = &range;
    table.ShaderVisibility = D3D12_SHADER_VISIBILITY_ALL;
    ID3D12RootSignature* root_table = make_root(device, table);
    if (!root_uav || !root_table) return finish();

    // 1. DXIL passes the gate and runs.
    ID3D12PipelineState* pso = nullptr;
    hr = create_compute(device, root_uav, g_fill_cs66, sizeof(g_fill_cs66), &pso);
    check(SUCCEEDED(hr) && pso, "DXIL SM 6.6 CreateComputePipelineState hr=0x%08lx", ul(hr));
    const HMODULE bridge = GetModuleHandleW(L"bx_shader_bridge.dll");
    if (!check(bridge != nullptr,
               "bx_shader_bridge.dll was not loaded beside d3d12_xs.dll: scripts/build-shims.sh stages it in "
               "build-shims/bin (#484)")) {
        if (pso) pso->Release();
        return finish();
    }
    // The shim must find the bridge beside itself, not through the EXE's directory (crate README).
    wchar_t bridge_path[MAX_PATH] = {}, shim_path[MAX_PATH] = {};
    GetModuleFileNameW(bridge, bridge_path, MAX_PATH);
    GetModuleFileNameW(GetModuleHandleW(L"d3d12_xs.dll"), shim_path, MAX_PATH);
    const auto dir_of = [](std::wstring path) { return path.substr(0, path.find_last_of(L'\\')); };
    check(_wcsicmp(dir_of(bridge_path).c_str(), dir_of(shim_path).c_str()) == 0,
          "bx_shader_bridge.dll was loaded from %ls, not beside d3d12_xs.dll (%ls)", bridge_path, shim_path);
    if (pso) {
        run_dxil(device, root_uav, pso);
        pso->Release();
    }

    // 2. An ACCC-only container is refused, for compute and graphics, without a host object.
    const std::vector<uint8_t> accc = accc_only_container();
    hr = create_compute(device, root_uav, accc.data(), accc.size(), &pso);
    check(hr == E_NOTIMPL && !pso, "ACCC-only compute PSO: hr=0x%08lx (want E_NOTIMPL) pso=%p", ul(hr),
          static_cast<void*>(pso));
    if (pso) pso->Release();
    D3D12_GRAPHICS_PIPELINE_STATE_DESC graphics = {};
    graphics.pRootSignature = root_uav;
    graphics.VS.pShaderBytecode = accc.data();
    graphics.VS.BytecodeLength = accc.size();
    graphics.SampleMask = UINT_MAX;
    graphics.PrimitiveTopologyType = D3D12_PRIMITIVE_TOPOLOGY_TYPE_TRIANGLE;
    graphics.SampleDesc.Count = 1;
    hr = device->CreateGraphicsPipelineState(&graphics, __uuidof(ID3D12PipelineState), reinterpret_cast<void**>(&pso));
    check(hr == E_NOTIMPL && !pso, "ACCC-only graphics VS: hr=0x%08lx (want E_NOTIMPL) pso=%p", ul(hr),
          static_cast<void*>(pso));
    if (pso) pso->Release();
    // The device is still usable after a refusal.
    hr = create_compute(device, root_uav, g_fill_cs66, sizeof(g_fill_cs66), &pso);
    check(SUCCEEDED(hr) && pso, "DXIL PSO after a refusal hr=0x%08lx", ul(hr));
    if (pso) pso->Release();

    // 3. Constraint-carrying DXIL still passes; its constraints are logged (results not dispatched).
    // A wave wider than 32 lanes is refused at creation (#591): D3DMetal creates the PSO
    // and never runs it, so an S_OK here would be a silent no-op. The fixture asserts the
    // failure rather than only reading the log, because the log alone cannot tell a caller
    // what happened.
    hr = create_compute(device, root_uav, g_wave64_lanes_cs, sizeof(g_wave64_lanes_cs), &pso);
    check(hr == E_NOTIMPL && !pso,
          "wave64 PSO is refused at creation: hr=0x%08lx (want E_NOTIMPL) pso=%p", ul(hr),
          static_cast<void*>(pso));
    if (pso) pso->Release();

    hr = create_compute(device, root_table, g_atomic64_image_cs, sizeof(g_atomic64_image_cs), &pso);
    std::printf("atomic64 typed PSO hr=0x%08lx\n", ul(hr));
    if (pso) pso->Release();

    // 4. Without the bridge (a separate process from a directory without it).
    run_no_bridge_child();

    // 5. The JSONL records.
    const std::string log = read_log();
    char fill_bytes[32];
    std::snprintf(fill_bytes, sizeof(fill_bytes), "bytes=%u ", static_cast<unsigned>(sizeof(g_fill_cs66)));
    check(log_has(log, {"\"event\":\"shader_gate\"", "pso=compute", "decision=pass_dxil", "kind=dxil", "sm=6.6",
                        fill_bytes, "atomic64= ", "wave_lanes=0-4294967295"}),
          "shader_gate record for the DXIL SM 6.6 compute shader");
    check(log_has(log, {"\"event\":\"shader_gate\"", "pso=compute", "decision=reject_accc_only", "hr=0x80004001",
                        "kind=accc_only", "sm=6.0"}),
          "shader_gate record rejecting the ACCC-only compute shader");
    check(log_has(log, {"\"event\":\"shader_gate\"", "pso=graphics", "stage=VS", "decision=reject_accc_only"}),
          "shader_gate record rejecting the ACCC-only vertex shader");
    check(log_has(log, {"\"event\":\"shader_gate\"", "decision=reject_wave_wide", "hr=0x80004001"}),
          "shader_gate record refusing the wave64 stage");
    check(log_has(log, {"\"event\":\"shader_gate\"", "decision=pass_dxil", "wave_ops=1", "wave_lanes=64-64"}),
          "shader_gate record with WaveSize(64)");
    check(log_has(log, {"\"event\":\"shader_d3dmetal_gap\"", "wave_lanes_min=64"}),
          "shader_d3dmetal_gap record for WaveSize(64)");
    check(log_has(log, {"\"event\":\"shader_gate\"", "decision=pass_dxil", "int64_ops=1", "atomic64=typed,"}),
          "shader_gate record with 64-bit typed atomics");
    check(log_has(log, {"\"event\":\"shader_d3dmetal_gap\"", "atomic64_flags=0x400000"}),
          "shader_d3dmetal_gap record for 64-bit typed atomics");
    check(log_has(log, {"\"event\":\"shader_bridge_missing\"", "not found beside d3d12_xs.dll"}),
          "shader_bridge_missing record from the no-bridge child");
    check(log_has(log, {"\"event\":\"shader_gate\"", "pso=compute", "decision=reject_accc_only_unclassified",
                        "hr=0x80004001"}),
          "no-bridge child refused the ACCC-only compute shader");
    check(log_has(log, {"\"event\":\"shader_gate\"", "pso=graphics", "decision=reject_accc_only_unclassified"}),
          "no-bridge child refused the ACCC-only vertex shader");
    check(log_has(log, {"\"event\":\"shader_gate\"", "pso=compute", "decision=pass_unclassified"}),
          "no-bridge child passed DXIL unclassified");

    root_table->Release();
    root_uav->Release();
    device->Release();
    return finish();
}

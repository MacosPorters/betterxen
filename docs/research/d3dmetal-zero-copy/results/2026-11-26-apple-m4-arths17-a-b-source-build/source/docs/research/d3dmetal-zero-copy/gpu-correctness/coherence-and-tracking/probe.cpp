// SPDX-License-Identifier: GPL-3.0-or-later
#include "../../copy-detection/support.h"
#include <d3dcompiler.h>
#include <algorithm>
#include <array>
#include <vector>

constexpr UINT capacity = 4 * 1024 * 1024;
constexpr UINT words = capacity / 4;
constexpr UINT poison = 0x6ba8fa03u;

static void watch() {
    constexpr UINT size = 65536;
    auto* ptr = static_cast<unsigned char*>(VirtualAlloc(nullptr, size,
                        MEM_RESERVE | MEM_COMMIT | MEM_WRITE_WATCH, PAGE_READWRITE));
    if (!ptr) throw std::runtime_error("VirtualAlloc MEM_WRITE_WATCH");
    SYSTEM_INFO system{};
    GetSystemInfo(&system);
    std::printf("WATCH guest_page=%lu allocation_granularity=%lu cpu_mod16k=%llu\n",
                system.dwPageSize, system.dwAllocationGranularity,
                UINT64(reinterpret_cast<uintptr_t>(ptr) % 16384));
    bool exact = true;
    auto expect = [&](const char* label, DWORD flags, const std::vector<UINT>& expected) {
        std::array<void*, 64> addresses{};
        ULONG_PTR count = addresses.size();
        DWORD granularity = 0;
        const UINT status = GetWriteWatch(flags, ptr, size, addresses.data(), &count, &granularity);
        std::vector<UINT> actual;
        if (!status && count <= addresses.size())
            for (ULONG_PTR i = 0; i < count; ++i)
                actual.push_back(UINT(static_cast<unsigned char*>(addresses[i]) - ptr));
        std::sort(actual.begin(), actual.end());
        std::printf("WATCH phase=%s rc=%u granularity=%lu count=%llu actual=",
                    label, status, granularity, UINT64(count));
        for (const auto offset : actual) std::printf("%u,", offset);
        std::printf(" expected=");
        for (const auto offset : expected) std::printf("%u,", offset);
        std::puts("");
        if (status || count > addresses.size() || granularity != 4096 || actual != expected)
            exact = false;
    };
    auto* mapped = static_cast<volatile unsigned char*>(ptr);
    expect("fresh", 0, {});
    mapped[4096] = 1;
    expect("only_second_guest_page", 0, {4096});
    expect("query_without_reset", 0, {4096});
    mapped[8192] = 2;
    expect("second_and_third", WRITE_WATCH_FLAG_RESET, {4096, 8192});
    expect("after_query_reset", 0, {});
    mapped[4096] = 3;
    expect("same_page_rearmed", WRITE_WATCH_FLAG_RESET, {4096});
    mapped[16383] = 4;
    mapped[16384] = 5;
    expect("host_boundary", 0, {12288, 16384});
    const UINT reset = ResetWriteWatch(ptr, size);
    std::printf("WATCH ResetWriteWatch rc=%u\n", reset);
    if (reset) throw std::runtime_error("ResetWriteWatch");
    expect("after_explicit_reset", 0, {});
    const auto read = mapped[16384];
    std::printf("WATCH read_only value=%u\n", UINT(read));
    expect("after_read_only", 0, {});
    mapped[16384] = 5;
    expect("same_value_store_after_reset", WRITE_WATCH_FLAG_RESET, {16384});
    expect("final_empty", 0, {});
    if (!VirtualFree(ptr, 0, MEM_RELEASE)) throw std::runtime_error("VirtualFree");
    if (!exact) throw std::runtime_error("watch page oracle");
}

static void wrap_watch(Device& d) {
    constexpr UINT size = 65536;
    Com<ID3D12Device3> dev;
    check(d.device->QueryInterface(__uuidof(ID3D12Device3), dev.out()), "ID3D12Device3");
    void* pointer = VirtualAlloc(nullptr, size, MEM_RESERVE | MEM_COMMIT | MEM_WRITE_WATCH, PAGE_READWRITE);
    if (!pointer) throw std::runtime_error("VirtualAlloc wrapper control");
    HRESULT hr;
    {
        Com<ID3D12Heap> heap;
        hr = dev->OpenExistingHeapFromAddress(pointer, __uuidof(ID3D12Heap), heap.out());
        std::printf("WATCH_WRAP hr=%08lx object=%d gpu_writes=NOT_RUN\n",
                    static_cast<unsigned long>(hr), heap.get() != nullptr);
    }
    if (!VirtualFree(pointer, 0, MEM_RELEASE)) throw std::runtime_error("VirtualFree");
    if (SUCCEEDED(hr)) throw std::runtime_error("wrapper available; GPU tracking remains untested");
}

struct Pipeline {
    Com<ID3D12RootSignature> root;
    Com<ID3D12PipelineState> pso;
    explicit Pipeline(Device& d) {
        auto* compiler = LoadLibraryW(L"d3dcompiler_47.dll");
        if (!compiler) throw std::runtime_error("D3DCompiler 47");
        char path[MAX_PATH]{};
        GetModuleFileNameA(compiler, path, MAX_PATH);
        std::printf("SHADER compiler=%s profile=cs_5_0 flags=O3\n", path);
        FILE* file = std::fopen("access.hlsl", "rb");
        if (!file) throw std::runtime_error("open HLSL");
        std::array<char, 8192> hlsl{};
        const size_t length = std::fread(hlsl.data(), 1, hlsl.size(), file);
        std::fclose(file);
        if (!length || length == hlsl.size()) throw std::runtime_error("HLSL length");
        auto compile = reinterpret_cast<decltype(&D3DCompile)>(
            reinterpret_cast<void*>(GetProcAddress(compiler, "D3DCompile")));
        auto serialize = reinterpret_cast<decltype(&D3D12SerializeRootSignature)>(
            reinterpret_cast<void*>(GetProcAddress(d.dll, "D3D12SerializeRootSignature")));
        if (!compile || !serialize) throw std::runtime_error("shader tool symbols");
        Com<ID3DBlob> shader, errors;
        const auto hr = compile(hlsl.data(), length, "access.hlsl", nullptr, nullptr,
                                "main", "cs_5_0", D3DCOMPILE_OPTIMIZATION_LEVEL3, 0,
                                reinterpret_cast<ID3DBlob**>(shader.out()),
                                reinterpret_cast<ID3DBlob**>(errors.out()));
        if (errors.get()) std::fwrite(errors->GetBufferPointer(), 1, errors->GetBufferSize(), stderr);
        check(hr, "D3DCompile");
        file = std::fopen("access.dxbc", "wb");
        if (!file) throw std::runtime_error("open shader receipt");
        const size_t written = std::fwrite(shader->GetBufferPointer(), 1, shader->GetBufferSize(), file);
        if (std::fclose(file) || written != shader->GetBufferSize())
            throw std::runtime_error("write shader receipt");
        D3D12_ROOT_PARAMETER params[3]{};
        params[0].ParameterType = D3D12_ROOT_PARAMETER_TYPE_SRV;
        params[1].ParameterType = D3D12_ROOT_PARAMETER_TYPE_UAV;
        params[2].ParameterType = D3D12_ROOT_PARAMETER_TYPE_32BIT_CONSTANTS;
        params[2].Constants.Num32BitValues = 4;
        D3D12_ROOT_SIGNATURE_DESC signature{3, params, 0, nullptr, D3D12_ROOT_SIGNATURE_FLAG_NONE};
        Com<ID3DBlob> blob, root_errors;
        check(serialize(&signature, D3D_ROOT_SIGNATURE_VERSION_1,
                        reinterpret_cast<ID3DBlob**>(blob.out()),
                        reinterpret_cast<ID3DBlob**>(root_errors.out())), "serialize root");
        check(d.device->CreateRootSignature(0, blob->GetBufferPointer(), blob->GetBufferSize(),
                                           __uuidof(ID3D12RootSignature), root.out()), "root signature");
        D3D12_COMPUTE_PIPELINE_STATE_DESC desc{};
        desc.pRootSignature = root.get();
        desc.CS = {shader->GetBufferPointer(), shader->GetBufferSize()};
        check(d.device->CreateComputePipelineState(&desc, __uuidof(ID3D12PipelineState), pso.out()),
              "compute PSO");
    }
};

static uint32_t input_word(UINT index, UINT seed) {
    return (index ^ (seed * 0x01010301u)) + index * 17u;
}

static void compute(Device& d, Heap input_kind, Heap output_kind) {
    using State = D3D12_RESOURCE_STATES;
    constexpr State common = D3D12_RESOURCE_STATE_COMMON;
    constexpr State generic = D3D12_RESOURCE_STATE_GENERIC_READ;
    constexpr State uav = D3D12_RESOURCE_STATE_UNORDERED_ACCESS;
    constexpr State copy_dest = D3D12_RESOURCE_STATE_COPY_DEST;
    constexpr State copy_src = D3D12_RESOURCE_STATE_COPY_SOURCE;

    Buffer source(d, input_kind, capacity, input_kind == Heap::upload ? generic : common);
    Buffer target(d, output_kind, capacity, common, D3D12_RESOURCE_FLAG_ALLOW_UNORDERED_ACCESS);
    Buffer readback(d, Heap::readback, capacity, copy_dest);
    Buffer stage(d, Heap::upload, capacity, generic);
    const bool direct_output = output_kind != Heap::gpu;
    auto* cpu_source = input_kind == Heap::gpu ? stage.map() : source.map();
    auto* cpu_result = direct_output ? target.map() : readback.map();
    auto* stage_data = input_kind == Heap::gpu ? cpu_source : stage.map();
    Pipeline shader(d);
    Queue q(d);
    Timestamp timing(d, q);

    struct Work { const char* name; UINT outputs, stride, reads; bool omit; };
    for (const auto work : {Work{"both", words, 1, 1, false},
                            Work{"full_read", words / 16, 1, 16, false},
                            Work{"sparse_read_4k", words / 1024, 1024, 1, false},
                            Work{"sparse_read_16k", words / 4096, 4096, 1, false},
                            Work{"write", words, 1, 0, false},
                            Work{"omitted", words, 1, 1, true}}) {
        for (int iteration = -3; iteration < 12; ++iteration) {
            const UINT seed = 407 + UINT(iteration + 3) * 13u;
            for (UINT j = 0; j < words; ++j) cpu_source[j] = input_word(j, seed);
            if (direct_output) std::fill_n(cpu_result, work.outputs, poison);
            _mm_sfence();

            // DEFAULT controls stage data and poison in a separate, fenced list.
            if (input_kind == Heap::gpu) {
                q.begin();
                q.list->CopyBufferRegion(source.resource.get(), 0, stage.resource.get(), 0, capacity);
                q.close();
                q.execute();
            }
            if (!direct_output) {
                std::fill_n(stage_data, work.outputs, poison);
                _mm_sfence();
                q.begin();
                q.list->CopyBufferRegion(target.resource.get(), 0, stage.resource.get(), 0,
                                         work.outputs * 4u);
                q.close();
                q.execute();
            }
            const auto start = ticks();
            q.begin();
            q.transition(target.resource.get(), common, uav);
            q.list->SetPipelineState(shader.pso.get());
            q.list->SetComputeRootSignature(shader.root.get());
            q.list->SetComputeRootShaderResourceView(0, source.resource->GetGPUVirtualAddress());
            q.list->SetComputeRootUnorderedAccessView(1, target.resource->GetGPUVirtualAddress());
            const UINT constants[] = {work.outputs, work.stride, work.reads, seed};
            q.list->SetComputeRoot32BitConstants(2, 4, constants, 0);
            timing.start(q);
            if (!work.omit) q.list->Dispatch((work.outputs + 63) / 64, 1, 1);
            timing.end(q);
            if (!direct_output) {
                q.transition(target.resource.get(), uav, copy_src);
                q.list->CopyBufferRegion(readback.resource.get(), 0, target.resource.get(), 0,
                                         work.outputs * 4u);
                q.transition(target.resource.get(), copy_src, common);
            } else {
                q.transition(target.resource.get(), uav, common);
            }
            q.close();
            const auto encoded = ticks();
            const auto latency = q.execute();
            UINT matches = 0;
            uint32_t first_expected = 0, last_expected = 0;
            for (UINT j = 0; j < work.outputs; ++j) {
                // Reconstruct the conceptual indices and input values without
                // consulting either mapped buffer. Use a widened sum and reduce
                // modulo 2^32 once to model unsigned shader overflow.
                uint64_t sum = uint64_t(j) * 37 + seed;
                for (UINT n = 0; n < work.reads; ++n)
                    sum += input_word(work.stride * (j + n * work.outputs), seed);
                const uint32_t expected = work.omit ? poison : uint32_t(sum) ^ 0xa5c35a3cu;
                if (!j) first_expected = expected;
                if (j == work.outputs - 1) last_expected = expected;
                if (matches == j && cpu_result[j] != expected)
                    std::printf("MISMATCH index=%u actual=%08x expected=%08x\n", j, cpu_result[j], expected);
                matches += cpu_result[j] == expected;
            }
            std::printf("COMPUTE mode=%s iter=%d seed=%u direct_out=%d read_bytes=%u written_bytes=%u "
                        "encode_us=%.3f execute_us=%.3f signal_us=%.3f wait_us=%.3f completion_us=%.3f gpu_us=%.3f "
                        "expected=%08x,%08x actual=%08x,%08x matches=%u/%u\n",
                        work.name, iteration, seed, direct_output,
                        work.omit ? 0 : work.outputs * work.reads * 4u,
                        work.omit ? 0 : work.outputs * 4u, us(encoded - start),
                        latency.execute, latency.signal, latency.wait, latency.total, timing.elapsed(),
                        first_expected, last_expected, cpu_result[0], cpu_result[work.outputs - 1],
                        matches, work.outputs);
            if (matches != work.outputs) throw std::runtime_error("compute or poison oracle");
        }
    }
}

int main(int argc, char** argv) {
    setvbuf(stdout, nullptr, _IONBF, 0);
    try {
        if (argc != 2) throw std::runtime_error("usage: probe watch|wrap-watch|wb|wc|upload-wb|default");
        const std::string mode = argv[1];
        if (mode == "watch") {
            watch();
        } else {
            Device device;
            receipt_pause();
            if (mode == "wrap-watch") wrap_watch(device);
            else if (mode == "wb") compute(device, Heap::wb, Heap::wb);
            else if (mode == "wc") compute(device, Heap::wc, Heap::wc);
            else if (mode == "upload-wb") compute(device, Heap::upload, Heap::wb);
            else if (mode == "default") compute(device, Heap::gpu, Heap::gpu);
            else throw std::runtime_error("unknown mode");
        }
        std::puts("END complete");
        return 0;
    } catch (const std::exception& e) {
        std::fprintf(stderr, "END failure: %s\n", e.what());
        return 1;
    }
}

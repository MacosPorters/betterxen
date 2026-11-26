// SPDX-License-Identifier: GPL-3.0-or-later
#include "support.h"

#include <algorithm>
#include <bit>
#include <numeric>
#include <random>
#include <vector>

constexpr UINT MiB = 1024 * 1024;
constexpr UINT placement = D3D12_DEFAULT_RESOURCE_PLACEMENT_ALIGNMENT;

static void placement_probe(Device& d) {
    const auto desc = buffer_desc(placement, D3D12_RESOURCE_FLAG_NONE);
    const auto alloc = d.device->GetResourceAllocationInfo(0, 1, &desc);
    for (const auto& [heap_size, offset] : {
            std::pair{UINT64(placement), 0ull}, std::pair{UINT64(placement), 1ull},
            std::pair{UINT64(placement), UINT64(placement)}, std::pair{UINT64(2 * placement), 1ull}}) {
        auto heap = d.heap(Heap::wb, heap_size);
        Com<ID3D12Resource> resource;
        const HRESULT hr = d.device->CreatePlacedResource(heap.get(), offset, &desc,
                  D3D12_RESOURCE_STATE_COMMON, nullptr, __uuidof(ID3D12Resource), resource.out());
        const bool aligned = alloc.Alignment && offset % alloc.Alignment == 0;
        const bool in_bounds = offset <= heap_size && alloc.SizeInBytes <= heap_size - offset;
        const bool legal = aligned && in_bounds;
        std::printf("PLACEMENT offset=%llu heap=%llu size=%llu alignment=%llu aligned=%d in_bounds=%d eligible=%d hr=%08lx object=%d\n",
                    offset, heap_size, alloc.SizeInBytes, alloc.Alignment, aligned, in_bounds, legal,
                    static_cast<unsigned long>(hr), resource.get() != nullptr);
        // Creation-only negatives: no Map, GPU VA lookup, command recording, or submission.
        if (!legal) continue;
        check(hr, "valid placement");
        void* pointer = nullptr;
        check(resource->Map(0, nullptr, &pointer), "valid placement Map");
        if (!pointer) throw std::runtime_error("valid placement Map returned null");
        const auto addr = reinterpret_cast<uintptr_t>(pointer);
        std::printf("MAP heap=wb size=%u cpu_mod16k=%llu cpu_mod64k=%llu gpu_mod64k=%llu\n",
                    placement, UINT64(addr % 16384), UINT64(addr % placement),
                    resource->GetGPUVirtualAddress() % placement);
        resource->Unmap(0, nullptr);
    }
    for (const auto& [kind, name] : {std::pair{Heap::wb, "wb"}, {Heap::wc, "wc"}, {Heap::upload, "upload"}}) {
        Buffer buffer(d, kind, 4 * MiB, kind == Heap::upload
                      ? D3D12_RESOURCE_STATE_GENERIC_READ : D3D12_RESOURCE_STATE_COMMON);
        const auto addr = reinterpret_cast<uintptr_t>(buffer.map());
        std::printf("MAP heap=%s size=%u cpu_mod16k=%llu cpu_mod64k=%llu gpu_mod64k=%llu\n",
                    name, 4 * MiB, UINT64(addr % 16384), UINT64(addr % placement),
                    buffer.resource->GetGPUVirtualAddress() % placement);
    }
    void* ptr = VirtualAlloc(nullptr, placement, MEM_RESERVE | MEM_COMMIT, PAGE_READWRITE);
    if (!ptr) throw std::runtime_error("VirtualAlloc alignment control");
    std::printf("MAP heap=virtualalloc size=%u cpu_mod64k=%llu\n",
                placement, UINT64(reinterpret_cast<uintptr_t>(ptr) % placement));
    if (!VirtualFree(ptr, 0, MEM_RELEASE)) throw std::runtime_error("VirtualFree");
}

static void clock_probe(Device& d) {
    Queue queue(d);
    Timestamp stamp(d, queue);
    const UINT64 poison[] = {0x9cde12154b320001ull, 0x9cde12154b320002ull};
    std::memcpy(stamp.output.pointer, poison, sizeof(poison));
    _mm_sfence();

    const auto start = ticks();
    queue.begin();
    stamp.start(queue);
    queue.list->ResolveQueryData(stamp.queries.get(), D3D12_QUERY_TYPE_TIMESTAMP, 0, 1,
                                 stamp.output.resource.get(), 0);
    queue.close();
    queue.execute();
    const UINT64 early = stamp.data[0];
    const UINT64 untouched = stamp.data[1];

    Sleep(150);
    queue.begin();
    queue.list->EndQuery(stamp.queries.get(), D3D12_QUERY_TYPE_TIMESTAMP, 1);
    queue.list->ResolveQueryData(stamp.queries.get(), D3D12_QUERY_TYPE_TIMESTAMP, 1, 1,
                                 stamp.output.resource.get(), sizeof(UINT64));
    queue.close();
    queue.execute();
    UINT64 gpu_calibration = 0, cpu_calibration = 0;
    const HRESULT hr = queue.queue->GetClockCalibration(&gpu_calibration, &cpu_calibration);
    const bool resolved_first = early != poison[0];
    const bool resolved_last = stamp.data[1] != poison[1];
    const double gpu_us = resolved_first && resolved_last ? stamp.elapsed() : -1;
    std::printf("CLOCK cpu_elapsed_us=%.3f first=%llu expected_poison_first=%llu untouched=%llu "
                "expected_poison_last=%llu last=%llu resolved_first=%d resolved_last=%d gpu_elapsed_us=%.3f "
                "calibration_hr=%08lx calibration_gpu=%llu calibration_cpu=%llu\n",
                us(ticks() - start), early, poison[0], untouched, poison[1], stamp.data[1],
                resolved_first, resolved_last, gpu_us, static_cast<unsigned long>(hr), gpu_calibration, cpu_calibration);
    if (untouched != poison[1]) throw std::runtime_error("query resolve overwrote untouched slot");
}

static uint32_t repeated_byte(uint8_t value) {
    return 0x01010101u * value;
}

struct Case {
    UINT dirty;
    bool full_write;
    bool full_read;
    bool omit;
};

static void sweep(Device& d, Heap kind, UINT bytes, UINT stride, UINT seed) {
    // Buffers in COMMON promote to COPY_SOURCE and decay at each completed Execute.
    // UPLOAD stays GENERIC_READ, which includes COPY_SOURCE.
    Buffer src(d, kind, bytes, kind == Heap::upload ? D3D12_RESOURCE_STATE_GENERIC_READ
                                                   : D3D12_RESOURCE_STATE_COMMON);
    Buffer dst(d, Heap::readback, bytes, D3D12_RESOURCE_STATE_COPY_DEST);
    auto* in = src.map();
    auto* out = dst.map();
    Queue q(d);
    Timestamp stamp(d, q);
    const UINT words = bytes / 4;
    const UINT pages = bytes / stride;
    const UINT page_words = stride / 4;
    const UINT offsets[] = {0, words / 2, words - page_words};
    std::vector<UINT> order(pages);
    std::iota(order.begin(), order.end(), 0u);
    std::mt19937 random(seed);
    // Fix endpoint and interior coverage. Randomize the remaining pages and case order.
    std::shuffle(order.begin(), order.end(), random);
    for (UINT i = 0; i < 3; ++i)
        std::iter_swap(order.begin() + i, std::find(order.begin(), order.end(), offsets[i] / page_words));

    std::vector<Case> cases;
    for (bool full_read : {false, true}) {
        for (UINT dirty : {0u, 1u, std::min(64u, pages), pages}) {
            if (dirty == pages && cases.back().dirty == pages) continue;
            cases.push_back({dirty, false, full_read, false});
        }
        cases.push_back({pages, true, full_read, false});
        cases.push_back({pages, false, full_read, true});
    }
    std::shuffle(cases.begin(), cases.end(), random);
    for (const Case test : cases) {
        constexpr uint8_t initial = 0x27;
        constexpr uint32_t poison = 0xefefefefu;
        std::memset(in, initial, bytes);
        _mm_sfence();
        std::vector<bool> changed(pages);
        for (UINT i = 0; i < test.dirty; ++i) changed[order[i]] = true;
        const UINT copied_words = test.full_read ? words : 3;
        for (int i = -3; i < 12; ++i) {
            const auto byte = uint8_t(0x60 + i);
            std::memset(out, 0xef, copied_words * 4u);
            _mm_sfence();
            const auto write_start = ticks();
            if (test.full_write) {
                std::memset(in, byte, bytes);
            } else {
                for (UINT j = 0; j < test.dirty; ++j)
                    in[order[j] * page_words] = repeated_byte(byte);
            }
            _mm_sfence();
            const auto written = ticks();
            q.begin();
            if (test.full_read) stamp.start(q);
            if (!test.omit) {
                if (test.full_read) {
                    q.list->CopyBufferRegion(dst.resource.get(), 0, src.resource.get(), 0, bytes);
                } else {
                    for (UINT j = 0; j < 3; ++j)
                        q.list->CopyBufferRegion(dst.resource.get(), j * 4u,
                                                src.resource.get(), offsets[j] * 4u, 4);
                }
            }
            if (test.full_read) stamp.end(q);
            q.close();
            const auto encoded = ticks();
            const auto latency = q.execute();
            const auto validate_start = ticks();
            bool ok = true;
            uint32_t first_expected = 0, last_expected = 0;
            UINT matches = 0;
            for (UINT j = 0; j < copied_words; ++j) {
                const UINT index = test.full_read ? j : offsets[j];
                const bool modified = test.full_write ||
                     (index % page_words == 0 && changed[index / page_words]);
                const auto expected = test.omit ? poison : repeated_byte(modified ? byte : initial);
                if (!j) first_expected = expected;
                if (j == copied_words - 1) last_expected = expected;
                const auto actual = out[j];
                if (ok && actual != expected) {
                    std::printf("MISMATCH index=%u expected=%08x actual=%08x\n", index, expected, actual);
                    ok = false;
                }
                matches += actual == expected;
            }
            const auto verified = ticks();
            const double gpu_us = test.full_read ? stamp.elapsed() : -1;
            std::printf("COPY bytes=%u stride=%u seed=%u dirty=%u full_write=%d full_read=%d omit=%d iter=%d "
                        "cpu_written=%u gpu_payload=%u gpu_tick_begin=%llu gpu_tick_end=%llu cpu_write_us=%.3f encode_us=%.3f execute_us=%.3f "
                        "signal_us=%.3f wait_us=%.3f completion_us=%.3f gpu_us=%.3f validation_us=%.3f "
                        "expected=%08x,%08x actual=%08x,%08x matches=%u/%u\n",
                        bytes, stride, seed, test.dirty, test.full_write, test.full_read, test.omit, i,
                        test.full_write ? bytes : test.dirty * 4, test.omit ? 0 : copied_words * 4,
                        test.full_read ? stamp.data[0] : 0, test.full_read ? stamp.data[1] : 0,
                        us(written - write_start), us(encoded - written), latency.execute, latency.signal,
                        latency.wait, latency.total, gpu_us, us(verified - validate_start),
                        first_expected, last_expected, out[0], out[copied_words - 1], matches, copied_words);
            if (!ok) throw std::runtime_error("copy/omit oracle");
        }
    }

    for (int i = -3; i < 12; ++i) {
        const auto byte = uint8_t(0x40 + i);
        const auto write_start = ticks();
        std::memset(in, byte, bytes);
        _mm_sfence();
        const auto written = ticks();
        uint64_t sum = 0;
        for (UINT j = 0; j < words; ++j) sum += in[j];
        const auto read = ticks();
        const auto expected = uint64_t(repeated_byte(byte)) * words;
        std::printf("CPU bytes=%u iter=%d write_us=%.3f read_us=%.3f sum=%llu expected=%llu\n",
                    bytes, i, us(written - write_start), us(read - written),
                    static_cast<unsigned long long>(sum), static_cast<unsigned long long>(expected));
        if (sum != expected) throw std::runtime_error("CPU bandwidth oracle");
    }
    for (int i = -3; i < 12; ++i) {
        const auto start = ticks();
        q.begin();
        q.close();
        const auto encoded = ticks();
        const auto latency = q.execute();
        const auto signal_only = q.execute(false);
        std::printf("EMPTY iter=%d encode_us=%.3f execute_us=%.3f signal_us=%.3f wait_us=%.3f "
                    "completion_us=%.3f signal_only_us=%.3f\n", i, us(encoded - start),
                    latency.execute, latency.signal, latency.wait, latency.total, signal_only.total);
    }
}

static void trace_copy(Device& d, Heap kind) {
    constexpr UINT bytes = 64 * MiB;
    constexpr UINT count = bytes / 4;
    Buffer src(d, kind, bytes, kind == Heap::upload ? D3D12_RESOURCE_STATE_GENERIC_READ
                                                   : D3D12_RESOURCE_STATE_COMMON);
    Buffer dst(d, Heap::readback, bytes, D3D12_RESOURCE_STATE_COPY_DEST);
    auto* in = src.map();
    auto* out = dst.map();
    for (UINT j = 0; j < count; ++j) in[j] = j ^ 0x5A3367F9u;
    Queue q(d);
    for (int i = -3; i < 12; ++i) {
        const UINT salt = 0xC6D2B000u + UINT(i + 3);
        in[0] = salt;
        in[count - 1] = ~salt;
        std::memset(out, 0x5D, bytes);
        _mm_sfence();
        q.begin();
        q.list->CopyBufferRegion(dst.resource.get(), 0, src.resource.get(), 0, bytes);
        q.close();
        const auto latency = q.execute();
        UINT matches = (out[0] == salt) + (out[count - 1] == ~salt);
        for (UINT j = 1; j < count - 1; ++j) matches += out[j] == (j ^ 0x5A3367F9u);
        std::printf("TRACE_COPY iter=%d gpu_payload=%u completion_us=%.3f "
                    "actual=%08x,%08x expected=%08x,%08x matches=%u/%u\n",
                    i, bytes, latency.total, out[0], out[count - 1], salt, ~salt, matches, count);
        if (matches != count) throw std::runtime_error("trace copy oracle");
    }
}

static Heap parse_heap(const std::string& name) {
    if (name == "wb") return Heap::wb;
    if (name == "wc") return Heap::wc;
    if (name == "upload") return Heap::upload;
    throw std::runtime_error("unknown heap");
}

int main(int argc, char** argv) {
    setvbuf(stdout, nullptr, _IONBF, 0);
    try {
        if (argc != 2 && argc != 3 && argc != 6)
            throw std::runtime_error("usage: probe placement | clock | trace-copy HEAP | sweep HEAP MIB STRIDE SEED");
        Device device;
        receipt_pause();
        if (argc == 2 && std::string(argv[1]) == "placement") {
            placement_probe(device);
        } else if (argc == 2 && std::string(argv[1]) == "clock") {
            clock_probe(device);
        } else if (argc == 3 && std::string(argv[1]) == "trace-copy") {
            trace_copy(device, parse_heap(argv[2]));
        } else if (argc == 6 && std::string(argv[1]) == "sweep") {
            const std::string name = argv[2];
            const auto mib = std::stoul(argv[3]);
            const auto stride = std::stoul(argv[4]);
            const auto seed = std::stoul(argv[5]);
            if (!std::has_single_bit(mib) || mib > 64 || (stride != 4096 && stride != 16384))
                throw std::runtime_error("sweep bounds");
            std::printf("CONFIG heap=%s MiB=%lu stride=%lu seed=%lu warmups=3 measured=12\n",
                        name.c_str(), mib, stride, seed);
            sweep(device, parse_heap(name),
                  UINT(mib) * MiB, UINT(stride), UINT(seed));
        } else {
            throw std::runtime_error("unknown mode");
        }
        std::puts("END complete");
        return 0;
    } catch (const std::exception& e) {
        std::fprintf(stderr, "END failure: %s\n", e.what());
        return 1;
    }
}

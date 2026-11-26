// SPDX-License-Identifier: GPL-3.0-or-later
// Original bounded allocation trace. Not a production allocator.
#include "support.h"
#include <memory>
#include <random>

static constexpr UINT64 slabBytes = 32ULL * 1024 * 1024;
static constexpr int slots = 24;
static void stage(const char *phase) {
  std::printf("STAGE %s cpu_ptr=0\n", phase);
  if (std::getchar() == EOF)
    throw std::runtime_error("stage handshake lost");
}
static double milliseconds(double start) {
  return (ticks() - start) * 1000 / frequency();
}
struct Span {
  UINT64 offset, size;
};
struct FreeList {
  std::vector<Span> free{{0, slabBytes}};
  UINT64 take(UINT64 size, UINT64 align) {
    for (size_t i = 0; i < free.size(); ++i) {
      Span old = free[i];
      UINT64 start = (old.offset + align - 1) / align * align;
      if (start < old.offset || start - old.offset > old.size ||
          size > old.size - (start - old.offset))
        continue;
      free.erase(free.begin() + i);
      if (start > old.offset)
        free.push_back({old.offset, start - old.offset});
      if (start + size < old.offset + old.size)
        free.push_back({start + size, old.offset + old.size - start - size});
      return start;
    }
    throw std::runtime_error("bounded slab exhausted");
  }
  void give(UINT64 offset, UINT64 size) {
    free.push_back({offset, size});
    std::sort(free.begin(), free.end(),
              [](auto a, auto b) { return a.offset < b.offset; });
    std::vector<Span> merged;
    for (auto v : free) {
      if (!merged.empty() &&
          merged.back().offset + merged.back().size > v.offset)
        throw std::runtime_error("overlapping free span");
      if (!merged.empty() &&
          merged.back().offset + merged.back().size == v.offset)
        merged.back().size += v.size;
      else
        merged.push_back(v);
    }
    free = merged;
  }
};
struct Allocation {
  Ref<ID3D12Heap> heap;
  Ref<ID3D12Resource> resource;
  void *cpu = nullptr;
  UINT64 bytes = 0, reserved = 0, offset = 0;
  UINT seed = 0;
};
struct Trace {
  Context c;
  std::unique_ptr<Queue> q;
  Ref<ID3D12Heap> slab;
  FreeList free;
  std::array<Allocation, slots> live;
  Buffer read;
  bool pooled, omit = false;
  UINT seed;
  std::mt19937 rng;
  double heapSetup = 0;
  UINT64 peakLive = 0, peakReserved = 0;
  Trace(bool pool, UINT s, bool omitCopy)
      : pooled(pool), omit(omitCopy), seed(s), rng(s) {
    q = std::make_unique<Queue>(c.device.p);
    read.allocate(c.device.p, slots * 192, "readback");
    read.map();
    if (pooled) {
      double start = ticks();
      createHeap(slabBytes, slab);
      heapSetup = milliseconds(start);
    }
    std::printf("TRACE mode=%s seed=%u slab_bytes=%llu slots=%d "
                "heap_setup_ms=%.6f warmups=2 timed=10\n",
                pooled ? "pooled" : "individual", seed, pooled ? slabBytes : 0,
                slots, heapSetup);
  }
  void createHeap(UINT64 bytes, Ref<ID3D12Heap> &heap) {
    D3D12_HEAP_DESC d{};
    d.SizeInBytes = bytes;
    d.Alignment = 65536;
    d.Properties = properties(D3D12_HEAP_TYPE_CUSTOM);
    d.Properties.CPUPageProperty = D3D12_CPU_PAGE_PROPERTY_WRITE_BACK;
    d.Properties.MemoryPoolPreference = D3D12_MEMORY_POOL_L0;
    d.Flags = D3D12_HEAP_FLAG_ALLOW_ONLY_BUFFERS;
    require(c.device->CreateHeap(&d, __uuidof(ID3D12Heap), heap.out()),
            "trace CreateHeap");
  }
  void allocate(int slot, int iteration, int phase) {
    auto &a = live[slot];
    if (a.resource.p)
      throw std::runtime_error("occupied slot");
    const UINT64 sizes[] = {65540, 262148, 1048580, 393220};
    a.bytes = sizes[rng() % 4];
    a.seed = seed ^ UINT(iteration + 11) * 0x1020301u ^ UINT(slot) * 0x9001u ^
             UINT(phase) * 0xB3u;
    auto desc = bufferDesc(a.bytes);
    auto ai = c.device->GetResourceAllocationInfo(0, 1, &desc);
    if (!ai.Alignment || ai.SizeInBytes == ~UINT64(0))
      throw std::runtime_error("buffer allocation info");
    a.reserved = ai.SizeInBytes;
    double start = ticks();
    if (pooled)
      a.offset = free.take(ai.SizeInBytes, ai.Alignment);
    else
      createHeap(ai.SizeInBytes, a.heap);
    double heapMs = milliseconds(start);
    start = ticks();
    require(c.device->CreatePlacedResource(
                pooled ? slab.p : a.heap.p, a.offset, &desc,
                D3D12_RESOURCE_STATE_GENERIC_READ, nullptr,
                __uuidof(ID3D12Resource), a.resource.out()),
            "trace placement");
    double placeMs = milliseconds(start);
    start = ticks();
    D3D12_RANGE empty{0, 0};
    require(a.resource->Map(0, &empty, &a.cpu), "trace Map");
    double mapMs = milliseconds(start);
    if (!a.cpu)
      throw std::runtime_error("trace null map");
    auto p = static_cast<volatile uint32_t *>(a.cpu);
    for (UINT64 i = 0; i < a.bytes / 4; ++i)
      p[i] = pattern(i, a.seed);
    _mm_sfence();
    std::printf("ALLOC iteration=%d phase=%d slot=%d bytes=%llu reserved=%llu "
                "offset=%llu alignment=%llu heap_or_suballoc_ms=%.6f "
                "placement_ms=%.6f map_ms=%.6f warmup=%d\n",
                iteration, phase, slot, a.bytes, a.reserved, a.offset,
                ai.Alignment, heapMs, placeMs, mapMs, iteration < 0);
  }
  void stats(int iteration, const char *phase) {
    UINT64 payload = 0, reserved = 0, totalFree = 0, largest = 0;
    for (auto &a : live)
      if (a.resource.p) {
        payload += a.bytes;
        reserved += a.reserved;
      }
    if (pooled)
      for (auto s : free.free) {
        totalFree += s.size;
        largest = std::max(largest, s.size);
      }
    UINT64 backing = pooled ? slabBytes : reserved;
    peakLive = std::max(peakLive, payload);
    peakReserved = std::max(peakReserved, backing);
    std::printf("FRAGMENT iteration=%d phase=%s live_payload=%llu "
                "live_reserved=%llu backing_reserved=%llu alignment_waste=%llu "
                "total_free=%llu largest_free=%llu free_span_count=%zu\n",
                iteration, phase, payload, reserved, backing,
                reserved - payload, totalFree, largest,
                pooled ? free.free.size() : 0);
  }
  void verify(int iteration, int phase) {
    // Every recycled placed resource is explicitly activated; no inheritance is
    // assumed.
    q->begin();
    auto rb = static_cast<unsigned char *>(read.cpu);
    std::memset(rb, 0xA7, slots * 192);
    for (int slot = 0; slot < slots; ++slot) {
      auto &a = live[slot];
      if (!a.resource.p)
        continue;
      D3D12_RESOURCE_BARRIER alias{};
      alias.Type = D3D12_RESOURCE_BARRIER_TYPE_ALIASING;
      alias.Aliasing.pResourceAfter = a.resource.p;
      q->list->ResourceBarrier(1, &alias);
      if (!omit)
        for (int part = 0; part < 3; ++part) {
          UINT64 src = part == 0   ? 0
                       : part == 1 ? (a.bytes / 2 / 4) * 4
                                   : a.bytes - 64;
          q->list->CopyBufferRegion(read.resource.p, slot * 192 + part * 64,
                                    a.resource.p, src, 64);
        }
    }
    _mm_sfence();
    q->finish();
    int matches = 0, expected = 0;
    for (int slot = 0; slot < slots; ++slot) {
      auto &a = live[slot];
      if (!a.resource.p)
        continue;
      for (int part = 0; part < 3; ++part) {
        UINT64 src = part == 0   ? 0
                     : part == 1 ? (a.bytes / 2 / 4) * 4
                                 : a.bytes - 64;
        auto p =
            reinterpret_cast<volatile uint32_t *>(rb + slot * 192 + part * 64);
        for (int i = 0; i < 16; ++i) {
          ++expected;
          matches += p[i] == pattern(src / 4 + i, a.seed);
        }
      }
    }
    std::printf("READBACK iteration=%d phase=%d matches=%d/%d fence=%llu "
                "completed=%llu\n",
                iteration, phase, matches, expected, q->value,
                q->fence->GetCompletedValue());
    if (matches != expected)
      throw std::runtime_error("trace guard/interior mismatch");
    // Complete list retirement precedes free-list reuse; a fence alone is not
    // ref-count evidence.
    q->begin();
    q->close();
  }
  void release(int slot, int iteration) {
    auto &a = live[slot];
    if (!a.resource.p)
      return;
    UINT64 completed = q->fence->GetCompletedValue();
    if (completed < q->value)
      throw std::runtime_error("premature reuse rejected");
    double start = ticks();
    a.resource->Unmap(0, nullptr);
    a.cpu = nullptr;
    a.resource.clear();
    double resourceMs = milliseconds(start);
    start = ticks();
    if (pooled)
      free.give(a.offset, a.reserved);
    else
      a.heap.clear();
    double backingMs = milliseconds(start);
    std::printf(
        "FREE iteration=%d slot=%d resource_ms=%.6f "
        "backing_or_freelist_ms=%.6f retire_fence=%llu completed=%llu\n",
        iteration, slot, resourceMs, backingMs, q->value, completed);
  }
  void delayedRetirement() {
    allocate(0, -3, 0);
    auto &a = live[0];
    Ref<ID3D12Fence> gate;
    require(c.device->CreateFence(0, D3D12_FENCE_FLAG_NONE,
                                  __uuidof(ID3D12Fence), gate.out()),
            "retirement gate");
    struct OpenGate {
      ID3D12Fence *f;
      ~OpenGate() { f->Signal(1); }
    } cleanup{gate.p};
    q->begin();
    D3D12_RESOURCE_BARRIER activation{};
    activation.Type = D3D12_RESOURCE_BARRIER_TYPE_ALIASING;
    activation.Aliasing.pResourceAfter = a.resource.p;
    q->list->ResourceBarrier(1, &activation);
    q->list->CopyBufferRegion(read.resource.p, 0, a.resource.p, 0, 64);
    q->close();
    _mm_sfence();
    require(q->q->Wait(gate.p, 1), "queue gate wait");
    ID3D12CommandList *lists[] = {q->list.p};
    q->q->ExecuteCommandLists(1, lists);
    UINT64 target = ++q->value;
    require(q->q->Signal(q->fence.p, target), "retirement signal");
    UINT64 completed = q->fence->GetCompletedValue();
    bool withheld = completed < target;
    // No free, overwrite, or second resource activation occurs while this range
    // is pending.
    require(gate->Signal(1), "open retirement gate");
    require(q->fence->SetEventOnCompletion(target, q->event),
            "retirement event");
    if (WaitForSingleObject(q->event, 10000) != WAIT_OBJECT_0)
      throw std::runtime_error("retirement wait timeout");
    std::printf(
        "DELAYED_RETIRE target=%llu before=%llu withheld=%d after=%llu\n",
        target, completed, withheld, q->fence->GetCompletedValue());
    if (!withheld)
      throw std::runtime_error("pending range was not withheld");
    for (int i = 0; i < 16; ++i)
      if (static_cast<volatile uint32_t *>(read.cpu)[i] != pattern(i, a.seed))
        throw std::runtime_error("delayed readback mismatch");
    q->begin();
    q->close();
    release(0, -3);
  }
  void run() {
    stage("baseline");
    delayedRetirement();
    for (int iteration = -2; iteration < 10; ++iteration) {
      double start = ticks();
      for (int i = 0; i < slots; ++i)
        allocate(i, iteration, 0);
      stats(iteration, "full");
      verify(iteration, 0);
      for (int i = 0; i < slots; i += 2)
        release(i, iteration);
      stats(iteration, "holes");
      // Before the next real fence, a fictitious future retirement value must
      // be rejected.
      UINT64 completed = q->fence->GetCompletedValue(), future = q->value + 1;
      bool withheld = completed < future;
      std::printf("RETIRE_POLICY future=%llu completed=%llu withheld=%d\n",
                  future, completed, withheld);
      if (!withheld)
        throw std::runtime_error("retire policy control");
      for (int i = 0; i < slots; i += 2)
        allocate(i, iteration, 1);
      stats(iteration, "refill");
      verify(iteration, 1);
      if (iteration == 0) {
        double pause = ticks();
        stage("live");
        start += ticks() - pause;
      }
      for (int i = 0; i < slots; ++i)
        release(i, iteration);
      stats(iteration, "empty");
      if (pooled && (free.free.size() != 1 || free.free[0].offset ||
                     free.free[0].size != slabBytes))
        throw std::runtime_error("free-list failed to coalesce");
      std::printf("CYCLE iteration=%d ms=%.6f warmup=%d\n", iteration,
                  milliseconds(start), iteration < 0);
    }
    stage("resources_released");
    q.reset();
    read.release();
    slab.clear();
    stage("commands_retired");
    Sleep(500);
    stage("retired_settled");
    std::printf("TRACE_END peak_logical_live=%llu peak_reserved=%llu\n",
                peakLive, peakReserved);
  }
};
int main(int argc, char **argv) {
  setvbuf(stdout, nullptr, _IONBF, 0);
  try {
    if (argc != 3)
      throw std::runtime_error("usage: pool individual|pooled|omit SEED");
    std::string mode = argv[1];
    if (mode != "individual" && mode != "pooled" && mode != "omit")
      throw std::runtime_error("mode");
    Trace trace(mode != "individual", UINT(std::strtoul(argv[2], nullptr, 10)),
                mode == "omit");
    trace.run();
    std::puts("END success");
    return 0;
  } catch (const std::exception &e) {
    std::printf("END failure reason=%s\n", e.what());
    return 1;
  }
}

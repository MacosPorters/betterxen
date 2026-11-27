// SPDX-License-Identifier: GPL-3.0-or-later
// Original isolated compatibility/statistics fixture; upstream allocator stays
// external.
#include "../support.h"
#include "D3D12MemAlloc.h"
#include <memory>
#include <random>
static void stage(const char *name) {
  std::printf("STAGE %s cpu_ptr=0\n", name);
  if (std::getchar() == EOF)
    throw std::runtime_error("stage handshake");
}
static double ms(double start) {
  return (ticks() - start) * 1000 / frequency();
}
struct Item {
  Ref<D3D12MA::Allocation> allocation;
  Ref<ID3D12Resource> resource;
  UINT64 bytes = 0;
  UINT seed = 0;
  void *cpu = nullptr;
};
static void statistics(D3D12MA::Pool *pool, int cycle, const char *label) {
  D3D12MA::DetailedStatistics s{};
  pool->CalculateStatistics(&s);
  std::printf("ALLOCATOR_STATS cycle=%d phase=%s blocks=%u block_bytes=%llu "
              "allocation_count=%u allocation_bytes=%llu unused_ranges=%u "
              "largest_free=%llu\n",
              cycle, label, s.Stats.BlockCount, s.Stats.BlockBytes,
              s.Stats.AllocationCount, s.Stats.AllocationBytes,
              s.UnusedRangeCount, s.UnusedRangeSizeMax);
  if (s.Stats.BlockCount > 1 || s.Stats.BlockBytes > 33554432ULL)
    throw std::runtime_error("allocator pool exceeded bound");
}
int main(int argc, char **argv) {
  setvbuf(stdout, nullptr, _IONBF, 0);
  try {
    if (argc != 3)
      throw std::runtime_error("usage: allocator wb|wc SEED");
    std::string type = argv[1];
    if (type != "wb" && type != "wc")
      throw std::runtime_error("type");
    UINT seed = UINT(std::strtoul(argv[2], nullptr, 10));
    std::mt19937 rng(seed);
    Context c;
    identity(c);
    HMODULE dxgi = LoadLibraryA("dxgi.dll");
    using Factory = HRESULT(WINAPI *)(UINT, REFIID, void **);
    Ref<IDXGIFactory4> factory;
    require(symbol<Factory>(dxgi, "CreateDXGIFactory2")(
                0, __uuidof(IDXGIFactory4), factory.out()),
            "allocator factory");
    Ref<IDXGIAdapter> adapter;
    require(factory->EnumAdapterByLuid(c.device->GetAdapterLuid(),
                                       __uuidof(IDXGIAdapter), adapter.out()),
            "allocator actual adapter");
    Ref<D3D12MA::Allocator> allocator;
    D3D12MA::ALLOCATOR_DESC ad{};
    ad.Flags = D3D12MA::ALLOCATOR_FLAG_SINGLETHREADED;
    ad.pDevice = c.device.p;
    ad.pAdapter = adapter.p;
    ad.PreferredBlockSize = 33554432;
    double start = ticks();
    require(D3D12MA::CreateAllocator(&ad, &allocator.p),
            "D3D12MA CreateAllocator");
    std::printf("ALLOCATOR_CREATE type=%s ms=%.6f\n", type.c_str(), ms(start));
    std::unique_ptr<Queue> q = std::make_unique<Queue>(c.device.p);
    Buffer read;
    read.allocate(c.device.p, 24 * 192, "readback");
    read.map();
    stage("baseline");
    Ref<D3D12MA::Pool> pool;
    D3D12MA::POOL_DESC pd{};
    pd.HeapProperties = properties(D3D12_HEAP_TYPE_CUSTOM);
    pd.HeapProperties.CPUPageProperty =
        type == "wb" ? D3D12_CPU_PAGE_PROPERTY_WRITE_BACK
                     : D3D12_CPU_PAGE_PROPERTY_WRITE_COMBINE;
    pd.HeapProperties.MemoryPoolPreference = D3D12_MEMORY_POOL_L0;
    pd.HeapFlags = D3D12_HEAP_FLAG_ALLOW_ONLY_BUFFERS;
    pd.BlockSize = 33554432;
    pd.MinBlockCount = 1;
    pd.MaxBlockCount = 1;
    start = ticks();
    require(allocator->CreatePool(&pd, &pool.p), "D3D12MA CreatePool");
    std::printf("ALLOCATOR_POOL type=%s ms=%.6f bytes=33554432 min_blocks=1 "
                "max_blocks=1\n",
                type.c_str(), ms(start));
    stage("heap_ready");
    // Valid allocation request that cannot fit an explicit, single 32 MiB pool.
    {
      D3D12MA::ALLOCATION_DESC request{};
      request.CustomPool = pool.p;
      auto tooLarge = bufferDesc(33554436);
      Ref<D3D12MA::Allocation> a;
      Ref<ID3D12Resource> resource;
      HRESULT rejected = allocator->CreateResource(
          &request, &tooLarge, D3D12_RESOURCE_STATE_GENERIC_READ, nullptr, &a.p,
          __uuidof(ID3D12Resource), resource.out());
      std::printf("POOL_BOUND_CONTROL requested_bytes=33554436 HRESULT=0x%08lX "
                  "rejected=%d\n",
                  (unsigned long)rejected, FAILED(rejected));
      if (SUCCEEDED(rejected))
        throw std::runtime_error(
            "single-block pool accepted oversized request");
    }

    // Match the published raw trace RNG draw used by its separate gate-control
    // allocation.
    (void)rng();
    std::array<Item, 24> live;
    auto allocate = [&](int slot, int cycle, int phase) {
      auto &item = live[slot];
      UINT64 sizes[] = {65540, 262148, 1048580, 393220};
      item.bytes = sizes[rng() % 4];
      item.seed = seed ^ UINT(cycle + 11) * 0x1020301u ^ UINT(slot) * 0x9001u ^
                  UINT(phase) * 0xB3u;
      auto desc = bufferDesc(item.bytes);
      D3D12MA::ALLOCATION_DESC d{};
      d.CustomPool = pool.p;
      double t = ticks();
      require(allocator->CreateResource(
                  &d, &desc, D3D12_RESOURCE_STATE_GENERIC_READ, nullptr,
                  &item.allocation.p, __uuidof(ID3D12Resource),
                  item.resource.out()),
              "D3D12MA CreateResource");
      double createMs = ms(t);
      t = ticks();
      D3D12_RANGE empty{0, 0};
      require(item.resource->Map(0, &empty, &item.cpu), "D3D12MA resource Map");
      double mapMs = ms(t);
      if (!item.cpu)
        throw std::runtime_error("allocator null Map");
      auto cpu = static_cast<volatile uint32_t *>(item.cpu);
      for (UINT64 i = 0; i < item.bytes / 4; ++i)
        cpu[i] = pattern(i, item.seed);
      _mm_sfence();
      std::printf("REFERENCE_ALLOC cycle=%d phase=%d slot=%d bytes=%llu "
                  "allocation_size=%llu offset=%llu create_resource_ms=%.6f "
                  "map_ms=%.6f\n",
                  cycle, phase, slot, item.bytes, item.allocation->GetSize(),
                  item.allocation->GetOffset(), createMs, mapMs);
    };
    auto release = [&](int slot, int cycle) {
      auto &item = live[slot];
      if (q->fence->GetCompletedValue() < q->value)
        throw std::runtime_error("unsafe allocator retirement");
      double t = ticks();
      item.resource->Unmap(0, nullptr);
      item.cpu = nullptr;
      item.resource.clear();
      item.allocation.clear();
      std::printf("REFERENCE_FREE cycle=%d slot=%d "
                  "release_resource_and_allocation_ms=%.6f\n",
                  cycle, slot, ms(t));
    };
    auto verify = [&](int cycle, int phase) {
      q->begin();
      std::memset(read.cpu, 0xA7, 24 * 192);
      for (int slot = 0; slot < 24; ++slot) {
        auto &x = live[slot];
        if (!x.resource.p)
          continue;
        D3D12_RESOURCE_BARRIER b{};
        b.Type = D3D12_RESOURCE_BARRIER_TYPE_ALIASING;
        b.Aliasing.pResourceAfter = x.resource.p;
        q->list->ResourceBarrier(1, &b);
        for (int part = 0; part < 3; ++part) {
          UINT64 offset = part == 0   ? 0
                          : part == 1 ? x.bytes / 2 / 4 * 4
                                      : x.bytes - 64;
          q->list->CopyBufferRegion(read.resource.p, slot * 192 + part * 64,
                                    x.resource.p, offset, 64);
        }
      }
      q->finish();
      UINT matched = 0, count = 0;
      for (int slot = 0; slot < 24; ++slot) {
        auto &x = live[slot];
        if (!x.resource.p)
          continue;
        for (int part = 0; part < 3; ++part) {
          UINT64 offset = part == 0   ? 0
                          : part == 1 ? x.bytes / 2 / 4 * 4
                                      : x.bytes - 64;
          auto p = reinterpret_cast<volatile uint32_t *>(
              static_cast<unsigned char *>(read.cpu) + slot * 192 + part * 64);
          for (int i = 0; i < 16; ++i) {
            ++count;
            matched += p[i] == pattern(offset / 4 + i, x.seed);
          }
        }
      }
      std::printf("REFERENCE_VERIFY cycle=%d phase=%d matched=%u/%u\n", cycle,
                  phase, matched, count);
      if (matched != count)
        throw std::runtime_error("allocator guard/interior mismatch");
      q->begin();
      q->close();
    };
    for (int cycle = -2; cycle < 10; ++cycle) {
      start = ticks();
      for (int i = 0; i < 24; ++i)
        allocate(i, cycle, 0);
      statistics(pool.p, cycle, "full");
      verify(cycle, 0);
      for (int i = 0; i < 24; i += 2)
        release(i, cycle);
      statistics(pool.p, cycle, "holes");
      for (int i = 0; i < 24; i += 2)
        allocate(i, cycle, 1);
      verify(cycle, 1);
      if (cycle == 0) {
        double pause = ticks();
        stage("live");
        start += ticks() - pause;
      }
      for (int i = 0; i < 24; ++i)
        release(i, cycle);
      statistics(pool.p, cycle, "empty");
      std::printf("REFERENCE_CYCLE cycle=%d ms=%.6f\n", cycle, ms(start));
    }
    stage("resources_released");
    q.reset();
    read.release();
    pool.clear();
    allocator.clear();
    stage("commands_retired");
    std::puts("END success");
    return 0;
  } catch (const std::exception &e) {
    std::printf("END failure reason=%s\n", e.what());
    return 1;
  }
}

// SPDX-License-Identifier: GPL-3.0-or-later
// Buffer data inheritance under the D3D12 advanced aliasing model; GPU copies
// only.
#include "../../allocation-profiling/support.h"
static void activate(Queue &q, ID3D12Resource *before, ID3D12Resource *after) {
  D3D12_RESOURCE_BARRIER b{};
  b.Type = D3D12_RESOURCE_BARRIER_TYPE_ALIASING;
  b.Aliasing.pResourceBefore = before;
  b.Aliasing.pResourceAfter = after;
  q.list->ResourceBarrier(1, &b);
}
static void stage(const char *name) {
  std::printf("STAGE %s cpu_ptr=0\n", name);
  if (std::getchar() == EOF)
    throw std::runtime_error("handshake");
}
int main(int argc, char **argv) {
  setvbuf(stdout, nullptr, _IONBF, 0);
  try {
    if (argc != 2)
      throw std::runtime_error("usage: alias SESSION");
    UINT session = UINT(std::strtoul(argv[1], nullptr, 10));
    Context c;
    stage("baseline");
    Ref<ID3D12Heap> heap;
    D3D12_HEAP_DESC hd{};
    hd.SizeInBytes = 131072;
    hd.Alignment = 65536;
    hd.Properties = properties(D3D12_HEAP_TYPE_CUSTOM);
    hd.Properties.CPUPageProperty = D3D12_CPU_PAGE_PROPERTY_WRITE_BACK;
    hd.Properties.MemoryPoolPreference = D3D12_MEMORY_POOL_L0;
    hd.Flags = D3D12_HEAP_FLAG_ALLOW_ONLY_BUFFERS;
    require(c.device->CreateHeap(&hd, __uuidof(ID3D12Heap), heap.out()),
            "alias heap");
    Ref<ID3D12Resource> placed[3];
    auto desc = bufferDesc(65536);
    for (int i = 0; i < 3; ++i) {
      UINT64 offset = i == 2 ? 65536 : 0;
      require(c.device->CreatePlacedResource(
                  heap.p, offset, &desc, D3D12_RESOURCE_STATE_COMMON, nullptr,
                  __uuidof(ID3D12Resource), placed[i].out()),
              "alias placed buffer");
      void *pointer = nullptr;
      require(placed[i]->Map(0, nullptr, &pointer),
              "alias Map pointer receipt");
      std::printf(
          "ALIAS_RESOURCE index=%d heap_offset=%llu cpu_ptr=%p gpu_va=%llu\n",
          i, offset, pointer, placed[i]->GetGPUVirtualAddress());
    }
    Buffer source, guardSource, read;
    source.allocate(c.device.p, 65536, "upload");
    source.map();
    guardSource.allocate(c.device.p, 65536, "upload");
    guardSource.map();
    read.allocate(c.device.p, 3 * 65536, "readback");
    read.map();
    Queue q(c.device.p);
    UINT discrepancies = 0;
    for (UINT round = 0; round < 3; ++round) {
      UINT salt = 0x13579BDFu + session * 0x10000u + round * 0x101u;
      auto u = static_cast<volatile uint32_t *>(source.cpu);
      auto g = static_cast<volatile uint32_t *>(guardSource.cpu);
      for (UINT i = 0; i < 16384; ++i) {
        u[i] = i * 1664525u + salt;
        g[i] = i ^ 0xC1A0BEEFu;
      }
      _mm_sfence();
      std::memset(read.cpu, 0xA7, 3 * 65536);
      // Alternate which overlapping buffer receives the new full-subresource
      // GPU write.
      int from = round % 2, to = 1 - from;
      q.begin();
      activate(q, nullptr, placed[from].p);
      activate(q, nullptr, placed[2].p);
      q.transition(placed[from].p, D3D12_RESOURCE_STATE_COMMON,
                   D3D12_RESOURCE_STATE_COPY_DEST);
      q.list->CopyBufferRegion(placed[from].p, 0, source.resource.p, 0, 65536);
      q.transition(placed[from].p, D3D12_RESOURCE_STATE_COPY_DEST,
                   D3D12_RESOURCE_STATE_COPY_SOURCE);
      q.list->CopyBufferRegion(read.resource.p, 0, placed[from].p, 0, 65536);
      q.transition(placed[from].p, D3D12_RESOURCE_STATE_COPY_SOURCE,
                   D3D12_RESOURCE_STATE_COMMON);
      q.transition(placed[2].p, D3D12_RESOURCE_STATE_COMMON,
                   D3D12_RESOURCE_STATE_COPY_DEST);
      q.list->CopyBufferRegion(placed[2].p, 0, guardSource.resource.p, 0,
                               65536);
      q.transition(placed[2].p, D3D12_RESOURCE_STATE_COPY_DEST,
                   D3D12_RESOURCE_STATE_COPY_SOURCE);
      activate(q, placed[from].p, placed[to].p);
      q.transition(placed[to].p, D3D12_RESOURCE_STATE_COMMON,
                   D3D12_RESOURCE_STATE_COPY_SOURCE);
      q.list->CopyBufferRegion(read.resource.p, 65536, placed[to].p, 0, 65536);
      q.transition(placed[to].p, D3D12_RESOURCE_STATE_COPY_SOURCE,
                   D3D12_RESOURCE_STATE_COMMON);
      q.list->CopyBufferRegion(read.resource.p, 131072, placed[2].p, 0, 65536);
      q.transition(placed[2].p, D3D12_RESOURCE_STATE_COPY_SOURCE,
                   D3D12_RESOURCE_STATE_COMMON);
      q.finish();
      auto p = static_cast<volatile uint32_t *>(read.cpu);
      UINT a = 0, b = 0, gmatch = 0;
      for (UINT i = 0; i < 16384; ++i) {
        UINT expected = i * 1664525u + salt;
        a += p[i] == expected;
        b += p[16384 + i] == expected;
        gmatch += p[32768 + i] == (i ^ 0xC1A0BEEFu);
      }
      std::printf("BUFFER_INHERIT session=%u round=%u from=%d to=%d "
                  "source_matches=%u/16384 inherited_matches=%u/16384 "
                  "disjoint_guard_matches=%u/16384 first_source=%08X "
                  "first_inherited=%08X expected=%08X\n",
                  session, round, from, to, a, b, gmatch, p[0], p[16384], salt);
      discrepancies += a != 16384 || b != 16384 || gmatch != 16384;
    }
    q.begin();
    q.close();
    for (auto &p : placed)
      p->Unmap(0, nullptr);
    stage("gpu_complete");
    if (discrepancies)
      throw std::runtime_error("buffer data inheritance mismatch");
    std::puts("END success");
    return 0;
  } catch (const std::exception &e) {
    std::printf("END failure reason=%s\n", e.what());
    return 1;
  }
}

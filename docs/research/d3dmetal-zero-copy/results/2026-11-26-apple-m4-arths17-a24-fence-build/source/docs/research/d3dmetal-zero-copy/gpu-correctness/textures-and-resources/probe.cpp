// SPDX-License-Identifier: GPL-3.0-or-later
// Original Workstream A placement/texture fixtures; no production shim is
// loaded.
#include "../../allocation-profiling/support.h"
#include <memory>
static void stage(const char *name) {
  std::printf("STAGE %s cpu_ptr=0\n", name);
  if (std::getchar() == EOF)
    throw std::runtime_error("handshake");
}
static UINT64 alignUp(UINT64 n, UINT64 a) { return (n + a - 1) / a * a; }
static D3D12_HEAP_PROPERTIES heapProperties(const std::string &type) {
  if (type == "default")
    return properties(D3D12_HEAP_TYPE_DEFAULT);
  auto p = properties(D3D12_HEAP_TYPE_CUSTOM);
  p.CPUPageProperty = type == "wc" ? D3D12_CPU_PAGE_PROPERTY_WRITE_COMBINE
                                   : D3D12_CPU_PAGE_PROPERTY_WRITE_BACK;
  p.MemoryPoolPreference = D3D12_MEMORY_POOL_L0;
  return p;
}
static void activate(Queue &q, ID3D12Resource *after,
                     ID3D12Resource *before = nullptr) {
  D3D12_RESOURCE_BARRIER b{};
  b.Type = D3D12_RESOURCE_BARRIER_TYPE_ALIASING;
  b.Aliasing.pResourceBefore = before;
  b.Aliasing.pResourceAfter = after;
  q.list->ResourceBarrier(1, &b);
}
static D3D12_RESOURCE_DESC textureDesc(DXGI_FORMAT format,
                                       D3D12_RESOURCE_FLAGS flags, UINT mips) {
  D3D12_RESOURCE_DESC d{};
  d.Dimension = D3D12_RESOURCE_DIMENSION_TEXTURE2D;
  d.Width = 65;
  d.Height = 37;
  d.DepthOrArraySize = 1;
  d.MipLevels = UINT16(mips);
  d.Format = format;
  d.SampleDesc.Count = 1;
  d.Layout = D3D12_TEXTURE_LAYOUT_UNKNOWN;
  d.Flags = flags;
  return d;
}
static void copyOut(Queue &q, ID3D12Resource *texture, UINT sub,
                    ID3D12Resource *read,
                    const D3D12_PLACED_SUBRESOURCE_FOOTPRINT &fp) {
  D3D12_TEXTURE_COPY_LOCATION from{}, to{};
  from.pResource = texture;
  from.Type = D3D12_TEXTURE_COPY_TYPE_SUBRESOURCE_INDEX;
  from.SubresourceIndex = sub;
  to.pResource = read;
  to.Type = D3D12_TEXTURE_COPY_TYPE_PLACED_FOOTPRINT;
  to.PlacedFootprint = fp;
  q.list->CopyTextureRegion(&to, 0, 0, 0, &from, nullptr);
}
static void copyIn(Queue &q, ID3D12Resource *texture, UINT sub,
                   ID3D12Resource *upload,
                   const D3D12_PLACED_SUBRESOURCE_FOOTPRINT &fp) {
  D3D12_TEXTURE_COPY_LOCATION from{}, to{};
  from.pResource = upload;
  from.Type = D3D12_TEXTURE_COPY_TYPE_PLACED_FOOTPRINT;
  from.PlacedFootprint = fp;
  to.pResource = texture;
  to.Type = D3D12_TEXTURE_COPY_TYPE_SUBRESOURCE_INDEX;
  to.SubresourceIndex = sub;
  q.list->CopyTextureRegion(&to, 0, 0, 0, &from, nullptr);
}
static unsigned char dataset(UINT x, UINT y, UINT sub, UINT channel,
                             UINT round) {
  return static_cast<unsigned char>(
      (x * 17 + y * 29 + sub * 71 + channel * 43 + round * 53) ^ 0x96);
}
static bool supportedFormat(Context &c, DXGI_FORMAT format, bool rt = false,
                            bool depth = false) {
  D3D12_FEATURE_DATA_FORMAT_SUPPORT f{};
  f.Format = format;
  HRESULT hr = c.device->CheckFeatureSupport(D3D12_FEATURE_FORMAT_SUPPORT, &f,
                                             sizeof(f));
  std::printf(
      "FORMAT format=%u HRESULT=0x%08lX support1=0x%08X support2=0x%08X\n",
      format, (unsigned long)hr, unsigned(f.Support1), unsigned(f.Support2));
  bool supported =
      SUCCEEDED(hr) && (f.Support1 & D3D12_FORMAT_SUPPORT1_TEXTURE2D);
  if (rt)
    supported = supported && (f.Support1 & D3D12_FORMAT_SUPPORT1_RENDER_TARGET);
  if (depth)
    supported = supported && (f.Support1 & D3D12_FORMAT_SUPPORT1_DEPTH_STENCIL);
  if (!supported)
    std::puts("OUTCOME unsupported advertised_format_support");
  return supported;
}
static void mixed(Context &c, const std::string &type, DXGI_FORMAT format,
                  UINT bpp, UINT round) {
  if (!supportedFormat(c, format))
    return;
  auto d = textureDesc(format, D3D12_RESOURCE_FLAG_NONE, 3);
  auto ai = c.device->GetResourceAllocationInfo(0, 1, &d);
  auto bd = bufferDesc(65536);
  auto bi = c.device->GetResourceAllocationInfo(0, 1, &bd);
  std::printf("CASE mixed type=%s format=%u mips=3 texture_size=%llu "
              "texture_alignment=%llu\n",
              type.c_str(), format, ai.SizeInBytes, ai.Alignment);
  if (!ai.Alignment || ai.SizeInBytes == ~UINT64(0)) {
    std::puts("OUTCOME unsupported allocation_info");
    return;
  }
  UINT64 offset = alignUp(bi.SizeInBytes, ai.Alignment);
  Ref<ID3D12Heap> heap;
  D3D12_HEAP_DESC hd{};
  hd.Properties = heapProperties(type);
  hd.Alignment = std::max(ai.Alignment, bi.Alignment);
  hd.SizeInBytes = alignUp(offset + ai.SizeInBytes, hd.Alignment);
  HRESULT hr = c.device->CreateHeap(&hd, __uuidof(ID3D12Heap), heap.out());
  report(hr, "mixed CreateHeap");
  if (FAILED(hr)) {
    std::puts("OUTCOME unsupported heap");
    return;
  }
  Ref<ID3D12Resource> guard, t;
  require(c.device->CreatePlacedResource(heap.p, 0, &bd,
                                         D3D12_RESOURCE_STATE_COMMON, nullptr,
                                         __uuidof(ID3D12Resource), guard.out()),
          "mixed guard");
  hr = c.device->CreatePlacedResource(heap.p, offset, &d,
                                      D3D12_RESOURCE_STATE_COMMON, nullptr,
                                      __uuidof(ID3D12Resource), t.out());
  report(hr, "mixed texture");
  if (FAILED(hr)) {
    std::puts("OUTCOME unsupported texture");
    return;
  }
  std::printf("PLACEMENT guard_offset=0 guard_size=%llu texture_offset=%llu "
              "heap_bytes=%llu disjoint=1\n",
              bi.SizeInBytes, offset, hd.SizeInBytes);
  std::array<D3D12_PLACED_SUBRESOURCE_FOOTPRINT, 3> fp{};
  UINT rows[3]{};
  UINT64 rowBytes[3]{}, total = 0;
  c.device->GetCopyableFootprints(&d, 0, 3, 512, fp.data(), rows, rowBytes,
                                  &total);
  if (total == ~UINT64(0))
    throw std::runtime_error("copy footprints");
  for (UINT m = 0; m < 3; ++m)
    total = std::max(total, fp[m].Offset +
                                UINT64(fp[m].Footprint.Height - 1) *
                                    fp[m].Footprint.RowPitch +
                                rowBytes[m]);
  Buffer upload, read, guardUpload, guardRead;
  upload.allocate(c.device.p, total, "upload");
  upload.map();
  read.allocate(c.device.p, total, "readback");
  read.map();
  guardUpload.allocate(c.device.p, 65536, "upload");
  guardUpload.map();
  guardRead.allocate(c.device.p, 65536, "readback");
  guardRead.map();
  std::memset(upload.cpu, 0xDA, total);
  std::memset(read.cpu, 0xA7, total);
  auto gu = static_cast<volatile uint32_t *>(guardUpload.cpu);
  for (UINT i = 0; i < 65536 / 4; ++i)
    gu[i] = 0xF1A20000u + i + round;
  std::array<std::vector<unsigned char>, 3> expected;
  for (UINT m = 0; m < 3; ++m) {
    UINT width = std::max(1u, 65u >> m), height = std::max(1u, 37u >> m);
    expected[m].resize(width * height * bpp);
    if (fp[m].Offset % 512 || fp[m].Footprint.RowPitch % 256 ||
        rowBytes[m] != width * bpp || rows[m] != height)
      throw std::runtime_error("unexpected footprint contract");
    std::printf("FOOTPRINT mip=%u offset=%llu rows=%u row_bytes=%llu "
                "row_pitch=%u total=%llu\n",
                m, fp[m].Offset, rows[m], rowBytes[m], fp[m].Footprint.RowPitch,
                total);
    for (UINT y = 0; y < height; ++y)
      for (UINT x = 0; x < width; ++x)
        for (UINT b = 0; b < bpp; ++b) {
          auto v = dataset(x, y, m, b, round);
          expected[m][(y * width + x) * bpp + b] = v;
          static_cast<unsigned char *>(
              upload.cpu)[fp[m].Offset + UINT64(y) * fp[m].Footprint.RowPitch +
                          x * bpp + b] = v;
        }
  }
  Queue q(c.device.p);
  q.begin();
  activate(q, guard.p);
  activate(q, t.p);
  q.transition(guard.p, D3D12_RESOURCE_STATE_COMMON,
               D3D12_RESOURCE_STATE_COPY_DEST);
  q.list->CopyBufferRegion(guard.p, 0, guardUpload.resource.p, 0, 65536);
  q.transition(guard.p, D3D12_RESOURCE_STATE_COPY_DEST,
               D3D12_RESOURCE_STATE_COMMON);
  q.finish();
  if (type != "default") {
    for (UINT m = 0; m < 3; ++m) {
      hr = t->Map(m, nullptr, nullptr);
      report(hr, "mixed Map null pointer");
      if (FAILED(hr)) {
        std::puts("OUTCOME unsupported CPU_texture_Map");
        return;
      }
      UINT width = std::max(1u, 65u >> m), height = std::max(1u, 37u >> m);
      hr = t->WriteToSubresource(m, nullptr, expected[m].data(), width * bpp,
                                 width * height * bpp);
      report(hr, "mixed WriteToSubresource");
      t->Unmap(m, nullptr);
      if (FAILED(hr)) {
        std::puts("OUTCOME unsupported CPU_texture_write");
        return;
      }
    }
  } else {
    q.begin();
    q.transition(t.p, D3D12_RESOURCE_STATE_COMMON,
                 D3D12_RESOURCE_STATE_COPY_DEST);
    for (UINT m = 0; m < 3; ++m)
      copyIn(q, t.p, m, upload.resource.p, fp[m]);
    q.transition(t.p, D3D12_RESOURCE_STATE_COPY_DEST,
                 D3D12_RESOURCE_STATE_COMMON);
    q.finish();
  }
  _mm_sfence();
  q.begin();
  q.transition(t.p, D3D12_RESOURCE_STATE_COMMON,
               D3D12_RESOURCE_STATE_COPY_SOURCE);
  for (UINT m = 0; m < 3; ++m)
    copyOut(q, t.p, m, read.resource.p, fp[m]);
  q.transition(t.p, D3D12_RESOURCE_STATE_COPY_SOURCE,
               D3D12_RESOURCE_STATE_COMMON);
  q.transition(guard.p, D3D12_RESOURCE_STATE_COMMON,
               D3D12_RESOURCE_STATE_COPY_SOURCE);
  q.list->CopyBufferRegion(guardRead.resource.p, 0, guard.p, 0, 65536);
  q.transition(guard.p, D3D12_RESOURCE_STATE_COPY_SOURCE,
               D3D12_RESOURCE_STATE_COMMON);
  q.finish();
  UINT64 matches = 0, count = 0;
  for (UINT m = 0; m < 3; ++m) {
    UINT width = std::max(1u, 65u >> m), height = std::max(1u, 37u >> m);
    for (UINT y = 0; y < height; ++y)
      for (UINT x = 0; x < width * bpp; ++x) {
        ++count;
        matches +=
            static_cast<volatile unsigned char *>(
                read.cpu)[fp[m].Offset + UINT64(y) * fp[m].Footprint.RowPitch +
                          x] == expected[m][y * width * bpp + x];
      }
  }
  UINT guards = 0;
  for (UINT i = 0; i < 65536 / 4; ++i)
    guards += static_cast<volatile uint32_t *>(guardRead.cpu)[i] ==
              0xF1A20000u + i + round;
  std::printf("OUTCOME mixed type=%s format=%u bytes_match=%llu/%llu "
              "guard_words=%u/16384\n",
              type.c_str(), format, matches, count, guards);
  if (matches != count || guards != 16384)
    throw std::runtime_error("mixed texture/guard mismatch");
}
static void clearAlias(Context &c, const std::string &type, bool depth) {
  auto d =
      textureDesc(depth ? DXGI_FORMAT_D32_FLOAT : DXGI_FORMAT_R8G8B8A8_UNORM,
                  depth ? D3D12_RESOURCE_FLAG_ALLOW_DEPTH_STENCIL
                        : D3D12_RESOURCE_FLAG_ALLOW_RENDER_TARGET,
                  1);
  if (!supportedFormat(c, d.Format, !depth, depth))
    return;
  auto ai = c.device->GetResourceAllocationInfo(0, 1, &d);
  std::printf("CASE clear_alias type=%s depth=%d size=%llu alignment=%llu\n",
              type.c_str(), depth, ai.SizeInBytes, ai.Alignment);
  if (!ai.Alignment || ai.SizeInBytes == ~UINT64(0)) {
    std::puts("OUTCOME unsupported allocation_info");
    return;
  }
  Ref<ID3D12Heap> heap;
  D3D12_HEAP_DESC hd{};
  hd.Properties = heapProperties(type);
  hd.Alignment = ai.Alignment;
  hd.SizeInBytes = ai.SizeInBytes;
  hd.Flags = D3D12_HEAP_FLAG_ALLOW_ONLY_RT_DS_TEXTURES;
  HRESULT hr = c.device->CreateHeap(&hd, __uuidof(ID3D12Heap), heap.out());
  report(hr, "clear heap");
  if (FAILED(hr)) {
    std::puts("OUTCOME unsupported clear_heap");
    return;
  }
  Ref<ID3D12Resource> textures[2];
  for (int i = 0; i < 2; ++i) {
    hr = c.device->CreatePlacedResource(
        heap.p, 0, &d, D3D12_RESOURCE_STATE_COMMON, nullptr,
        __uuidof(ID3D12Resource), textures[i].out());
    report(hr, "clear alias texture");
    if (FAILED(hr)) {
      std::puts("OUTCOME unsupported clear_texture");
      return;
    }
  }
  Ref<ID3D12DescriptorHeap> views;
  D3D12_DESCRIPTOR_HEAP_DESC vd{};
  vd.Type =
      depth ? D3D12_DESCRIPTOR_HEAP_TYPE_DSV : D3D12_DESCRIPTOR_HEAP_TYPE_RTV;
  vd.NumDescriptors = 2;
  require(c.device->CreateDescriptorHeap(&vd, __uuidof(ID3D12DescriptorHeap),
                                         views.out()),
          "clear views");
  auto start = views->GetCPUDescriptorHandleForHeapStart();
  UINT inc = c.device->GetDescriptorHandleIncrementSize(vd.Type);
  D3D12_CPU_DESCRIPTOR_HANDLE handles[2] = {{start.ptr}, {start.ptr + inc}};
  for (int i = 0; i < 2; ++i)
    if (depth)
      c.device->CreateDepthStencilView(textures[i].p, nullptr, handles[i]);
    else
      c.device->CreateRenderTargetView(textures[i].p, nullptr, handles[i]);
  D3D12_PLACED_SUBRESOURCE_FOOTPRINT fp{};
  UINT rows = 0;
  UINT64 rowBytes = 0, total = 0;
  c.device->GetCopyableFootprints(&d, 0, 1, 0, &fp, &rows, &rowBytes, &total);
  Buffer read[2];
  for (auto &b : read) {
    b.allocate(c.device.p, total, "readback");
    b.map();
    std::memset(b.cpu, 0xA7, total);
  }
  Queue q(c.device.p);
  q.begin();
  for (int i = 0; i < 2; ++i) {
    activate(q, textures[i].p, i ? textures[0].p : nullptr);
    auto state = depth ? D3D12_RESOURCE_STATE_DEPTH_WRITE
                       : D3D12_RESOURCE_STATE_RENDER_TARGET;
    q.transition(textures[i].p, D3D12_RESOURCE_STATE_COMMON, state);
    if (depth)
      q.list->ClearDepthStencilView(handles[i], D3D12_CLEAR_FLAG_DEPTH,
                                    i ? 0.25f : 0.5f, 0, 0, nullptr);
    else {
      FLOAT color[4] = {i ? 0.0f : 1.0f, i ? 1.0f : 0.0f, 1.0f, 1.0f};
      q.list->ClearRenderTargetView(handles[i], color, 0, nullptr);
    }
    q.transition(textures[i].p, state, D3D12_RESOURCE_STATE_COPY_SOURCE);
    copyOut(q, textures[i].p, 0, read[i].resource.p, fp);
    q.transition(textures[i].p, D3D12_RESOURCE_STATE_COPY_SOURCE,
                 D3D12_RESOURCE_STATE_COMMON);
  }
  q.finish();
  for (int i = 0; i < 2; ++i) {
    UINT expected = depth ? (i ? 0x3E800000u : 0x3F000000u)
                          : (i ? 0xFFFFFF00u : 0xFFFF00FFu);
    UINT matches = 0;
    for (UINT y = 0; y < 37; ++y) {
      auto p = reinterpret_cast<volatile uint32_t *>(
          static_cast<unsigned char *>(read[i].cpu) + fp.Offset +
          UINT64(y) * fp.Footprint.RowPitch);
      for (UINT x = 0; x < 65; ++x)
        matches += p[x] == expected;
    }
    std::printf("OUTCOME clear_alias type=%s depth=%d alias=%d expected=%08X "
                "matches=%u/2405\n",
                type.c_str(), depth, i, expected, matches);
    if (matches != 2405)
      throw std::runtime_error("clear/alias mismatch");
  }
}
static UINT rejectedPlacements(Context &c) {
  UINT violations = 0;
  Ref<ID3D12Heap> h;
  D3D12_HEAP_DESC hd{};
  hd.Properties = heapProperties("wb");
  hd.SizeInBytes = 65536;
  hd.Alignment = 65536;
  hd.Flags = D3D12_HEAP_FLAG_ALLOW_ONLY_BUFFERS;
  require(c.device->CreateHeap(&hd, __uuidof(ID3D12Heap), h.out()),
          "negative heap");
  auto b = bufferDesc(65536);
  for (UINT64 offset : {1ULL, 65536ULL}) {
    Ref<ID3D12Resource> r;
    HRESULT hr = c.device->CreatePlacedResource(
        h.p, offset, &b, D3D12_RESOURCE_STATE_COMMON, nullptr,
        __uuidof(ID3D12Resource), r.out());
    std::printf("NEGATIVE placement offset=%llu HRESULT=0x%08lX rejected=%d\n",
                offset, (unsigned long)hr, FAILED(hr));
    if (SUCCEEDED(hr))
      ++violations;
  }
  auto t = textureDesc(DXGI_FORMAT_R8G8B8A8_UNORM, D3D12_RESOURCE_FLAG_NONE, 1);
  Ref<ID3D12Resource> r;
  HRESULT hr = c.device->CreatePlacedResource(
      h.p, 0, &t, D3D12_RESOURCE_STATE_COMMON, nullptr,
      __uuidof(ID3D12Resource), r.out());
  std::printf("NEGATIVE texture_on_buffer_only HRESULT=0x%08lX rejected=%d\n",
              (unsigned long)hr, FAILED(hr));
  if (SUCCEEDED(hr))
    ++violations;
  std::printf("NEGATIVE_SUMMARY unexpected_successes=%u/3\n", violations);
  return violations;
}
int main(int argc, char **argv) {
  setvbuf(stdout, nullptr, _IONBF, 0);
  try {
    if (argc != 2)
      throw std::runtime_error("usage: textures ROUND");
    UINT round = UINT(std::strtoul(argv[1], nullptr, 10));
    Context c;
    identity(c);
    stage("baseline");
    D3D12_FEATURE_DATA_D3D12_OPTIONS opts{};
    require(c.device->CheckFeatureSupport(D3D12_FEATURE_D3D12_OPTIONS, &opts,
                                          sizeof(opts)),
            "heap tier");
    if (opts.ResourceHeapTier < D3D12_RESOURCE_HEAP_TIER_2)
      std::puts("MIXED NOT_RUN tier_below_2");
    else
      for (const std::string type : {"wb", "wc", "default"}) {
        mixed(c, type, DXGI_FORMAT_R8G8B8A8_UNORM, 4, round);
        mixed(c, type, DXGI_FORMAT_R8_UNORM, 1, round);
        mixed(c, type, DXGI_FORMAT_R32_UINT, 4, round);
      }
    for (const std::string type : {"wb", "default"}) {
      clearAlias(c, type, false);
      clearAlias(c, type, true);
    }
    UINT violations = rejectedPlacements(c);
    stage("resources_released");
    if (violations)
      throw std::runtime_error("invalid creations accepted");
    std::puts("END success");
    return 0;
  } catch (const std::exception &e) {
    std::printf("END failure reason=%s\n", e.what());
    return 1;
  }
}

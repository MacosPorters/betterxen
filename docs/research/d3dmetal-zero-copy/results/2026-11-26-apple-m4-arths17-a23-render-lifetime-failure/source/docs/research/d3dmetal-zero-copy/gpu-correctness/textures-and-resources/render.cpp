// SPDX-License-Identifier: GPL-3.0-or-later
// Original tiny graphics resource tests: sampling, depth, stencil; no game
// shaders.
#include "../../allocation-profiling/support.h"
static constexpr UINT width = 32, height = 16;
static void stage(const char *name) {
  std::printf("STAGE %s cpu_ptr=0\n", name);
  if (std::getchar() == EOF)
    throw std::runtime_error("stage handshake");
}
static void activate(Queue &q, ID3D12Resource *r) {
  D3D12_RESOURCE_BARRIER b{};
  b.Type = D3D12_RESOURCE_BARRIER_TYPE_ALIASING;
  b.Aliasing.pResourceAfter = r;
  q.list->ResourceBarrier(1, &b);
}
static UINT pixel(UINT x, UINT y, UINT variant) {
  UINT r = (x < 8) ? 255 : 0, g = (x >= 8) ? 255 : 0, b = (y >= 8) ? 255 : 0;
  if (variant & 1)
    std::swap(r, b);
  return 0xFF000000u | r | (g << 8) | (b << 16);
}
struct Texture {
  Ref<ID3D12Heap> heap;
  Ref<ID3D12Resource> resource;
  D3D12_RESOURCE_DESC desc{};
};
static void texture(Context &c, Texture &t, UINT w, UINT h, DXGI_FORMAT format,
                    D3D12_RESOURCE_FLAGS flags, const std::string &storage) {
  t.desc.Dimension = D3D12_RESOURCE_DIMENSION_TEXTURE2D;
  t.desc.Width = w;
  t.desc.Height = h;
  t.desc.DepthOrArraySize = 1;
  t.desc.MipLevels = 1;
  t.desc.Format = format;
  t.desc.SampleDesc.Count = 1;
  t.desc.Layout = D3D12_TEXTURE_LAYOUT_UNKNOWN;
  t.desc.Flags = flags;
  auto ai = c.device->GetResourceAllocationInfo(0, 1, &t.desc);
  if (ai.SizeInBytes == ~UINT64(0) || !ai.Alignment)
    throw std::runtime_error("graphics allocation info");
  D3D12_HEAP_DESC hd{};
  hd.SizeInBytes = ai.SizeInBytes;
  hd.Alignment = ai.Alignment;
  hd.Properties = properties(storage == "default" ? D3D12_HEAP_TYPE_DEFAULT
                                                  : D3D12_HEAP_TYPE_CUSTOM);
  if (storage != "default") {
    hd.Properties.CPUPageProperty = storage == "wc"
                                        ? D3D12_CPU_PAGE_PROPERTY_WRITE_COMBINE
                                        : D3D12_CPU_PAGE_PROPERTY_WRITE_BACK;
    hd.Properties.MemoryPoolPreference = D3D12_MEMORY_POOL_L0;
  }
  hd.Flags = flags == D3D12_RESOURCE_FLAG_NONE
                 ? D3D12_HEAP_FLAG_ALLOW_ONLY_NON_RT_DS_TEXTURES
                 : D3D12_HEAP_FLAG_ALLOW_ONLY_RT_DS_TEXTURES;
  require(c.device->CreateHeap(&hd, __uuidof(ID3D12Heap), t.heap.out()),
          "graphics texture heap");
  require(c.device->CreatePlacedResource(
              t.heap.p, 0, &t.desc, D3D12_RESOURCE_STATE_COMMON, nullptr,
              __uuidof(ID3D12Resource), t.resource.out()),
          "graphics placed texture");
}
static Ref<ID3DBlob> *compile(Context &c, const char *name, const char *source,
                              const char *target, Ref<ID3DBlob> &binary) {
  (void)c;
  HMODULE compiler = LoadLibraryA("d3dcompiler_47.dll");
  if (!compiler)
    throw std::runtime_error("shader compiler");
  using Compile = HRESULT(WINAPI *)(
      LPCVOID, SIZE_T, LPCSTR, const D3D_SHADER_MACRO *, ID3DInclude *, LPCSTR,
      LPCSTR, UINT, UINT, ID3DBlob **, ID3DBlob **);
  Ref<ID3DBlob> errors;
  HRESULT h = symbol<Compile>(compiler, "D3DCompile")(
      source, std::strlen(source), name, nullptr, nullptr, "main", target,
      D3DCOMPILE_OPTIMIZATION_LEVEL3, 0, &binary.p, &errors.p);
  if (errors.p)
    std::printf("COMPILER_LOG %s %.*s\n", name, int(errors->GetBufferSize()),
                static_cast<char *>(errors->GetBufferPointer()));
  require(h, "compile graphics shader");
  for (int code = 0; code < 2; ++code) {
    std::string file = std::string(name) + (code ? ".dxbc" : ".hlsl");
    FILE *f = std::fopen(file.c_str(), "wb");
    if (!f)
      throw std::runtime_error("archive shader");
    const void *p = code ? binary->GetBufferPointer() : source;
    size_t bytes = code ? binary->GetBufferSize() : std::strlen(source);
    if (std::fwrite(p, 1, bytes, f) != bytes)
      throw std::runtime_error("archive shader bytes");
    std::fclose(f);
  }
  FreeLibrary(compiler);
  return &binary;
}
struct Graphics {
  Ref<ID3DBlob> vs, ps;
  Ref<ID3D12RootSignature> root;
  explicit Graphics(Context &c) {
    const char *vertex = R"HLSL(cbuffer Depth : register(b0) { float z; };
struct V { float4 position : SV_Position; float2 uv : TEXCOORD0; };
V main(uint id : SV_VertexID) { V o; float2 p=float2((id<<1)&2,id&2); o.uv=p; o.position=float4(p*float2(2,-2)+float2(-1,1),z,1); return o; })HLSL";
    const char *fragment =
        R"HLSL(Texture2D<float4> image : register(t0); SamplerState pointClamp : register(s0);
float4 main(float4 position : SV_Position, float2 uv : TEXCOORD0) : SV_Target { return image.SampleLevel(pointClamp,uv,0); })HLSL";
    compile(c, "sample_vs", vertex, "vs_5_0", vs);
    compile(c, "sample_ps", fragment, "ps_5_0", ps);
    D3D12_DESCRIPTOR_RANGE range{};
    range.RangeType = D3D12_DESCRIPTOR_RANGE_TYPE_SRV;
    range.NumDescriptors = 1;
    range.OffsetInDescriptorsFromTableStart =
        D3D12_DESCRIPTOR_RANGE_OFFSET_APPEND;
    D3D12_ROOT_PARAMETER params[2]{};
    params[0].ParameterType = D3D12_ROOT_PARAMETER_TYPE_DESCRIPTOR_TABLE;
    params[0].DescriptorTable = {1, &range};
    params[0].ShaderVisibility = D3D12_SHADER_VISIBILITY_PIXEL;
    params[1].ParameterType = D3D12_ROOT_PARAMETER_TYPE_32BIT_CONSTANTS;
    params[1].Constants.Num32BitValues = 1;
    params[1].ShaderVisibility = D3D12_SHADER_VISIBILITY_VERTEX;
    D3D12_STATIC_SAMPLER_DESC sampler{};
    sampler.Filter = D3D12_FILTER_MIN_MAG_MIP_POINT;
    sampler.AddressU = sampler.AddressV = sampler.AddressW =
        D3D12_TEXTURE_ADDRESS_MODE_CLAMP;
    sampler.ComparisonFunc = D3D12_COMPARISON_FUNC_NEVER;
    sampler.MaxLOD = D3D12_FLOAT32_MAX;
    sampler.MaxAnisotropy = 1;
    sampler.ShaderVisibility = D3D12_SHADER_VISIBILITY_PIXEL;
    D3D12_ROOT_SIGNATURE_DESC d{};
    d.NumParameters = 2;
    d.pParameters = params;
    d.NumStaticSamplers = 1;
    d.pStaticSamplers = &sampler;
    d.Flags = D3D12_ROOT_SIGNATURE_FLAG_ALLOW_INPUT_ASSEMBLER_INPUT_LAYOUT;
    using Serialize =
        HRESULT(WINAPI *)(const D3D12_ROOT_SIGNATURE_DESC *,
                          D3D_ROOT_SIGNATURE_VERSION, ID3DBlob **, ID3DBlob **);
    Ref<ID3DBlob> serialized, error;
    require(symbol<Serialize>(c.dll, "D3D12SerializeRootSignature")(
                &d, D3D_ROOT_SIGNATURE_VERSION_1, &serialized.p, &error.p),
            "graphics root serialization");
    require(c.device->CreateRootSignature(
                0, serialized->GetBufferPointer(), serialized->GetBufferSize(),
                __uuidof(ID3D12RootSignature), root.out()),
            "graphics root");
  }
  void pipeline(Context &c, int mode, Ref<ID3D12PipelineState> &state) {
    D3D12_GRAPHICS_PIPELINE_STATE_DESC p{};
    p.pRootSignature = root.p;
    p.VS = {vs->GetBufferPointer(), vs->GetBufferSize()};
    p.PS = {ps->GetBufferPointer(), ps->GetBufferSize()};
    p.SampleMask = UINT_MAX;
    p.PrimitiveTopologyType = D3D12_PRIMITIVE_TOPOLOGY_TYPE_TRIANGLE;
    p.NumRenderTargets = 1;
    p.RTVFormats[0] = DXGI_FORMAT_R8G8B8A8_UNORM;
    p.SampleDesc.Count = 1;
    auto &blend = p.BlendState.RenderTarget[0];
    blend.SrcBlend = blend.SrcBlendAlpha = D3D12_BLEND_ONE;
    blend.DestBlend = blend.DestBlendAlpha = D3D12_BLEND_ZERO;
    blend.BlendOp = blend.BlendOpAlpha = D3D12_BLEND_OP_ADD;
    blend.LogicOp = D3D12_LOGIC_OP_NOOP;
    blend.RenderTargetWriteMask = mode == 3 ? 0 : D3D12_COLOR_WRITE_ENABLE_ALL;
    p.RasterizerState.FillMode = D3D12_FILL_MODE_SOLID;
    p.RasterizerState.CullMode = D3D12_CULL_MODE_NONE;
    p.RasterizerState.DepthClipEnable = TRUE;
    auto &ds = p.DepthStencilState;
    ds.DepthEnable = mode == 1;
    ds.DepthWriteMask =
        mode == 1 ? D3D12_DEPTH_WRITE_MASK_ALL : D3D12_DEPTH_WRITE_MASK_ZERO;
    ds.DepthFunc =
        mode == 1 ? D3D12_COMPARISON_FUNC_LESS : D3D12_COMPARISON_FUNC_ALWAYS;
    ds.StencilEnable = mode >= 2;
    ds.StencilReadMask = ds.StencilWriteMask = 255;
    ds.FrontFace.StencilFailOp = ds.FrontFace.StencilDepthFailOp =
        D3D12_STENCIL_OP_KEEP;
    ds.FrontFace.StencilPassOp =
        mode == 3 ? D3D12_STENCIL_OP_INCR_SAT : D3D12_STENCIL_OP_KEEP;
    ds.FrontFace.StencilFunc =
        mode == 3 ? D3D12_COMPARISON_FUNC_ALWAYS : D3D12_COMPARISON_FUNC_EQUAL;
    ds.BackFace = ds.FrontFace;
    p.DSVFormat = mode == 1   ? DXGI_FORMAT_D32_FLOAT
                  : mode >= 2 ? DXGI_FORMAT_D24_UNORM_S8_UINT
                              : DXGI_FORMAT_UNKNOWN;
    require(c.device->CreateGraphicsPipelineState(
                &p, __uuidof(ID3D12PipelineState), state.out()),
            "graphics PSO");
  }
};
static void runCase(Context &c, Graphics &graphics, const std::string &storage,
                    int mode, UINT variant, bool omit) {
  std::printf("GRAPHICS_CASE source=%s mode=%d variant=%u omitted_draw=%d\n",
              storage.c_str(), mode, variant, omit);
  Texture source, target, depth;
  texture(c, source, 16, 16, DXGI_FORMAT_R8G8B8A8_UNORM,
          D3D12_RESOURCE_FLAG_NONE, storage);
  texture(c, target, width, height, DXGI_FORMAT_R8G8B8A8_UNORM,
          D3D12_RESOURCE_FLAG_ALLOW_RENDER_TARGET, "default");
  if (mode)
    texture(c, depth, width, height,
            mode == 1 ? DXGI_FORMAT_D32_FLOAT : DXGI_FORMAT_D24_UNORM_S8_UINT,
            D3D12_RESOURCE_FLAG_ALLOW_DEPTH_STENCIL, "default");
  std::vector<UINT> pixels(256);
  for (UINT y = 0; y < 16; ++y)
    for (UINT x = 0; x < 16; ++x)
      pixels[y * 16 + x] = pixel(x, y, variant);
  Buffer upload;
  D3D12_PLACED_SUBRESOURCE_FOOTPRINT inFp{};
  UINT rows;
  UINT64 rowBytes, total;
  c.device->GetCopyableFootprints(&source.desc, 0, 1, 0, &inFp, &rows,
                                  &rowBytes, &total);
  if (storage == "default") {
    upload.allocate(c.device.p, total, "upload");
    upload.map();
    for (UINT y = 0; y < 16; ++y)
      std::memcpy(static_cast<unsigned char *>(upload.cpu) + inFp.Offset +
                      UINT64(y) * inFp.Footprint.RowPitch,
                  pixels.data() + y * 16, 64);
  } else {
    require(source.resource->Map(0, nullptr, nullptr), "sampling texture Map");
    require(source.resource->WriteToSubresource(0, nullptr, pixels.data(), 64,
                                                1024),
            "sampling texture CPU write");
    source.resource->Unmap(0, nullptr);
  }
  Ref<ID3D12DescriptorHeap> srv, rtv, dsv;
  D3D12_DESCRIPTOR_HEAP_DESC dh{};
  dh.Type = D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV;
  dh.NumDescriptors = 1;
  dh.Flags = D3D12_DESCRIPTOR_HEAP_FLAG_SHADER_VISIBLE;
  require(c.device->CreateDescriptorHeap(&dh, __uuidof(ID3D12DescriptorHeap),
                                         srv.out()),
          "SRV heap");
  D3D12_SHADER_RESOURCE_VIEW_DESC sd{};
  sd.Format = DXGI_FORMAT_R8G8B8A8_UNORM;
  sd.ViewDimension = D3D12_SRV_DIMENSION_TEXTURE2D;
  sd.Shader4ComponentMapping = D3D12_DEFAULT_SHADER_4_COMPONENT_MAPPING;
  sd.Texture2D.MipLevels = 1;
  c.device->CreateShaderResourceView(source.resource.p, &sd,
                                     srv->GetCPUDescriptorHandleForHeapStart());
  dh.Type = D3D12_DESCRIPTOR_HEAP_TYPE_RTV;
  dh.Flags = D3D12_DESCRIPTOR_HEAP_FLAG_NONE;
  require(c.device->CreateDescriptorHeap(&dh, __uuidof(ID3D12DescriptorHeap),
                                         rtv.out()),
          "RTV heap");
  auto rh = rtv->GetCPUDescriptorHandleForHeapStart();
  c.device->CreateRenderTargetView(target.resource.p, nullptr, rh);
  D3D12_CPU_DESCRIPTOR_HANDLE zh{};
  if (mode) {
    dh.Type = D3D12_DESCRIPTOR_HEAP_TYPE_DSV;
    require(c.device->CreateDescriptorHeap(&dh, __uuidof(ID3D12DescriptorHeap),
                                           dsv.out()),
            "DSV heap");
    zh = dsv->GetCPUDescriptorHandleForHeapStart();
    c.device->CreateDepthStencilView(depth.resource.p, nullptr, zh);
  }
  Ref<ID3D12PipelineState> pso, writeStencil;
  graphics.pipeline(c, mode, pso);
  if (mode == 2)
    graphics.pipeline(c, 3, writeStencil);
  D3D12_PLACED_SUBRESOURCE_FOOTPRINT fp{};
  c.device->GetCopyableFootprints(&target.desc, 0, 1, 512, &fp, &rows,
                                  &rowBytes, &total);
  total = std::max(
      total, fp.Offset + UINT64(height - 1) * fp.Footprint.RowPitch + rowBytes);
  Buffer read;
  read.allocate(c.device.p, total, "readback");
  read.map();
  Queue q(c.device.p);
  q.begin();
  activate(q, source.resource.p);
  activate(q, target.resource.p);
  if (mode)
    activate(q, depth.resource.p);
  if (storage == "default") {
    q.transition(source.resource.p, D3D12_RESOURCE_STATE_COMMON,
                 D3D12_RESOURCE_STATE_COPY_DEST);
    D3D12_TEXTURE_COPY_LOCATION from{}, to{};
    from.pResource = upload.resource.p;
    from.Type = D3D12_TEXTURE_COPY_TYPE_PLACED_FOOTPRINT;
    from.PlacedFootprint = inFp;
    to.pResource = source.resource.p;
    to.Type = D3D12_TEXTURE_COPY_TYPE_SUBRESOURCE_INDEX;
    q.list->CopyTextureRegion(&to, 0, 0, 0, &from, nullptr);
    q.transition(source.resource.p, D3D12_RESOURCE_STATE_COPY_DEST,
                 D3D12_RESOURCE_STATE_PIXEL_SHADER_RESOURCE);
  } else
    q.transition(source.resource.p, D3D12_RESOURCE_STATE_COMMON,
                 D3D12_RESOURCE_STATE_PIXEL_SHADER_RESOURCE);
  if (mode)
    q.transition(depth.resource.p, D3D12_RESOURCE_STATE_COMMON,
                 D3D12_RESOURCE_STATE_DEPTH_WRITE);
  q.finish();
  for (int pass = 0; pass < (mode ? 2 : 1); ++pass) {
    bool visible = mode ? pass == 1 : true;
    float z = mode == 1 ? (visible ? 0.25f : 0.75f) : 0.25f;
    UINT zbits;
    std::memcpy(&zbits, &z, 4);
    std::memset(read.cpu, 0xA7, total);
    q.begin();
    q.transition(target.resource.p, D3D12_RESOURCE_STATE_COMMON,
                 D3D12_RESOURCE_STATE_RENDER_TARGET);
    FLOAT black[] = {0, 0, 0, 0};
    q.list->ClearRenderTargetView(rh, black, 0, nullptr);
    if (mode)
      q.list->ClearDepthStencilView(zh,
                                    mode == 1 ? D3D12_CLEAR_FLAG_DEPTH
                                              : D3D12_CLEAR_FLAG_DEPTH |
                                                    D3D12_CLEAR_FLAG_STENCIL,
                                    0.5f, 0, 0, nullptr);
    q.list->OMSetRenderTargets(1, &rh, FALSE, mode ? &zh : nullptr);
    D3D12_VIEWPORT vp{0, 0, float(width), float(height), 0, 1};
    D3D12_RECT sc{0, 0, LONG(width), LONG(height)};
    q.list->RSSetViewports(1, &vp);
    q.list->RSSetScissorRects(1, &sc);
    q.list->SetGraphicsRootSignature(graphics.root.p);
    ID3D12DescriptorHeap *heaps[] = {srv.p};
    q.list->SetDescriptorHeaps(1, heaps);
    q.list->SetGraphicsRootDescriptorTable(
        0, srv->GetGPUDescriptorHandleForHeapStart());
    q.list->SetGraphicsRoot32BitConstant(1, zbits, 0);
    q.list->IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
    if (mode == 2) {
      q.list->SetPipelineState(writeStencil.p);
      q.list->OMSetStencilRef(0);
      q.list->DrawInstanced(3, 1, 0, 0);
      q.list->OMSetStencilRef(visible ? 1 : 2);
    }
    q.list->SetPipelineState(pso.p);
    if (!omit)
      q.list->DrawInstanced(3, 1, 0, 0);
    q.transition(target.resource.p, D3D12_RESOURCE_STATE_RENDER_TARGET,
                 D3D12_RESOURCE_STATE_COPY_SOURCE);
    D3D12_TEXTURE_COPY_LOCATION from{}, to{};
    from.pResource = target.resource.p;
    from.Type = D3D12_TEXTURE_COPY_TYPE_SUBRESOURCE_INDEX;
    to.pResource = read.resource.p;
    to.Type = D3D12_TEXTURE_COPY_TYPE_PLACED_FOOTPRINT;
    to.PlacedFootprint = fp;
    q.list->CopyTextureRegion(&to, 0, 0, 0, &from, nullptr);
    q.transition(target.resource.p, D3D12_RESOURCE_STATE_COPY_SOURCE,
                 D3D12_RESOURCE_STATE_COMMON);
    q.finish();
    UINT matches = 0;
    for (UINT y = 0; y < height; ++y) {
      auto p = reinterpret_cast<volatile UINT *>(
          static_cast<unsigned char *>(read.cpu) + fp.Offset +
          UINT64(y) * fp.Footprint.RowPitch);
      for (UINT x = 0; x < width; ++x) {
        UINT expected = visible ? pixel(x / 2, y, variant) : 0;
        matches += p[x] == expected;
      }
    }
    std::printf(
        "GRAPHICS_VERIFY source=%s mode=%d pass=%d visible=%d matches=%u/512\n",
        storage.c_str(), mode, pass, visible, matches);
    if (matches != 512)
      throw std::runtime_error("sampled pixels mismatch");
  }
}
int main(int argc, char **argv) {
  setvbuf(stdout, nullptr, _IONBF, 0);
  try {
    if (argc != 2)
      throw std::runtime_error("usage: render SESSION|omit");
    bool omit = std::string(argv[1]) == "omit";
    UINT variant = omit ? 0 : UINT(std::strtoul(argv[1], nullptr, 10));
    Context c;
    identity(c);
    stage("baseline");
    Graphics graphics(c);
    if (omit)
      runCase(c, graphics, "default", 0, variant, true);
    else {
      for (const std::string storage : {"wb", "wc", "default"}) {
        stage(storage.c_str());
        runCase(c, graphics, storage, 0, variant, false);
      }
      stage("depth");
      runCase(c, graphics, "default", 1, variant, false);
      stage("stencil");
      runCase(c, graphics, "default", 2, variant, false);
    }
    stage("gpu_complete");
    std::puts("END success");
    return 0;
  } catch (const std::exception &e) {
    std::printf("END failure reason=%s\n", e.what());
    return 1;
  }
}

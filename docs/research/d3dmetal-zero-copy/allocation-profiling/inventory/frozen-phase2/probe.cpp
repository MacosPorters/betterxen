// SPDX-License-Identifier: GPL-3.0-or-later
// Supplemental original synthetic tests; baseline ../probe.cpp is unchanged.
#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include <windows.h>
#include <d3d12.h>
#include <dxgi1_4.h>
#include <d3dcompiler.h>
#include <emmintrin.h>
#include <algorithm>
#include <array>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <stdexcept>
#include <string>
#include <vector>

template<class T> struct Ref{
    T* p=nullptr;Ref()=default;Ref(const Ref&)=delete;Ref& operator=(const Ref&)=delete;
    ~Ref(){clear();} void clear(){if(p){p->Release();p=nullptr;}}
    T* operator->()const{return p;} void** out(){return reinterpret_cast<void**>(&p);}
};
static void require(HRESULT h,const char* name){
    if(FAILED(h)){std::printf("FAIL %s HRESULT=0x%08lX\n",name,(unsigned long)h);throw std::runtime_error(name);}
}
static void report(HRESULT h,const char* name){std::printf("API %s HRESULT=0x%08lX\n",name,(unsigned long)h);}
template<class T> static T symbol(HMODULE m,const char* name){
    FARPROC p=GetProcAddress(m,name);if(!p)throw std::runtime_error(name);
    T result;static_assert(sizeof(result)==sizeof(p));std::memcpy(&result,&p,sizeof(p));return result;
}
static double ticks(){LARGE_INTEGER t;QueryPerformanceCounter(&t);return double(t.QuadPart);}
static double frequency(){LARGE_INTEGER t;QueryPerformanceFrequency(&t);return double(t.QuadPart);}
static D3D12_HEAP_PROPERTIES properties(D3D12_HEAP_TYPE type){
    D3D12_HEAP_PROPERTIES p{};p.Type=type;p.CreationNodeMask=1;p.VisibleNodeMask=1;return p;
}
static D3D12_RESOURCE_DESC bufferDesc(UINT64 bytes,D3D12_RESOURCE_FLAGS flags=D3D12_RESOURCE_FLAG_NONE){
    D3D12_RESOURCE_DESC r{};r.Dimension=D3D12_RESOURCE_DIMENSION_BUFFER;r.Width=bytes;r.Height=1;r.DepthOrArraySize=1;r.MipLevels=1;r.SampleDesc.Count=1;r.Layout=D3D12_TEXTURE_LAYOUT_ROW_MAJOR;r.Flags=flags;return r;
}
struct Context{
    HMODULE dll=nullptr;Ref<ID3D12Device3> device;
    Context(){
        dll=LoadLibraryA("d3d12.dll");if(!dll)throw std::runtime_error("LoadLibrary d3d12");
        char path[2048]{};GetModuleFileNameA(dll,path,sizeof(path));std::printf("D3D12_DLL %s\n",path);
        using Create=HRESULT(WINAPI*)(IUnknown*,D3D_FEATURE_LEVEL,REFIID,void**);
        require(symbol<Create>(dll,"D3D12CreateDevice")(nullptr,D3D_FEATURE_LEVEL_11_0,__uuidof(ID3D12Device3),device.out()),"CreateDevice3");
        bool shim=GetModuleHandleA("d3d12_xs.dll")!=nullptr;std::printf("SHIM_LOADED %d\n",shim);if(shim)throw std::runtime_error("shim contamination");
    }
};
struct Queue{
    ID3D12Device3* d;Ref<ID3D12CommandQueue> q;Ref<ID3D12CommandAllocator> allocator;Ref<ID3D12GraphicsCommandList> list;Ref<ID3D12Fence> fence;
    HANDLE event=nullptr;UINT64 value=0;
    explicit Queue(ID3D12Device3* device):d(device){
        D3D12_COMMAND_QUEUE_DESC desc{};desc.Type=D3D12_COMMAND_LIST_TYPE_DIRECT;
        require(d->CreateCommandQueue(&desc,__uuidof(ID3D12CommandQueue),q.out()),"CreateCommandQueue");
        require(d->CreateCommandAllocator(desc.Type,__uuidof(ID3D12CommandAllocator),allocator.out()),"CreateCommandAllocator");
        require(d->CreateCommandList(0,desc.Type,allocator.p,nullptr,__uuidof(ID3D12GraphicsCommandList),list.out()),"CreateCommandList");
        require(list->Close(),"initial Close");require(d->CreateFence(0,D3D12_FENCE_FLAG_NONE,__uuidof(ID3D12Fence),fence.out()),"CreateFence");
        event=CreateEventW(nullptr,FALSE,FALSE,nullptr);if(!event)throw std::runtime_error("CreateEvent");
    }
    ~Queue(){if(event)CloseHandle(event);}
    void begin(){require(allocator->Reset(),"allocator Reset");require(list->Reset(allocator.p,nullptr),"list Reset");}
    void transition(ID3D12Resource* r,D3D12_RESOURCE_STATES a,D3D12_RESOURCE_STATES b){
        if(a==b)return;
        D3D12_RESOURCE_BARRIER x{};x.Type=D3D12_RESOURCE_BARRIER_TYPE_TRANSITION;x.Transition.pResource=r;x.Transition.StateBefore=a;x.Transition.StateAfter=b;x.Transition.Subresource=D3D12_RESOURCE_BARRIER_ALL_SUBRESOURCES;list->ResourceBarrier(1,&x);
    }
    void close(){require(list->Close(),"Close");}
    void executeClosed(){
        ID3D12CommandList* lists[]={list.p};q->ExecuteCommandLists(1,lists);require(q->Signal(fence.p,++value),"Signal");
        UINT64 completed=fence->GetCompletedValue();if(completed==~UINT64(0))throw std::runtime_error("device removed fence");
        if(completed<value){require(fence->SetEventOnCompletion(value,event),"SetEventOnCompletion");DWORD w=WaitForSingleObject(event,10000);if(w!=WAIT_OBJECT_0)throw std::runtime_error("fence timeout");}
    }
    void finish(){close();_mm_sfence();executeClosed();require(d->GetDeviceRemovedReason(),"GetDeviceRemovedReason");}
};
struct Buffer{
    Ref<ID3D12Heap> heap;Ref<ID3D12Resource> resource;void* cpu=nullptr;UINT64 size=0;
    void allocate(ID3D12Device3* d,UINT64 bytes,const std::string& type,bool uav=false){
        size=bytes;auto desc=bufferDesc(bytes,uav?D3D12_RESOURCE_FLAG_ALLOW_UNORDERED_ACCESS:D3D12_RESOURCE_FLAG_NONE);
        auto state=uav?D3D12_RESOURCE_STATE_COMMON:D3D12_RESOURCE_STATE_GENERIC_READ;
        if(type=="upload"||type=="readback"){
            if(uav)throw std::runtime_error("UAV on upload/readback forbidden in this probe");
            auto p=properties(type=="upload"?D3D12_HEAP_TYPE_UPLOAD:D3D12_HEAP_TYPE_READBACK);
            if(type=="readback")state=D3D12_RESOURCE_STATE_COPY_DEST;
            require(d->CreateCommittedResource(&p,D3D12_HEAP_FLAG_NONE,&desc,state,nullptr,__uuidof(ID3D12Resource),resource.out()),"committed buffer");
        }else{
            D3D12_HEAP_DESC hd{};hd.SizeInBytes=(bytes+65535)&~UINT64(65535);hd.Alignment=65536;hd.Properties=properties(D3D12_HEAP_TYPE_CUSTOM);hd.Properties.CPUPageProperty=type=="wc"?D3D12_CPU_PAGE_PROPERTY_WRITE_COMBINE:D3D12_CPU_PAGE_PROPERTY_WRITE_BACK;hd.Properties.MemoryPoolPreference=D3D12_MEMORY_POOL_L0;
            require(d->CreateHeap(&hd,__uuidof(ID3D12Heap),heap.out()),"custom heap");
            require(d->CreatePlacedResource(heap.p,0,&desc,state,nullptr,__uuidof(ID3D12Resource),resource.out()),"placed buffer");
        }
    }
    void map(){require(resource->Map(0,nullptr,&cpu),"Map buffer");if(!cpu)throw std::runtime_error("null buffer pointer");}
    void release(){cpu=nullptr;resource.clear();heap.clear();}
};
static void identity(Context& c){
    using Factory=HRESULT(WINAPI*)(UINT,REFIID,void**);
    HMODULE dxgi=LoadLibraryA("dxgi.dll");if(!dxgi)throw std::runtime_error("dxgi load");
    Ref<IDXGIFactory4> f;require(symbol<Factory>(dxgi,"CreateDXGIFactory2")(0,__uuidof(IDXGIFactory4),f.out()),"factory4");
    LUID luid=c.device->GetAdapterLuid();Ref<IDXGIAdapter2> a;require(f->EnumAdapterByLuid(luid,__uuidof(IDXGIAdapter2),a.out()),"adapter by device LUID");
    DXGI_ADAPTER_DESC2 ad{};require(a->GetDesc2(&ad),"GetDesc2");char name[512]{};WideCharToMultiByte(CP_UTF8,0,ad.Description,-1,name,sizeof(name),nullptr,nullptr);
    std::printf("ADAPTER description=%s vendor=0x%04X device=0x%04X subsystem=0x%08X revision=%u flags=%u dedicated_video_bytes=%llu dedicated_system_bytes=%llu shared_system_bytes=%llu luid=%08lX:%08lX\n",name,ad.VendorId,ad.DeviceId,ad.SubSysId,ad.Revision,ad.Flags,(unsigned long long)ad.DedicatedVideoMemory,(unsigned long long)ad.DedicatedSystemMemory,(unsigned long long)ad.SharedSystemMemory,(unsigned long)luid.HighPart,(unsigned long)luid.LowPart);
    D3D12_FEATURE_DATA_D3D12_OPTIONS opts{};require(c.device->CheckFeatureSupport(D3D12_FEATURE_D3D12_OPTIONS,&opts,sizeof(opts)),"options");std::printf("OPTIONS ResourceHeapTier=%d CrossAdapterRowMajorTextureSupported=%d StandardSwizzle64KBSupported=%d\n",opts.ResourceHeapTier,opts.CrossAdapterRowMajorTextureSupported,opts.StandardSwizzle64KBSupported);
    D3D12_FEATURE_DATA_ARCHITECTURE1 arch{};require(c.device->CheckFeatureSupport(D3D12_FEATURE_ARCHITECTURE1,&arch,sizeof(arch)),"architecture1");std::printf("ARCH UMA=%d CacheCoherentUMA=%d TileBasedRenderer=%d IsolatedMMU=%d\n",arch.UMA,arch.CacheCoherentUMA,arch.TileBasedRenderer,arch.IsolatedMMU);
    for(auto type:{D3D12_HEAP_TYPE_DEFAULT,D3D12_HEAP_TYPE_UPLOAD,D3D12_HEAP_TYPE_READBACK}){
        auto p=c.device->GetCustomHeapProperties(0,type);std::printf("CUSTOM_PROPERTIES input_type=%d Type=%d CPUPageProperty=%d MemoryPoolPreference=%d CreationNodeMask=%u VisibleNodeMask=%u\n",type,p.Type,p.CPUPageProperty,p.MemoryPoolPreference,p.CreationNodeMask,p.VisibleNodeMask);
    }
    SYSTEM_INFO si{};GetSystemInfo(&si);std::printf("GUEST_PAGE_SIZE %lu ALLOCATION_GRANULARITY %lu\n",si.dwPageSize,si.dwAllocationGranularity);
}
struct Shader{
    Ref<ID3D12RootSignature> root;Ref<ID3D12PipelineState> pipeline;
    static constexpr const char* code=R"HLSL(
ByteAddressBuffer source : register(t0);
RWByteAddressBuffer result : register(u0);
cbuffer Parameters : register(b0) { uint count; uint salt; uint wordStride; };
[numthreads(64,1,1)]
void main(uint3 tid : SV_DispatchThreadID) {
    uint accum=0;
    for(uint i=tid.x; i<count; i+=256) accum ^= source.Load(i*wordStride*4)+salt;
    result.Store(tid.x*4,accum);
}
)HLSL";
    explicit Shader(Context& c){
        HMODULE compiler=LoadLibraryA("d3dcompiler_47.dll");if(!compiler)throw std::runtime_error("d3dcompiler47 unavailable");
        char path[2048]{};GetModuleFileNameA(compiler,path,sizeof(path));std::printf("SHADER_COMPILER %s\n",path);
        using Compile=HRESULT(WINAPI*)(LPCVOID,SIZE_T,LPCSTR,const D3D_SHADER_MACRO*,ID3DInclude*,LPCSTR,LPCSTR,UINT,UINT,ID3DBlob**,ID3DBlob**);
        Ref<ID3DBlob> binary,error;
        HRESULT h=symbol<Compile>(compiler,"D3DCompile")(code,std::strlen(code),"original-phase2.hlsl",nullptr,nullptr,"main","cs_5_0",D3DCOMPILE_OPTIMIZATION_LEVEL3,0,&binary.p,&error.p);
        if(error.p)std::printf("COMPILER_LOG %.*s\n",(int)error->GetBufferSize(),(const char*)error->GetBufferPointer());
        require(h,"D3DCompile cs5_0");
        std::printf("SHADER_BYTECODE bytes=%zu magic=%.4s\n",binary->GetBufferSize(),(char*)binary->GetBufferPointer());
        D3D12_ROOT_PARAMETER params[3]{};
        params[0].ParameterType=D3D12_ROOT_PARAMETER_TYPE_SRV;params[0].Descriptor.ShaderRegister=0;
        params[1].ParameterType=D3D12_ROOT_PARAMETER_TYPE_UAV;params[1].Descriptor.ShaderRegister=0;
        params[2].ParameterType=D3D12_ROOT_PARAMETER_TYPE_32BIT_CONSTANTS;params[2].Constants.ShaderRegister=0;params[2].Constants.Num32BitValues=3;
        D3D12_ROOT_SIGNATURE_DESC desc{};desc.NumParameters=3;desc.pParameters=params;
        using Serialize=HRESULT(WINAPI*)(const D3D12_ROOT_SIGNATURE_DESC*,D3D_ROOT_SIGNATURE_VERSION,ID3DBlob**,ID3DBlob**);
        Ref<ID3DBlob> serialized,serializeError;require(symbol<Serialize>(c.dll,"D3D12SerializeRootSignature")(&desc,D3D_ROOT_SIGNATURE_VERSION_1,&serialized.p,&serializeError.p),"serialize root");
        require(c.device->CreateRootSignature(0,serialized->GetBufferPointer(),serialized->GetBufferSize(),__uuidof(ID3D12RootSignature),root.out()),"CreateRootSignature");
        D3D12_COMPUTE_PIPELINE_STATE_DESC pd{};pd.pRootSignature=root.p;pd.CS={binary->GetBufferPointer(),binary->GetBufferSize()};require(c.device->CreateComputePipelineState(&pd,__uuidof(ID3D12PipelineState),pipeline.out()),"CreateComputePipelineState");
    }
    void dispatch(Queue& q,Buffer& src,Buffer& dst,UINT count,UINT salt,UINT stride){
        q.begin();q.transition(dst.resource.p,D3D12_RESOURCE_STATE_COMMON,D3D12_RESOURCE_STATE_UNORDERED_ACCESS);
        q.list->SetPipelineState(pipeline.p);q.list->SetComputeRootSignature(root.p);q.list->SetComputeRootShaderResourceView(0,src.resource->GetGPUVirtualAddress());q.list->SetComputeRootUnorderedAccessView(1,dst.resource->GetGPUVirtualAddress());UINT values[]={count,salt,stride};q.list->SetComputeRoot32BitConstants(2,3,values,0);q.list->Dispatch(4,1,1);
        q.transition(dst.resource.p,D3D12_RESOURCE_STATE_UNORDERED_ACCESS,D3D12_RESOURCE_STATE_COMMON);q.finish();
    }
};
static uint32_t pattern(UINT64 i,UINT seed){uint32_t x=uint32_t(i)^seed;x^=x>>16;x*=0x7FEB352Du;x^=x>>15;x*=0x846CA68Bu;return x^(x>>16);}
static void compare(const char* label,const std::array<uint32_t,256>& expected,void* cpu){
    auto p=static_cast<volatile uint32_t*>(cpu);size_t n=0;for(size_t i=0;i<256;++i)n+=p[i]==expected[i];std::printf("VERIFY %s matches=%zu/256 first_actual=%08X first_expected=%08X\n",label,n,p[0],expected[0]);if(n!=256)throw std::runtime_error("GPU checksum mismatch");
}
static void stage(const char* name,void* pointer=nullptr){
    std::printf("STAGE %s cpu_ptr=%p\n",name,pointer);
    // Host snapshots happen while no GPU work is pending; never in latency windows.
    if(std::getchar()==EOF)throw std::runtime_error("stage handshake lost");
}
static void coherent(Context& c,bool omitDispatch=false){
    Shader shader(c);Queue q(c.device.p);Buffer src,dst;src.allocate(c.device.p,65536,"wb");src.map();dst.allocate(c.device.p,65536,"wb",true);dst.map();
    for(UINT round=0;round<3;++round){
        auto p=static_cast<volatile uint32_t*>(src.cpu);std::array<uint32_t,256> expected{};UINT salt=0x100103u*(round+1);
        for(UINT i=0;i<16384;++i){uint32_t v=pattern(i,0x3412198u+round);p[i]=v;expected[i%256]^=v+salt;}
        // Poison result on CPU before every dispatch so an omitted shader cannot pass.
        auto output=static_cast<volatile uint32_t*>(dst.cpu);for(UINT i=0;i<256;++i)output[i]=0xDEADC0DEu;_mm_sfence();
        if(!omitDispatch)shader.dispatch(q,src,dst,16384,salt,1);
        compare("direct_shader_same_cpu_pointer",expected,dst.cpu);
        std::printf("COHERENCE_ROUND %u input_ptr=%p output_ptr=%p shader_read_words=16384 shader_written_words=256\n",round,src.cpu,dst.cpu);
    }
}
static void memory(Context& c,const std::string& type,UINT64 bytes){
    // Warm shader, queue and tiny result resources BEFORE the baseline snapshot.
    Shader shader(c);Queue q(c.device.p);Buffer small,out;small.allocate(c.device.p,65536,"upload");small.map();std::memset(small.cpu,0,65536);out.allocate(c.device.p,65536,"wb",true);out.map();shader.dispatch(q,small,out,1,0,1);small.release();
    std::printf("MEMORY type=%s bytes=%llu\n",type.c_str(),bytes);stage("baseline");
    Buffer src;void* cpu=nullptr;
    if(type=="cpu"){cpu=VirtualAlloc(nullptr,bytes,MEM_COMMIT|MEM_RESERVE,PAGE_READWRITE);if(!cpu)throw std::runtime_error("VirtualAlloc control");}
    else src.allocate(c.device.p,bytes,type);
    stage("created");
    if(type!="cpu"){src.map();cpu=src.cpu;}stage("mapped",cpu);
    auto p=static_cast<volatile uint32_t*>(cpu);std::array<uint32_t,256> expected{};UINT64 words=bytes/4;
    for(UINT64 i=0;i<words;++i){uint32_t value=pattern(i,0x24681357u);p[i]=value;expected[i%256]^=value+0x10101u;}_mm_sfence();
    stage("fully_initialized",cpu);
    if(type!="cpu"){shader.dispatch(q,src,out,UINT(words),0x10101u,1);compare("full_allocation_shader_read",expected,out.cpu);}else std::puts("GPU_PASS NOT_RUN CPU_control");
    stage("gpu_complete",cpu);
    if(type=="cpu")VirtualFree(cpu,0,MEM_RELEASE);else src.release();
    stage("released");
}
static void dirty(Context& c,const std::string& type,UINT stride,UINT session){
    const UINT64 bytes=268435456;const UINT pages=UINT(bytes/stride);Shader shader(c);Queue copy(c.device.p),verify(c.device.p);Buffer src,out,read;
    src.allocate(c.device.p,bytes,type);src.map();out.allocate(c.device.p,65536,"wb",true);out.map();read.allocate(c.device.p,128,"readback");read.map();
    std::memset(src.cpu,0,bytes);_mm_sfence();std::vector<uint32_t> pageValues(pages,0);
    // Both heap types use identical GENERIC_READ source states and no source barriers.
    copy.begin();copy.list->CopyBufferRegion(read.resource.p,0,src.resource.p,0,64);copy.list->CopyBufferRegion(read.resource.p,64,src.resource.p,bytes-stride,64);copy.close();
    std::vector<UINT> ks={0,1,64,4096,pages};std::rotate(ks.begin(),ks.begin()+session%ks.size(),ks.end());
    for(UINT k:ks){
        std::vector<double> times;int matches=0;
        for(int iteration=-5;iteration<30;++iteration){
            double dirtyStart=ticks();
            for(UINT j=0;j<k;++j){UINT page=k==1?0:UINT(UINT64(j)*(pages-1)/(k-1));uint32_t value=pattern(UINT64(page)+UINT64(iteration+5)*pages,0xAB3000u+session*17+k);*reinterpret_cast<volatile uint32_t*>(static_cast<unsigned char*>(src.cpu)+UINT64(page)*stride)=value;pageValues[page]=value;}
            _mm_sfence();double dirtyMs=(ticks()-dirtyStart)*1000.0/frequency();double start=ticks();copy.executeClosed();double ms=(ticks()-start)*1000.0/frequency();require(c.device->GetDeviceRemovedReason(),"dirty device status");
            auto rp=static_cast<volatile uint32_t*>(read.cpu);bool sentinels=rp[0]==pageValues[0]&&rp[16]==pageValues[pages-1];for(int i=1;i<16;++i)sentinels=sentinels&&rp[i]==0&&rp[16+i]==0;
            std::array<uint32_t,256> expected{};for(UINT i=0;i<pages;++i)expected[i%256]^=pageValues[i]+0x321u;
            // Additional full page-first-word validation is outside the timed window.
            shader.dispatch(verify,src,out,pages,0x321u,stride/4);
            auto op=static_cast<volatile uint32_t*>(out.cpu);bool all=sentinels;for(UINT i=0;i<256;++i)all=all&&op[i]==expected[i];
            if(iteration>=0){times.push_back(ms);matches+=all;std::printf("DIRTY_SAMPLE type=%s stride=%u session=%u k=%u iteration=%d dirty_cpu_ms=%.6f ms=%.6f match=%d\n",type.c_str(),stride,session,k,iteration,dirtyMs,ms,all);}
            if(!all)throw std::runtime_error("dirty page validation");
        }
        std::sort(times.begin(),times.end());std::printf("DIRTY_RESULT type=%s bytes=%llu stride=%u total_pages=%u session=%u k=%u n=30 median_ms=%.6f p10_ms=%.6f p90_ms=%.6f matches=%d/30\n",type.c_str(),bytes,stride,pages,session,k,(times[14]+times[15])/2,times[2],times[26],matches);
    }
}
static void watch(){
    auto p=static_cast<unsigned char*>(VirtualAlloc(nullptr,65536,MEM_COMMIT|MEM_RESERVE|MEM_WRITE_WATCH,PAGE_READWRITE));if(!p)throw std::runtime_error("writewatch allocation");
    const std::vector<std::vector<UINT>> cases={{0},{4096},{12288},{16383},{16384},{0,4096},{0,4096,8192,12288},{12288,16384},{16383,16384}};
    for(size_t c=0;c<cases.size();++c){
        require(HRESULT_FROM_WIN32(ResetWriteWatch(p,65536)),"ResetWriteWatch");
        for(UINT off:cases[c])*reinterpret_cast<volatile unsigned char*>(p+off)=0x6B;
        PVOID addresses[64]{};ULONG_PTR count=64;DWORD gran=0;UINT rc=GetWriteWatch(WRITE_WATCH_FLAG_RESET,p,65536,addresses,&count,&gran);
        std::printf("WATCH case=%zu writes=",c);for(UINT off:cases[c])std::printf("%u,",off);std::printf(" return=%u count=%llu granularity=%lu dirty=",rc,(unsigned long long)count,gran);if(!rc)for(ULONG_PTR i=0;i<count;++i)std::printf("%llu,",(unsigned long long)(static_cast<unsigned char*>(addresses[i])-p));std::puts("");if(rc)throw std::runtime_error("GetWriteWatch failed");
        count=64;gran=0;rc=GetWriteWatch(0,p,65536,addresses,&count,&gran);std::printf("WATCH_AFTER_RESET case=%zu return=%u count=%llu granularity=%lu\n",c,rc,(unsigned long long)count,gran);if(rc||count)throw std::runtime_error("writewatch reset not empty");
    }
    VirtualFree(p,0,MEM_RELEASE);
}
static void textures(Context& c){
    Queue q(c.device.p);
    for(auto layout:{D3D12_TEXTURE_LAYOUT_UNKNOWN,D3D12_TEXTURE_LAYOUT_ROW_MAJOR}){
        D3D12_RESOURCE_DESC td{};td.Dimension=D3D12_RESOURCE_DIMENSION_TEXTURE2D;td.Width=65;td.Height=37;td.DepthOrArraySize=1;td.MipLevels=1;td.Format=DXGI_FORMAT_R8G8B8A8_UNORM;td.SampleDesc.Count=1;td.Layout=layout;
        if(layout==D3D12_TEXTURE_LAYOUT_ROW_MAJOR)td.Flags=D3D12_RESOURCE_FLAG_ALLOW_CROSS_ADAPTER;
        auto ai=c.device->GetResourceAllocationInfo(0,1,&td);std::printf("TEXTURE layout=%d allocation_bytes=%llu alignment=%llu\n",layout,ai.SizeInBytes,ai.Alignment);
        if(ai.SizeInBytes==~UINT64(0)||!ai.Alignment){std::puts("TEXTURE_UNSUPPORTED allocation info invalid");continue;}
        Ref<ID3D12Heap> h;D3D12_HEAP_DESC hd{};hd.SizeInBytes=ai.SizeInBytes;hd.Alignment=ai.Alignment;hd.Properties=properties(D3D12_HEAP_TYPE_CUSTOM);hd.Properties.CPUPageProperty=D3D12_CPU_PAGE_PROPERTY_WRITE_BACK;hd.Properties.MemoryPoolPreference=D3D12_MEMORY_POOL_L0;
        if(layout==D3D12_TEXTURE_LAYOUT_ROW_MAJOR){hd.Properties=properties(D3D12_HEAP_TYPE_DEFAULT);hd.Flags=D3D12_HEAP_FLAG_SHARED|D3D12_HEAP_FLAG_SHARED_CROSS_ADAPTER;}
        HRESULT heapResult=c.device->CreateHeap(&hd,__uuidof(ID3D12Heap),h.out());report(heapResult,"texture heap");if(FAILED(heapResult)){if(layout==D3D12_TEXTURE_LAYOUT_UNKNOWN)require(heapResult,"required UNKNOWN texture heap");std::puts("TEXTURE_NOT_RUN heap unsupported");continue;}Ref<ID3D12Resource> t;HRESULT created=c.device->CreatePlacedResource(h.p,0,&td,D3D12_RESOURCE_STATE_COMMON,nullptr,__uuidof(ID3D12Resource),t.out());report(created,"placed texture");if(FAILED(created))continue;
        // UNKNOWN: request access without asking runtime to disclose opaque layout.
        void* pointer=nullptr;HRESULT mapped=layout==D3D12_TEXTURE_LAYOUT_UNKNOWN?t->Map(0,nullptr,nullptr):S_OK;if(layout==D3D12_TEXTURE_LAYOUT_UNKNOWN)report(mapped,"texture Map");else std::puts("TEXTURE Map NOT_RUN CPU-inaccessible DEFAULT cross-adapter heap");std::printf("TEXTURE layout=%d returned_cpu_ptr=%p pointer_requested=%d\n",layout,pointer,0);if(FAILED(mapped))continue;
        std::vector<uint32_t> input(65*37);for(UINT i=0;i<input.size();++i)input[i]=pattern(i,0x12345678u);
        if(layout==D3D12_TEXTURE_LAYOUT_UNKNOWN){HRESULT w=t->WriteToSubresource(0,nullptr,input.data(),65*4,65*37*4);report(w,"WriteToSubresource");require(w,"required UNKNOWN CPU write");}
        D3D12_PLACED_SUBRESOURCE_FOOTPRINT fp{};UINT rows=0;UINT64 rowBytes=0,total=0;c.device->GetCopyableFootprints(&td,0,1,0,&fp,&rows,&rowBytes,&total);std::printf("TEXTURE_FOOTPRINT layout=%d rows=%u row_bytes=%llu row_pitch=%u total_bytes=%llu offset=%llu\n",layout,rows,rowBytes,fp.Footprint.RowPitch,total,fp.Offset);
        Buffer upload;if(layout==D3D12_TEXTURE_LAYOUT_ROW_MAJOR){
            upload.allocate(c.device.p,total,"upload");upload.map();std::memset(upload.cpu,0,total);for(UINT y=0;y<37;++y)std::memcpy(static_cast<unsigned char*>(upload.cpu)+fp.Offset+UINT64(y)*fp.Footprint.RowPitch,input.data()+y*65,260);_mm_sfence();
            D3D12_TEXTURE_COPY_LOCATION fromUpload{},toTexture{};fromUpload.pResource=upload.resource.p;fromUpload.Type=D3D12_TEXTURE_COPY_TYPE_PLACED_FOOTPRINT;fromUpload.PlacedFootprint=fp;toTexture.pResource=t.p;toTexture.Type=D3D12_TEXTURE_COPY_TYPE_SUBRESOURCE_INDEX;
            q.begin();q.transition(t.p,D3D12_RESOURCE_STATE_COMMON,D3D12_RESOURCE_STATE_COPY_DEST);q.list->CopyTextureRegion(&toTexture,0,0,0,&fromUpload,nullptr);q.transition(t.p,D3D12_RESOURCE_STATE_COPY_DEST,D3D12_RESOURCE_STATE_COMMON);q.finish();
        }
        Buffer read;read.allocate(c.device.p,total,"readback");read.map();std::memset(read.cpu,0xA7,total);
        D3D12_TEXTURE_COPY_LOCATION from{},to{};from.pResource=t.p;from.Type=D3D12_TEXTURE_COPY_TYPE_SUBRESOURCE_INDEX;from.SubresourceIndex=0;to.pResource=read.resource.p;to.Type=D3D12_TEXTURE_COPY_TYPE_PLACED_FOOTPRINT;to.PlacedFootprint=fp;
        double start=ticks();q.begin();q.transition(t.p,D3D12_RESOURCE_STATE_COMMON,D3D12_RESOURCE_STATE_COPY_SOURCE);q.list->CopyTextureRegion(&to,0,0,0,&from,nullptr);q.transition(t.p,D3D12_RESOURCE_STATE_COPY_SOURCE,D3D12_RESOURCE_STATE_COMMON);q.finish();double ms=(ticks()-start)*1000.0/frequency();
        size_t matches=0;for(UINT y=0;y<37;++y)for(UINT x=0;x<65;++x){auto row=reinterpret_cast<volatile uint32_t*>(static_cast<unsigned char*>(read.cpu)+fp.Offset+UINT64(y)*fp.Footprint.RowPitch);matches+=row[x]==input[y*65+x];}
        std::printf("TEXTURE_GPU_READBACK layout=%d matches=%zu/2405 ms=%.6f\n",layout,matches,ms);if(matches!=2405)throw std::runtime_error("texture GPU mismatch");if(layout==D3D12_TEXTURE_LAYOUT_UNKNOWN)t->Unmap(0,nullptr);
    }
}
int main(int argc,char** argv){
    setvbuf(stdout,nullptr,_IONBF,0);
    try{
        if(argc<2)throw std::runtime_error("usage: phase2 identity|coherence|watch|texture|memory TYPE BYTES|dirty TYPE STRIDE SESSION");
        std::printf("PHASE2 protocol=1 mode=%s guest_pid=%lu\n",argv[1],GetCurrentProcessId());
        if(std::string(argv[1])=="watch"){watch();std::puts("END success");return 0;}
        Context c;
        if(std::string(argv[1])=="identity")identity(c);
        else if(std::string(argv[1])=="coherence")coherent(c);
        else if(std::string(argv[1])=="negative-control")coherent(c,true);
        else if(std::string(argv[1])=="texture")textures(c);
        else if(std::string(argv[1])=="memory"&&argc==4)memory(c,argv[2],std::strtoull(argv[3],nullptr,10));
        else if(std::string(argv[1])=="dirty"&&argc==5)dirty(c,argv[2],UINT(std::strtoul(argv[3],nullptr,10)),UINT(std::strtoul(argv[4],nullptr,10)));
        else throw std::runtime_error("invalid mode or arguments");
        std::puts("END success");return 0;
    }catch(const std::exception& e){std::printf("END failure reason=%s\n",e.what());return 1;}
}

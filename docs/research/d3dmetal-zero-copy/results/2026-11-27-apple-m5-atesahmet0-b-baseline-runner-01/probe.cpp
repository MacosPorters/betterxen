// SPDX-License-Identifier: GPL-3.0-or-later
// Original synthetic public-D3D12 research probe; issue #473.
#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include <windows.h>
#include <d3d12.h>
#include <tlhelp32.h>
#include <emmintrin.h>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <vector>
#include <utility>

template<class T> struct Ptr {
    T* p=nullptr;
    Ptr()=default; Ptr(const Ptr&)=delete; Ptr& operator=(const Ptr&)=delete;
    ~Ptr(){if(p)p->Release();}
    T* operator->()const{return p;}
    explicit operator bool()const{return p!=nullptr;}
    void** out(){return reinterpret_cast<void**>(&p);}
};
static int failures=0;
static bool hr(const char* label,HRESULT h){
    std::printf("%s HRESULT=0x%08lX\n",label,static_cast<unsigned long>(h));
    return SUCCEEDED(h);
}
static D3D12_HEAP_PROPERTIES props(D3D12_HEAP_TYPE type,D3D12_CPU_PAGE_PROPERTY cpu=D3D12_CPU_PAGE_PROPERTY_UNKNOWN,D3D12_MEMORY_POOL pool=D3D12_MEMORY_POOL_UNKNOWN){
    D3D12_HEAP_PROPERTIES p{};p.Type=type;p.CPUPageProperty=cpu;p.MemoryPoolPreference=pool;p.CreationNodeMask=1;p.VisibleNodeMask=1;return p;
}
static D3D12_RESOURCE_DESC buffer(UINT64 n,D3D12_RESOURCE_FLAGS flags=D3D12_RESOURCE_FLAG_NONE){
    D3D12_RESOURCE_DESC d{};d.Dimension=D3D12_RESOURCE_DIMENSION_BUFFER;d.Width=n;d.Height=1;d.DepthOrArraySize=1;d.MipLevels=1;d.SampleDesc.Count=1;d.Layout=D3D12_TEXTURE_LAYOUT_ROW_MAJOR;d.Flags=flags;return d;
}
static bool heap(ID3D12Device3* d,UINT64 size,D3D12_CPU_PAGE_PROPERTY cpu,D3D12_MEMORY_POOL pool,D3D12_HEAP_FLAGS flags,Ptr<ID3D12Heap>& h,const char* label){
    D3D12_HEAP_DESC desc{};desc.SizeInBytes=size;desc.Properties=props(D3D12_HEAP_TYPE_CUSTOM,cpu,pool);desc.Alignment=65536;desc.Flags=flags;
    return hr(label,d->CreateHeap(&desc,__uuidof(ID3D12Heap),h.out()));
}
static bool committed(ID3D12Device3* d,UINT64 n,D3D12_HEAP_TYPE type,D3D12_RESOURCE_STATES state,Ptr<ID3D12Resource>& r,const char* label){
    auto p=props(type);auto b=buffer(n);
    return hr(label,d->CreateCommittedResource(&p,D3D12_HEAP_FLAG_NONE,&b,state,nullptr,__uuidof(ID3D12Resource),r.out()));
}
struct Engine {
    ID3D12Device3* dev;
    Ptr<ID3D12CommandQueue> queue;Ptr<ID3D12CommandAllocator> alloc;Ptr<ID3D12GraphicsCommandList> list;Ptr<ID3D12Fence> fence;
    HANDLE event=nullptr;UINT64 sequence=0;
    explicit Engine(ID3D12Device3* d):dev(d){}
    ~Engine(){if(event)CloseHandle(event);}
    bool init(){
        D3D12_COMMAND_QUEUE_DESC q{};q.Type=D3D12_COMMAND_LIST_TYPE_DIRECT;
        if(!hr("queue",dev->CreateCommandQueue(&q,__uuidof(ID3D12CommandQueue),queue.out())))return false;
        if(!hr("allocator",dev->CreateCommandAllocator(q.Type,__uuidof(ID3D12CommandAllocator),alloc.out())))return false;
        if(!hr("command_list",dev->CreateCommandList(0,q.Type,alloc.p,nullptr,__uuidof(ID3D12GraphicsCommandList),list.out())))return false;
        if(!hr("initial_close",list->Close()))return false;
        if(!hr("fence",dev->CreateFence(0,D3D12_FENCE_FLAG_NONE,__uuidof(ID3D12Fence),fence.out())))return false;
        event=CreateEventW(nullptr,FALSE,FALSE,nullptr);
        std::printf("fence_event success=%d error=%lu\n",event!=nullptr,event?0:GetLastError());return event!=nullptr;
    }
    bool begin(){
        HRESULT h=alloc->Reset();if(FAILED(h))return hr("allocator.Reset",h);
        h=list->Reset(alloc.p,nullptr);if(FAILED(h))return hr("list.Reset",h);return true;
    }
    void transition(ID3D12Resource* r,D3D12_RESOURCE_STATES from,D3D12_RESOURCE_STATES to){
        if(from==to)return;
        D3D12_RESOURCE_BARRIER b{};b.Type=D3D12_RESOURCE_BARRIER_TYPE_TRANSITION;b.Transition.pResource=r;b.Transition.StateBefore=from;b.Transition.StateAfter=to;b.Transition.Subresource=D3D12_RESOURCE_BARRIER_ALL_SUBRESOURCES;list->ResourceBarrier(1,&b);
    }
    bool submit(){
        HRESULT h=list->Close();if(FAILED(h))return hr("list.Close",h);
        _mm_sfence();
        ID3D12CommandList* lists[]={list.p};queue->ExecuteCommandLists(1,lists);
        h=queue->Signal(fence.p,++sequence);if(FAILED(h))return hr("queue.Signal",h);
        if(fence->GetCompletedValue()<sequence){
            h=fence->SetEventOnCompletion(sequence,event);if(FAILED(h))return hr("fence.SetEventOnCompletion",h);
            DWORD w=WaitForSingleObject(event,5000);if(w!=WAIT_OBJECT_0){std::printf("fence_wait result=0x%08lX last_error=%lu\n",w,GetLastError());return false;}
        }
        h=dev->GetDeviceRemovedReason();if(FAILED(h))return hr("device_removed",h);return true;
    }
    bool readEnds(ID3D12Resource* source,UINT64 n,D3D12_RESOURCE_STATES state,ID3D12Resource* dest){
        if(!begin())return false;
        const bool needsBarrier=(state & D3D12_RESOURCE_STATE_COPY_SOURCE)==0;
        if(needsBarrier)transition(source,state,D3D12_RESOURCE_STATE_COPY_SOURCE);
        list->CopyBufferRegion(dest,0,source,0,64);
        list->CopyBufferRegion(dest,64,source,n-64,64);
        if(needsBarrier)transition(source,D3D12_RESOURCE_STATE_COPY_SOURCE,state);
        return submit();
    }
};
static void modules(){
    HANDLE s=CreateToolhelp32Snapshot(TH32CS_SNAPMODULE,GetCurrentProcessId());
    MODULEENTRY32 m{};m.dwSize=sizeof(m);
    if(s!=INVALID_HANDLE_VALUE){
        if(Module32First(s,&m))do{
            if(strstr(m.szModule,"d3d12")||strstr(m.szModule,"dxgi"))std::printf("module %s path=%s\n",m.szModule,m.szExePath);
        }while(Module32Next(s,&m));CloseHandle(s);
    }
    std::printf("d3d12_xs_loaded=%d\n",GetModuleHandleA("d3d12_xs.dll")!=nullptr);
}
static void wrapping(ID3D12Device3* d,Engine* e){
    std::puts("[2] Existing-memory wrapping (64 KiB)");
    void* mem=VirtualAlloc(nullptr,65536,MEM_COMMIT|MEM_RESERVE,PAGE_READWRITE);
    std::printf("VirtualAlloc success=%d error=%lu\n",mem!=nullptr,mem?0:GetLastError());
    HANDLE mapping=CreateFileMappingW(INVALID_HANDLE_VALUE,nullptr,PAGE_READWRITE,0,65536,nullptr);
    std::printf("CreateFileMappingW success=%d error=%lu\n",mapping!=nullptr,mapping?0:GetLastError());
    void* view=mapping?MapViewOfFile(mapping,FILE_MAP_ALL_ACCESS,0,0,65536):nullptr;
    if(mapping)std::printf("MapViewOfFile success=%d error=%lu\n",view!=nullptr,view?0:GetLastError());
    {
        Ptr<ID3D12Heap> a,f;
        if(mem)hr("OpenExistingHeapFromAddress",d->OpenExistingHeapFromAddress(mem,__uuidof(ID3D12Heap),a.out()));
        if(mapping)hr("OpenExistingHeapFromFileMapping",d->OpenExistingHeapFromFileMapping(mapping,__uuidof(ID3D12Heap),f.out()));
        for(int i=0;i<2;++i){
            ID3D12Heap* h=i?f.p:a.p;auto* cpu=static_cast<volatile uint32_t*>(i?view:mem);
            const char* label=i?"file_mapping_visibility":"address_visibility";
            if(!h){std::printf("%s NOT_RUN (heap open failed)\n",label);continue;}
            if(!cpu||!e){std::printf("%s NOT_RUN (missing CPU view or queue)\n",label);continue;}
            Ptr<ID3D12Resource> placed,readback;auto b=buffer(65536);
            if(!hr(label,d->CreatePlacedResource(h,0,&b,D3D12_RESOURCE_STATE_COMMON,nullptr,__uuidof(ID3D12Resource),placed.out())))continue;
            if(!committed(d,128,D3D12_HEAP_TYPE_READBACK,D3D12_RESOURCE_STATE_COPY_DEST,readback,"wrap_readback"))continue;
            void* p=nullptr;D3D12_RANGE range{0,128};if(!hr("wrap_readback.Map",readback->Map(0,&range,&p)))continue;
            cpu[0]=0x13579BDFu;cpu[(65536-64)/4]=0x2468ACE0u;_mm_sfence();
            bool submitted=e->readEnds(placed.p,65536,D3D12_RESOURCE_STATE_COMMON,readback.p);
            auto* actual=static_cast<volatile uint32_t*>(p);bool ok=submitted&&actual[0]==0x13579BDFu&&actual[16]==0x2468ACE0u;
            std::printf("%s actual=%08X,%08X expected=13579BDF,2468ACE0 match=%d\n",label,actual[0],actual[16],ok);if(!ok)++failures;
        }
    }
    if(view)UnmapViewOfFile(view);
    if(mapping)CloseHandle(mapping);
    if(mem)VirtualFree(mem,0,MEM_RELEASE);
}
static void placement(ID3D12Device3* d){
    std::puts("[3] CUSTOM heap creation (size=2097152 alignment=65536)");
    const D3D12_HEAP_FLAGS flags[]={D3D12_HEAP_FLAG_NONE,D3D12_HEAP_FLAG_NONE,D3D12_HEAP_FLAG_ALLOW_ONLY_BUFFERS,D3D12_HEAP_FLAG_ALLOW_ONLY_NON_RT_DS_TEXTURES,D3D12_HEAP_FLAG_ALLOW_ONLY_RT_DS_TEXTURES,D3D12_HEAP_FLAG_NONE};
    const char* labels[]={"WRITE_BACK/L0/ALLOW_ALL","WRITE_COMBINE/L0/ALLOW_ALL","WRITE_BACK/L0/ALLOW_ONLY_BUFFERS","WRITE_BACK/L0/ALLOW_ONLY_NON_RT_DS_TEXTURES","WRITE_BACK/L0/ALLOW_ONLY_RT_DS_TEXTURES","WRITE_BACK/L1/ALLOW_ALL control"};
    for(int i=0;i<6;++i){Ptr<ID3D12Heap> h;heap(d,2097152,i==1?D3D12_CPU_PAGE_PROPERTY_WRITE_COMBINE:D3D12_CPU_PAGE_PROPERTY_WRITE_BACK,i==5?D3D12_MEMORY_POOL_L1:D3D12_MEMORY_POOL_L0,flags[i],h,labels[i]);}
    Ptr<ID3D12Heap> h;if(!heap(d,2097152,D3D12_CPU_PAGE_PROPERTY_WRITE_BACK,D3D12_MEMORY_POOL_L0,D3D12_HEAP_FLAG_NONE,h,"mixed_heap")){std::puts("mixed placement NOT_RUN");return;}
    Ptr<ID3D12Resource> b,t;auto bd=buffer(65536,D3D12_RESOURCE_FLAG_ALLOW_UNORDERED_ACCESS);
    bool bok=hr("placed_UAV_buffer offset=0",d->CreatePlacedResource(h.p,0,&bd,D3D12_RESOURCE_STATE_COMMON,nullptr,__uuidof(ID3D12Resource),b.out()));
    D3D12_RESOURCE_DESC td{};td.Dimension=D3D12_RESOURCE_DIMENSION_TEXTURE2D;td.Width=64;td.Height=64;td.DepthOrArraySize=1;td.MipLevels=1;td.Format=DXGI_FORMAT_R8G8B8A8_UNORM;td.SampleDesc.Count=1;td.Layout=D3D12_TEXTURE_LAYOUT_UNKNOWN;
    auto ba=d->GetResourceAllocationInfo(0,1,&bd);auto ta=d->GetResourceAllocationInfo(0,1,&td);
    if(!ta.Alignment||ba.SizeInBytes==~UINT64(0)||ta.SizeInBytes==~UINT64(0)){std::puts("invalid allocation info; texture NOT_RUN");return;}
    UINT64 off=(ba.SizeInBytes+ta.Alignment-1)/ta.Alignment*ta.Alignment;
    std::printf("buffer allocation size=%llu alignment=%llu; texture size=%llu alignment=%llu offset=%llu\n",ba.SizeInBytes,ba.Alignment,ta.SizeInBytes,ta.Alignment,off);
    bool tok=hr("placed_RGBA8_texture same_heap",d->CreatePlacedResource(h.p,off,&td,D3D12_RESOURCE_STATE_COMMON,nullptr,__uuidof(ID3D12Resource),t.out()));
    std::printf("mixed_buffer_and_texture_live=%d\n",bok&&tok);
    if(!tok)return;
    HRESULT mapped=t->Map(0,nullptr,nullptr);if(!hr("texture.Map(null ppData)",mapped)){std::puts("texture roundtrip NOT_RUN");return;}
    std::vector<uint32_t> input(4096),output(4096,0xDEADBEEFu);for(size_t i=0;i<input.size();++i)input[i]=0x8A31C57Fu^(uint32_t(i)*0x0103070Bu);
    HRESULT w=t->WriteToSubresource(0,nullptr,input.data(),256,16384);hr("texture.WriteToSubresource",w);
    HRESULT r=t->ReadFromSubresource(output.data(),256,16384,0,nullptr);hr("texture.ReadFromSubresource",r);
    size_t matches=0;for(size_t i=0;i<input.size();++i)matches+=input[i]==output[i];
    std::printf("texture roundtrip exact_words=%zu/4096 match=%d\n",matches,SUCCEEDED(w)&&SUCCEEDED(r)&&matches==4096);
    if(FAILED(w)||FAILED(r)||matches!=4096)++failures;
    // The texture test is separate from the persistent buffer tests.
    t->Unmap(0,nullptr);
}
static void coherence(ID3D12Device3* d,Engine& e){
    std::puts("[4] Bidirectional persistent Map; no Unmap or remap");
    Ptr<ID3D12Heap> h;Ptr<ID3D12Resource> b,readback,upload;
    if(!heap(d,65536,D3D12_CPU_PAGE_PROPERTY_WRITE_BACK,D3D12_MEMORY_POOL_L0,D3D12_HEAP_FLAG_NONE,h,"coherence_heap"))return;
    auto bd=buffer(65536,D3D12_RESOURCE_FLAG_ALLOW_UNORDERED_ACCESS);
    if(!hr("coherence_placed_UAV_buffer",d->CreatePlacedResource(h.p,0,&bd,D3D12_RESOURCE_STATE_COMMON,nullptr,__uuidof(ID3D12Resource),b.out())))return;
    if(!committed(d,128,D3D12_HEAP_TYPE_READBACK,D3D12_RESOURCE_STATE_COPY_DEST,readback,"coherence_readback"))return;
    if(!committed(d,65536,D3D12_HEAP_TYPE_UPLOAD,D3D12_RESOURCE_STATE_GENERIC_READ,upload,"coherence_upload"))return;
    void *cp=nullptr,*rp=nullptr,*up=nullptr;D3D12_RANGE rr{0,128},none{0,0};
    if(!hr("placed.Map",b->Map(0,nullptr,&cp)))return;
    if(!hr("readback.Map",readback->Map(0,&rr,&rp)))return;
    if(!hr("upload.Map",upload->Map(0,&none,&up)))return;
    auto* cpu=static_cast<volatile uint32_t*>(cp);auto* actual=static_cast<volatile uint32_t*>(rp);
    const uint32_t values[3][2]={{0x11223344u,0xA1B2C3D4u},{0x55AA00FFu,0xF00DBA5Eu},{0xC001D00Du,0x9E3779B9u}};
    for(int i=0;i<2;++i){
        cpu[0]=values[i][0];cpu[1]=values[i][1];_mm_sfence();
        bool submitted=e.readEnds(b.p,65536,D3D12_RESOURCE_STATE_COMMON,readback.p);
        bool ok=submitted&&actual[0]==values[i][0]&&actual[1]==values[i][1];
        std::printf("CPU->GPU round=%d expected=%08X,%08X actual=%08X,%08X match=%d\n",i+1,values[i][0],values[i][1],actual[0],actual[1],ok);if(!ok)++failures;
    }
    auto* src=static_cast<volatile uint32_t*>(up);src[0]=values[2][0];src[1]=values[2][1];_mm_sfence();
    if(!e.begin())return;
    e.transition(b.p,D3D12_RESOURCE_STATE_COMMON,D3D12_RESOURCE_STATE_COPY_DEST);
    e.list->CopyBufferRegion(b.p,0,upload.p,0,8);
    e.transition(b.p,D3D12_RESOURCE_STATE_COPY_DEST,D3D12_RESOURCE_STATE_COMMON);
    bool submitted=e.submit();bool ok=submitted&&cpu[0]==values[2][0]&&cpu[1]==values[2][1];
    std::printf("GPU->CPU same_cpu_ptr expected=%08X,%08X actual=%08X,%08X match=%d\n",values[2][0],values[2][1],cpu[0],cpu[1],ok);if(!ok)++failures;
}
static void benchmark(ID3D12Device3* d,Engine& e,const char* label,UINT64 n,bool isUpload,D3D12_CPU_PAGE_PROPERTY property){
    std::printf("[5] BENCH %s size=%llu warmup=5 timed=40\n",label,n);
    Ptr<ID3D12Heap> h;Ptr<ID3D12Resource> b,r;
    D3D12_RESOURCE_STATES state=isUpload?D3D12_RESOURCE_STATE_GENERIC_READ:D3D12_RESOURCE_STATE_COMMON;
    if(isUpload){if(!committed(d,n,D3D12_HEAP_TYPE_UPLOAD,state,b,"bench_upload"))return;}
    else{
        if(!heap(d,n,property,D3D12_MEMORY_POOL_L0,D3D12_HEAP_FLAG_NONE,h,"bench_heap"))return;
        auto bd=buffer(n);if(!hr("bench_placed_buffer",d->CreatePlacedResource(h.p,0,&bd,state,nullptr,__uuidof(ID3D12Resource),b.out())))return;
    }
    if(!committed(d,128,D3D12_HEAP_TYPE_READBACK,D3D12_RESOURCE_STATE_COPY_DEST,r,"bench_readback"))return;
    void *bp=nullptr,*rp=nullptr;D3D12_RANGE none{0,0},read{0,128};
    if(!hr("bench_source.Map",b->Map(0,&none,&bp)))return;
    if(!hr("bench_readback.Map",r->Map(0,&read,&rp)))return;
    auto* bytes=static_cast<volatile unsigned char*>(bp);
    for(UINT64 i=0;i<n;i+=65536)bytes[i]=0x5A;
    _mm_sfence();
    std::printf("prefault stride=65536 touches=%llu\n",n/65536);
    auto* first=static_cast<volatile uint32_t*>(bp);auto* last=reinterpret_cast<volatile uint32_t*>(static_cast<unsigned char*>(bp)+n-64);auto* actual=static_cast<volatile uint32_t*>(rp);
    LARGE_INTEGER frequency{},start{},end{};QueryPerformanceFrequency(&frequency);
    int warmMatches=0,timedMatches=0;double total=0;
    for(int i=-5;i<40;++i){
        uint32_t value=0xB1630000u+uint32_t(i+5)*0x00010307u;
        if(i==0)QueryPerformanceCounter(&start);
        for(int j=0;j<16;++j){first[j]=value^uint32_t(j)*0x01010101u;last[j]=(~value)^uint32_t(j)*0x0103070Bu;}
        _mm_sfence();
        bool ok=e.readEnds(b.p,n,state,r.p);if(!ok){std::puts("BENCH aborted (submission failed)");++failures;return;}
        for(int j=0;j<16;++j)ok=ok&&actual[j]==(value^uint32_t(j)*0x01010101u)&&actual[16+j]==((~value)^uint32_t(j)*0x0103070Bu);
        if(i<0)warmMatches+=ok;else timedMatches+=ok;
        if(!ok)std::printf("BENCH mismatch iteration=%d first=%08X expected=%08X last=%08X expected=%08X\n",i,actual[0],value,actual[16],~value);
    }
    QueryPerformanceCounter(&end);total=1000.0*double(end.QuadPart-start.QuadPart)/double(frequency.QuadPart);
    std::printf("BENCH_RESULT %s bytes=%llu total_ms=%.6f avg_ms_per_submit=%.6f exact_matches=%d/40 warmup_matches=%d/5\n",label,n,total,total/40,timedMatches,warmMatches);
    if(timedMatches!=40||warmMatches!=5)++failures;
}
static void writewatch(){
    std::puts("[6] MEM_WRITE_WATCH 65536 bytes; writes at 0 and 16384");
    void* p=VirtualAlloc(nullptr,65536,MEM_COMMIT|MEM_RESERVE|MEM_WRITE_WATCH,PAGE_READWRITE);
    std::printf("writewatch.VirtualAlloc success=%d error=%lu\n",p!=nullptr,p?0:GetLastError());if(!p)return;
    auto* bytes=static_cast<volatile unsigned char*>(p);bytes[0]=0x61;bytes[16384]=0x72;
    PVOID addresses[64]{};ULONG_PTR count=64;DWORD granularity=0;
    UINT rc=GetWriteWatch(WRITE_WATCH_FLAG_RESET,p,65536,addresses,&count,&granularity);
    std::printf("GetWriteWatch return=%u dirty_page_count=%llu granularity=%lu\n",rc,static_cast<unsigned long long>(count),granularity);
    if(rc==0)for(ULONG_PTR i=0;i<count&&i<64;++i)std::printf("dirty_page[%llu] offset=%llu\n",static_cast<unsigned long long>(i),static_cast<unsigned long long>(static_cast<unsigned char*>(addresses[i])-static_cast<unsigned char*>(p)));
    VirtualFree(p,0,MEM_RELEASE);
}
int main(int argc,char**){
    setvbuf(stdout,nullptr,_IONBF,0);std::puts("Independent standalone D3D12 UMA probe; C++20; sizeof(void*)=8 expected");
    std::printf("sizeof(void*)=%zu\n",sizeof(void*));
    HMODULE dll=LoadLibraryA("d3d12.dll");if(!dll){std::printf("LoadLibraryA(d3d12.dll) failed error=%lu\n",GetLastError());writewatch();return 2;}
    char path[32768]{};GetModuleFileNameA(dll,path,sizeof(path));std::printf("LoadLibraryA(d3d12.dll) path=%s\n",path);
    using Create=HRESULT(WINAPI*)(IUnknown*,D3D_FEATURE_LEVEL,REFIID,void**);
    FARPROC symbol=GetProcAddress(dll,"D3D12CreateDevice");
    Create create=nullptr;static_assert(sizeof(create)==sizeof(symbol));std::memcpy(&create,&symbol,sizeof(create));
    if(!create){std::printf("GetProcAddress failed error=%lu\n",GetLastError());writewatch();return 2;}
    Ptr<ID3D12Device> base;
    if(!hr("D3D12CreateDevice FL11_0",create(nullptr,D3D_FEATURE_LEVEL_11_0,__uuidof(ID3D12Device),base.out()))){writewatch();return 2;}
    Ptr<ID3D12Device3> d;if(!hr("QueryInterface ID3D12Device3",base->QueryInterface(__uuidof(ID3D12Device3),d.out()))){writewatch();return 2;}
    modules();if(argc>1){wrapping(d.p,nullptr);std::puts("backend receipt pause 10 seconds");Sleep(10000);return 0;}std::puts("[1] Device capability queries, NodeIndex=0");
    D3D12_FEATURE_DATA_ARCHITECTURE1 a{};
    if(hr("D3D12_FEATURE_ARCHITECTURE1",d->CheckFeatureSupport(D3D12_FEATURE_ARCHITECTURE1,&a,sizeof(a))))std::printf("UMA=%d CacheCoherentUMA=%d TileBasedRenderer=%d IsolatedMMU=%d\n",a.UMA,a.CacheCoherentUMA,a.TileBasedRenderer,a.IsolatedMMU);
    D3D12_FEATURE_DATA_D3D12_OPTIONS o{};
    if(hr("D3D12_FEATURE_D3D12_OPTIONS",d->CheckFeatureSupport(D3D12_FEATURE_D3D12_OPTIONS,&o,sizeof(o))))std::printf("ResourceHeapTier=%d\n",int(o.ResourceHeapTier));
    Engine e(d.p);bool ready=e.init();wrapping(d.p,ready?&e:nullptr);placement(d.p);
    if(ready){
        coherence(d.p,e);
        benchmark(d.p,e,"CUSTOM/WRITE_BACK/L0/64KiB",65536,false,D3D12_CPU_PAGE_PROPERTY_WRITE_BACK);
        benchmark(d.p,e,"CUSTOM/WRITE_BACK/L0/256MiB",268435456,false,D3D12_CPU_PAGE_PROPERTY_WRITE_BACK);
        benchmark(d.p,e,"CUSTOM/WRITE_COMBINE/L0/256MiB",268435456,false,D3D12_CPU_PAGE_PROPERTY_WRITE_COMBINE);
        benchmark(d.p,e,"UPLOAD/256MiB",268435456,true,D3D12_CPU_PAGE_PROPERTY_UNKNOWN);
    }else{std::puts("[4,5] NOT_RUN: queue initialization failed");++failures;}
    writewatch();std::printf("END observed_data_or_submission_failures=%d (unsupported HRESULTs reported separately)\n",failures);return failures?1:0;
}

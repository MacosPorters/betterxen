// SPDX-License-Identifier: GPL-3.0-or-later
// Original minimal DXGI lifetime probe. No shaders, assets or third-party implementation.
// A private-data IUnknown acts as a lifetime witness: DXGI must AddRef it on attachment
// and Release it when the swapchain's private data is destroyed. This independently tests
// retention without treating a backend's numeric Release return as the only evidence.
// Public contract: https://learn.microsoft.com/windows/win32/api/dxgi/nf-dxgi-idxgiobject-setprivatedatainterface
#define COBJMACROS
#define WIDL_C_INLINE_WRAPPERS /* struct-returning D3D12 methods in C (mingw-w64) */
#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <d3d12.h>
#include <dxgi1_4.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

typedef struct Witness { IUnknown iface; LONG refs; } Witness;
static HRESULT STDMETHODCALLTYPE query(IUnknown *self, REFIID iid, void **out) {
    if (!out) return E_POINTER;
    *out = NULL;
    if (!IsEqualGUID(iid, &IID_IUnknown)) return E_NOINTERFACE;
    *out = self;
    IUnknown_AddRef(self);
    return S_OK;
}
static ULONG STDMETHODCALLTYPE addref(IUnknown *self) {
    return (ULONG)InterlockedIncrement(&((Witness *)self)->refs);
}
static ULONG STDMETHODCALLTYPE release(IUnknown *self) {
    LONG refs = InterlockedDecrement(&((Witness *)self)->refs);
    if (!refs) free(self);
    return (ULONG)refs;
}
static IUnknownVtbl witness_vtbl = {query, addref, release};
static const GUID witness_guid = {0xd84f72b1, 0xe7f2, 0x449f, {0xa4,0x48,0x52,0x11,0x01,0x47,0x66,0x10}};
#define API(expr) do { HRESULT code=(expr); if (FAILED(code)) { printf("FAIL %s hr=0x%08lx\n", #expr, (unsigned long)code); return 3; } } while(0)

static void drain(unsigned ms) {
    DWORD start = GetTickCount();
    do {
        MSG message;
        while (PeekMessageA(&message, NULL, 0, 0, PM_REMOVE)) {
            TranslateMessage(&message); DispatchMessageA(&message);
        }
        Sleep(10);
    } while (GetTickCount() - start < ms);
}

int main(int argc, char **argv) {
    setvbuf(stdout, NULL, _IONBF, 0);
    int cycles=12, early_destroy=0, detach=0, resize=0, query3=0, present=0, retain_control=0;
    for (int i=1;i<argc;i++) {
        if (!strcmp(argv[i],"--destroy-first")) early_destroy=1;
        else if (!strcmp(argv[i],"--detach-witness")) detach=1;
        else if (!strcmp(argv[i],"--resize")) resize=1;
        else if (!strcmp(argv[i],"--query-interface")) query3=1;
        else if (!strcmp(argv[i],"--present")) present=1;
        else if (!strcmp(argv[i],"--retain-app-reference")) retain_control=1;
        else if (!strcmp(argv[i],"--cycles") && i+1<argc) cycles=atoi(argv[++i]);
        else { puts("FAIL usage"); return 3; }
    }
    if (cycles<1 || cycles>100) return 3;
    ID3D12Device *device=NULL;
    HRESULT code=D3D12CreateDevice(NULL,D3D_FEATURE_LEVEL_11_0,&IID_ID3D12Device,(void **)&device);
    if (FAILED(code)) { printf("NO DEVICE hr=0x%08lx\n",(unsigned long)code); return 2; }
    IDXGIFactory4 *factory=NULL;
    API(CreateDXGIFactory1(&IID_IDXGIFactory4,(void **)&factory));
    /* Name the implementation that actually answered; a requested backend is not evidence. */
    IDXGIAdapter1 *adapter=NULL;
    DXGI_ADAPTER_DESC1 desc={0};
    LUID luid;
    luid=ID3D12Device_GetAdapterLuid(device);
    API(IDXGIFactory4_EnumAdapterByLuid(factory,luid,&IID_IDXGIAdapter1,(void **)&adapter));
    API(IDXGIAdapter1_GetDesc1(adapter,&desc));
    printf("adapter=%ls vendor=0x%04x device=0x%04x\n",desc.Description,desc.VendorId,desc.DeviceId);
    IDXGIAdapter1_Release(adapter);
    ID3D12CommandQueue *queue=NULL;
    D3D12_COMMAND_QUEUE_DESC qd={.Type=D3D12_COMMAND_LIST_TYPE_DIRECT};
    API(ID3D12Device_CreateCommandQueue(device,&qd,&IID_ID3D12CommandQueue,(void **)&queue));
    Witness *w=calloc(1,sizeof *w);
    if (!w) return 3;
    w->iface.lpVtbl=&witness_vtbl; w->refs=1;
    WNDCLASSA wc={.lpfnWndProc=DefWindowProcA,.hInstance=GetModuleHandleA(NULL),.lpszClassName="BetterXenLifetime"};
    if (!RegisterClassA(&wc)) return 3;
    for (int n=0;n<cycles;n++) {
        HWND window=CreateWindowA(wc.lpszClassName,"BetterXen lifetime probe",WS_OVERLAPPEDWINDOW|WS_VISIBLE,
            32,32,128,128,NULL,NULL,wc.hInstance,NULL);
        if (!window) return 3;
        DXGI_SWAP_CHAIN_DESC1 sd={.Width=64,.Height=64,.Format=DXGI_FORMAT_R8G8B8A8_UNORM,
            .SampleDesc={1,0},.BufferUsage=DXGI_USAGE_RENDER_TARGET_OUTPUT,.BufferCount=2,
            .SwapEffect=DXGI_SWAP_EFFECT_FLIP_DISCARD};
        IDXGISwapChain1 *swap=NULL;
        API(IDXGIFactory4_CreateSwapChainForHwnd(factory,(IUnknown *)queue,window,&sd,NULL,NULL,&swap));
        /* AddRef/Release pair: the count the application is handed, before it does anything else. */
        IDXGISwapChain1_AddRef(swap);
        ULONG refs_after_create=IDXGISwapChain1_Release(swap);
        LONG before=w->refs;
        API(IDXGISwapChain1_SetPrivateDataInterface(swap,&witness_guid,(IUnknown *)w));
        if (w->refs != before+1) { puts("FAIL private-data attachment did not AddRef the witness"); return 1; }
        IDXGISwapChain3 *swap3=NULL;
        if (query3) API(IDXGISwapChain1_QueryInterface(swap,&IID_IDXGISwapChain3,(void **)&swap3));
        if (resize) API(IDXGISwapChain1_ResizeBuffers(swap,2,96,64,DXGI_FORMAT_UNKNOWN,0));
        if (present) { API(IDXGISwapChain1_Present(swap,0,0)); drain(50); }
        if (detach) API(IDXGISwapChain1_SetPrivateDataInterface(swap,&witness_guid,NULL));
        if (detach && w->refs != before) { puts("FAIL private-data detachment did not Release the witness"); return 1; }
        if (retain_control) IUnknown_AddRef((IUnknown *)w); /* deliberate ownership fault */
        if (early_destroy && !DestroyWindow(window)) return 3;
        if (swap3) IDXGISwapChain3_Release(swap3);
        ULONG swap_refs=IDXGISwapChain1_Release(swap);
        if (!early_destroy && !DestroyWindow(window)) return 3;
        drain(100);
        ULONG device_refs=ID3D12Device_AddRef(device)-1;
        ID3D12Device_Release(device);
        printf("cycle=%d witness_refs=%ld swap_refs_after_create=%lu swap_release_return=%lu device_refs=%lu\n",
            n+1,(long)w->refs,(unsigned long)refs_after_create,(unsigned long)swap_refs,(unsigned long)device_refs);
    }
    ID3D12CommandQueue_Release(queue);
    IDXGIFactory4_Release(factory);
    ULONG device_refs=ID3D12Device_Release(device);
    drain(1000);
    const LONG retained=w->refs-1;
    printf("gate %s %s retained_witness_refs=%ld cycles=%d final_device_release=%lu no_app_gpu_submissions=1 app_reference_fault=%d\n",
        detach?"resources.private-data-detach":"resources.swapchain-release",
        retained==0?"pass":"FAIL",(long)retained,cycles,(unsigned long)device_refs,retain_control);
    IUnknown_Release((IUnknown *)w);
    UnregisterClassA(wc.lpszClassName,wc.hInstance);
    return retained==0?0:1;
}

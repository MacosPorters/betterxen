# Evidence record, 2026-11-26

> **DRAFT — FOR MAINTAINER DISCUSSION ONLY — NOT APPROVED FOR IMPLEMENTATION**

Heads have since moved (#486 to `7056671`, #494 to `fbd2f8c`). Results below apply to the commits listed and were not re-run on the new ones.

**One Apple M4, run by one AI agent for @Arths17. Nothing here is independently reproduced.** It does not modify the #473 research evidence. Raw logs are kept outside the repository (they contain local paths); their SHA-256 values are listed so a copy can be checked.

## Environment

| Item | Value |
| --- | --- |
| Hardware | Apple M4, 24 GiB |
| macOS | 27.0 (26A428) |
| CrossOver | 26.3.0.39832 (`cxoffice-26.3.0rc2`) |
| D3DMetal | 3.0 |
| Compiler | x86_64-w64-mingw32-g++ 16.2.0, CMake 4.3.1 |
| Rust | 1.100.0 |
| Bottle | `betterxen-maintainer-test`, `win11_64`, no `[EnvironmentVariables]` |
| Backend | `--backend d3dmetal`, which `bx-cli` passes per run as `--env CX_GRAPHICS_BACKEND=d3dmetal` |
| Overrides | `bx-cli` passes its shim list as `--dll ...=n,b` and sets `ROSETTA_ADVERTISE_AVX=1` |

## Commits and what was changed

| Experiment | Tree | Production code | Temporary change |
| --- | --- | --- | --- |
| Fixture failure, 143/145 | #486 `bd546d8a39f04872f4c957fac65883cdc0a74b32` | Unchanged | None |
| Charge trace | same | **Instrumented** (log lines in `vmem.cpp`) | Also test prints |
| Corrected fixture, 145/145 | same | Unchanged | Two test lines removed |
| Budget order and early credit | same | Unchanged (shims rebuilt after restoring) | Separate probe program |
| `xmem` view empty | `main` `cd945ba7` + #486 `bd546d8a` + #471 `b096424f` + #494 `c64534fb` | PR code unchanged; one conflict resolved by keeping both source lines | One test line added |
| Destruction sentinel | Host `d3d12.dll` only | Not involved | Separate probe program |

## Common commands

From a worktree at the tree above:

```sh
export RUSTUP_TOOLCHAIN=1.100.0-aarch64-apple-darwin CARGO_BUILD_JOBS=4
cargo build --locked -p bx-cli
scripts/build-shims.sh
target/debug/bx-cli run --bottle <bottle> --backend d3dmetal --timeout 90 \
  --log <log.jsonl> --exe build-shims/bin/<test>.exe
```

## 1. Fixture failure and the two `AddRef` calls

Three runs of the unchanged fixture:

```text
FAIL: one backing heap remains charged before device recreation (gpu_heap_bytes=393216)
FAIL: budget charge after device B is one region, not two (gpu_heap_bytes=393216)
FAIL: test_d3d12_placed (2 of 145 checks failed)
```

Charge trace with instrumentation (`gpu_heap_bytes` stays 131072 across the newer-placement step and ends at 393216, two extra 128 KiB heaps). Temporary patch:

```diff
diff --git a/shims/d3d12_xs/vmem.cpp b/shims/d3d12_xs/vmem.cpp
index 843942f9..1ca9bbef 100644
--- a/shims/d3d12_xs/vmem.cpp
+++ b/shims/d3d12_xs/vmem.cpp
@@ -109,6 +109,8 @@ const char* profile_name(bx_hardware_profile_t profile) {
 }
 
 /* Private host heaps count against the same Lockhart/Anaconda cap as xmem. */
+static void probe_log(const char* what, uint64_t bytes) { char d[128]; std::snprintf(d, sizeof(d), "%s bytes=%llu live=%llu", what, (unsigned long long)bytes, (unsigned long long)g_record.charged_private_bytes); bx_log_emit(BX_LOG_INFO, "d3d12_xs", "probe_charge", d); }
+
 bool try_charge(uint64_t bytes) {
     Guard guard(g_section);
     bx_xmem_stats_t stats{};
@@ -123,6 +125,7 @@ bool try_charge(uint64_t bytes) {
         return false;
     }
     g_record.charged_private_bytes += bytes;
+    probe_log("charge", bytes);
     return true;
 }
 
@@ -134,6 +137,7 @@ void release_charge(uint64_t bytes) {
         return;
     }
     g_record.charged_private_bytes -= bytes;
+    probe_log("release", bytes);
 }
 
 /*
diff --git a/tests/conformance/test_d3d12_placed/main.cpp b/tests/conformance/test_d3d12_placed/main.cpp
index 6864ca73..f9713e70 100644
--- a/tests/conformance/test_d3d12_placed/main.cpp
+++ b/tests/conformance/test_d3d12_placed/main.cpp
@@ -1235,7 +1235,10 @@ int main() {
      * the host check, retarget, and returned reference happen in one critical
      * section. There is no timing stress test for those windows.
      */
+    OutputDebugStringA("x");
+    if (backing) { BxD3d12VirtualMemoryConfig p0 = make_block(region_base, kRegionBytes, kFlagBacking); D3D12ConfigureVirtualMemoryX(&p0); std::printf("PROBE before_newer_placement gpu_heap_bytes=%llu\n", (unsigned long long)last_log_u64(std::getenv("BX_LOG_FILE"), "gpu_heap_bytes=")); }
     check_newer_placement(device);
+    if (backing) { BxD3d12VirtualMemoryConfig p1 = make_block(region_base, kRegionBytes, kFlagBacking); D3D12ConfigureVirtualMemoryX(&p1); std::printf("PROBE after_newer_placement gpu_heap_bytes=%llu\n", (unsigned long long)last_log_u64(std::getenv("BX_LOG_FILE"), "gpu_heap_bytes=")); }
 
     if (backing) {
         backing->AddRef();
```

Fixture correction (test only; a suggestion for the PR owner, not applied to their branch):

```diff
diff --git a/tests/conformance/test_d3d12_placed/main.cpp b/tests/conformance/test_d3d12_placed/main.cpp
index 6864ca73..1a4d6e17 100644
--- a/tests/conformance/test_d3d12_placed/main.cpp
+++ b/tests/conformance/test_d3d12_placed/main.cpp
@@ -1238,7 +1238,6 @@ int main() {
     check_newer_placement(device);
 
     if (backing) {
-        backing->AddRef();
         BxD3d12VirtualMemoryConfig detached = make_block(0, 0, kFlagBacking);
         const HRESULT away = D3D12ConfigureVirtualMemoryX(&detached);
         check(away == S_OK, "reconfigure drops the cached backing heap hr=0x%08lx", ul(away));
@@ -1261,7 +1260,6 @@ int main() {
         }
     }
     if (device && backing) {
-        backing->AddRef();
         check(SetEnvironmentVariableA("BX_D3D12_FORCE_BACKING_REPLACE", "1") != 0,
               "test hook BX_D3D12_FORCE_BACKING_REPLACE=1 is set");
         ID3D12Heap* replaced = nullptr;
```

```text
PASS: test_d3d12_placed (145 checks passed, buffer 64/64 bytes, texture 1024/1024 bytes, texture_path=WriteToSubresource, cpu_after_gpu 64/64, small_align=4096, upload_100000=100000)
```

## 2. Combined-budget admission and early credit

Build the probe, copy it next to the built shims, and run with a 512 MiB cap:

```sh
x86_64-w64-mingw32-g++ -std=c++20 -O1 -static budget_probe.cpp -o budget_probe.exe
cp budget_probe.exe build-shims/bin/
BX_MEM_LIMIT_MB=512 target/debug/bx-cli run --inherit-env --bottle <bottle> \
  --backend d3dmetal --timeout 90 --log probe.jsonl --exe build-shims/bin/budget_probe.exe
```

`--inherit-env` passes the whole host environment to the guest. Use a clean shell.

Output, identical on two runs:

```text
A xmem256_then_heap384: xmem=ok heap_hr=0x8007000e
B heap384_then_xmem256: heap_hr=0x00000000 xmem=ok combined_live_MiB=640
C release_heap_keep_placed: first_hr=0x00000000 placed_hr=0x00000000 second_heap300_hr=0x00000000 first_placed_still_mappable=1
D CreateHeap600: hr=0x8007000e
D CreateHeap1_600: hr=0x00000000
D CreateCommittedResource600: hr=0x00000000
DONE
```

- `A` and `D CreateHeap600` are controls: the cap is active.
- `B`: 640 MiB live under a 512 MiB cap.
- `C`: a second 300 MiB heap is admitted while the first is still mapped through its placed buffer.
- `D`: `CreateHeap1` and a committed resource are not charged.

Source:

```cpp
// Review probe (not for the repository). Run with BX_MEM_LIMIT_MB=512.
#define WIDL_EXPLICIT_AGGREGATE_RETURNS
#include <windows.h>
#include <d3d12.h>
#include <cstdint>
#include <cstdio>
#include <cstring>
using CreateFn = HRESULT(WINAPI*)(void*, REFIID, void**);
using ConfigFn = HRESULT(WINAPI*)(void*);
using VAllocFn = PVOID(WINAPI*)(PVOID, SIZE_T, DWORD, DWORD);
static const UINT64 MiB = 1024 * 1024;
static D3D12_HEAP_DESC heap(D3D12_HEAP_TYPE type, UINT64 size) {
    D3D12_HEAP_DESC d{}; d.SizeInBytes = size; d.Properties.Type = type;
    d.Flags = D3D12_HEAP_FLAG_ALLOW_ONLY_BUFFERS; return d;
}
static D3D12_RESOURCE_DESC buffer(UINT64 w) {
    D3D12_RESOURCE_DESC d{}; d.Dimension = D3D12_RESOURCE_DIMENSION_BUFFER; d.Width = w; d.Height = 1;
    d.DepthOrArraySize = 1; d.MipLevels = 1; d.SampleDesc.Count = 1; d.Layout = D3D12_TEXTURE_LAYOUT_ROW_MAJOR; return d;
}
int main() {
    HMODULE xs = LoadLibraryA("d3d12_xs.dll"), xm = LoadLibraryA("xmem.dll");
    auto create = reinterpret_cast<CreateFn>(reinterpret_cast<void*>(GetProcAddress(xs, "D3D12XboxCreateDevice")));
    auto valloc = reinterpret_cast<VAllocFn>(reinterpret_cast<void*>(GetProcAddress(xm, "XMemVirtualAlloc")));
    if (!create || !valloc) { std::puts("load failed"); return 2; }
    ID3D12Device* dev = nullptr;
    if (create(nullptr, __uuidof(ID3D12Device), reinterpret_cast<void**>(&dev)) != S_OK) { std::puts("no device"); return 2; }
    auto commit = [&](SIZE_T n) { return valloc(nullptr, n, MEM_RESERVE | MEM_COMMIT, PAGE_READWRITE); };
    auto make = [&](D3D12_HEAP_TYPE t, UINT64 n, ID3D12Heap** out) {
        D3D12_HEAP_DESC d = heap(t, n);
        return dev->CreateHeap(&d, __uuidof(ID3D12Heap), reinterpret_cast<void**>(out));
    };
    ID3D12Heap* h = nullptr;
    // A: control. xmem first, then a heap.
    void* v = commit(256 * MiB);
    HRESULT hr = make(D3D12_HEAP_TYPE_DEFAULT, 384 * MiB, &h);
    std::printf("A xmem256_then_heap384: xmem=%s heap_hr=0x%08lx\n", v ? "ok" : "fail", (unsigned long)hr);
    if (h) { h->Release(); h = nullptr; }
    if (v) { VirtualFree(v, 0, MEM_RELEASE); v = nullptr; }
    // B: heap first, then xmem.
    hr = make(D3D12_HEAP_TYPE_DEFAULT, 384 * MiB, &h);
    v = commit(256 * MiB);
    std::printf("B heap384_then_xmem256: heap_hr=0x%08lx xmem=%s combined_live_MiB=%d\n", (unsigned long)hr, v ? "ok" : "fail", (SUCCEEDED(hr) && v) ? 640 : 0);
    if (h) { h->Release(); h = nullptr; }
    if (v) { VirtualFree(v, 0, MEM_RELEASE); v = nullptr; }
    // C: release the heap while a placed resource is still alive.
    ID3D12Resource* res = nullptr;
    hr = make(D3D12_HEAP_TYPE_UPLOAD, 300 * MiB, &h);
    D3D12_RESOURCE_DESC bd = buffer(64 * 1024);
    HRESULT pr = h ? dev->CreatePlacedResource(h, 0, &bd, D3D12_RESOURCE_STATE_GENERIC_READ, nullptr, __uuidof(ID3D12Resource), reinterpret_cast<void**>(&res)) : E_FAIL;
    if (h) { h->Release(); h = nullptr; }
    ID3D12Heap* h2 = nullptr;
    HRESULT second = make(D3D12_HEAP_TYPE_UPLOAD, 300 * MiB, &h2);
    void* map = nullptr; bool usable = false;
    if (res && res->Map(0, nullptr, &map) == S_OK && map) { std::memset(map, 0x5a, 4096); usable = static_cast<unsigned char*>(map)[4095] == 0x5a; res->Unmap(0, nullptr); }
    std::printf("C release_heap_keep_placed: first_hr=0x%08lx placed_hr=0x%08lx second_heap300_hr=0x%08lx first_placed_still_mappable=%d\n",
                (unsigned long)hr, (unsigned long)pr, (unsigned long)second, usable);
    if (h2) h2->Release();
    if (res) res->Release();
    // D: ordinary CreateHeap over the cap (control), then the newer entry points.
    hr = make(D3D12_HEAP_TYPE_DEFAULT, 600 * MiB, &h);
    std::printf("D CreateHeap600: hr=0x%08lx\n", (unsigned long)hr);
    if (h) { h->Release(); h = nullptr; }
    ID3D12Device4* dev4 = nullptr;
    if (dev->QueryInterface(__uuidof(ID3D12Device4), reinterpret_cast<void**>(&dev4)) == S_OK) {
        D3D12_HEAP_DESC d = heap(D3D12_HEAP_TYPE_DEFAULT, 600 * MiB);
        hr = dev4->CreateHeap1(&d, nullptr, __uuidof(ID3D12Heap), reinterpret_cast<void**>(&h));
        std::printf("D CreateHeap1_600: hr=0x%08lx\n", (unsigned long)hr);
        if (h) { h->Release(); h = nullptr; }
        dev4->Release();
    }
    D3D12_HEAP_PROPERTIES hp{}; hp.Type = D3D12_HEAP_TYPE_DEFAULT;
    D3D12_RESOURCE_DESC big = buffer(600 * MiB);
    ID3D12Resource* committed = nullptr;
    hr = dev->CreateCommittedResource(&hp, D3D12_HEAP_FLAG_NONE, &big, D3D12_RESOURCE_STATE_COMMON, nullptr, __uuidof(ID3D12Resource), reinterpret_cast<void**>(&committed));
    std::printf("D CreateCommittedResource600: hr=0x%08lx\n", (unsigned long)hr);
    if (committed) committed->Release();
    dev->Release();
    std::puts("DONE");
    return 0;
}
```

## 3. `xmem` view holds none of the frame (#494)

Recreate the tree from `cd945ba7`: apply each PR's diff from its merge base, excluding `CHANGELOG.md`, and keep both `vmem.cpp` and `presentx.cpp` in `shims/d3d12_xs/CMakeLists.txt`. Then add:

```diff
diff --git a/tests/conformance/test_m2_first_frame/main.cpp b/tests/conformance/test_m2_first_frame/main.cpp
index 455f55fe..0bd220bf 100644
--- a/tests/conformance/test_m2_first_frame/main.cpp
+++ b/tests/conformance/test_m2_first_frame/main.cpp
@@ -251,6 +251,7 @@ void run() {
             require(false, detail);
         }
     }
+    { const uint32_t* x = reinterpret_cast<const uint32_t*>(region.view); unsigned m = 0, z = 0; for (UINT i = 0; i < width * height; ++i) { uint32_t v = i * 0x9e3779b1u; m += x[i] == (v ^ (v >> 15)) + 0x01234567u; z += x[i] == 0; } std::printf("PROBE xmem_view=%p backing_map=%p xmem_matching=%u/256 xmem_zero=%u/256\n", region.view, reinterpret_cast<void*>(region.config.cpu_pointer), m, z); }
     checkpoint(true, "256 RGBA8 pixels matched");
     stage = "teardown";
 }
```

One run with the probe (after three unmodified runs passing 6/6):

```text
PROBE xmem_view=0000000000c70000 backing_map=0000000215c8c000 xmem_matching=0/256 xmem_zero=256/256
```

The unmodified fixture on #494 alone fails at `placed_resource`.

## 4. Destruction sentinel

```sh
x86_64-w64-mingw32-g++ -std=c++20 -O1 -static destroy_probe.cpp -o destroy_probe.exe
target/debug/bx-cli run --shims build-shims/bin --bottle <bottle> --backend d3dmetal \
  --timeout 60 --log destroy.jsonl --exe <path>/destroy_probe.exe
```

The program imports no shim. Output, identical on three runs:

```text
PLAIN_HEAP set=0x00000000 refs_while_alive=2 refs_after_release=1 destruction_notifier_qi=0x80004002
PLACED_HEAP placed_hr=0x00000000 heap_release_returned=0 sentinel_refs_while_resource_alive=2 after_resource_release=1
COMMITTED refs_while_alive=2 refs_after_release=1
AFTER_DEVICE_RELEASE plain=1 placed=1 committed=1
DONE
```

Source:

```cpp
// Original synthetic probe: when does the runtime release a private-data interface?
#define WIDL_EXPLICIT_AGGREGATE_RETURNS
#include <windows.h>
#include <d3d12.h>
#include <cstdio>
static const GUID kKey = {0x5c1a7d10, 0x1b2c, 0x4f0e, {0x9a, 0x11, 0x20, 0x26, 0x11, 0x26, 0x00, 0x01}};
static const GUID kNotifier = {0xa06eb39a, 0x50da, 0x425b, {0x8c, 0x31, 0x4e, 0xec, 0xd6, 0xc2, 0x70, 0xf3}};
struct Sentinel : IUnknown {
    LONG refs = 1;
    HRESULT STDMETHODCALLTYPE QueryInterface(REFIID r, void** p) override {
        if (r == __uuidof(IUnknown)) { *p = this; AddRef(); return S_OK; } *p = nullptr; return E_NOINTERFACE; }
    ULONG STDMETHODCALLTYPE AddRef() override { return InterlockedIncrement(&refs); }
    ULONG STDMETHODCALLTYPE Release() override { return InterlockedDecrement(&refs); }
};
using CreateFn = HRESULT(WINAPI*)(IUnknown*, D3D_FEATURE_LEVEL, REFIID, void**);
int main() {
    auto create = reinterpret_cast<CreateFn>(reinterpret_cast<void*>(GetProcAddress(LoadLibraryA("d3d12.dll"), "D3D12CreateDevice")));
    ID3D12Device* dev = nullptr;
    if (!create || create(nullptr, D3D_FEATURE_LEVEL_11_0, __uuidof(ID3D12Device), (void**)&dev) != S_OK) return 2;
    D3D12_HEAP_DESC hd{}; hd.SizeInBytes = 65536; hd.Properties.Type = D3D12_HEAP_TYPE_UPLOAD; hd.Flags = D3D12_HEAP_FLAG_ALLOW_ONLY_BUFFERS;
    D3D12_RESOURCE_DESC bd{}; bd.Dimension = D3D12_RESOURCE_DIMENSION_BUFFER; bd.Width = 65536; bd.Height = 1; bd.DepthOrArraySize = 1;
    bd.MipLevels = 1; bd.SampleDesc.Count = 1; bd.Layout = D3D12_TEXTURE_LAYOUT_ROW_MAJOR;
    static Sentinel s_plain, s_placed, s_committed;
    ID3D12Heap* h = nullptr; ID3D12Resource* r = nullptr;
    dev->CreateHeap(&hd, __uuidof(ID3D12Heap), (void**)&h);
    HRESULT set = h->SetPrivateDataInterface(kKey, &s_plain);
    void* n = nullptr; HRESULT nq = h->QueryInterface(kNotifier, &n); if (n) ((IUnknown*)n)->Release();
    LONG held = s_plain.refs; h->Release(); Sleep(200);
    std::printf("PLAIN_HEAP set=0x%08lx refs_while_alive=%ld refs_after_release=%ld destruction_notifier_qi=0x%08lx\n", (unsigned long)set, held, s_plain.refs, (unsigned long)nq);
    dev->CreateHeap(&hd, __uuidof(ID3D12Heap), (void**)&h);
    h->SetPrivateDataInterface(kKey, &s_placed);
    HRESULT pr = dev->CreatePlacedResource(h, 0, &bd, D3D12_RESOURCE_STATE_GENERIC_READ, nullptr, __uuidof(ID3D12Resource), (void**)&r);
    ULONG heap_left = h->Release(); Sleep(200);
    LONG with_resource = s_placed.refs; r->Release(); Sleep(200);
    std::printf("PLACED_HEAP placed_hr=0x%08lx heap_release_returned=%lu sentinel_refs_while_resource_alive=%ld after_resource_release=%ld\n", (unsigned long)pr, (unsigned long)heap_left, with_resource, s_placed.refs);
    D3D12_HEAP_PROPERTIES hp{}; hp.Type = D3D12_HEAP_TYPE_UPLOAD;
    dev->CreateCommittedResource(&hp, D3D12_HEAP_FLAG_NONE, &bd, D3D12_RESOURCE_STATE_GENERIC_READ, nullptr, __uuidof(ID3D12Resource), (void**)&r);
    r->SetPrivateDataInterface(kKey, &s_committed); held = s_committed.refs; r->Release(); Sleep(200);
    std::printf("COMMITTED refs_while_alive=%ld refs_after_release=%ld\n", held, s_committed.refs);
    dev->Release(); Sleep(200);
    std::printf("AFTER_DEVICE_RELEASE plain=%ld placed=%ld committed=%ld\nDONE\n", s_plain.refs, s_placed.refs, s_committed.refs);
    return 0;
}
```

## Raw log and binary hashes

| File | SHA-256 |
| --- | --- |
| `raw/pr486/test_d3d12_placed.log` | `9838b3e21a2345bd7a1872521ff0d6eae7bce9c394a419ff1c3fe72d59facde2` |
| `raw/pr486/placed2.log` | `71b0b986d30af14da9be27a1dcb2bbd87ec3b7f4eeb7c31367febc72af0ba46c` |
| `raw/pr486/placed3.log` | `5fa44ce8e11cf149a2e60c11b8d21a1d492bc435587aa3fe05eba008075d62a5` |
| `raw/pr486/instr.log` | `b19505ae6205bc36a789c16e212fa05d939482c4a1d2e68fb035d21a308e633f` |
| `raw/pr486/instr.jsonl` | `c4a93b496b396745c4ae71dfc62d681604d36dcfd5590fd5a9a8745fcee92c38` |
| `raw/pr486/fix.log` | `6a160529c7bd7ee25c8c7155d7061b900de7b6c784eb5ad736056c702756417f` |
| `raw/pr486/probe1.log` | `318b7f9d6d059fe076c45d5e2b4770ef43cbf9a8ee918516641f16022d08cfca` |
| `raw/pr486/probe2.log` | `4d03dafbce7fc5d00365c93b68f2f538ed8ed4242d5b152a835edf38df267253` |
| `raw/pr486/probe1.jsonl` | `c08978ef990fc233a196b4f0448e170a2a0f1b9107c16d2e3981a32a9d82d6d5` |
| `raw/pr486/probe2.jsonl` | `2af04a963db12736c2697f72b1f3c98300efa9ca31fc9dac96e347f645929daa` |
| `raw/pr494/m2_1.log` | `bdb4397684de9e3cc6e76af3661d9b83d0b2b26c3e82c996ecc9109b442bc96d` |
| `raw/pr494/m2_2.log` | `d8cf5dc849acf194e1b529e31355fb19c62e9bd8ae6da81af9f9b6af815e1dbf` |
| `raw/pr494/m2_3.log` | `2556ee1eebf5bf903381456d0fef14b09b389fcea678d5b7555307d563f347b1` |
| `raw/pr494/probe.log` | `7176dee6f2d7a4f7991e39b39ceb062b8101974111f2a61c10b39d2795dc31ab` |
| `raw/pr494/base.log` | `9f91a2a9798b06b179867df3f5c9bf873d9918fcf5b2e04d99f740d51e656bda` |
| `raw/probes/destroy1.log` | `2f9c4a3492e1d7bf01dc3bd943fba62958b078cc0a920722707e28cbfd2dd3ea` |
| `raw/probes/destroy2.log` | `2f9c4a3492e1d7bf01dc3bd943fba62958b078cc0a920722707e28cbfd2dd3ea` |
| `raw/probes/destroy3.log` | `2f9c4a3492e1d7bf01dc3bd943fba62958b078cc0a920722707e28cbfd2dd3ea` |
| `source/budget_probe.exe` | `9664d3c7e63384077b5464e7e95ca3ca77d21783b8ac374bda639f09f359d432` |
| `source/destroy_probe.exe` | `85f377599c5d5dba0c53d941ce086c5cb16da558dd04724bbf5b7e973bfcc976` |

## Test bottle state after these runs

Before the first run, `system32` of `betterxen-maintainer-test` held no BetterXen shim. `bx-cli` then staged three files, last written by the **combined unmerged tree** in experiment 3:

| File in `drive_c/windows/system32` | Size | SHA-256 |
| --- | --- | --- |
| `bx_runtime_veh.dll` | 284588 | `1c1afdcfe1302b4115a20716d0371ea1ae82320bfc512de958a87e22f5d409fe` |
| `xgameruntime.dll` | 6350274 | `7abb32a4ff9d6827c20e5aae88f8e4ce3ba9b65e1e130ab805c342722f79f2bb` |
| `gameinput.dll` (replaced Wine's builtin) | 2503497 | `f16e29af5ad3f30977bc58f170d014584b475fcc9ad7e57e6e30486708b8cb9b` |
| `.gameinput.dll.wine-builtin.bak` (original) | 17472 | `89013ac6cf29138c3b3926a01eda5338888f58b84e5784b122fd813fcada4ef5` |
| `.bx-staged` (ownership record) | 329 | — |

The backup is byte-identical to CrossOver's own `lib/wine/x86_64-windows/gameinput.dll`. `system.reg`, `user.reg` and `userdef.reg` were rewritten by the runs; their earlier contents were not recorded. `cxbottle.conf` was not modified. The DLLs carry no version resource that was checked; the hashes identify them. Nothing was deleted or restored.

**Contamination risks.** Anything run in this bottle without `bx-cli` staging would load these review-build DLLs, including a replaced GameInput. `bx-cli` overwrites only the files it recorded, so a file a later build no longer stages stays behind. `xmem.dll` and `d3d12_xs.dll` come from the `--shims` directory and are not staged. D3DMetal caches outside the bottle were not examined.

**Clean reproduction (not run).** Use a new bottle rather than reusing one:

```sh
CX=/Applications/CrossOver.app/Contents/SharedSupport/CrossOver/bin
"$CX/cxbottle" --bottle bx-review-clean --create --template win11_64
target/debug/bx-cli run --bottle bx-review-clean --backend d3dmetal --timeout 90 \
  --log clean.jsonl --exe build-shims/bin/test_shim_exports.exe
```

`betterxen-test` is unsuitable as it stands: it holds its own `bx_runtime_veh.dll` that `bx-cli` refuses to overwrite.

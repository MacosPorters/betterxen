# Wave 1 Parallel Contributor Backlog (M0 → M1)

All issues below were written with **disjoint file scopes** so contributors could work in parallel after M0. Each issue uses `.github/ISSUE_TEMPLATE/track-handoff.yml`. The entries below record the original contracts, not what was delivered.

## Status as of 2026-11-26

| Entry | Issue | State | Follow-up |
| --- | --- | --- | --- |
| M1-A1 | #408 | open | |
| M1-A2 | #409 | merged | |
| M1-A3 | #410 | merged | #437 |
| M1-B1 | #411 | merged; hook dispatch not implemented | #442 |
| M1-B2 | #412 | merged | |
| M1-B3 | #413 | merged | |
| M1-C1 | #414 | merged; newer interfaces not proxied | #443, #444 |
| M1-C2 | #415 | merged | |
| M1-D1 | #416 | merged; not reachable from Windows guests, no histogram | #445 |
| M1-D2 | #417 | merged | |
| M1-E1 | #418 | merged | #439 (open) |
| M1-E2 | #419 | merged | |
| M1-F1 | #420 | merged; stages still pending | #437, #446 |

The [milestone page](https://github.com/MacosPorters/betterxen/milestone/17) is current. The exit gate is in the [roadmap](../roadmap.md#m1-exit-gate).

---

## Track A — Host Launcher, Runtime VEH & Win32 Bridge

### `[M1-A1]` CrossOver 26.3 Bottle Provisioner & Trace Runner (`bx-cli run`)
- **Scope:** `launcher/crates/bx-cli/**`, `launcher/crates/bx-telemetry/**`
- **Contract:** Extend `bx-cli run` to create/reuse an isolated CrossOver/Wine prefix, stage built DLLs from `build-shims/bin/`, configure `ROSETTA_ADVERTISE_AVX=1`, `WINEDLLOVERRIDES`, `BX_PROFILE`, and `BX_LOG_FILE`, capture exit status, and automatically invoke `bx-telemetry` summary on exit.
- **Test:** Run `tests/conformance/test_veh_clwb.exe` and `test_shim_exports.exe` via `bx-cli run` and verify exit code 0 + JSONL summary.

### `[M1-A2]` Full `clwb` + Crash Register Dumper in `bx_runtime_veh.dll`
- **Scope:** `shims/runtime_veh/**`, `tests/conformance/test_veh_clwb/**`
- **Contract:** Add unit/conformance coverage for all x86-64 `clwb` addressing modes (`[reg]`, `[reg+disp8]`, `[reg+disp32]`, `[r8..r15]`, SIB `[base+idx*scale+disp]`, RIP-relative), plus an unhandled-exception filter logging crashing module RVA, RIP, RSP, and general-purpose registers to `BX_LOG_FILE`.
- **Test:** `test_veh_clwb.exe` executes all 6 `clwb` ModRM/SIB encodings under Rosetta 2 and asserts exact trap count.

### `[M1-A3]` Win32 Bridge Conformance & Window Message Smoke Test
- **Scope:** `shims/win32_bridge/**`, `tests/conformance/test_win32_bridge/**`
- **Contract:** Verify all 6 DLLs (`api-ms-win-core-psm-appnotify-l1-1-0/1`, `ext-ms-win-rtcore-ntuser-{message,window}[-ansi]-l1-1-0`) forward cleanly to Wine `user32.dll` and register/unregister PSM notifications thread-safely.
- **Test:** Synthetic `test_win32_bridge.exe` creates a hidden window via `ext-ms-win-rtcore-ntuser-window-ansi-l1-1-0.dll` and pumps messages via `PeekMessageA`/`TranslateMessage`/`DispatchMessageA`.

---

## Track B — Memory (`xmem.dll`) & Storage (`XCRDAPI.dll`)

### `[M1-B1]` `xmem.dll` Core Allocator, `__XMemAllocationHooks__` & Lockhart Working-Set Cap
- **Scope:** `shims/xmem/**`, `tests/conformance/test_xmem/**`
- **Contract:** Implement `XMemAlloc`, `XMemAllocDefault`, `XMemFree`, `XMemFreeDefault`, `XMemVirtualAlloc`, `XMemVirtualQuery`, `XMemTransferMemory`, `XMemGetWorkingSetStatistics`, `XMemIsToolsMemAvailable`. Resolve optional host `__XMemAllocationHooks__` from the main executable via `GetProcAddress(GetModuleHandleA(NULL), "__XMemAllocationHooks__")` and enforce the configured Lockhart/Anaconda working-set ceiling from `bx_get_runtime_config()`.
- **Test:** `test_xmem.exe` verifies aligned allocations, hook override dispatch, `VirtualQuery` metadata, and working-set reporting.

### `[M1-B2]` `xmem.dll` Physical & Scatter Page Mapping Engine
- **Scope:** `shims/xmem/**`, `tests/conformance/test_xmem/**` (coordinate with M1-B1 via `shims/xmem/physical_pages.cpp`)
- **Contract:** Implement `XMemAllocatePhysicalPages`, `XMemFreePhysicalPages`, `XMemMapPhysicalPages`, `XMemMapPhysicalPagesScatter`, and `bx_xmem_query_region` (backing 64 KB / 2 MB console page arrays with Win32 file-mapping views or `VirtualAlloc2` placeholders so scattered physical pages alias cleanly to contiguous VAs).
- **Test:** `test_xmem_physical.exe` allocates physical pages, maps them into two distinct VAs (and scatter order), writes through VA1, and reads expected bytes through VA2.

### `[M1-B3]` `XCRDAPI.dll` DirectStorage Factory & Async File Queue MVP
- **Scope:** `shims/xcrdapi/**`, `tests/conformance/test_xcrdapi/**`
- **Contract:** Implement `DStorageGetFactory`, `IDStorageFactoryX` / `X1` / `X2` (`OpenFile`, `CreateQueue`, `CreateStatusArray`, `CreateErrorRecord`), `IDStorageFileX`, and `IDStorageQueueX` backed by a Win32 async I/O worker pool supporting uncompressed file-to-memory and memory-to-memory requests + fence/event signaling.
- **Test:** `test_xcrdapi.exe` enqueues batched file reads, submits, waits on status/event, and validates CRC32 of read buffers.

---

## Track C — Graphics (`d3d12_xs.dll` & `xg_xs.dll`)

### `[M1-C1]` `d3d12_xs.dll` Device Creation & Auto-Logging COM Vtable Proxy
- **Scope:** `shims/d3d12_xs/**`, `tests/conformance/test_d3d12_xs/**`
- **Contract:** Implement `D3D12XboxCreateDevice` wrapping Wine `d3d12.dll` (`D3D12CreateDevice` on D3DMetal 3.0) in a COM proxy implementing `ID3D12Device` through `ID3D12Device11`, `ID3D12CommandQueue`, `ID3D12CommandAllocator`, and `ID3D12GraphicsCommandList` through `10`. Every unimplemented Xbox-only vtable slot must call `BX_VTABLE_STUB_LOG(iface, slot, method)`. Wire `CreateGraphicsPipelineState` / `CreateComputePipelineState` to inspect shader blobs via `abi/include/bx_shader_bridge.h`.
- **Test:** `test_d3d12_xs.exe` calls `D3D12XboxCreateDevice`, creates a command queue + allocator + list + root signature, and exits cleanly under CrossOver + D3DMetal 3.0.

### `[M1-C2]` `xg_xs.dll` Texture Address Computer MVP
- **Scope:** `shims/xg_xs/**`, `tests/conformance/test_xg_xs/**`
- **Contract:** Implement `XGCreateTextureComputer`, `XGSuggestSwizzleMode`, and a COM `IXGTextureAddressComputer` (`GetResourceLayout`, `GetTexelElementOffsetBytes`, `CopyIntoSubresource`, `CopyFromSubresource`) starting with linear-compatible layouts and structured unit tables for RDNA 2 `SW_64KB_S/D/R` tile math (expanded in M3).
- **Test:** `test_xg_xs.exe` queries 2D RGBA8 and BC1/BC7 mip-chain layouts and validates monotonic subresource offsets and round-trip copies.

---

## Track D — Shader Pipeline & D3DMetal Isolation

### `[M1-D1]` `dxbc-accc` C ABI DLL (`bx_shader_bridge.dll`) & Shader Telemetry
- **Scope:** `shader-transpiler/crates/dxbc-accc/**`
- **Contract:** Expose `bx_shader_inspect`, `bx_shader_prepare_for_d3dmetal`, and `bx_shader_free_buffer` matching `abi/include/bx_shader_bridge.h`, add an `FSPX` section-table parser (section 0/1/6/7 header extraction), and log shader-kind histograms (DXIL vs. ACCC/FSPX) to prepare for the M2 Decision Gate.
- **Test:** `cargo test -p dxbc-accc` passes on synthetic `DXBC+DXIL`, `DXBC+ACCC(FSPX)`, and malformed blobs.

### `[M1-D2]` Standalone `utmapp/d3dmetal-native` Evaluation & Harness Prototype
- **Scope:** `shader-transpiler/crates/d3dmetal-harness/**`, `docs/research/upstream-registry.md`
- **Contract:** Record `utmapp/d3dmetal-native` (MIT) in `docs/research/upstream-registry.md` and build an opt-in macOS test binary that links user-supplied `D3DMetal.framework` via `d3dmetal-native` to compile and execute a synthetic SM 6.6 DXIL compute shader outside CrossOver/Wine.
- **Test:** Documented reproducible local run against user-supplied `D3DMetal.framework` with exact buffer readback check.

---

## Track E — Platform Runtime, Input & Audio/DSP

### `[M1-E1]` `xgameruntime.dll` Offline GDK Runtime & `XAsync` / `XTaskQueue` Engine
- **Scope:** `shims/xgameruntime/**`, `tests/conformance/test_xgameruntime/**`
- **Contract:** Implement a real thread-pool `XTaskQueue` + `XAsync` completion state machine backing `XUserAddAsync`, `XStoreQueryGameLicenseAsync`, `XStoreAcquireLicenseForPackageAsync`, `XSystemGetDeviceType` (Lockhart/Anaconda), and `XPersistentLocalStorage` mapped into the Wine bottle.
- **Test:** `test_xgameruntime.exe` creates a task queue, runs async user + license + console ID queries, dispatches completion callbacks, and verifies results.

### `[M1-E2]` `AcpHal` / `xapu` / `chad` Non-Blocking Audio Stubs & `GameInput` Ordinal 1 Wrapper
- **Scope:** `shims/audio/**`, `shims/gameinput/**`, `tests/conformance/test_audio_input/**`
- **Contract:** Ensure `AcpHalCreate`, `ApuCreateHeap`/`ApuAlloc`, `XApuConnect`/`EnqueueCommand`/`DequeueResult`, and `XDsp*` return valid opaque handles and drain submitted commands cleanly without hanging audio worker threads; verify `GameInput.dll` ordinal 1 (`GameInputCreate`) returns a valid minimal `IGameInput` stub (or forwards to Wine's `gameinput.dll`).
- **Test:** `test_audio_input.exe` exercises the full create → enqueue → drain → release lifecycle for all four DLLs.

---

## Track F — Conformance, Integration & Telemetry

### `[M1-F1]` End-to-End Synthetic Mini-Engine Fixture (`tests/conformance/test_integration_boot/`)
- **Scope:** `tests/conformance/**`
- **Contract:** Build a single synthetic PE32+ executable that simultaneously links `xmem.dll`, `XCRDAPI.dll`, `d3d12_xs.dll`, `xg_xs.dll`, `xgameruntime.dll`, `AcpHal.dll`, `xapu.dll`, `chad.dll`, `GameInput.dll`, `psm-appnotify`, and `ntuser` forwarders, executes AVX2 + `clwb`, initializes every subsystem in startup order, and verifies zero unresolved symbols and clean shutdown.
- **Test:** Passes end-to-end via `bx-cli run --shims build-shims/bin --exe build-shims/bin/test_integration_boot.exe`.

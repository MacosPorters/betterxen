# d3dmetal-native: evaluation as a possible graphics backend (2026-11-25)

[utmapp/d3dmetal-native](https://github.com/utmapp/d3dmetal-native) hosts Apple's `D3DMetal.framework` in an ordinary macOS process, without Wine, by implementing the host interface the framework expects. This record covers a source review and a build. **It was not run against D3DMetal**, so it contains no rendering, memory, frame-time or stability results. Maintainer request; no issue. Not an independent review and not legal advice.

**Disposition: reference only. No code adopted, no backend implemented.**

## Revisions and method

| Item | Value |
| --- | --- |
| d3dmetal-native | `af1ee1710711fadff41dead87960d0f1336c1047`; MIT, Turing Software LLC; test COM helpers and one demo adapted from DXVK (zlib) |
| Dependency `directx-native` | `7258be1c56cdbf1e55c56b1bc4a866169c8ca0da`; Apache-2.0 licence file; fetched by the pinned wrap |
| Vendored headers | MinGW-w64 DirectX and Windows headers; several are LGPL-2.1-or-later. Not audited file by file |
| Host | Apple M4, macOS 27.0, meson 1.12.1, ninja 1.13.2 |
| Reviewed | public header, README, build files, initialization, and the main parts of sharing, residency, sparse, fence, swapchain, hooking and allocation code |
| Not reviewed line by line | about 15,000 lines of `src/`; most of `dmn_com_hooks.cpp`, `dmn_formats.mm`, `dmn_kmtx.cpp`, GFXT event, registry, path and monitor files, and test sources |

## Build and tests

`meson setup` with the repository's x86_64 cross file and `-Ddev_framework_path=/nonexistent`, with `D3DMETAL_FRAMEWORK_PATH` unset and no framework anywhere near the build, so Apple's binary could not be loaded.

- The library built: x86_64 Mach-O, linking Foundation, AppKit, Metal, QuartzCore, CoreGraphics, CoreFoundation, libc++, libobjc and libSystem. 96 compiler warnings, no errors.
- `meson test --num-processes 1`: **4 passed, 51 failed.** Passed: `abi-check`, `dxil-container-bounds`, `shadow-heap-forward`, `event-api`. 50 failures report that the framework was not found; `lifecycle-churn` failed because its child control run produced no result.

None of the four passing tests renders anything or involves D3DMetal.

## Why it was not run against D3DMetal

The only copy on the test machine is version 3.0 inside CrossOver (`Contents/SharedSupport/CrossOver/lib64/apple_gptk/external`). The licence file in that framework covers the xxHash library only; no D3DMetal licence text was found beside it. CodeWeavers distributes the copy as part of CrossOver, and nothing read here permits loading it from another program. No Game Porting Toolkit download from Apple was present, and obtaining one requires a person to accept Apple's terms.

A copy from Apple would settle where the binary comes from. It would not by itself settle the following, which need a maintainer decision (and probably legal advice):

1. The reported Game Porting Toolkit 4.0 terms forbid reverse engineering and limit use ([licensing note](crossover-licensing.md): read from a third-party copy, unconfirmed).
2. The host interface appears to be derived from the compiled framework: `src/d3dmetal_gfxt.h` documents registers, instructions such as `MOVDQU`, and structure offsets (lines 35 to 80).
3. At run time the library changes the framework's behaviour: it writes private static flags found by mangled name (`src/dmn_init.cpp` lines 194 to 202, 296, 323), swizzles Metal methods (`dmn_share_install_swizzles`) and patches COM vtables (`src/dmn_hook.h`, `dmn_patch_slot`).
4. `scripts/patch_d3dmetal_imports.py` rewrites the framework binary's import table and re-signs it. The engineering rules forbid modifying proprietary components without permission. It was not run.
5. `dmn_set_executable_path` makes D3DMetal apply the per-program profile of a different executable name; a test passes the name of a commercial game.

## Assessment against BetterXen's questions

All statements in this table come from the source and its comments. None is measured here.

| Question | Finding |
| --- | --- |
| Can it replace the CrossOver backend? | No. It runs no Windows program. It offers Direct3D to a native macOS program compiled against it, so the loader, Windows API, threading, files and input that CrossOver provides are out of its scope |
| ARM64 host | The whole process must be x86_64 (`meson.build` line 14). A BetterXen host linking it would be an x86_64 executable that BetterXen builds, which the engineering rules forbid. An arm64 process could only talk to a separate x86_64 helper, which meets the same rule |
| Cross-process path for an arm64 peer | Shared textures, buffers and fences are described by plain structures plus a file descriptor over anonymous shared memory. An arm64 consumer looks possible in principle; no such consumer exists or was tried |
| D3D11 and D3D12 | Whatever the loaded framework implements; the library forwards the entry points and adds sharing, tiled resources and format answers. Older toolkit releases lack some interfaces, including `D3DCompile` |
| Shader compiler | D3DMetal consumes DXBC and DXIL. BetterXen's native compiler emits Metal Shading Language, so nothing connects them. The excluded `victory-shader` import emitted DXIL; this does not change its status |
| Existing Metal research | `xenora-metal` runs trusted kernels and render stages for synthetic GFX10.3 programs. d3dmetal-native is a Direct3D host. They share only Metal presentation concerns |
| Shared resource memory | Shared textures are substituted by linear `StorageModeShared` textures over shared memory, so producer and consumer map the same pages. Linear layout may cost GPU performance; not measured |
| Sparse resources | The library says D3DMetal has none and adds tier 2 using sparse Metal heaps, which commit their whole size at first use (1024 MiB chunks; budget from 6144 MiB). On a 24 GiB machine this needs measurement |
| Residency | A residency set on macOS 15 and later; a per-command-buffer `useResources:` batch for sparse textures |
| Synchronization | Shared fence values live in shared memory. D3D12 producers store them from the GPU with a private flag forced on; D3D11 producers store them from the CPU after completion; consumers run watcher threads, which adds wake-up latency |
| Presentation | `nextDrawable` on a caller's `NSView` or `CAMetalLayer`, on D3DMetal worker threads; 1x backing scale by default |
| Executable memory | The host interface allocates pages and makes them executable with `mprotect`, relying on Rosetta behaviour |

## Comparison with the CrossOver backend

**Not performed.** Memory, correctness, frame times, allocation overhead and stability are all unmeasured.

When it can be run, an equal comparison needs the same HLSL, resources and expected pixels as `workloads/d3d11-integrated`, with the window, thread and input code replaced by native macOS equivalents. That is a different binary under a different runtime, so only the graphics results (pixel digest, frame intervals, graphics memory) would be comparable; the runtime-service gates would not apply.

## Is there a supported way to host D3DMetal outside Wine?

None was found. A web search on 2026-11-25 did not surface Apple's licence text or any Apple documentation of a native-process host interface; the supported route described is Apple's Wine-based evaluation environment. Secondary sources (for example the [holt-d3dmetal release notes](https://github.com/al0x99/holt-d3dmetal/releases/tag/4.0b3), [MaPC](https://mapc.games/opensource.html) and [D4Mac's notices](https://github.com/MichaelLod/D4Mac/blob/main/THIRD_PARTY_LICENSES.md)) report that use is granted for developing, testing or evaluating video games on Apple hardware and that redistribution is non-commercial and whole-framework only. These are not Apple's words and stay unconfirmed. The document that would settle it is the licence inside a Game Porting Toolkit download from Apple.

Requirements before any proposal to adopt:

| Area | Requirement |
| --- | --- |
| Licence | Apple's own licence text read; maintainer (and ideally legal) view that this use, in-process flag writes and a derived interface header are permitted |
| Binary | Downloaded from Apple by the user; never copied from CrossOver, bundled or patched |
| Architecture | Either an approved exception to the ARM64 host rule, or an arm64 host that talks to a separately licensed x86-64 helper |
| Evidence | A demonstrated CrossOver limitation it removes, then the same scenes and expected pixels as the existing workloads |

## What would reopen this

All of:

1. A maintainer decision on the licensing and reverse-engineering questions above.
2. A Game Porting Toolkit copy obtained from Apple by a person who accepted its terms.
3. A recorded exception, or a design that satisfies the native ARM64 host rule.
4. A demonstrated limitation of the CrossOver path that this would remove, as the engineering rules require before adding a second backend.

## Registry entry (proposed)

`upstream-registry.md` and `component-reuse-matrix.md` are under claimed review (#363, PR #381), so they were not edited. Proposed row: *utmapp/d3dmetal-native, `af1ee17`, MIT, native-process host for D3DMetal; reference only, not adopted: depends on reverse-engineered interfaces of a proprietary framework and an x86_64 host process.*

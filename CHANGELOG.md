# Changelog

Notable changes are grouped using [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
Experimental interfaces may change. Authorized releases should identify their
exact revision and known limitations.

## [Unreleased]

Guest execution uses original, purpose-built test programs. Commercial Xbox game execution
remains unsupported.

### Added

- Added `scripts/compare-benchmarks.py` (with built-in `--self-test` unit tests) and documented it in `docs/testing/benchmarks.md`: compares structured benchmark run directories only when revision-independent configuration matches (`device`, `host_arch`, OS/toolchain versions, build profile, Metal validation flags, power source, thermal state, `reps`, and `warmup`), reports medians, IQR/span dispersion, missing or excluded entries, and factual uncertainty signals without an arbitrary pass/fail threshold.
- Added explicit claim-before-work rules for contributors and coding agents to `AGENTS.md`, `CONTRIBUTING.md` and the pull request template: check for existing claims, claim with `/claim`, keep a 24-hour lease alive with `/progress`, `/release` when stopping, and link the claimed issue. Without the claim workflow, the earliest valid claim comment wins.
- Added native Metal upload and readback of BC2 and BC3 textures: `xenora-texture` gains a
  pitched `BcAlphaUpload` for 16-byte blocks, and `NativeCompute::read_bc_alpha_upload` sends
  the original blocks to `BC2_RGBA` or `BC3_RGBA` textures when the device reports BC support.
  GPU tests cover explicit and interpolated (six- and eight-value) alpha, both colour endpoint
  orders, padded row and slice pitches, two layers and two mips against the CPU decoders and
  hand-derived anchors. On an Apple M5 Pro the native decoder's interpolated values were within
  1.14 codes of the exact values, so up to two codes from the truncating CPU reference.
- Added CPU block decoders for BC2, BC3, BC4 and BC5 (UNORM and SNORM) in `xenora-texture`, with tests whose expected texels were derived by hand from the Microsoft and Khronos block-compression specifications. Interpolated values truncate toward zero, as the existing BC1 decoder does; GPU upload and sampling of these formats are not implemented yet.
- Added `dbt::tests::sib_store_executes_through_the_native_dbt`, an independent DBT execution
  test that runs the SIB store form `mov [rsp + 0x20], rbx` through the native ARM64 DBT and
  verifies the value lands in guest memory.
- Added 8/16-bit MOV memory-operand support (opcodes 0x88/0x8A, register-to-memory and
  memory-to-register) to the native ARM64 DBT.
- Added GS/FS legacy segment prefix support to the native DBT and the bounded x86-64
  interpreter, including `CpuState.gs_base`/`CpuState.fs_base` offsets added to the effective
  address computation.
- Added Win64 TEB allocation and `gs_base` initialization to synthetic guest startup.

### Changed

- The static PE probe now marks x86-64 SIB memory addressing as DBT-supported, matching the
  native DBT.

### Fixed

- Updated `test_probe_static_analysis_does_not_execute_code`: the SIB store `mov [rsp + 0x20],
  rbx` at RVA 0x1004 is now genuinely supported by the native DBT (verified by the independent
  execution test above), so the probe reports no blocker for this program.
- Added `test_probe_static_analysis_detects_genuinely_unsupported_instruction` with a new
  synthetic fixture containing an INT3 at RVA 0x1004, preserving blocker-detection coverage
  for genuinely unsupported DBT instructions.

## [X18] - Automated Import Compatibility Generation & Metadata Ingestion

Guest execution uses original, purpose-built test programs. Commercial Xbox game execution
remains unsupported.

### Added

- Added `xenora-import-gen` deterministic code generation crate and `xenora imports generate <--pe <exe> | --csv <file>> [--out <dir>]` CLI command. Automatically extracts imported symbols from PE executables or CSV manifests, classifies them against a built-in offline database of documented public Win32/CRT and ISO C specifications without consuming AI tokens or network resources, maps compatible CRT functions to native macOS libc ABI adapters with guest memory bounds checking, generates logged Win32 service stubs, generates deterministic conformance test suites, and produces coverage reports in both Markdown and CSV format.
- Added automated metadata ingestion (`ingest_standard_system_headers`) and `xenora imports metadata sync` command capable of parsing real POSIX/C system headers into typed `MetadataCatalog` definitions (188 entries).
- Added `xenora imports verify` automated behavioral verification runner checking CRT math, string, and memory bounds invariants across 12 independent behavioral test cases.
- Added explicit failure semantics for unsupported stubs returning `ExitReason::IllegalInstruction(0xFE)` rather than fabricated success.
- Added public registration helper `ImportRegistry::register_with_status` and runtime integration test verifying generated bindings and stub failure semantics.
- Added 8-bit and 16-bit guest memory access helpers (`read_u8`, `write_u8`, `read_u16`, `write_u16`) to `xenora-loader::GuestMemory`.

- Added guest-facing D3D12-style graphics runtime interfaces (`GraphicsContext`, `GraphicsDevice`, `GraphicsCommandQueue`, `GraphicsCommandAllocator`, `GraphicsCommandList`, `GraphicsResource`) with explicit backend selection (`GraphicsBackendId::MetalNative` vs `GraphicsBackendId::SoftwareReference`). Original PE32+ integration tests verify device creation, generation-checked handle ownership, command recording, barriers, and presentation.
- Connected guest execution via ARM64 DBT and IAT dispatch to native Apple Silicon Metal rendering (`test_guest_dbt_graphics_pipeline_metal_render_dual_inputs`), proving that different guest vertex positions and clear colors produce distinct, verified GPU-rendered framebuffers with active Metal validation on Apple M5.
- Added explicit test graphics service exports (`xenora_fixture.dll!CreateGraphicsDevice`, `xenora_fixture.dll!SubmitGuestDraw`) cleanly separated from unsupported production Xbox ABI claims.
- Added control-flow-aware x86-64 static PE probe using worklist CFG traversal, eliminating false-positive startup blocker reports on unreachable INT3 padding.
- Added `xenora imports sync` command with `--check` verification mode for automated, idempotent import compatibility backlog generation.
- Added current-process handle and ID queries and a readable priority boost policy for the synthetic guest process. Original PE fixtures verify Win64 arguments, state changes and errors through imported calls.
- Static import reports identify canonical bindings, API-set aliases, implemented and partial handlers, existing stubs, and missing services.

- Added a single startup path for original PE32+ fixtures that maps and rebases the image, binds imports before running thread-local storage callbacks, then enters the guest.
- The static PE probe compares ordinary imports with the standard synthetic registry and exports unresolved DLL, symbol or ordinal, and import slot addresses to `missing_imports.csv`.

- Added synthetic PE32+ runtime support for 64-bit base relocations, thread-local storage setup and callbacks, Windows x64 calling convention arguments, and import binding to a limited service registry. The Xbox-style allocation service maps guest memory. Its free entry point remains a stub.
- Extended native ARM64 dynamic binary translation with selected memory addressing, immediate stores, register increments and decrements, and indirect control flow through guest import slots.
- Added runtime fixtures for rebasing, ordered callbacks, deterministic timing, guest memory allocation, and mock import dispatch. The GPU gate checks every pixel from a native offscreen draw through a synthetic import.

- Added a static PE compatibility probe that exports section, relocation, thread-local storage, import and bounded x86-64 entry-point analysis. Synthetic tests cover inspection and report export.
- Added native Apple Silicon ARM64 Dynamic Binary Translation (DBT) engine (`DbtEngine`) with basic block decoding, native ARM64 code lowering, and W^X-compliant JIT code cache (`JitMemory`) utilizing macOS `MAP_JIT` and `pthread_jit_write_protect_np`.
- Added an ARM64 machine instruction assembler (`Arm64Assembler`) supporting register/immediate arithmetic, logical operations, memory loads/stores, branch/call/ret control flow, and calling convention preservation.
- Added synthetic guest graphics service dispatch connecting guest x86-64 execution through native ARM64 translated blocks directly to Xenora's Metal graphics pipeline (`NativeCompute::render_rgba8`), producing and independently checking the first guest-rendered frame.
- Added Scale-Index-Base (SIB) memory addressing mode support to `xenora-loader`'s x86-64 interpreter (`[base + index*scale + disp]`) for general-purpose memory operand resolution.
- Added synthetic test fixture `test_sib_addressing_mode` verifying SIB stack-relative memory loads and stores in software interpretation.
- Added `xenora-loader` crate providing standalone PE32+ (64-bit Portable Executable) parsing, isolated guest virtual memory with strict page permissions (Read/Write/Execute), bounded software x86-64 instruction interpreter (`CpuState`, `Interpreter`, `RFlags`), and read-only diagnostic triage (`triage_file`, `triage_bytes`).
- Added CLI inspect command (`xenora inspect <path>`) for read-only PE32+ header and section structure examination.
- Added comprehensive integration tests verifying synthetic PE32+ image mapping, register arithmetic, stack push/pop, subroutine calls/returns, deterministic unsupported opcode rejection, memory fault on unmapped access, and execution budget enforcement.
- Added native Metal sampler abstraction (`MetalSampler`) supporting nearest-neighbor minification/magnification filtering, clamp-to-edge addressing, normalized coordinates, validated LOD bounds, and explicit `SamplerDesc` mapping.
- Added native Metal RGBA8 2D texture allocation, mip upload validation (`MetalRgba8Texture`, `Rgba8MipLevel`, `Rgba8TextureDesc`), and texture/sampler pipeline binding definitions (`BoundSampledTexture`, `BindingSlot`) for future translated shader stages.
- Added an independent pure-Rust CPU texture sampling reference for deterministic RGBA8 2D textures with nearest-neighbor filtering, normalized UV coordinates, clamp-to-edge addressing, explicit mip selection, and bounds validation.
- Added typed texture sampler descriptors and texture metadata validation supporting nearest filtering and clamp-to-edge modes for future pipeline integration.
- Added a persistent resource service for original fixed-base PE32+ fixtures. Generation-checked handles support buffer updates, checked submission views, and native Metal readback. Queued work captures its input across updates and slot reuse.
- Added translated gfx1030 vertex and pixel stages for a bounded graphics subset. Original shaders exercise position export, a vertex transform, constant color and perspective interpolation in offscreen Metal pixel comparisons on Apple M5.
- Added immutable guest texture handles for validated pitched BC1 uploads and native Metal readback. Tests cover mip levels, layers, partial edge blocks and transparency.
- Added an acceptance matrix for persistent resources and translated graphics, with a CPU test for explicit cross-lane instruction rejection.

- Added a synthetic PE32+ guest compute import. The guest submits a checked buffer and one-shot descriptor, then reports the result of an original gfx1030 XOR fixture translated to Metal. The Apple M5 gate checks repeatable readback, guest exit status, malformed requests and stale-handle rejection.
- Added native Metal upload and texel readback for validated synthetic BC1 subresources. The Apple M5 fixture checks 61 RGBA8 texels across mip levels and array layers using authored Metal Shading Language.
- Added statically known partial Wave32 execution masks for supported vector arithmetic and moves, with inactive-lane value preservation. An independent CPU decoder and an Apple M5 readback check exercise 67 values across wave boundaries.
- Added a CPU-only synthetic tiled texture reader with checked mip and array-layer addressing and BC1 block decoding.
- Added validated input buffer views for synthetic guest compute. An original translated gfx1030 shader read only the selected range and returned the expected bytes in a local Apple M5 test.
- Added forward Wave32 branches on statically known execution and vector condition masks. A local Apple M5 test checked that inactive vector operations preserve the input to a later arithmetic right shift.
- Added bounded named import resolution for an independently compiled synthetic PE32+ guest. The fixture checks guest memory transformation and guest exit status. An unresolved import fails before guest entry.
- Added synthetic guest fixtures for service dispatch and nested function calls. The X6 fixture checks both writable guest memory and instruction retirement after loading its fixed-base PE32+ image.

- Added a public documentation index for architecture, validation, and contributor references.

- Added a bounded x86-64 guest interpreter on native ARM64 macOS (X1), plus a loader for original fixed-base 64-bit Portable Executable (PE32+) fixtures and an experimental add-one service (X2).
- Added guest-to-GPU command submission for a validated synthetic compute workload (X3) and guest-driven Metal rendering to an offscreen 8×8 RGBA8 texture (X4). Independent review confirmed the compute output and every pixel for both rendering inputs on Apple M5.
- Added a typed shader intermediate representation and Metal Shading Language generation for a bounded subset of 32-lane AMD RDNA 2 compute on the desktop gfx1030 target, with fixture validation on Apple M5. Expanded the verified vector-register-only forms to include bitwise OR and logical shifts. Arithmetic right shift passed local CPU and Metal checks, with independent acceptance still open.
- Added independent CPU known-answer fixtures, LLVM decoder comparisons, and source and native semantic mutation checks.
- Added an arithmetic-right-shift (ASHR) entry to the synthetic fixture provenance manifest and extended the independent CPU oracle and known-answer suite with explicit checkpoints, a positive-value control, aliasing and malformed-stream coverage for `v_ashrrev_i32_e32`.
- Added a correctness-gated benchmark harness that records stage timings, power source, and thermal state.
- Repository contribution guidelines, issue forms, pull request checklist, community expectations and security reporting policy.

### Changed

- Added native ARM64 CI and repository audits for dependency direction, claims, and generated status. Expanded CI to check the standalone guest experiment, synthetic PE fixture bytes, and progress asset consistency.

- Made reading and updating the changelog mandatory for engineering work.

### Fixed

- Stop ignoring `fixtures/pe/trusted-runner.exe`. The runner embeds it with `include_bytes!`, so ignoring the file made a fresh checkout fail to compile.
- Corrected the ARM64 STP pre-index encoder, which had a hardcoded -32-byte immediate and therefore adjusted the host stack by the wrong amount once the translator frame grew to 48 bytes. The wrong frame silently corrupted the caller's stack and caused SIGSEGV in every native translation test. Emitter encodings are now checked against LLVM's assembler, and a test asserts that each translated block opens and closes the same 48-byte frame.
- Translated guest memory faults now stop the block before it commits further architectural effects. A faulting push, call or store leaves guest RSP and registers unchanged, the reported instruction count covers only completed work, and guest RIP identifies the faulting instruction. Previously the block continued after the helper recorded a fault, so later instructions and writes still took effect.
- Compute the static PE probe SHA-256 from the full input, including trailing bytes.
- Report TLS callbacks that halt or exhaust their execution budget as failures. Reject unsupported 32-bit increment and decrement forms in the native translator.

- Reject unknown guest experiment arguments with a nonzero exit status.

- Validate branch targets, buffer bindings, and program termination throughout a shader fixture, including instructions skipped by a resolved branch.
- Include arithmetic right shift in fixture reassembly checks and let bootstrap continue when optional LLVM discovery fails.
- Corrected the gfx1030 three-way comparison and rejection counts in the instruction-coverage documentation to match the current LLVM test output.

### Removed

- Removed the superseded Phase 01 continuation prompt and redundant planning instructions. Candidate verification records remain in the assurance documentation.

- Removed the speculative seven-phase roadmap from the README.

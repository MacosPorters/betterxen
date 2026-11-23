# Changelog

Notable changes are grouped using [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
Experimental interfaces may change. Authorized releases should identify their
exact revision and known limitations.

## [Unreleased]

Guest execution uses original, purpose-built test programs. Commercial Xbox game execution
remains unsupported.

### Added

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

- Reject unknown guest experiment arguments with a nonzero exit status.

- Validate branch targets, buffer bindings, and program termination throughout a shader fixture, including instructions skipped by a resolved branch.
- Include arithmetic right shift in fixture reassembly checks and let bootstrap continue when optional LLVM discovery fails.
- Corrected the gfx1030 three-way comparison and rejection counts in the instruction-coverage documentation to match the current LLVM test output.

### Removed

- Removed the superseded Phase 01 continuation prompt and redundant planning instructions. Candidate verification records remain in the assurance documentation.

- Removed the speculative seven-phase roadmap from the README.

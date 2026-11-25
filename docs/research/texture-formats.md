# Texture format and layout inventory

This page records which texture formats and memory layouts BetterXen handles today, and at which stage. It also records which gaps are worth closing and which are deferred. It is an inventory of the repository at the time of writing, built from the code and tests it links. It is not a support claim: a format listed here works only for the original synthetic fixtures named, on desktop specifications, and nothing here says anything about Xbox hardware or commercial titles.

## Stages

A format can be handled at one stage and missing at the next. The columns below keep them apart:

- **Container**: recognising a file or package format that holds textures (DDS, KTX and similar).
- **CPU decode**: turning one compressed block into texels on the CPU, used as an independent reference.
- **Layout**: converting between a stored memory layout and the row-major layout Metal expects.
- **Native**: uploading to a Metal texture of that pixel format and reading it back on the GPU.
- **Sampling**: a translated RDNA 2 image instruction reading the texture inside a shader.
- **Host graphics API**: the D3D12-shaped `GraphicsContext` entry points, exercised by tests that call them directly from Rust. [The interface contract](../interfaces.md#guest-texture-binding-descriptor-tables) states they are not yet wired to guest imports, so this column is not guest evidence.
- **Guest**: an original guest program (PE fixture) reaching the path through imported services, with a test that runs it.
- **Specification**: the public sections the implementation and its expected values cite.

"—" means not implemented. "Rejected" means the input is refused with an error rather than handled.

## Formats

| Format | Container | CPU decode | Layout | Native | Sampling | Host graphics API | Guest | Specification |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| RGBA8 unorm | — | not needed | row-major mips, direct or budgeted staged upload ([`crates/metal/src/texture_pipeline.rs`](../../crates/metal/src/texture_pipeline.rs)) | upload and render target (`render_rgba8*`, [`crates/metal/tests/native/texture_sampling.rs`](../../crates/metal/tests/native/texture_sampling.rs)) | `image_sample_lz`, 2D, level 0 ([`crates/isa/src/lib.rs`](../../crates/isa/src/lib.rs), [`crates/metal/tests/native/translated_render.rs`](../../crates/metal/tests/native/translated_render.rs)) | one-level 2D SRV, `DXGI_FORMAT_R8G8B8A8_UNORM`, called from Rust ([`crates/loader/src/graphics.rs`](../../crates/loader/src/graphics.rs), [`crates/loader/tests/graphics_textures.rs`](../../crates/loader/tests/graphics_textures.rs)) | synthetic `xenora.dll` texture services (`texture_create`, `texture_update`, `frame_submit_textured`, `texture_destroy`) driven by the Gate 5 original PE fixture ([`crates/ffi/src/lib.rs`](../../crates/ffi/src/lib.rs), test `textured_pe_fixture_executes_dynamic_updates_and_retirement` in [`crates/ffi/tests/resources.rs`](../../crates/ffi/tests/resources.rs)); not through the descriptor-table API | [`MTLPixelFormat.rgba8Unorm`](https://developer.apple.com/documentation/metal/mtlpixelformat/rgba8unorm) |
| BC1 | — | `decode_bc1` ([`crates/texture/src/lib.rs`](../../crates/texture/src/lib.rs)) | synthetic tiled layout, pitched uploads (`TiledBc1`, `Bc1Upload`) and per-subresource packed footprints (`BcTextureLayout`, `PackedBcUpload`) | `read_tiled_bc1`, `read_bc1_upload`, including packed footprints ([`crates/metal/tests/native/texture.rs`](../../crates/metal/tests/native/texture.rs), [`crates/metal/tests/packed_bc_upload.rs`](../../crates/metal/tests/packed_bc_upload.rs)) | sampled by the translated `image_sample_lz` pixel stage (`render_rgba8_sampled_bc1`, [`crates/metal/tests/native/translated_bc1.rs`](../../crates/metal/tests/native/translated_bc1.rs)) | one-level 2D SRV, `DXGI_FORMAT_BC1_UNORM`, called from Rust (same files as RGBA8) | — | [Khronos DFS 1.3, S3TC](https://registry.khronos.org/DataFormat/specs/1.3/dataformat.1.3.html#S3TC) (BC1 with and without alpha); [Microsoft, BC1](https://learn.microsoft.com/en-us/windows/win32/direct3d11/texture-block-compression-in-direct3d-11) |
| BC2, BC3 | — | `decode_bc2`, `decode_bc3` ([`crates/texture/src/bc.rs`](../../crates/texture/src/bc.rs), [`crates/texture/tests/bc_formats.rs`](../../crates/texture/tests/bc_formats.rs)) | pitched upload (`BcAlphaUpload`) and packed footprints | `read_bc_alpha_upload`, including packed footprints ([`crates/metal/tests/native/texture_bc_alpha.rs`](../../crates/metal/tests/native/texture_bc_alpha.rs), [`crates/metal/tests/packed_bc_upload.rs`](../../crates/metal/tests/packed_bc_upload.rs)) | — | — | — | [Khronos DFS 1.3, S3TC BC2](https://registry.khronos.org/DataFormat/specs/1.3/dataformat.1.3.html#s3tc_bc2) and [BC3](https://registry.khronos.org/DataFormat/specs/1.3/dataformat.1.3.html#s3tc_bc3); [Microsoft, BC2 and BC3](https://learn.microsoft.com/en-us/windows/win32/direct3d11/texture-block-compression-in-direct3d-11) |
| BC4, BC5 (unorm, snorm) | — | `decode_bc4_*`, `decode_bc5_*` ([`crates/texture/src/bc.rs`](../../crates/texture/src/bc.rs), [`crates/texture/tests/bc_formats.rs`](../../crates/texture/tests/bc_formats.rs)) | pitched upload (`BcChannelUpload`) and packed footprints | `read_bc_channel_upload`, including packed footprints ([`crates/metal/tests/native/texture_bc_channel.rs`](../../crates/metal/tests/native/texture_bc_channel.rs), [`crates/metal/tests/packed_bc_upload.rs`](../../crates/metal/tests/packed_bc_upload.rs)) | — | — | — | [Khronos DFS 1.3, RGTC BC4](https://registry.khronos.org/DataFormat/specs/1.3/dataformat.1.3.html#rgtc_bc4) and [BC5](https://registry.khronos.org/DataFormat/specs/1.3/dataformat.1.3.html#rgtc_bc5); [Microsoft, BC4 and BC5](https://learn.microsoft.com/en-us/windows/win32/direct3d11/texture-block-compression-in-direct3d-11) |
| BC6H (ufloat, sfloat) | — | single-subset modes only; two-subset modes rejected ([`crates/texture/src/bc6h.rs`](../../crates/texture/src/bc6h.rs), [`crates/texture/tests/bc6h.rs`](../../crates/texture/tests/bc6h.rs), vectors [`crates/texture/tests/fixtures/bc6h_vectors.txt`](../../crates/texture/tests/fixtures/bc6h_vectors.txt)) | packed footprints only, 16-byte blocks ([`crates/texture/src/footprint.rs`](../../crates/texture/src/footprint.rs)) | — | — | — | — | [Khronos DFS 1.3, BPTC BC6H](https://registry.khronos.org/DataFormat/specs/1.3/dataformat.1.3.html#bptc_bc6h) (section 20.2, Tables 121 and 122); [Microsoft, BC6H Format](https://learn.microsoft.com/en-us/windows/win32/direct3d11/bc6h-format) |
| BC7 unorm | — | all eight modes ([`crates/texture/src/bc7.rs`](../../crates/texture/src/bc7.rs), [`crates/texture/tests/bc7.rs`](../../crates/texture/tests/bc7.rs), vectors [`crates/texture/tests/fixtures/bc7_vectors.txt`](../../crates/texture/tests/fixtures/bc7_vectors.txt)) | packed footprints only, 16-byte blocks ([`crates/texture/src/footprint.rs`](../../crates/texture/src/footprint.rs)) | — | — | — | — | [Khronos DFS 1.3, BPTC BC7](https://registry.khronos.org/DataFormat/specs/1.3/dataformat.1.3.html#bptc_bc7) (section 20.1, Tables 109–120); [Microsoft, BC7 Format](https://learn.microsoft.com/en-us/windows/win32/direct3d11/bc7-format) |
| BC7 sRGB | — | same bytes as BC7; the sRGB transfer function is outside the decoder | — | — | — | — | — | as BC7 |
| Depth32Float | — | not needed | render attachment | `render_depth` with depth readback ([`crates/metal/tests/native/depth.rs`](../../crates/metal/tests/native/depth.rs)) | — | — | — | [`MTLPixelFormat.depth32Float`](https://developer.apple.com/documentation/metal/mtlpixelformat/depth32float) |

Sampler state is limited to nearest filtering and clamp-to-edge addressing ([`crates/texture/src/sampler.rs`](../../crates/texture/src/sampler.rs)). Other filter or address modes are rejected.

Native BC paths require `supportsBCTextureCompression`. A device without it fails as a capability error ([`crates/metal/src/compute.rs`](../../crates/metal/src/compute.rs)), separately from implementation failures.

Known source differences, recorded where they are implemented:

- **BC6H:** the Khronos text and Microsoft's sample code disagree on whether a negative value that rounds to zero keeps its sign (`-0.0` against `0.0`). `bc6h.rs` follows Khronos.
- **BC1:** Apple's hardware rounds BC1's interpolated palette entries differently from the spec-exact decoder (171/84 against 170/85, see `CHANGELOG.md`). The sampled BC1 fixtures therefore use only endpoint colours.

## Layouts

| Layout | State | Where |
| --- | --- | --- |
| Row-major RGBA8 mips | implemented | [`crates/metal/src/texture_pipeline.rs`](../../crates/metal/src/texture_pipeline.rs) |
| Budgeted staged RGBA8 upload | implemented | `MetalRgba8Texture::upload_staged`, [`crates/metal/src/texture_pipeline.rs`](../../crates/metal/src/texture_pipeline.rs) |
| Pitched, layer-major BC uploads (one pitch for every mip, padded slices) | implemented for BC1–BC5 | [`crates/texture/src/upload.rs`](../../crates/texture/src/upload.rs) |
| Per-subresource packed footprints for block-compressed mip chains (tight or aligned rows) | layout implemented for BC1–BC7; native readback covers BC1–BC5 (#241, PR #278) | [`crates/texture/src/footprint.rs`](../../crates/texture/src/footprint.rs), [`crates/texture/tests/bc_footprint.rs`](../../crates/texture/tests/bc_footprint.rs), [`crates/metal/tests/packed_bc_upload.rs`](../../crates/metal/tests/packed_bc_upload.rs) |
| The synthetic 2×2-tile BC1 layout | implemented; defined by the project's fixtures, not by any console | [`crates/texture/src/lib.rs`](../../crates/texture/src/lib.rs) |
| Desktop RDNA 2 surface tiling (swizzle modes) | unknown; not researched in this repository | — |
| Console texture memory layouts | unknown; no public source reviewed | — |

## Public specifications

These are the primary sources the implementations above cite. The format rows already name them in their source files.

- [Khronos Data Format Specification 1.3](https://registry.khronos.org/DataFormat/specs/1.3/dataformat.1.3.html): the [S3TC](https://registry.khronos.org/DataFormat/specs/1.3/dataformat.1.3.html#S3TC) section (BC1–BC3), the [RGTC](https://registry.khronos.org/DataFormat/specs/1.3/dataformat.1.3.html#RGTC) section (BC4, BC5), section 20.1 ["BC7"](https://registry.khronos.org/DataFormat/specs/1.3/dataformat.1.3.html#bptc_bc7) and section 20.2 ["BC6H"](https://registry.khronos.org/DataFormat/specs/1.3/dataformat.1.3.html#bptc_bc6h).
- Microsoft Direct3D documentation: ["Texture Block Compression in Direct3D 11"](https://learn.microsoft.com/en-us/windows/win32/direct3d11/texture-block-compression-in-direct3d-11), ["Block Compression (Direct3D 10)"](https://learn.microsoft.com/en-us/windows/win32/direct3d10/d3d10-graphics-programming-guide-resources-block-compression), ["BC6H Format"](https://learn.microsoft.com/en-us/windows/win32/direct3d11/bc6h-format) and ["BC7 Format"](https://learn.microsoft.com/en-us/windows/win32/direct3d11/bc7-format). These are used for the format definitions and DXGI format codes, not for console behaviour.
- Apple Metal documentation: [`MTLPixelFormat`](https://developer.apple.com/documentation/metal/mtlpixelformat) and [`MTLDevice.supportsBCTextureCompression`](https://developer.apple.com/documentation/metal/mtldevice/supportsbctexturecompression), for the native pixel formats and the capability check.
- AMD "RDNA 2" Instruction Set Architecture reference guide ([GPUOpen architecture documentation](https://gpuopen.com/amd-gpu-architecture-programming-documentation/)): MIMG instructions such as `image_sample_lz`, and the image resource and sampler descriptor formats. These cover desktop gfx1030 only. [Xbox GPU evidence](xbox-gpu.md) records why desktop statements do not transfer to the console without separate evidence.

## Unknowns

These are research questions, not missing features:

- **Console texture memory layouts and tiling.** No public specification has been reviewed. Desktop RDNA 2 tiling would be a separate research note and would still not establish console behaviour.
- **Console resource descriptor formats.** Unknown. See [Xbox GPU evidence](xbox-gpu.md), which lists descriptor layout among the unresolved items.
- **Asset and audio containers.** BetterXen parses none. The runtime's audio path accepts only raw PCM (`XENORA_AUDIO_FORMAT_PCM_S16LE` and `PCM_F32LE` in [`crates/ffi/include/xenora.h`](../../crates/ffi/include/xenora.h)). No original workload has shown a need for a container or a compressed audio format.

## Follow-ups

Ranked by how close each is to existing tested paths. The first item is already tracked; the others are proposals, so do not start them without an issue and a claim.

1. **BC6H two-subset modes and native BC6H/BC7.** Tracked in #208 (CPU decode, with an archived request to take it) and #209 (native upload and readback, whose imported proposal waits for maintainer approval). BC6H and BC7 already have packed footprints, so #209 can build on `PackedBcUpload`. Do not duplicate them.
2. **Translated sampling of BC2–BC5.**
   - Fixture: original blocks with hand-chosen endpoints and indices, sampled by the existing `image_sample_lz` pixel stage.
   - Expected values: derived by hand from the Khronos tables. The `bc.rs` decoders must not produce them, so that the decoder and the reference stay independent.
   - Owners: Metal and Compiler.
3. **Additional sampler state** (linear filtering, wrap and mirror addressing).
   - Fixture: a small original RGBA8 texture with distinct texels.
   - Expected values: from the public Direct3D and Metal sampling equations, computed in exact fractions.
   - Owners: Metal and Compiler.
4. **Desktop RDNA 2 image resource descriptor decoding.** Needed before guest-supplied descriptors can select formats.
   - Fixture: original descriptor words for each supported format.
   - Expected values: from the AMD RDNA 2 ISA guide's descriptor tables.
   - Owner: ISA.
   - This does not cover console descriptors.

Deferred, with no demonstrated need:

- BC7 sRGB
- DDS and KTX containers
- ASTC and ETC (not part of the desktop Direct3D 12 BC set this target uses)
- any compressed audio format
- console tiling, until a public source exists

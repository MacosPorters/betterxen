# Proposed shader entry and resource bindings

[Issue #302](https://github.com/MacosPorters/betterxen/issues/302) defines a
synthetic `gfx1030` pixel entry with two independently bound textures and
samplers. This proposal uses public desktop RDNA 2 instruction semantics and
the existing BetterXen entry conventions. The console shader ABI remains open
research.

## Existing entry and binding rules

| Stage | Guest entry | Metal interface |
| --- | --- | --- |
| Compute | Wave32 with full initial `EXEC`. `v0` is the linear grid index. The bounded vector-memory ABI selects a read-only word input and a disjoint, equally sized word output. | `xenora_compute` uses buffers 0 and 1 and `thread_position_in_grid`. |
| Vertex | `v[0:3]` receives a clip-space position and `v[4:7]` receives the vertex's color or texture-coordinate parameter. The stage exports `POS0` and optionally `PARAM0`. | `xenora_vertex` reads one float4 per vertex from each of buffers 0 and 1, and exports the optional parameter at `user(locn0)`. |
| Pixel | `v0` and `v1` are reserved for the matching interpolation phases for attribute 0. A pair reads the linked `PARAM0` using center-perspective interpolation. `MRT0` exports one float4. | `xenora_pixel` lowers the currently admitted `image_sample_lz` operands `s[0:7]` and `s[8:11]` to fragment `texture(0)` and `sampler(0)`. The sample uses `level(0.0f)`. |

See the [compute compiler](../../crates/shader/src/lib.rs),
[graphics compiler](../../crates/shader/src/graphics.rs) and
[interface contracts](../interfaces.md#guest-texture-binding-descriptor-tables).

The existing guest descriptor-table path accepts one pixel-visible
shader-resource view (SRV) at `t0` and one sampler at `s0`, both in space 0.
Each table resolves one occupied slot in the bound heap of its type. The SRV
selects mip 0 of a single-slice, single-level RGBA8 UNORM or BC1 UNORM texture.
The sampler uses point filtering and clamp addressing. The backend supplies
the translated fixture shader and binds the resolved texture and sampler at
Metal slot zero. The current entry contract covers these synthetic operations.

Desktop D3D12 describes SRV and sampler tables as typed ranges in separate
heaps with explicit shader registers and visibility. Applications keep
referenced descriptors valid through GPU completion. This is a reference
for the table and lifetime concepts in the proposal. [D3D12 Resource Binding
Functional Spec][d3d12].

## Synthetic binding manifest, version 1

The manifest consists of an entry and layout validated when the fixture is
compiled, plus one set of bindings resolved for each draw. The shader code
is supplied by the admitted fixture. Moving a view or changing sampler
state changes the draw bindings without changing the compiled layout.

| Record | Required fields and limits |
| --- | --- |
| Manifest | `version = 1`, `target = gfx1030`, `stage = pixel`. Use the existing linked vertex input and single `RGBA8Unorm` color attachment. |
| Entry | A nonempty fixture-local `name` and the shader byte `offset` and `length`. Both bounds are multiples of four. The range must be nonempty and contained in the fixture with checked arithmetic. Decode the whole range as one entry. Include stage, target, the exact code bytes and the canonicalized layout in compiled-artifact identity. |
| Texture layout | One or two entries `{slot, sgpr_base, kind, access, format}`. Initially `kind = texture2d`, `access = sample-read`, `format = rgba8unorm`. Each entry claims eight scalar general-purpose registers (SGPRs). |
| Sampler layout | One or two entries `{slot, sgpr_base}`. Each claims four SGPRs. Texture and sampler slot numbers have separate namespaces. Each namespace uses dense slots starting at zero. |
| Texture view for a draw | `{slot, handle, base_mip, mip_count, base_slice, slice_count}`. The handle includes the object ID and generation. Require `mip_count = 1`, `base_slice = 0`, `slice_count = 1`, a matching 2D resource, and a mip within the resource's initialized levels. The selected mip is level 0 of the bound view. |
| Sampler for a draw | `{slot, normalized, min_filter, mag_filter, mip_filter, address_u, address_v}`. Require normalized coordinates, nearest minification and magnification, and `mip_filter = none`. U and V independently admit `clamp-to-edge` and `repeat`. Comparison, anisotropy, border color and LOD overrides are outside version 1. |

### Validation and lowering

1. Require existing synthetic fixture admission. Reject unknown versions,
   stages, fields and enum values, excessive counts, duplicate slots, and
   unaligned or overflowing entry ranges. Check each SGPR base is divisible
   by four and each register range lies within `s0` through `s105`. Reject
   overlaps across both descriptor kinds, even when the slot IDs differ.
2. Resolve each `image_sample_lz` texture and sampler operand independently
   by exact SGPR base and descriptor kind. Reject undeclared operands and
   require the coordinate VGPR pair to be initialized and the destination
   span to be legal for the stage. Bind each resolved slot to the matching
   Metal fragment texture or sampler index. Preserve the intermediate
   representation's separate texture and sampler indices. A sample can
   pair texture 0 with sampler 1 or texture 1 with sampler 0.
3. Require exactly one compatible binding for each declared slot. Validate
   handle owner, type, generation, read state and initialized range before
   constructing the native view. Limit each 2D side to 1–64 texels and the
   mip count to the complete chain for those dimensions. Check level byte
   counts and offsets with checked arithmetic using four bytes per texel.
   Sampled views may share a read-only subresource. Reject any sampled view
   that overlaps a write to the same underlying subresource in the draw.
4. Check the selected Metal device and pipeline can bind both namespaces,
   sample `RGBA8Unorm`, and create the requested same-format views and
   samplers. On capability or view-creation failure, return a contextual
   error before encoding the draw. Retain the resolved views, samplers and
   backing resources through GPU completion. Retiring a handle removes it
   from future lookup immediately. Reusing an object ID requires advancing
   its generation without wraparound. A stale recorded handle fails even
   if the same slot now holds a new object. Submission acquires the backing
   resources while checking generations so retirement cannot race lookup.

Validation errors identify the entry and offending instruction byte offset
or slot as applicable. A rejected draw preserves its target and submits no
GPU commands for that draw. Bindings and the render target use the validated
snapshot through completion. [Metal texture views][metal-views]. [Metal
Shading Language, sampler state and texture sampling][msl].

### Initial image instruction subset

The [two-texture fixture](../../tests/compat_fixtures/shader-bindings/README.md)
needs one MIMG form, `IMAGE_SAMPLE_LZ` (opcode 39). Use the current contiguous
pair of FP32 coordinate registers and the four-component FP32 result,
`dim:SQ_RSRC_IMG_2D`, `dmask:0xf`, and zero for the optional modifiers. The
eight-register resource base and four-register sampler base select entries
in the validated manifest. Use `sample(..., level(0.0f))` on each binding's
view, so selecting a different base mip requires no new shader opcode.
Give `IMAGE_SAMPLE_L`, `IMAGE_SAMPLE_B`, `IMAGE_SAMPLE_D`, and `IMAGE_GATHER4`
separate contracts for operand ordering, LOD, bias, gradients or gather
components and their own expected results. Report them as unsupported in
version 1. [AMD RDNA 2 ISA, §12.16 and §13.7.1][rdna2].

## Evidence and open questions

The published RDNA 2 encodings and desktop D3D12 binding model establish
the basis for an original, bounded fixture. The following console details
remain research blockers until public platform documentation or authorized
measurements establish them.

| Unknown | Evidence needed |
| --- | --- |
| Shader container, metadata and entry identity | Public documentation for the console container and how an entry is selected, including the format of metadata associated with it. |
| Stage initialization | Public console ABI for initial scalar and vector register assignments, wave state, interpolation inputs and system values per stage. |
| Hardware descriptors and resource addresses | Public console ABI for descriptor layout, root-signature encoding, address translation, subresource layout and applicable limits. |
| Heap visibility and lifetime | Public evidence for when descriptors are resolved and may be updated or recycled across console queues. |

The first implementation should verify this bounded layout in the compiler,
then resolve draw snapshots, then compare both the CPU oracle and actual
Metal output with the fixture's independent expected pixels. Runtime control
flow, fences, semantic-registry expansion and format support have their own
work under [#179](https://github.com/MacosPorters/betterxen/issues/179),
[#180](https://github.com/MacosPorters/betterxen/issues/180),
[#205](https://github.com/MacosPorters/betterxen/issues/205),
[#224](https://github.com/MacosPorters/betterxen/issues/224) and
[#260](https://github.com/MacosPorters/betterxen/issues/260).

[d3d12]: https://microsoft.github.io/DirectX-Specs/d3d/ResourceBinding.html
[rdna2]: https://gpuopen.com/download/rdna2-shader-instruction-set-architecture.pdf
[metal-views]: https://developer.apple.com/documentation/metal/mtltexture/maketextureview%28pixelformat%3Atexturetype%3Alevels%3Aslices%3A%29
[msl]: https://developer.apple.com/metal/Metal-Shading-Language-Specification.pdf

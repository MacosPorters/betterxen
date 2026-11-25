# Original two-texture binding fixture

This fixture specifies known answers for the proposed [shader binding
manifest](../../../docs/shaders/resource-entry-contract.md). It uses one
synthetic `gfx1030` pixel entry throughout. Draw variants change only the
bound views and sampler states.

## Vertex and pixel entries

Draw one triangle with clip-space positions `(-1,-1,0,1)`, `(3,-1,0,1)`,
and `(-1,3,0,1)` into a 4×4 `RGBA8Unorm` target. Use no blending or depth
test. The vertex entry passes through `POS0` and exports `PARAM0` with
`(9/8, 1/4, 0, 1)` at all three vertices. Every target pixel center is
inside the triangle and receives the same texture coordinates.

The version 1 manifest names the `gfx1030` pixel entry `two_texture_lod_zero`
at offset 0, with a byte length of 44. Fragment texture slots 0 and 1 use
`s[0:7]` and `s[16:23]`. Fragment sampler slots 0 and 1 use `s[8:11]` and
`s[24:27]`. The two image instructions deliberately cross the slot pairs.
BetterXen's translated fixture model evaluates each sample synchronously.
The pixel exports the first sample's red and green and the second sample's
blue and alpha.

```asm
v_interp_p1_f32_e32 v2, v0, attr0.x
v_interp_p2_f32_e32 v2, v1, attr0.x
v_interp_p1_f32_e32 v3, v0, attr0.y
v_interp_p2_f32_e32 v3, v1, attr0.y
image_sample_lz v[8:11], v[2:3], s[0:7], s[24:27] dmask:0xf dim:SQ_RSRC_IMG_2D
image_sample_lz v[12:15], v[2:3], s[16:23], s[8:11] dmask:0xf dim:SQ_RSRC_IMG_2D
exp mrt0, v8, v9, v14, v15 done vm
s_endpgm
```

The two MIMG instructions are each eight bytes. Their little-endian dwords
are `f09c0f08 00c00802` and `f09c0f08 00440c02`. In the second dword the
texture and sampler fields store the corresponding SGPR bases divided by
four. Check both operand pairs in the decoded stream and in the generated
Metal bindings. [AMD RDNA 2 ISA, MIMG fields and opcodes][rdna2].

## Texture data and expected pixels

Create two single-slice textures with linear RGBA8 UNORM data. At each
level write texels in row order, starting at the top left. For a texel
at integer coordinates `(x, y)`, set `n = base + x + 8*y` and write
`(n, n+10, n+20, 255)` as four bytes.

| Texture and mip | Dimensions | Base |
| --- | --- | ---: |
| A, mip 0 | 4×2 | 11 |
| A, mip 1 | 2×1 | 91 |
| B, mip 0 | 4×2 | 51 |
| A′, mip 0, created after retiring A | 4×2 | 131 |

Both samplers use normalized coordinates, nearest filtering, no mip
filter and clamp on V. The U modes are clamp-to-edge and repeat. At
`u = 9/8`, clamp picks the last column and repeat reduces `u` to `1/8`,
picking column 0 at both widths. At `v = 1/4`, both texture heights select
row 0. Each bound view exposes one mip, so LOD zero addresses the selected
view's base. Compare all 16 pixels in each successful case with the table.
Derive these expectations from texel coordinates and input bytes for both
CPU and Metal checks. [Metal sampler address modes][metal-address].
[Metal texture views][metal-views].

| Draw | Texture slot 0 | Texture slot 1 | Sampler 0 U | Sampler 1 U | Expected RGBA8 |
| --- | --- | --- | --- | --- | --- |
| Baseline | A mip 0 | B mip 0 | Clamp | Repeat | `(11,21,74,255)` |
| Swap textures | B mip 0 | A mip 0 | Clamp | Repeat | `(51,61,34,255)` |
| Swap samplers | A mip 0 | B mip 0 | Repeat | Clamp | `(14,24,71,255)` |
| Change sampler 1 only | A mip 0 | B mip 0 | Clamp | Clamp | `(14,24,74,255)` |
| Select A's second mip | A mip 1 | B mip 0 | Clamp | Repeat | `(91,101,74,255)` |
| Reuse the same read-only view | B mip 0 | B mip 0 | Clamp | Repeat | `(51,61,74,255)` |
| Use the live replacement handle | A′ mip 0 | B mip 0 | Clamp | Repeat | `(131,141,74,255)` |

## Rejection and completion controls

- First complete a baseline draw. Record another draw using A, then retire
  A before submitting that draw. Reuse A's object ID for A′ with a new
  generation. The recorded draw fails as stale before encoding, while a
  fresh draw with A′ produces the replacement row above.
- Reject an undeclared sampler SGPR base in the code, overlapping descriptor
  SGPR ranges and duplicate slots at layout validation. Identify the
  offending instruction offset or binding in the error.
- Reject A mip 2, a sampler outside the admitted modes, and a missing view
  before encoding. Reject a sampled view whose subresource is also the
  render target. Use a sentinel target and a zero-submission assertion for
  each failure. A missing native capability has a distinct error.
- After implementation, compare the software and native paths separately
  with the known answers. Verify on an ARM64 host with Metal API and shader
  validation enabled. Record the device, entry identity and readback.
  Make a deliberately wrong slot assignment fail the pixel comparison.

[rdna2]: https://gpuopen.com/download/rdna2-shader-instruction-set-architecture.pdf
[metal-address]: https://developer.apple.com/documentation/metal/mtlsampleraddressmode
[metal-views]: https://developer.apple.com/documentation/metal/mtltexture/maketextureview%28pixelformat%3Atexturetype%3Alevels%3Aslices%3A%29

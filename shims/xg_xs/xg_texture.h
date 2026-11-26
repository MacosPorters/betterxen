#ifndef BETTERXEN_XG_TEXTURE_H
#define BETTERXEN_XG_TEXTURE_H

/*
 * BetterXen M1 linear texture-address contract for xg_xs.dll.
 *
 * Field meanings follow the public D3D12 resource and footprint documents
 * (D3D12_RESOURCE_DIMENSION, D3D12_SUBRESOURCE_FOOTPRINT, and
 * D3D12_TEXTURE_DATA_PITCH_ALIGNMENT / D3D12_TEXTURE_DATA_PLACEMENT_ALIGNMENT).
 * Block-compressed footprint width and height are texels rounded up to the
 * 4x4 block, the same rounding D3D12 uses for a copyable footprint.
 * Format numbers are the public DXGI_FORMAT values. This is not the Xbox
 * GDK XG interface and it is not an Xbox hardware result.
 */

#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif
#ifndef NOMINMAX
#define NOMINMAX
#endif

#include <windows.h>
#include <stdint.h>
#include <dxgiformat.h>

#ifdef __cplusplus
extern "C" {
#endif

/* Public D3D12_RESOURCE_DIMENSION values used by this shim. */
#define BX_XG_DIMENSION_BUFFER 1u
#define BX_XG_DIMENSION_TEXTURE1D 2u
#define BX_XG_DIMENSION_TEXTURE2D 3u
#define BX_XG_DIMENSION_TEXTURE3D 4u

/* Public D3D12 texture copy alignments, in bytes. */
#define BX_XG_LINEAR_PITCH_ALIGNMENT 256u
#define BX_XG_LINEAR_PLACEMENT_ALIGNMENT 512u

/* Public D3D12 texture size limits used as the acceptance cap. */
#define BX_XG_MAX_TEXTURE_EDGE 16384u
#define BX_XG_MAX_ARRAY_SLICES 2048u

/*
 * Layout selectors for this shim. LINEAR is computed exactly.
 * SW_64KB_S / D / R are named RDNA 2 tile modes reserved for M3. They are
 * accepted by XGCreateTextureComputer and every layout or copy call on them
 * returns E_NOTIMPL without writing an address.
 * XGSuggestSwizzleMode never returns a tile mode. It returns LINEAR or NONE.
 */
typedef enum bx_xg_layout_mode {
    BX_XG_LAYOUT_LINEAR = 0,
    BX_XG_LAYOUT_SW_64KB_S = 1,
    BX_XG_LAYOUT_SW_64KB_D = 2,
    BX_XG_LAYOUT_SW_64KB_R = 3,
    BX_XG_LAYOUT_NONE = 0xFFFFFFFFu
} bx_xg_layout_mode;

typedef struct bx_xg_sample_desc {
    uint32_t count;
    uint32_t quality;
} bx_xg_sample_desc;

typedef struct bx_xg_resource_desc {
    uint32_t dimension;
    uint32_t alignment; /* 0 or BX_XG_LINEAR_PLACEMENT_ALIGNMENT */
    uint64_t width;     /* texels */
    uint32_t height;    /* texels */
    uint16_t depth_or_array_size;
    uint16_t mip_levels; /* 0 selects the full chain, down to 1x1 */
    uint32_t format;     /* DXGI_FORMAT */
    bx_xg_sample_desc sample_desc;
    uint32_t layout; /* bx_xg_layout_mode */
    uint32_t flags;  /* must be 0 */
} bx_xg_resource_desc;

typedef struct bx_xg_subresource_footprint {
    uint32_t format;
    uint32_t width;  /* texels, rounded up to the format block width */
    uint32_t height; /* texels, rounded up to the format block height */
    uint32_t depth;
    uint32_t row_pitch; /* bytes, multiple of BX_XG_LINEAR_PITCH_ALIGNMENT */
} bx_xg_subresource_footprint;

typedef struct bx_xg_placed_footprint {
    uint64_t offset;
    bx_xg_subresource_footprint footprint;
    /*
     * Absolute byte just past this subresource's last payload byte:
     * offset + (row_count - 1) * row_pitch + row_bytes.
     * That is the D3D12 GetCopyableFootprints end. It does not include the
     * last row's pitch padding or the 512-byte tail. The copy span of a
     * GetResourceLayout call is layouts[last].copy_end - base_offset.
     */
    uint64_t copy_end;
} bx_xg_placed_footprint;

/* {B7E4C8A1-6F2D-4C55-8A19-3D0E5F7B91C4} BetterXen IXGTextureAddressComputer */
static const GUID BX_IID_IXGTextureAddressComputer = {
    0xB7E4C8A1u, 0x6F2Du, 0x4C55u, {0x8Au, 0x19u, 0x3Du, 0x0Eu, 0x5Fu, 0x7Bu, 0x91u, 0xC4u}};

/* Public COM IUnknown IID. */
static const GUID BX_IID_IUnknown = {
    0x00000000u, 0x0000u, 0x0000u, {0xC0u, 0x00u, 0x00u, 0x00u, 0x00u, 0x00u, 0x00u, 0x46u}};

/*
 * GetResourceLayout places every subresource at its absolute offset from
 * base_offset, including subresources before first_subresource. total_bytes
 * is the BetterXen allocation span: the 512-aligned end of the last requested
 * subresource, measured from base_offset, so a partial range still covers the
 * earlier subresources and a 1x1 RGBA8 texture reports 512. copy_end on each
 * footprint is the separate D3D12-style copy span end described above.
 */
typedef struct bx_xg_texture_computer bx_xg_texture_computer;

typedef struct bx_xg_texture_computer_vtbl {
    HRESULT(WINAPI* QueryInterface)(bx_xg_texture_computer* self, const GUID* riid, void** object);
    ULONG(WINAPI* AddRef)(bx_xg_texture_computer* self);
    ULONG(WINAPI* Release)(bx_xg_texture_computer* self);
    HRESULT(WINAPI* GetResourceLayout)(
        bx_xg_texture_computer* self,
        uint32_t first_subresource,
        uint32_t subresource_count,
        uint64_t base_offset,
        bx_xg_placed_footprint* layouts,
        uint32_t* row_counts,
        uint64_t* row_sizes_in_bytes,
        uint64_t* total_bytes);
    HRESULT(WINAPI* GetResourceSizeBytes)(bx_xg_texture_computer* self, uint64_t* size_bytes);
    HRESULT(WINAPI* GetResourceBaseAlignmentBytes)(bx_xg_texture_computer* self, uint64_t* alignment_bytes);
    HRESULT(WINAPI* GetMipLevelOffsetBytes)(
        bx_xg_texture_computer* self,
        uint32_t array_slice,
        uint32_t mip_level,
        uint64_t* offset_bytes);
    HRESULT(WINAPI* GetTexelElementOffsetBytes)(
        bx_xg_texture_computer* self,
        uint32_t array_slice,
        uint32_t mip_level,
        uint32_t x,
        uint32_t y,
        uint32_t z,
        uint64_t* offset_bytes);
    HRESULT(WINAPI* CopyIntoSubresource)(
        bx_xg_texture_computer* self,
        void* destination,
        uint64_t destination_size,
        uint32_t subresource,
        const void* source,
        uint64_t source_size,
        uint32_t source_row_pitch,
        uint32_t source_depth_pitch);
    HRESULT(WINAPI* CopyFromSubresource)(
        bx_xg_texture_computer* self,
        void* destination,
        uint64_t destination_size,
        uint32_t destination_row_pitch,
        uint32_t destination_depth_pitch,
        const void* source,
        uint64_t source_size,
        uint32_t subresource);
} bx_xg_texture_computer_vtbl;

struct bx_xg_texture_computer {
    const bx_xg_texture_computer_vtbl* lpVtbl;
};

/*
 * On success, *computer is an IXGTextureAddressComputer with a reference
 * count of 1. On failure, *computer is NULL when computer itself is non-NULL.
 * Subresource index is mip_level + array_slice * mip_levels.
 * A row pitch of 0 on a copy selects the tight logical row size.
 */
HRESULT WINAPI XGCreateTextureComputer(const bx_xg_resource_desc* desc, void** computer);

/*
 * Returns BX_XG_LAYOUT_LINEAR when this shim can lay the texture out, and
 * BX_XG_LAYOUT_NONE otherwise. The same arguments always return the same mode.
 */
uint32_t WINAPI XGSuggestSwizzleMode(
    uint32_t resource_type,
    uint32_t format,
    uint32_t width,
    uint32_t height,
    uint32_t depth,
    uint32_t mip_levels,
    uint32_t sample_count,
    uint32_t flags);

#ifdef __cplusplus
}
#endif

#endif /* BETTERXEN_XG_TEXTURE_H */

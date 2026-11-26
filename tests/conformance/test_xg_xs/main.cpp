#include <windows.h>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>

#include "xg_texture.h"

/*
 * Hand-checked layouts. A row is align_up(block_columns * bytes_per_block, 256).
 * The next subresource starts at align_up(offset + row_pitch * block_rows, 512).
 * These constants are not computed by xg_xs.dll.
 */

static int g_failures = 0;
static int g_checks = 0;

static void check(bool ok, const char* msg) {
    g_checks += 1;
    if (!ok) {
        std::fprintf(stderr, "FAIL: %s\n", msg);
        g_failures += 1;
    }
}

static void check_hr(const char* name, HRESULT hr, HRESULT want) {
    g_checks += 1;
    if (hr != want) {
        std::fprintf(stderr, "FAIL: %s hr=0x%08lx want=0x%08lx\n",
                     name,
                     static_cast<unsigned long>(static_cast<uint32_t>(hr)),
                     static_cast<unsigned long>(static_cast<uint32_t>(want)));
        g_failures += 1;
    }
}

static bx_xg_resource_desc desc_2d(
    uint32_t format,
    uint32_t width,
    uint32_t height,
    uint16_t arrays,
    uint16_t mips,
    uint32_t layout
) {
    bx_xg_resource_desc desc{};
    desc.dimension = BX_XG_DIMENSION_TEXTURE2D;
    desc.width = width;
    desc.height = height;
    desc.depth_or_array_size = arrays;
    desc.mip_levels = mips;
    desc.format = format;
    desc.sample_desc.count = 1u;
    desc.sample_desc.quality = 0u;
    desc.layout = layout;
    return desc;
}

static bx_xg_texture_computer* create_ok(const bx_xg_resource_desc* desc, const char* name) {
    void* object = nullptr;
    const HRESULT hr = XGCreateTextureComputer(desc, &object);
    check_hr(name, hr, S_OK);
    check(object != nullptr, name);
    return static_cast<bx_xg_texture_computer*>(object);
}

static void release_computer(bx_xg_texture_computer* computer) {
    if (computer) {
        computer->lpVtbl->Release(computer);
    }
}

static bool bytes_are(const uint8_t* data, size_t size, uint8_t value) {
    for (size_t i = 0; i < size; ++i) {
        if (data[i] != value) {
            return false;
        }
    }
    return true;
}

static void test_arguments() {
    void* object = reinterpret_cast<void*>(static_cast<uintptr_t>(1));
    check_hr("null desc", XGCreateTextureComputer(nullptr, &object), E_POINTER);
    check(object == nullptr, "null desc clears the output");

    bx_xg_resource_desc desc = desc_2d(DXGI_FORMAT_R8G8B8A8_UNORM, 4, 4, 1, 1, BX_XG_LAYOUT_LINEAR);
    check_hr("null computer", XGCreateTextureComputer(&desc, nullptr), E_POINTER);

    desc.format = 1u;
    object = reinterpret_cast<void*>(static_cast<uintptr_t>(1));
    check_hr("unknown format", XGCreateTextureComputer(&desc, &object), E_INVALIDARG);
    check(object == nullptr, "unknown format clears the output");

    desc = desc_2d(DXGI_FORMAT_R8G8B8A8_UNORM, 0, 4, 1, 1, BX_XG_LAYOUT_LINEAR);
    check_hr("zero width", XGCreateTextureComputer(&desc, &object), E_INVALIDARG);

    desc = desc_2d(DXGI_FORMAT_R8G8B8A8_UNORM, 16385, 1, 1, 1, BX_XG_LAYOUT_LINEAR);
    check_hr("width above the cap", XGCreateTextureComputer(&desc, &object), E_INVALIDARG);

    desc = desc_2d(DXGI_FORMAT_R8G8B8A8_UNORM, 4, 4, 2049, 1, BX_XG_LAYOUT_LINEAR);
    check_hr("array above the cap", XGCreateTextureComputer(&desc, &object), E_INVALIDARG);

    desc = desc_2d(DXGI_FORMAT_R8G8B8A8_UNORM, 8, 8, 1, 5, BX_XG_LAYOUT_LINEAR);
    check_hr("too many mips", XGCreateTextureComputer(&desc, &object), E_INVALIDARG);

    desc = desc_2d(DXGI_FORMAT_R8G8B8A8_UNORM, 4, 4, 1, 1, BX_XG_LAYOUT_LINEAR);
    desc.dimension = BX_XG_DIMENSION_TEXTURE3D;
    check_hr("texture3d", XGCreateTextureComputer(&desc, &object), E_NOTIMPL);

    desc.dimension = 0u;
    check_hr("unknown dimension", XGCreateTextureComputer(&desc, &object), E_INVALIDARG);

    desc = desc_2d(DXGI_FORMAT_R8G8B8A8_UNORM, 4, 4, 1, 1, BX_XG_LAYOUT_LINEAR);
    desc.sample_desc.count = 4u;
    check_hr("msaa", XGCreateTextureComputer(&desc, &object), E_NOTIMPL);

    desc = desc_2d(DXGI_FORMAT_R8G8B8A8_UNORM, 4, 4, 1, 1, 9u);
    check_hr("unknown layout", XGCreateTextureComputer(&desc, &object), E_INVALIDARG);

    const uint32_t linear = XGSuggestSwizzleMode(BX_XG_DIMENSION_TEXTURE2D, DXGI_FORMAT_R8G8B8A8_UNORM, 64, 32, 1, 0, 1, 0);
    const uint32_t linear_again = XGSuggestSwizzleMode(BX_XG_DIMENSION_TEXTURE2D, DXGI_FORMAT_R8G8B8A8_UNORM, 64, 32, 1, 0, 1, 0);
    check(linear == BX_XG_LAYOUT_LINEAR, "suggest linear rgba8");
    check(linear_again == linear, "suggest is stable");
    check(XGSuggestSwizzleMode(BX_XG_DIMENSION_TEXTURE2D, DXGI_FORMAT_BC7_UNORM, 32, 32, 1, 0, 1, 0) == BX_XG_LAYOUT_LINEAR,
          "suggest linear bc7");
    check(XGSuggestSwizzleMode(BX_XG_DIMENSION_TEXTURE2D, 1u, 64, 32, 1, 0, 1, 0) == BX_XG_LAYOUT_NONE,
          "suggest rejects an unknown format");
    check(XGSuggestSwizzleMode(BX_XG_DIMENSION_TEXTURE2D, DXGI_FORMAT_R8G8B8A8_UNORM, 0, 32, 1, 0, 1, 0) == BX_XG_LAYOUT_NONE,
          "suggest rejects zero width");
    check(XGSuggestSwizzleMode(BX_XG_DIMENSION_TEXTURE3D, DXGI_FORMAT_R8G8B8A8_UNORM, 64, 32, 1, 0, 1, 0) == BX_XG_LAYOUT_NONE,
          "suggest does not invent a 3D mode");
    check(XGSuggestSwizzleMode(BX_XG_DIMENSION_TEXTURE2D, DXGI_FORMAT_BC1_UNORM, 8, 8, 1, 0, 1, 1) == BX_XG_LAYOUT_NONE,
          "suggest rejects flags");
}

static void test_hand_layouts() {
    bx_xg_placed_footprint layouts[8]{};
    uint32_t rows[8]{};
    uint64_t row_bytes[8]{};
    uint64_t total = 0;
    uint64_t size = 0;
    uint64_t align = 0;
    uint64_t offset = 0;

    bx_xg_resource_desc desc = desc_2d(DXGI_FORMAT_R8G8B8A8_UNORM, 8, 8, 1, 0, BX_XG_LAYOUT_LINEAR);
    bx_xg_texture_computer* computer = create_ok(&desc, "create rgba 8x8");
    if (!computer) {
        return;
    }
    check_hr("rgba full layout",
             computer->lpVtbl->GetResourceLayout(computer, 0, 4, 0, layouts, rows, row_bytes, &total),
             S_OK);
    check_hr("rgba size", computer->lpVtbl->GetResourceSizeBytes(computer, &size), S_OK);
    check_hr("rgba align", computer->lpVtbl->GetResourceBaseAlignmentBytes(computer, &align), S_OK);
    check(size == total && total == 4096u, "rgba 8x8 chain is 4096 bytes");
    check(align == 512u, "linear base alignment is 512");
    /* mip0 8x8: 32 -> 256 pitch, 8 rows, 2048 payload, next 2048. */
    check(layouts[0].offset == 0u && rows[0] == 8u && row_bytes[0] == 32u && layouts[0].footprint.row_pitch == 256u &&
              layouts[0].footprint.width == 8u,
          "rgba mip0");
    /* mip1 4x4: 16 -> 256, 4 rows, offset 2048. */
    check(layouts[1].offset == 2048u && rows[1] == 4u && row_bytes[1] == 16u && layouts[1].footprint.width == 4u,
          "rgba mip1");
    /* mip2 2x2: 8 -> 256, 2 rows, offset 3072. */
    check(layouts[2].offset == 3072u && rows[2] == 2u && row_bytes[2] == 8u && layouts[2].footprint.width == 2u,
          "rgba mip2");
    /* mip3 1x1: 4 -> 256, 1 row, offset 3584. total align(3840, 512) = 4096. */
    check(layouts[3].offset == 3584u && rows[3] == 1u && row_bytes[3] == 4u && layouts[3].footprint.width == 1u,
          "rgba mip3");
    /* D3D12 copy end keeps the last row's payload and drops the 512-byte tail. */
    check(layouts[0].copy_end == 1824u && layouts[3].copy_end == 3588u, "rgba chain copy ends");
    check_hr("rgba mip1 offset", computer->lpVtbl->GetMipLevelOffsetBytes(computer, 0, 1, &offset), S_OK);
    check(offset == 2048u, "mip helper matches the footprint");
    check_hr("rgba texel", computer->lpVtbl->GetTexelElementOffsetBytes(computer, 0, 1, 3, 1, 0, &offset), S_OK);
    check(offset == 2316u, "rgba mip1 texel (3,1) is 2316");
    uint64_t sentinel = 0x1122334455667788ull;
    check_hr("texel x out of range",
             computer->lpVtbl->GetTexelElementOffsetBytes(computer, 0, 0, 8, 0, 0, &sentinel),
             E_INVALIDARG);
    check(sentinel == 0x1122334455667788ull, "rejected texel leaves the output unchanged");
    check_hr("texel z", computer->lpVtbl->GetTexelElementOffsetBytes(computer, 0, 0, 0, 0, 1, &sentinel), E_INVALIDARG);
    check_hr("mip out of range", computer->lpVtbl->GetMipLevelOffsetBytes(computer, 0, 4, &sentinel), E_INVALIDARG);
    release_computer(computer);

    desc = desc_2d(DXGI_FORMAT_BC1_UNORM, 8, 8, 1, 2, BX_XG_LAYOUT_LINEAR);
    computer = create_ok(&desc, "create bc1 8x8");
    if (!computer) {
        return;
    }
    check_hr("bc1 layout", computer->lpVtbl->GetResourceLayout(computer, 0, 2, 0, layouts, rows, row_bytes, &total), S_OK);
    /* 2x2 blocks of 8 bytes -> row 16, pitch 256, 2 rows, payload 512. mip1 one 8-byte block at 512. */
    check(layouts[0].offset == 0u && rows[0] == 2u && row_bytes[0] == 16u && total == 1024u, "bc1 mip0");
    check(layouts[1].offset == 512u && rows[1] == 1u && row_bytes[1] == 8u && layouts[1].footprint.width == 4u, "bc1 mip1");
    check(layouts[0].copy_end == 272u && layouts[1].copy_end == 520u, "bc1 copy ends");
    check_hr("bc1 texel", computer->lpVtbl->GetTexelElementOffsetBytes(computer, 0, 0, 4, 0, 0, &offset), S_OK);
    check(offset == 8u, "bc1 texel x=4 is the second block");
    check_hr("bc1 texel corner", computer->lpVtbl->GetTexelElementOffsetBytes(computer, 0, 0, 7, 7, 0, &offset), S_OK);
    check(offset == 264u, "bc1 texel (7,7) is block (1,1)");
    release_computer(computer);

    desc = desc_2d(DXGI_FORMAT_BC7_UNORM, 4, 4, 1, 1, BX_XG_LAYOUT_LINEAR);
    computer = create_ok(&desc, "create bc7 4x4");
    if (!computer) {
        return;
    }
    check_hr("bc7 layout", computer->lpVtbl->GetResourceLayout(computer, 0, 1, 0, layouts, rows, row_bytes, &total), S_OK);
    check(row_bytes[0] == 16u && rows[0] == 1u && layouts[0].footprint.row_pitch == 256u && total == 512u, "bc7 4x4");
    release_computer(computer);

    desc = desc_2d(DXGI_FORMAT_BC1_UNORM, 4, 4, 1, 1, BX_XG_LAYOUT_LINEAR);
    computer = create_ok(&desc, "create bc1 4x4");
    if (!computer) {
        return;
    }
    check_hr("bc1 4x4 layout", computer->lpVtbl->GetResourceLayout(computer, 0, 1, 0, layouts, rows, row_bytes, &total), S_OK);
    check(row_bytes[0] == 8u && total == 512u && layouts[0].copy_end == 8u, "bc1 4x4 row is 8 bytes");
    release_computer(computer);

    desc = desc_2d(DXGI_FORMAT_BC1_UNORM, 128, 4, 1, 1, BX_XG_LAYOUT_LINEAR);
    computer = create_ok(&desc, "create bc1 128");
    if (!computer) {
        return;
    }
    check_hr("bc1 128 layout", computer->lpVtbl->GetResourceLayout(computer, 0, 1, 0, layouts, rows, row_bytes, &total), S_OK);
    check(row_bytes[0] == 256u && layouts[0].footprint.row_pitch == 256u && total == 512u, "bc1 128x4 pitch");
    release_computer(computer);

    desc = desc_2d(DXGI_FORMAT_BC7_UNORM, 128, 4, 1, 1, BX_XG_LAYOUT_LINEAR);
    computer = create_ok(&desc, "create bc7 128");
    if (!computer) {
        return;
    }
    check_hr("bc7 128 layout", computer->lpVtbl->GetResourceLayout(computer, 0, 1, 0, layouts, rows, row_bytes, &total), S_OK);
    check(row_bytes[0] == 512u && layouts[0].footprint.row_pitch == 512u && total == 512u, "bc7 128x4 pitch");
    release_computer(computer);

    desc = desc_2d(DXGI_FORMAT_BC1_UNORM, 5, 3, 1, 1, BX_XG_LAYOUT_LINEAR);
    computer = create_ok(&desc, "create bc1 partial");
    if (!computer) {
        return;
    }
    check_hr("bc1 partial layout", computer->lpVtbl->GetResourceLayout(computer, 0, 1, 0, layouts, rows, row_bytes, &total), S_OK);
    /* 5x3 texels round to an 8x4 footprint: 2 block columns, 1 block row, 16 byte row, total 512. */
    check(rows[0] == 1u && row_bytes[0] == 16u && layouts[0].footprint.width == 8u &&
              layouts[0].footprint.height == 4u && total == 512u,
          "bc1 5x3");
    check_hr("bc1 5x3 texel", computer->lpVtbl->GetTexelElementOffsetBytes(computer, 0, 0, 4, 2, 0, &offset), S_OK);
    check(offset == 8u, "bc1 5x3 texel (4,2) stays inside the logical 5x3");
    offset = 0x99u;
    check_hr("bc1 past logical width",
             computer->lpVtbl->GetTexelElementOffsetBytes(computer, 0, 0, 5, 0, 0, &offset),
             E_INVALIDARG);
    check(offset == 0x99u, "padding texels outside 5x3 are not addressable");
    release_computer(computer);

    desc = desc_2d(DXGI_FORMAT_BC7_UNORM, 6, 5, 1, 1, BX_XG_LAYOUT_LINEAR);
    computer = create_ok(&desc, "create bc7 6x5");
    if (!computer) {
        return;
    }
    check_hr("bc7 6x5 layout", computer->lpVtbl->GetResourceLayout(computer, 0, 1, 0, layouts, rows, row_bytes, &total), S_OK);
    check(layouts[0].footprint.width == 8u && layouts[0].footprint.height == 8u && rows[0] == 2u && row_bytes[0] == 32u,
          "bc7 6x5 rounds to 8x8");
    release_computer(computer);

    desc = desc_2d(DXGI_FORMAT_R8G8B8A8_UNORM, 4, 4, 2, 1, BX_XG_LAYOUT_LINEAR);
    computer = create_ok(&desc, "create array");
    if (!computer) {
        return;
    }
    check_hr("array layout", computer->lpVtbl->GetResourceLayout(computer, 0, 2, 0, layouts, rows, row_bytes, &total), S_OK);
    check(layouts[0].offset == 0u && layouts[1].offset == 1024u && total == 2048u, "array offsets");
    check_hr("array mip offset", computer->lpVtbl->GetMipLevelOffsetBytes(computer, 1, 0, &offset), S_OK);
    check(offset == 1024u, "array slice 1 offset");
    check_hr("array texel", computer->lpVtbl->GetTexelElementOffsetBytes(computer, 1, 0, 0, 1, 0, &offset), S_OK);
    check(offset == 1280u, "array slice 1 texel y=1");
    check_hr("partial range", computer->lpVtbl->GetResourceLayout(computer, 1, 1, 0, layouts, rows, row_bytes, &total), S_OK);
    check(layouts[0].offset == 1024u && total == 2048u, "partial range keeps the absolute offset");

    std::memset(layouts, 0x11, sizeof(layouts));
    total = 0x2222u;
    check_hr("unaligned base",
             computer->lpVtbl->GetResourceLayout(computer, 0, 1, 256, layouts, rows, row_bytes, &total),
             E_INVALIDARG);
    check(layouts[0].offset == 0x1111111111111111ull && total == 0x2222u, "failed layout does not write");
    check_hr("shifted base", computer->lpVtbl->GetResourceLayout(computer, 0, 1, 512, layouts, rows, row_bytes, &total), S_OK);
    check(layouts[0].offset == 512u && total == 1024u, "base offset 512 shifts the footprint");
    release_computer(computer);

    const uint32_t rgba_formats[] = {
        DXGI_FORMAT_R8G8B8A8_TYPELESS,
        DXGI_FORMAT_R8G8B8A8_UNORM,
        DXGI_FORMAT_R8G8B8A8_UNORM_SRGB,
        DXGI_FORMAT_R8G8B8A8_UINT,
        DXGI_FORMAT_R8G8B8A8_SNORM,
        DXGI_FORMAT_R8G8B8A8_SINT,
    };
    for (uint32_t format : rgba_formats) {
        desc = desc_2d(format, 1, 1, 1, 1, BX_XG_LAYOUT_LINEAR);
        computer = create_ok(&desc, "rgba variant");
        if (!computer) {
            return;
        }
        check_hr("rgba variant layout",
                 computer->lpVtbl->GetResourceLayout(computer, 0, 1, 0, layouts, rows, row_bytes, &total),
                 S_OK);
        check(row_bytes[0] == 4u && total == 512u && layouts[0].copy_end == 4u && layouts[0].footprint.format == format,
              "rgba variant size");
        release_computer(computer);
    }

    desc = desc_2d(DXGI_FORMAT_R8G8B8A8_UNORM, 16384, 1, 1, 1, BX_XG_LAYOUT_LINEAR);
    computer = create_ok(&desc, "create max edge");
    if (!computer) {
        return;
    }
    check_hr("max edge layout", computer->lpVtbl->GetResourceLayout(computer, 0, 1, 0, layouts, rows, row_bytes, &total), S_OK);
    check(row_bytes[0] == 65536u && layouts[0].footprint.row_pitch == 65536u && total == 65536u, "16384 rgba row");
    release_computer(computer);

    desc = desc_2d(DXGI_FORMAT_R8G8B8A8_UNORM, 1, 1, 2048, 1, BX_XG_LAYOUT_LINEAR);
    computer = create_ok(&desc, "create max array");
    if (!computer) {
        return;
    }
    check_hr("max array size", computer->lpVtbl->GetResourceSizeBytes(computer, &size), S_OK);
    check(size == 1048576u, "2048 slices of 512 bytes");
    release_computer(computer);
}

static void test_monotonic_chain(uint32_t format, uint32_t width, uint32_t height, const char* name) {
    bx_xg_resource_desc desc = desc_2d(format, width, height, 1, 0, BX_XG_LAYOUT_LINEAR);
    bx_xg_texture_computer* computer = create_ok(&desc, name);
    if (!computer) {
        return;
    }
    bx_xg_placed_footprint layouts[16]{};
    uint32_t rows[16]{};
    uint64_t row_bytes[16]{};
    uint64_t total = 0;
    uint32_t count = 1u;
    while (count < 16u) {
        const HRESULT hr = computer->lpVtbl->GetMipLevelOffsetBytes(computer, 0, count, &total);
        if (hr != S_OK) {
            break;
        }
        count += 1u;
    }
    check(count >= 1u && count < 16u, "mip chain fits the stack buffer");
    check_hr(name, computer->lpVtbl->GetResourceLayout(computer, 0, count, 0, layouts, rows, row_bytes, &total), S_OK);
    uint64_t reported = 0;
    check_hr(name, computer->lpVtbl->GetResourceSizeBytes(computer, &reported), S_OK);
    check(reported == total && (total % 512u) == 0u, "size matches the placed chain");
    for (uint32_t i = 0; i < count; ++i) {
        check((layouts[i].offset % 512u) == 0u, "subresource offset is placement aligned");
        check((layouts[i].footprint.row_pitch % 256u) == 0u, "row pitch is 256 aligned");
        check(layouts[i].footprint.row_pitch >= row_bytes[i], "pitch covers the logical row");
        check(layouts[i].footprint.width >= 1u && layouts[i].footprint.height >= 1u, "mip is at least one texel");
        check(rows[i] >= 1u, "mip has a row");
        if (i > 0u) {
            check(layouts[i].offset > layouts[i - 1u].offset, "subresource offsets increase");
        }
    }
    check(total > layouts[count - 1u].offset, "total extends past the last offset");
    release_computer(computer);
}

static uint8_t* alloc_bytes(size_t size, uint8_t fill) {
    uint8_t* data = static_cast<uint8_t*>(std::malloc(size));
    if (!data) {
        check(false, "malloc");
        return nullptr;
    }
    std::memset(data, fill, size);
    return data;
}

static void test_round_trip(
    uint32_t format,
    uint32_t width,
    uint32_t height,
    uint32_t subresource,
    size_t logical_row,
    size_t logical_rows,
    size_t resource_bytes,
    uint64_t placed_row,
    const char* name
) {
    bx_xg_resource_desc desc = desc_2d(format, width, height, 1, 0, BX_XG_LAYOUT_LINEAR);
    if (subresource > 0u) {
        desc.mip_levels = static_cast<uint16_t>(subresource + 1u);
    }
    bx_xg_texture_computer* computer = create_ok(&desc, name);
    if (!computer) {
        return;
    }

    const size_t logical_bytes = logical_row * logical_rows;
    const size_t pitched_row = logical_row + 8u;
    const size_t pitched_bytes = pitched_row * logical_rows;
    uint8_t* logical = alloc_bytes(logical_bytes, 0);
    uint8_t* pitched = alloc_bytes(pitched_bytes, 0xFF);
    uint8_t* resource = alloc_bytes(resource_bytes + 32u, 0);
    uint8_t* readback = alloc_bytes(logical_bytes + 32u, 0xA5);
    if (!logical || !pitched || !resource || !readback) {
        std::free(logical);
        std::free(pitched);
        std::free(resource);
        std::free(readback);
        release_computer(computer);
        return;
    }
    std::memset(resource + resource_bytes, 0xA5, 32u);
    for (size_t i = 0; i < logical_bytes; ++i) {
        logical[i] = static_cast<uint8_t>((i * 13u + 7u) & 0xFFu);
    }
    for (size_t y = 0; y < logical_rows; ++y) {
        std::memcpy(pitched + y * pitched_row, logical + y * logical_row, logical_row);
    }

    check_hr(name,
             computer->lpVtbl->CopyIntoSubresource(
                 computer, resource, resource_bytes, subresource, pitched, pitched_bytes, static_cast<uint32_t>(pitched_row), 0),
             S_OK);
    check(bytes_are(resource + resource_bytes, 32u, 0xA5), "copy leaves the canary");
    check(resource[0] == (subresource == 0u ? logical[0] : 0u), "earlier bytes stay put when the mip is not zero");
    if (subresource == 0u && logical_rows > 1u) {
        check(resource[placed_row] == logical[logical_row], "second row uses the placed pitch");
        check(resource[logical_row] == 0u, "padding past the logical row stays zero");
    }

    std::memset(readback, 0xA5, logical_bytes + 32u);
    check_hr(name,
             computer->lpVtbl->CopyFromSubresource(
                 computer, readback, logical_bytes, 0, 0, resource, resource_bytes, subresource),
             S_OK);
    check(std::memcmp(readback, logical, logical_bytes) == 0, "round trip is byte identical");
    check(bytes_are(readback + logical_bytes, 32u, 0xA5), "readback leaves its canary");

    std::memset(resource, 0x5A, resource_bytes);
    check_hr("short destination",
             computer->lpVtbl->CopyIntoSubresource(
                 computer, resource, 1, subresource, logical, logical_bytes, 0, 0),
             E_INVALIDARG);
    check(bytes_are(resource, resource_bytes, 0x5A), "rejected copy writes nothing");

    check_hr("null copy source",
             computer->lpVtbl->CopyIntoSubresource(computer, resource, resource_bytes, subresource, nullptr, logical_bytes, 0, 0),
             E_POINTER);
    check(bytes_are(resource, resource_bytes, 0x5A), "null source writes nothing");

    std::free(logical);
    std::free(pitched);
    std::free(resource);
    std::free(readback);
    release_computer(computer);
}

static void test_copies() {
    /* RGBA8 6x3: logical row 24, placed pitch 256, 3 rows, resource 1024. */
    test_round_trip(DXGI_FORMAT_R8G8B8A8_UNORM, 6, 3, 0, 24, 3, 1024, 256, "rgba round trip");
    /* BC7 6x5: 2 x 2 blocks, logical row 32, placed pitch 256, resource 512. */
    test_round_trip(DXGI_FORMAT_BC7_UNORM, 6, 5, 0, 32, 2, 512, 256, "bc7 round trip");
    /* BC1 5x3: one 16-byte block row, resource 512. */
    test_round_trip(DXGI_FORMAT_BC1_UNORM, 5, 3, 0, 16, 1, 512, 256, "bc1 round trip");
    /* BC1 8x8 mip1 is a single 8-byte block at offset 512. Resource total is 1024. */
    test_round_trip(DXGI_FORMAT_BC1_UNORM, 8, 8, 1, 8, 1, 1024, 256, "bc1 mip1 round trip");
}

static void test_swizzle_placeholder() {
    const uint32_t modes[] = {BX_XG_LAYOUT_SW_64KB_S, BX_XG_LAYOUT_SW_64KB_D, BX_XG_LAYOUT_SW_64KB_R};
    for (uint32_t mode : modes) {
        bx_xg_resource_desc desc = desc_2d(DXGI_FORMAT_BC7_UNORM, 32, 32, 1, 0, mode);
        void* object = nullptr;
        check_hr("create tiled", XGCreateTextureComputer(&desc, &object), S_OK);
        auto* computer = static_cast<bx_xg_texture_computer*>(object);
        if (!computer) {
            continue;
        }
        bx_xg_placed_footprint layout{};
        std::memset(&layout, 0xEE, sizeof(layout));
        uint32_t rows = 0xEEEEEEEEu;
        uint64_t row_bytes = 0xEEEEEEEEEEEEEEEEull;
        uint64_t total = 0xEEEEEEEEEEEEEEEEull;
        check_hr("tiled layout",
                 computer->lpVtbl->GetResourceLayout(computer, 0, 1, 0, &layout, &rows, &row_bytes, &total),
                 E_NOTIMPL);
        check(layout.offset == 0xEEEEEEEEEEEEEEEEull && rows == 0xEEEEEEEEu && total == 0xEEEEEEEEEEEEEEEEull,
              "tiled layout writes nothing");

        uint64_t size = 0x0123456789ABCDEFull;
        check_hr("tiled size", computer->lpVtbl->GetResourceSizeBytes(computer, &size), E_NOTIMPL);
        check_hr("tiled align", computer->lpVtbl->GetResourceBaseAlignmentBytes(computer, &size), E_NOTIMPL);
        check_hr("tiled mip", computer->lpVtbl->GetMipLevelOffsetBytes(computer, 0, 0, &size), E_NOTIMPL);
        check_hr("tiled texel", computer->lpVtbl->GetTexelElementOffsetBytes(computer, 0, 0, 0, 0, 0, &size), E_NOTIMPL);
        check(size == 0x0123456789ABCDEFull, "tiled queries write nothing");

        uint8_t dest[16];
        uint8_t src[16];
        std::memset(dest, 0x3C, sizeof(dest));
        std::memset(src, 0x5A, sizeof(src));
        check_hr("tiled copy",
                 computer->lpVtbl->CopyIntoSubresource(computer, dest, sizeof(dest), 0, src, sizeof(src), 0, 0),
                 E_NOTIMPL);
        check(bytes_are(dest, sizeof(dest), 0x3C), "tiled copy writes nothing");
        std::memset(dest, 0x3C, sizeof(dest));
        check_hr("tiled copy from",
                 computer->lpVtbl->CopyFromSubresource(computer, dest, sizeof(dest), 0, 0, src, sizeof(src), 0),
                 E_NOTIMPL);
        check(bytes_are(dest, sizeof(dest), 0x3C), "tiled copy from writes nothing");
        check(bytes_are(src, sizeof(src), 0x5A), "tiled copy from leaves the source");
        release_computer(computer);
    }
    check(XGSuggestSwizzleMode(BX_XG_DIMENSION_TEXTURE2D, DXGI_FORMAT_BC7_UNORM, 32, 32, 1, 0, 1, 0) == BX_XG_LAYOUT_LINEAR,
          "suggest stays on the implemented mode");
}

static bool row_is(const uint8_t* row, uint8_t value) {
    return row[0] == value && row[1] == value && row[2] == value && row[3] == value;
}

static void paint_row(uint8_t* row, uint8_t value) {
    row[0] = value;
    row[1] = value;
    row[2] = value;
    row[3] = value;
}

static void test_overlapping_copies() {
    bx_xg_resource_desc desc = desc_2d(DXGI_FORMAT_R8G8B8A8_UNORM, 1, 3, 1, 1, BX_XG_LAYOUT_LINEAR);
    bx_xg_texture_computer* computer = create_ok(&desc, "create overlap");
    if (!computer) {
        return;
    }
    uint8_t* buf = alloc_bytes(1024, 0);
    if (!buf) {
        release_computer(computer);
        return;
    }
    paint_row(buf + 0, 1);
    paint_row(buf + 128, 2);
    paint_row(buf + 256, 3);
    check_hr("expand pitch in place",
             computer->lpVtbl->CopyIntoSubresource(computer, buf, 1024, 0, buf, 1024, 128, 0),
             S_OK);
    check(row_is(buf + 0, 1) && row_is(buf + 256, 2) && row_is(buf + 512, 3), "expanded rows stay 1, 2, 3");

    std::memset(buf, 0, 1024);
    paint_row(buf + 0, 1);
    paint_row(buf + 256, 2);
    paint_row(buf + 512, 3);
    check_hr("contract pitch in place",
             computer->lpVtbl->CopyFromSubresource(computer, buf, 1024, 128, 0, buf, 1024, 0),
             S_OK);
    check(row_is(buf + 0, 1) && row_is(buf + 128, 2) && row_is(buf + 256, 3), "contracted rows stay 1, 2, 3");
    std::free(buf);
    release_computer(computer);
}

static void test_large_array_offsets() {
    /* 64x64 RGBA8 is 7 mips. One slice stride is 32768.
     * mip6 sits at 32256 inside the slice.
     * Slice 2047 therefore starts at 2047 * 32768 = 67076096,
     * and its last mip is at 67076096 + 32256 = 67108352.
     * The allocation is 2048 * 32768 = 67108864.
     */
    bx_xg_resource_desc desc = desc_2d(DXGI_FORMAT_R8G8B8A8_UNORM, 64, 64, 2048, 7, BX_XG_LAYOUT_LINEAR);
    bx_xg_texture_computer* computer = create_ok(&desc, "create 2048x7");
    if (!computer) {
        return;
    }
    uint64_t offset = 0;
    uint64_t size = 0;
    check_hr("slice 0 mip6", computer->lpVtbl->GetMipLevelOffsetBytes(computer, 0, 6, &offset), S_OK);
    check(offset == 32256u, "first slice last mip is 32256");
    check_hr("last slice mip0", computer->lpVtbl->GetMipLevelOffsetBytes(computer, 2047, 0, &offset), S_OK);
    check(offset == 67076096ull, "last slice starts at 67076096");
    check_hr("last mip", computer->lpVtbl->GetMipLevelOffsetBytes(computer, 2047, 6, &offset), S_OK);
    check(offset == 67108352ull, "last subresource offset is 67108352");
    check_hr("array allocation", computer->lpVtbl->GetResourceSizeBytes(computer, &size), S_OK);
    check(size == 67108864ull, "2048 slices allocate 67108864 bytes");

    bx_xg_placed_footprint layout{};
    uint32_t rows = 0;
    uint64_t row_bytes = 0;
    uint64_t total = 0;
    const uint32_t last_index = 6u + 2047u * 7u;
    check_hr("last layout",
             computer->lpVtbl->GetResourceLayout(computer, last_index, 1, 0, &layout, &rows, &row_bytes, &total),
             S_OK);
    check(layout.offset == 67108352ull && layout.copy_end == 67108356ull && total == 67108864ull,
          "last footprint copy end is 4 bytes past its offset");
    release_computer(computer);
}

static void test_refcount() {
    bx_xg_resource_desc desc = desc_2d(DXGI_FORMAT_R8G8B8A8_UNORM, 4, 2, 1, 1, BX_XG_LAYOUT_LINEAR);
    bx_xg_texture_computer* computer = create_ok(&desc, "create refcount");
    if (!computer) {
        return;
    }
    check(computer->lpVtbl->AddRef(nullptr) == 0u, "AddRef null");
    check(computer->lpVtbl->Release(nullptr) == 0u, "Release null");
    void* missing = reinterpret_cast<void*>(static_cast<uintptr_t>(1));
    check_hr("QueryInterface null", computer->lpVtbl->QueryInterface(nullptr, &BX_IID_IUnknown, &missing), E_POINTER);
    check(missing == reinterpret_cast<void*>(static_cast<uintptr_t>(1)), "null QueryInterface leaves its output");
    check(computer->lpVtbl->AddRef(computer) == 2u, "AddRef 2");
    check(computer->lpVtbl->AddRef(computer) == 3u, "AddRef 3");
    check(computer->lpVtbl->Release(computer) == 2u, "Release 2");

    void* extra = nullptr;
    check_hr("QI IUnknown", computer->lpVtbl->QueryInterface(computer, &BX_IID_IUnknown, &extra), S_OK);
    check(extra == computer, "IUnknown is the object");
    check(computer->lpVtbl->Release(computer) == 2u, "release QI");

    check_hr("QI computer", computer->lpVtbl->QueryInterface(computer, &BX_IID_IXGTextureAddressComputer, &extra), S_OK);
    check(extra == computer, "computer IID is the object");
    check(computer->lpVtbl->Release(computer) == 2u, "release computer IID");

    GUID other{};
    other.Data1 = 0x11111111u;
    void* rejected = reinterpret_cast<void*>(static_cast<uintptr_t>(1));
    check_hr("QI unknown", computer->lpVtbl->QueryInterface(computer, &other, &rejected), E_NOINTERFACE);
    check(rejected == nullptr, "unknown IID clears the output");

    uint64_t offset = 0;
    check_hr("texel after QI", computer->lpVtbl->GetTexelElementOffsetBytes(computer, 0, 0, 1, 1, 0, &offset), S_OK);
    /* 4x2 rgba, pitch 256. (1,1) = 256 + 4. */
    check(offset == 260u, "rgba 4x2 texel (1,1)");

    check(computer->lpVtbl->Release(computer) == 1u, "Release 1");
    check(computer->lpVtbl->Release(computer) == 0u, "Release 0");
}

int main() {
    test_arguments();
    test_hand_layouts();
    test_monotonic_chain(DXGI_FORMAT_R8G8B8A8_UNORM, 64, 48, "rgba chain");
    test_monotonic_chain(DXGI_FORMAT_BC1_UNORM, 33, 17, "bc1 chain");
    test_monotonic_chain(DXGI_FORMAT_BC7_UNORM, 32, 32, "bc7 chain");
    test_copies();
    test_overlapping_copies();
    test_large_array_offsets();
    test_swizzle_placeholder();
    test_refcount();

    if (g_failures != 0) {
        std::fprintf(stderr, "FAIL: test_xg_xs (%d failures / %d checks)\n", g_failures, g_checks);
        return 1;
    }
    std::printf("PASS: test_xg_xs (%d checks)\n", g_checks);
    return 0;
}

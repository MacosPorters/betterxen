#pragma once

/*
 * BetterXen synthetic QueryApiImpl keys: the single place to find or change them.
 *
 * These class and interface ids are defined by BetterXen. They are NOT Microsoft
 * identifiers and were not derived from any Microsoft binary. A title linked against
 * Microsoft's xgameruntime.lib will not request them. They let BetterXen's own callers
 * and conformance fixtures reach the offline families through QueryApiImpl. The public
 * class and interface ids for XGameSave and XPackage are registered separately in
 * com_dispatch.cpp. These synthetic keys stay so the fixtures below keep working.
 * To replace a family's keys, change its entry here and
 * the expected values in the fixtures listed below. The fixtures keep their own copies on
 * purpose, so a wrong key here fails a test instead of agreeing with itself:
 *   tests/conformance/test_xgameruntime/main.cpp        (all families)
 *   tests/conformance/test_xgameruntime_saves/main.cpp  (xgamesave, xpackage)
 *
 * Encoding: Data1 is the key below. Data2 = 0x5853 ("XS"), Data3 = 0x4000,
 * Data4 = 80 00 00 00 00 00 00 01. For example, 0xB7581601 is
 * {B7581601-5853-4000-8000-000000000001}.
 */

#include <windows.h>

#include <cstdint>

namespace bx::gdk::synthetic {

constexpr uint16_t kData2 = 0x5853;
constexpr uint16_t kData3 = 0x4000;
constexpr uint8_t kData4[8] = {0x80, 0, 0, 0, 0, 0, 0, 0x01};

constexpr GUID key(uint32_t data1) {
    return {data1, kData2, kData3, {kData4[0], kData4[1], kData4[2], kData4[3],
                                    kData4[4], kData4[5], kData4[6], kData4[7]}};
}

struct FamilyKey {
    uint32_t clsid;
    uint32_t iid;
};

// Plain function tables (#439). QueryApiImpl returns a pointer to a C struct of
// function pointers; the layouts are the *Dispatch structs in xgameruntime.cpp.
constexpr FamilyKey kTaskQueue{0xB7581001, 0xB7581002};
constexpr FamilyKey kAsync{0xB7581101, 0xB7581102};
constexpr FamilyKey kUser{0xB7581201, 0xB7581202};
constexpr FamilyKey kStore{0xB7581301, 0xB7581302};
constexpr FamilyKey kSystem{0xB7581401, 0xB7581402};
constexpr FamilyKey kPersistentLocalStorage{0xB7581501, 0xB7581502};

// COM IUnknown objects (#468). QueryApiImpl returns an object whose first member is a
// vtable pointer. Slots 0-2 are IUnknown; the remaining slots follow the enums below.
// Every slot takes the object pointer first, then the parameters of the Microsoft Learn
// function it names, in the documented order.
constexpr FamilyKey kGameSave{0xB7581601, 0xB7581602};
constexpr FamilyKey kPackage{0xB7581701, 0xB7581702};

enum class GameSaveSlot : uint32_t {
    QueryInterface,
    AddRef,
    Release,
    InitializeProviderAsync,   // (XUserHandle, const char* scid, bool syncOnDemand, XAsyncBlock*)
    InitializeProviderResult,  // (XAsyncBlock*, XGameSaveProviderHandle*)
    CloseProvider,             // (XGameSaveProviderHandle)
    CreateContainer,           // (XGameSaveProviderHandle, const char* name, XGameSaveContainerHandle*)
    CloseContainer,            // (XGameSaveContainerHandle)
    CreateUpdate,              // (XGameSaveContainerHandle, const char* displayName, XGameSaveUpdateHandle*)
    CloseUpdate,               // (XGameSaveUpdateHandle)
    SubmitBlobWrite,           // (XGameSaveUpdateHandle, const char* blobName, const uint8_t*, size_t)
    SubmitUpdateAsync,         // (XGameSaveUpdateHandle, XAsyncBlock*)
    SubmitUpdateResult,        // (XAsyncBlock*)
    ReadBlobDataAsync,         // (XGameSaveContainerHandle, const char** names, uint32_t count, XAsyncBlock*)
    ReadBlobDataResult,        // (XAsyncBlock*, size_t, XGameSaveBlob*, uint32_t* count)
    EnumerateContainerInfo,    // (XGameSaveProviderHandle, void* context, XGameSaveContainerInfoCallback*)
    Count,
};

enum class PackageSlot : uint32_t {
    QueryInterface,
    AddRef,
    Release,
    GetCurrentProcessPackageIdentifier,  // (size_t bufferSize, char* buffer)
    GetPackageKind,                      // (const char* packageIdentifier, XPackageKind*)
    Mount,                               // (const char* packageIdentifier, XPackageMountHandle*)
    GetMountPathSize,                    // (XPackageMountHandle, size_t*)
    GetMountPath,                        // (XPackageMountHandle, size_t, char*)
    CloseMountHandle,                    // (XPackageMountHandle)
    Count,
};

} // namespace bx::gdk::synthetic

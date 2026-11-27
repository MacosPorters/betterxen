#pragma once

#include <cstddef>
#include <memory>
#include <new>
#include <utility>

// Test-only allocation seam for the device-callback map. It is not part of
// GameInput.dll's frozen export table: the DLL looks for an
// "BxGameInputTestAllocate" function in the host executable once, and falls
// back to the default allocator when the fixture does not provide one.
namespace bx::gameinput::testing {
void* Allocate(std::size_t size);
}

template<class T>
struct CallbackAllocator {
    using value_type = T;

    CallbackAllocator() noexcept = default;
    template<class U>
    CallbackAllocator(const CallbackAllocator<U>&) noexcept {}

    T* allocate(std::size_t count) {
        if (count > static_cast<std::size_t>(-1) / sizeof(T)) throw std::bad_alloc();
        void* block = bx::gameinput::testing::Allocate(count * sizeof(T));
        if (!block) throw std::bad_alloc();
        return static_cast<T*>(block);
    }
    void deallocate(T* pointer, std::size_t) noexcept { ::operator delete(pointer); }

    template<class U>
    struct rebind { using other = CallbackAllocator<U>; };
};

template<class T, class U>
bool operator==(const CallbackAllocator<T>&, const CallbackAllocator<U>&) noexcept { return true; }
template<class T, class U>
bool operator!=(const CallbackAllocator<T>&, const CallbackAllocator<U>&) noexcept { return false; }

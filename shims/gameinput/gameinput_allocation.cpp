#include "gameinput_allocation.h"

#include <windows.h>

namespace bx::gameinput::testing {
namespace {
using TestAllocate = void* (*)(std::size_t);
TestAllocate resolved_hook = nullptr;
bool resolved = false;
}

void* Allocate(std::size_t size) {
    if (!resolved) {
        resolved = true;
        resolved_hook = reinterpret_cast<TestAllocate>(reinterpret_cast<void*>(
            GetProcAddress(GetModuleHandleW(nullptr), "BxGameInputTestAllocate")));
    }
    if (resolved_hook) return resolved_hook(size);
    return ::operator new(size);
}
} // namespace bx::gameinput::testing

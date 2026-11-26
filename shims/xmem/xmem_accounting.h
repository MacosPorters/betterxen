#ifndef BETTERXEN_SHIMS_XMEM_ACCOUNTING_H
#define BETTERXEN_SHIMS_XMEM_ACCOUNTING_H

#include <cstdint>

namespace bx_xmem {

// Reserve or release physical-page bytes against the shared xmem title memory
// cap and working-set statistics (heap + virtual + physical <= title limit).
bool try_reserve_physical(uint64_t bytes);
void release_physical(uint64_t bytes);

} // namespace bx_xmem

#endif // BETTERXEN_SHIMS_XMEM_ACCOUNTING_H

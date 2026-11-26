#include "command_ring.h"

extern "C" {
HRESULT WINAPI XApuConnect(void** out) { return connect(out); }
HRESULT WINAPI XApuDisconnect(void* conn) { return disconnect(conn); }

HRESULT WINAPI XApuEnqueueCommand(void* conn, const void* cmd, uint32_t size) {
    std::lock_guard guard(connections_lock);
    auto* c = lookup(conn);
    return c ? c->queue.push(cmd, size) : E_HANDLE;
}

HRESULT WINAPI XApuDequeueResult(void* conn, void* result, uint32_t size) {
    std::lock_guard guard(connections_lock);
    auto* c = lookup(conn);
    return c ? c->queue.read(result, size) : E_HANDLE;
}

uint32_t WINAPI XApuGetResultCount(void* conn) {
    std::lock_guard guard(connections_lock);
    auto* c = lookup(conn);
    return c ? c->queue.count : 0;
}
}

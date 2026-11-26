#define BX_DSP_CONNECTIONS
#include "command_ring.h"

extern "C" {
// Validate and record the caller's stream limit; the synthetic command ring
// holds a fixed 32-packet backlog independent of stream count.
HRESULT WINAPI XDspConnectWithMaximumStreamLimit(uint32_t max_streams, void** out) {
    if (!out) return E_POINTER;
    *out = nullptr;
    if (!max_streams) return E_INVALIDARG;
    return connect(out, max_streams);
}
HRESULT WINAPI XDspDisconnect(void* handle) { return disconnect(handle); }

HRESULT WINAPI XDspActivate(void* handle) {
    std::lock_guard guard(connections_lock);
    auto* c = lookup(handle);
    if (!c) return E_HANDLE;
    c->active = true;
    // chad has no result export. Active DSP commands complete synchronously.
    while (c->queue.count) c->queue.pop();
    return S_OK;
}

HRESULT WINAPI XDspDeactivate(void* handle) {
    std::lock_guard guard(connections_lock);
    auto* c = lookup(handle);
    if (!c) return E_HANDLE;
    c->active = false;
    return S_OK;
}

HRESULT WINAPI XDspSubmitCommand(void* handle, const void* cmd, uint32_t size) {
    std::lock_guard guard(connections_lock);
    auto* c = lookup(handle);
    if (!c) return E_HANDLE;
    const HRESULT hr = c->queue.push(cmd, size);
    if (SUCCEEDED(hr) && c->active) c->queue.pop();
    return hr;
}
}

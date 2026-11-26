#pragma once

#include <windows.h>
#include <array>
#include <cstdint>
#include <cstring>
#include <mutex>
#include <new>

namespace {
// A synthetic command completes by returning its opaque bytes unchanged.
struct CommandRing {
    struct Packet {
        uint32_t size;
        unsigned char bytes[1024];
    };
    std::array<Packet, 32> packets;
    unsigned head = 0, count = 0;

    HRESULT push(const void* data, uint32_t size) {
        if (!data || !size || size > sizeof(Packet::bytes)) return E_INVALIDARG;
        if (count == packets.size()) return HRESULT_FROM_WIN32(ERROR_BUSY);
        auto& packet = packets[(head + count) % packets.size()];
        packet.size = size;
        std::memcpy(packet.bytes, data, size);
        ++count;
        return S_OK;
    }
    void pop() {
        head = (head + 1) % packets.size();
        --count;
    }
    HRESULT read(void* data, uint32_t size) {
        if (!data) return E_POINTER;
        if (!count) return S_FALSE;
        const auto& packet = packets[head];
        if (size < packet.size) return HRESULT_FROM_WIN32(ERROR_INSUFFICIENT_BUFFER);
        std::memcpy(data, packet.bytes, packet.size);
        pop();
        return S_OK;
    }
};

struct Connection {
    uintptr_t id;
    Connection* next;
    CommandRing queue;
    uint32_t max_streams = 0;
    bool active = false;
};

// Each DLL owns its registry. An ID stays invalid after disconnect, including
// when the allocator subsequently reuses the same Connection address.
std::mutex connections_lock;
Connection* connections;
#ifdef BX_DSP_CONNECTIONS
uintptr_t next_id = 2;
#else
uintptr_t next_id = 1;
#endif

HRESULT connect(void** out, uint32_t max_streams = 0) {
    if (!out) return E_POINTER;
    *out = nullptr;
    std::lock_guard guard(connections_lock);
    if (next_id > UINTPTR_MAX - 2) return E_OUTOFMEMORY;
    auto* c = new (std::nothrow) Connection;
    if (!c) return E_OUTOFMEMORY;
    c->id = next_id;
    c->max_streams = max_streams;
    next_id += 2;
    c->next = connections;
    connections = c;
    *out = reinterpret_cast<void*>(c->id);
    return S_OK;
}

// Caller holds connections_lock through the operation.
Connection* lookup(void* handle) {
    for (auto* c = connections; c; c = c->next)
        if (c->id == reinterpret_cast<uintptr_t>(handle)) return c;
    return nullptr;
}

HRESULT disconnect(void* handle) {
    std::lock_guard guard(connections_lock);
    for (auto** link = &connections; *link; link = &(*link)->next) {
        auto* c = *link;
        if (c->id != reinterpret_cast<uintptr_t>(handle)) continue;
        *link = c->next;
        delete c;
        return S_OK;
    }
    return E_HANDLE;
}
}

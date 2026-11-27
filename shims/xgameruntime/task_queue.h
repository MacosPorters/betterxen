#pragma once

#include <windows.h>
#include <cstdint>
#include <memory>

namespace bx::gdk {

enum class DispatchMode : uint32_t { manual, parallel, serialized, immediate };
enum class Port : uint32_t { work, completion };

class Queue;
using QueueRef = std::shared_ptr<Queue>;

struct Task {
    virtual ~Task() = default;
    virtual void run(const QueueRef& queue) = 0;
    virtual void cancel(const QueueRef& queue) { run(queue); }

    // GetTickCount64 deadline; zero means due immediately.
    uint64_t due_ms = 0;
    std::unique_ptr<Task> next;
};

QueueRef acquire_queue(void* handle);
bool submit(const QueueRef& queue, Port port, std::unique_ptr<Task> task, bool is_completion = false);

}

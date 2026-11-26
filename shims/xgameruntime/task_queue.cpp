#include "task_queue.h"

#include <chrono>
#include <condition_variable>
#include <mutex>
#include <new>
#include <unordered_map>

namespace bx::gdk {

class Queue : public std::enable_shared_from_this<Queue> {
public:
    struct Lane {
        Queue* owner;
        DispatchMode mode;
        PTP_WORK worker = nullptr;
        std::mutex mutex;
        std::condition_variable ready;
        std::unique_ptr<Task> first;
        Task* last = nullptr;
        // Keep the queue alive while Windows has outstanding worker callbacks.
        QueueRef worker_owner;
        size_t workers = 0;
        bool serial_running = false;
        bool closed = false;

        Lane(Queue* owner, DispatchMode mode) : owner(owner), mode(mode) {}

        ~Lane() {
            if (worker) CloseThreadpoolWork(worker);
            while (first) pop();
        }

        HRESULT initialize() {
            if (mode == DispatchMode::parallel || mode == DispatchMode::serialized) {
                worker = CreateThreadpoolWork(run_worker, this, nullptr);
                if (!worker) return HRESULT_FROM_WIN32(GetLastError());
            }
            return S_OK;
        }

        bool push(std::unique_ptr<Task> task, bool is_completion) {
            std::unique_lock lock(mutex);
            if (closed && !is_completion) return false;
            if (mode == DispatchMode::immediate || (closed && mode == DispatchMode::manual)) {
                lock.unlock();
                task->run(owner->shared_from_this());
                return true;
            }
            auto* tail = task.get();
            (last ? last->next : first) = std::move(task);
            last = tail;
            ready.notify_one();

            if (mode == DispatchMode::parallel) schedule();
            schedule_serial();
            return true;
        }

        void schedule() {
            if (workers++ == 0) worker_owner = owner->shared_from_this();
            SubmitThreadpoolWork(worker);
        }

        void schedule_serial() {
            if (mode == DispatchMode::serialized && first && !serial_running && workers == 0) {
                schedule();
            }
        }

        std::unique_ptr<Task> pop() {
            auto task = std::move(first);
            first = std::move(task->next);
            if (!first) last = nullptr;
            return task;
        }

        void close(const QueueRef& queue) {
            std::unique_lock lock(mutex);
            closed = true;
            ready.notify_all();
            if (mode != DispatchMode::manual) return;

            while (first) {
                auto task = pop();
                lock.unlock();
                task->cancel(queue);
                lock.lock();
            }
        }

        bool dispatch(uint32_t timeout) {
            if (mode == DispatchMode::immediate) return false;
            std::unique_lock lock(mutex);
            const auto has_task = [this] { return closed || (first && !serial_running); };
            if (timeout == INFINITE) {
                ready.wait(lock, has_task);
            } else if (!ready.wait_for(lock, std::chrono::milliseconds(timeout), has_task)) {
                return false;
            }
            if (closed) return false;
            auto task = pop();
            if (mode == DispatchMode::serialized) serial_running = true;
            lock.unlock();
            task->run(owner->shared_from_this());
            if (mode == DispatchMode::serialized) {
                lock.lock();
                serial_running = false;
                schedule_serial();
                ready.notify_all();
            }
            return true;
        }

        static void CALLBACK run_worker(PTP_CALLBACK_INSTANCE, void* context, PTP_WORK) {
            auto& lane = *static_cast<Lane*>(context);
            QueueRef queue;
            std::unique_lock lock(lane.mutex);
            queue = lane.worker_owner;
            while (lane.first && !lane.serial_running) {
                auto task = lane.pop();
                const bool serial = lane.mode == DispatchMode::serialized;
                lane.serial_running = serial;
                lock.unlock();
                task->run(queue);
                lock.lock();
                lane.serial_running = false;
                if (!serial) break;
            }
            if (--lane.workers == 0) lane.worker_owner.reset();
        }
    };

    Queue(DispatchMode work, DispatchMode completion) : lanes{{this, work}, {this, completion}} {}

    Lane lanes[2];
};

namespace {

std::mutex queues_mutex;
std::unordered_map<uintptr_t, QueueRef> handles;
uintptr_t next_handle = 1;
QueueRef process_queue;

HRESULT make_queue(DispatchMode work, DispatchMode completion, QueueRef& queue) {
    auto candidate = std::make_shared<Queue>(work, completion);
    for (auto& lane : candidate->lanes) {
        const HRESULT hr = lane.initialize();
        if (FAILED(hr)) return hr;
    }
    queue = std::move(candidate);
    return S_OK;
}

void* add_handle(const QueueRef& queue) {
    const auto id = next_handle;
    next_handle += 2;
    handles.emplace(id, queue);
    return reinterpret_cast<void*>(id);
}

struct CallbackTask final : Task {
    using Callback = void (CALLBACK*)(void*, bool);
    Callback callback;
    void* context;

    CallbackTask(Callback callback, void* context) : callback(callback), context(context) {}
    void run(const QueueRef&) override { callback(context, false); }
    void cancel(const QueueRef&) override { callback(context, true); }
};

} // namespace

QueueRef acquire_queue(void* handle) {
    std::lock_guard lock(queues_mutex);
    if (!handle) {
        try {
            if (!process_queue) make_queue(DispatchMode::parallel, DispatchMode::parallel, process_queue);
        } catch (const std::bad_alloc&) {
            return {};
        }
        return process_queue;
    }
    const auto it = handles.find(reinterpret_cast<uintptr_t>(handle));
    return it == handles.end() ? QueueRef{} : it->second;
}

bool submit(const QueueRef& queue, Port port, std::unique_ptr<Task> task, bool is_completion) {
    return queue->lanes[static_cast<uint32_t>(port)].push(std::move(task), is_completion);
}

} // namespace bx::gdk

using namespace bx::gdk;

extern "C" {

HRESULT WINAPI XTaskQueueCreate(DispatchMode work, DispatchMode completion, void** out) {
    if (!out) return E_POINTER;
    *out = nullptr;
    if (work > DispatchMode::immediate || completion > DispatchMode::immediate) return E_INVALIDARG;

    try {
        QueueRef queue;
        const auto hr = make_queue(work, completion, queue);
        if (FAILED(hr)) return hr;
        std::lock_guard lock(queues_mutex);
        *out = add_handle(queue);
        return S_OK;
    } catch (const std::bad_alloc&) {
        return E_OUTOFMEMORY;
    }
}

void WINAPI XTaskQueueCloseHandle(void* handle) {
    QueueRef queue;
    bool close_queue = false;
    {
        std::lock_guard lock(queues_mutex);
        const auto it = handles.find(reinterpret_cast<uintptr_t>(handle));
        if (it == handles.end()) return;
        queue = std::move(it->second);
        close_queue = queue != process_queue;
        handles.erase(it);
    }
    // The frozen ABI exposes no termination or duplicate-handle API. Drain manual
    // completion callbacks on close and let accepted pool work finish normally.
    if (close_queue) {
        for (auto& lane : queue->lanes) lane.close(queue);
    }
}

bool WINAPI XTaskQueueDispatch(void* handle, Port port, uint32_t timeout) {
    if (!handle || port > Port::completion) return false;
    const auto queue = acquire_queue(handle);
    return queue && queue->lanes[static_cast<uint32_t>(port)].dispatch(timeout);
}

HRESULT WINAPI XTaskQueueSubmitCallback(void* handle, Port port, void* context, CallbackTask::Callback callback) {
    if (!handle || port > Port::completion || !callback) return E_INVALIDARG;
    const auto queue = acquire_queue(handle);
    if (!queue) return E_HANDLE;
    auto task = std::unique_ptr<Task>(new (std::nothrow) CallbackTask(callback, context));
    if (!task) return E_OUTOFMEMORY;
    return submit(queue, port, std::move(task)) ? S_OK : E_HANDLE;
}

bool WINAPI XTaskQueueGetCurrentProcessTaskQueue(void** out) {
    if (!out) return false;
    *out = nullptr;
    const auto queue = acquire_queue(nullptr);
    if (!queue) return false;
    try {
        std::lock_guard lock(queues_mutex);
        *out = add_handle(queue);
        return true;
    } catch (const std::bad_alloc&) {
        return false;
    }
}

} // extern "C"

// SPDX-License-Identifier: GPL-3.0-or-later
#include "support.h"

struct DestructorCanary {
  const char *label;
  ~DestructorCanary() {
    std::printf("UNSAFE_UNWIND label=%s\n", label);
    std::fflush(nullptr);
  }
};

static void stage(const char *name) {
  std::printf("STAGE %s cpu_ptr=0\n", name);
  if (std::getchar() == EOF)
    throw std::runtime_error("handshake");
}

int main(int argc, char **argv) {
  setvbuf(stdout, nullptr, _IONBF, 0);
  if (argc != 2) return 3;
  std::string mode = argv[1];
  Context c;
  Queue q(c.device.p);
  stage("ready");
  if (mode == "positive_gate") {
    Buffer src, dst;
    src.allocate(c.device.p, 65536, "wb");
    src.map();
    dst.allocate(c.device.p, 65536, "readback");
    dst.map();
    static_cast<volatile uint32_t *>(src.cpu)[0] = 0xCAFE1234u;
    std::memset(dst.cpu, 0xA7, 64);
    _mm_sfence();
    Ref<ID3D12Fence> gate;
    require(c.device->CreateFence(0, D3D12_FENCE_FLAG_NONE, __uuidof(ID3D12Fence), gate.out()), "gate");
    q.begin();
    q.list->CopyBufferRegion(dst.resource.p, 0, src.resource.p, 0, 64);
    q.close();
    require(q.q->Wait(gate.p, 1), "Wait gate");
    ID3D12CommandList *lists[] = {q.list.p};
    q.q->ExecuteCommandLists(1, lists);
    UINT64 target = ++q.value;
    HRESULT s = q.q->Signal(q.fence.p, target);
    if (FAILED(s)) q.stopInFlight("retirement Signal", s, target, q.fence->GetCompletedValue());
    bool withheld = q.fence->GetCompletedValue() < target;
    HRESULT o = gate->Signal(1);
    if (FAILED(o)) q.stopInFlight("open retirement gate", o, target, q.fence->GetCompletedValue());
    q.waitFor(target);
    uint32_t got = static_cast<volatile uint32_t *>(dst.cpu)[0];
    std::printf("POSITIVE_GATE withheld=%d word=0x%08X completed=%llu\n", withheld, got, q.fence->GetCompletedValue());
    if (!withheld || got != 0xCAFE1234u) return 1;
    std::puts("END success");
    return 0;
  }
  if (mode == "fault_wait_timeout") {
    DestructorCanary canary{"live_stack_resource"};
    // Unsignaled target=1 with zero stalled GPU queue work: exercises SetEventOnCompletion,
    // 10s WaitForSingleObject timeout, post-wait GetCompletedValue re-check, and ExitProcess(2).
    q.waitFor(1);
    return 0;
  }
  if (mode == "fault_signal_stop") {
    DestructorCanary canary{"live_stack_resource"};
    q.stopInFlight("Signal", E_OUTOFMEMORY, 1, q.fence->GetCompletedValue());
  }
  return 3;
}

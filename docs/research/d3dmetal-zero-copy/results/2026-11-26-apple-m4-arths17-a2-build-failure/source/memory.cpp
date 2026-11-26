// SPDX-License-Identifier: GPL-3.0-or-later
// Workstream A protocol A2: correct baseline and retirement scope, not a copy
// detector.
#include "support.h"
#include <memory>
static void stage(const char *name, void *cpu = nullptr) {
  std::printf("STAGE %s cpu_ptr=%p\n", name, cpu);
  if (std::getchar() == EOF)
    throw std::runtime_error("stage handshake lost");
}
int main(int argc, char **argv) {
  setvbuf(stdout, nullptr, _IONBF, 0);
  try {
    if (argc != 3)
      throw std::runtime_error("usage: memory cpu|cpu-plain|wb|upload BYTES");
    std::string type = argv[1];
    UINT64 bytes = std::strtoull(argv[2], nullptr, 10);
    if (bytes != 64 * 1024 * 1024ULL && bytes != 256 * 1024 * 1024ULL &&
        bytes != 1024 * 1024 * 1024ULL)
      throw std::runtime_error("size outside common profile");
    if (type != "cpu" && type != "cpu-plain" && type != "wb" &&
        type != "upload")
      throw std::runtime_error("type");
    std::printf("MEMORY_A protocol=2 type=%s bytes=%llu\n", type.c_str(),
                bytes);
    std::unique_ptr<Context> c;
    std::unique_ptr<Shader> shader;
    std::unique_ptr<Queue> q;
    Buffer output;
    if (type != "cpu-plain") {
      c = std::make_unique<Context>();
      shader = std::make_unique<Shader>(*c);
      q = std::make_unique<Queue>(c->device.p);
      Buffer tiny;
      tiny.allocate(c->device.p, 65536, "upload");
      tiny.map();
      std::memset(tiny.cpu, 0, 65536);
      output.allocate(c->device.p, 65536, "wb", true);
      output.map();
      shader->dispatch(*q, tiny, output, 1, 0, 1);
      // Dispose the warm-up command objects before baseline; keep matched
      // pipeline/output.
      q.reset();
      tiny.release();
      q = std::make_unique<Queue>(c->device.p);
    }
    stage("warm_baseline");
    Sleep(500);
    stage("baseline");
    Buffer src;
    void *cpu = nullptr;
    if (type == "cpu" || type == "cpu-plain") {
      cpu = VirtualAlloc(nullptr, bytes, MEM_COMMIT | MEM_RESERVE,
                         PAGE_READWRITE);
      if (!cpu)
        throw std::runtime_error("VirtualAlloc");
    } else
      src.allocate(c->device.p, bytes, type);
    stage("created");
    if (src.resource.p) {
      src.map();
      cpu = src.cpu;
    }
    stage("mapped", cpu);
    auto p = static_cast<volatile uint32_t *>(cpu);
    std::array<uint32_t, 256> expected{};
    for (UINT64 i = 0; i < bytes / 4; ++i) {
      uint32_t v = pattern(i, 0x24681357u);
      p[i] = v;
      expected[i % 256] ^= v + 0x10101u;
    }
    _mm_sfence();
    stage("fully_initialized", cpu);
    if (src.resource.p) {
      shader->dispatch(*q, src, output, UINT(bytes / 4), 0x10101u, 1);
      size_t matches = 0;
      for (UINT i = 0; i < 256; ++i)
        matches +=
            static_cast<volatile uint32_t *>(output.cpu)[i] == expected[i];
      std::printf(
          "VERIFY full_consumption checksum_matches=%zu/256 words=%llu\n",
          matches, bytes / 4);
      if (matches != 256)
        throw std::runtime_error("full-consumption checksum mismatch");
    } else
      std::puts("GPU_CONSUMPTION NOT_RUN CPU_allocation_control");
    stage("gpu_complete", cpu);
    if (src.resource.p)
      src.release();
    else if (!VirtualFree(cpu, 0, MEM_RELEASE))
      throw std::runtime_error("VirtualFree");
    cpu = nullptr;
    stage("resources_released");
    // Fence already completed; dispose all
    // command-list/allocator/queue/fence/pipeline resources.
    q.reset();
    output.release();
    shader.reset();
    stage("commands_retired");
    Sleep(500);
    stage("retired_settled");
    c.reset();
    stage("device_released");
    std::puts("END success");
    return 0;
  } catch (const std::exception &e) {
    std::printf("END failure reason=%s\n", e.what());
    return 1;
  }
}

// SPDX-License-Identifier: GPL-3.0-or-later
// Original trusted harness. Remill headers remain external, under Apache-2.0.
#include "remill/Arch/X86/Runtime/State.h"
#include <csetjmp>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>

struct ResultState {
  uint64_t rax, rsp, rip, flags, data, data_hi, read_only, exit;
};
extern "C" {
Memory *bx_memory_new();
void bx_memory_free(Memory *);
uint8_t bx_read(const Memory *, uint64_t, uint64_t *);
uint8_t bx_write(Memory *, uint64_t, uint64_t);
uint8_t bx_run(uint32_t, const uint8_t *, size_t, uint64_t, uint64_t, uint64_t,
               ResultState *);
Memory *lifted_add(State &, uint64_t, Memory *);
Memory *lifted_load(State &, uint64_t, Memory *);
Memory *lifted_store(State &, uint64_t, Memory *);
Memory *lifted_ud2(State &, uint64_t, Memory *);
Memory *lifted_jump(State &, uint64_t, Memory *);
}

static std::jmp_buf escape;
static bool returned;
[[noreturn]] static void stop(int reason) { std::longjmp(escape, reason); }

extern "C" uint64_t __remill_read_memory_64(Memory *m, uint64_t address) {
  uint64_t result;
  if (!bx_read(m, address, &result))
    stop(1);
  return result;
}
extern "C" Memory *__remill_write_memory_64(Memory *m, uint64_t address,
                                            uint64_t value) {
  if (!bx_write(m, address, value))
    stop(1);
  return m;
}
extern "C" Memory *__remill_function_return(State &, uint64_t address,
                                            Memory *m) {
  if (address != 0x4000)
    stop(3);
  returned = true;
  return m;
}
// All other transfers fail explicitly; this harness has no guest dispatcher.
extern "C" Memory *__remill_jump(State &, uint64_t, Memory *) { stop(2); }
extern "C" Memory *__remill_error(State &, uint64_t, Memory *) { stop(2); }
extern "C" Memory *__remill_missing_block(State &, uint64_t, Memory *) {
  stop(2);
}
extern "C" Memory *__remill_function_call(State &, uint64_t, Memory *) {
  stop(2);
}
extern "C" Memory *__remill_async_hyper_call(State &, uint64_t, Memory *) {
  stop(2);
}
extern "C" Memory *__remill_sync_hyper_call(State &, Memory *, uint32_t) {
  stop(2);
}

// These intrinsics annotate an already-computed predicate; they do not compute
// flags.
extern "C" bool __remill_flag_computation_zero(bool value, ...) {
  return value;
}
extern "C" bool __remill_flag_computation_sign(bool value, ...) {
  return value;
}
extern "C" bool __remill_flag_computation_overflow(bool value, ...) {
  return value;
}
extern "C" bool __remill_flag_computation_carry(bool value, ...) {
  return value;
}

using Trace = Memory *(*)(State &, uint64_t, Memory *);
static State state;
static ResultState run(Trace trace, uint64_t a, uint64_t b, uint64_t address) {
  std::memset(&state, 0, sizeof(state));
  state.gpr.rax.qword = a;
  state.gpr.rcx.qword = b;
  state.gpr.rbx.qword = address;
  state.gpr.rsp.qword = 0x3000;
  Memory *memory = bx_memory_new();
  returned = false;
  int status = setjmp(escape);
  if (!status) {
    trace(state, 0x1000, memory);
    if (!returned)
      stop(3);
  }
  uint64_t data = 0, data_hi = 0, read_only = 0;
  if (!bx_read(memory, 0x2000, &data) || !bx_read(memory, 0x2008, &data_hi) ||
      !bx_read(memory, 0x6000, &read_only))
    std::abort();
  const auto &f = state.aflag;
  ResultState result{state.gpr.rax.qword,
                     state.gpr.rsp.qword,
                     state.gpr.rip.qword,
                     uint64_t(f.cf) | uint64_t(f.pf) << 2 |
                         uint64_t(f.af) << 4 | uint64_t(f.zf) << 6 |
                         uint64_t(f.sf) << 7 | uint64_t(f.of) << 11,
                     data,
                     data_hi,
                     read_only,
                     uint64_t(status)};
  bx_memory_free(memory);
  return result;
}

static void require(bool condition, const char *message) {
  if (!condition) {
    std::fprintf(stderr, "FAIL: %s\n", message);
    std::exit(1);
  }
}
static const uint8_t add_code[] = {0x48, 0x01, 0xc8, 0x48, 0x89, 0x03, 0xc3};
static const uint8_t load_code[] = {0x48, 0x8b, 0x03, 0xc3};
static const uint8_t store_code[] = {0x48, 0x89, 0x03, 0xc3};

static int integer_checks(int argc, char **argv) {
  // Hand-derived ADD results: CF/PF/AF/ZF/SF/OF occupy x86 RFLAGS positions.
  struct Case {
    uint64_t a, b, value, flags;
  };
  Case cases[] = {
      {1, 2, 3, 0x4},
      {~0ULL, 1, 0, 0x55},
      {0x7fffffffffffffffULL, 1, 0x8000000000000000ULL, 0x894},
      {0x8000000000000000ULL, 0x8000000000000000ULL, 0, 0x845},
      {15, 1, 16, 0x10},
      {0, 0, 0, 0x44},
  };
  if (argc == 2 && std::strcmp(argv[1], "--wrong-answer") == 0)
    cases[0].value ^= 1;
  else if (argc != 1)
    return 64;
  unsigned checks = 0;
  for (const auto &c : cases) {
    auto actual = run(lifted_add, c.a, c.b, 0x2000);
    require(actual.exit == 0 && actual.rax == c.value &&
                actual.data == c.value && actual.flags == c.flags &&
                actual.rsp == 0x3008 && actual.data_hi == 0 &&
                actual.read_only == 0xa5a5a5a5a5a5a5a5ULL,
            "Remill ADD known answer");
    for (unsigned engine = 0; engine < 2; ++engine) {
      ResultState baseline{};
      require(bx_run(engine, add_code, sizeof(add_code), c.a, c.b, 0x2000,
                     &baseline),
              "baseline available");
      require(baseline.exit == 0 && baseline.rax == c.value &&
                  baseline.data == c.value && baseline.flags == c.flags &&
                  baseline.rsp == 0x3008 && baseline.data_hi == 0 &&
                  baseline.read_only == 0xa5a5a5a5a5a5a5a5ULL,
              "BetterXen ADD known answer");
      ++checks;
    }
    ++checks;
  }
  // Checked GuestMemory rejects unmapped reads/writes, read-only writes and
  // split writes.
  struct Fault {
    Trace trace;
    const uint8_t *code;
    size_t size;
    uint64_t address;
  };
  const Fault faults[] = {
      {lifted_load, load_code, sizeof(load_code), 0x5000},
      {lifted_store, store_code, sizeof(store_code), 0x5000},
      {lifted_store, store_code, sizeof(store_code), 0x6000},
      {lifted_store, store_code, sizeof(store_code), 0x200c},
  };
  for (const auto &c : faults) {
    const auto actual = run(c.trace, 42, 0, c.address);
    require(actual.exit == 1 && actual.rip == 0x1000 && actual.rax == 42 &&
                actual.rsp == 0x3000 && actual.flags == 0 && actual.data == 0 &&
                actual.data_hi == 0 &&
                actual.read_only == 0xa5a5a5a5a5a5a5a5ULL,
            "Remill checked memory rejection");
    for (unsigned engine = 0; engine < 2; ++engine) {
      ResultState baseline{};
      require(bx_run(engine, c.code, c.size, 42, 0, c.address, &baseline),
              "fault baseline available");
      require(baseline.exit == 1 && baseline.rip == 0x1000 &&
                  baseline.rax == 42 && baseline.rsp == 0x3000 &&
                  baseline.flags == 0 && baseline.data == 0 &&
                  baseline.data_hi == 0 &&
                  baseline.read_only == 0xa5a5a5a5a5a5a5a5ULL,
              "BetterXen checked memory rejection");
      std::printf("fault address=0x%llx engine=%u baseline_rip=0x%llx "
                  "remill_rip=0x%llx\n",
                  (unsigned long long)c.address, engine,
                  (unsigned long long)baseline.rip,
                  (unsigned long long)actual.rip);
      ++checks;
    }
    ++checks;
  }
  require(run(lifted_ud2, 0, 0, 0x2000).exit == 2, "UD2 must stop");
  ++checks;
  require(run(lifted_jump, 0x5000, 0, 0x2000).exit == 2,
          "indirect jump must stop");
  ++checks;
  std::printf(
      "PASS %u checks: 6 arithmetic cases and 4 memory rejections across "
      "Remill, interpreter and DBT; 2 Remill control rejections\n",
      checks);
  return 0;
}

#ifndef REMILL_EXTENDED_PROBES
int main(int argc, char **argv) { return integer_checks(argc, argv); }
#endif

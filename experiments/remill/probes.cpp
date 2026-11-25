// SPDX-License-Identifier: GPL-3.0-or-later
#define REMILL_EXTENDED_PROBES
#include "prototype.cpp"
#include <array>
#include <cfenv>
#include <chrono>
#include <vector>

struct SimdResult {
  uint64_t xmm0[2], xmm1[2], mxcsr, exit;
};
struct BenchResult {
  uint64_t ns, compiled, cache_hits, fallbacks, native_bytes;
};
struct Bench;
extern "C" {
uint8_t bx_simd_run(uint32_t, const uint8_t *, size_t, const uint64_t *,
                    uint32_t, SimdResult *);
Bench *bx_bench_new(uint32_t, const uint8_t *, size_t);
uint8_t bx_bench_run(Bench *, uint32_t, BenchResult *);
void bx_bench_free(Bench *);
Memory *lifted_addss(State &, uint64_t, Memory *);
Memory *lifted_paddd(State &, uint64_t, Memory *);
Memory *lifted_vpaddd(State &, uint64_t, Memory *);
Memory *lifted_vpxor(State &, uint64_t, Memory *);
Memory *lifted_loop(State &, uint64_t, Memory *);
Memory *lifted_register256(State &, uint64_t, Memory *);
Memory *lifted_memory256(State &, uint64_t, Memory *);
// This intrinsic is a predicate annotation, just like the flag annotations.
bool __remill_compare_neq(bool value) { return value; }
}

static unsigned observations, mismatches;
static void observe(const char *name, unsigned engine, bool match,
                    uint64_t bits, uint64_t expected, uint64_t mxcsr,
                    uint64_t expected_mxcsr, uint64_t exit) {
  ++observations;
  if (!match)
    ++mismatches;
  std::printf(
      "{\"kind\":\"semantic\",\"name\":\"%s\",\"engine\":%u,\"match\":%s,"
      "\"bits\":%llu,\"expected\":%llu,\"mxcsr\":%llu,\"expected_mxcsr\":%llu,"
      "\"exit\":%llu}\n",
      name, engine, match ? "true" : "false", (unsigned long long)bits,
      (unsigned long long)expected, (unsigned long long)mxcsr,
      (unsigned long long)expected_mxcsr, (unsigned long long)exit);
}

static SimdResult remill_simd(Trace trace, const uint64_t *inputs,
                              uint32_t mxcsr) {
  std::memset(&state, 0, sizeof(state));
  std::memcpy(&state.vec[0], inputs, 16);
  std::memcpy(&state.vec[1], inputs + 2, 16);
  state.x87.fxsave.mxcsr.flat = mxcsr;
  state.gpr.rsp.qword = 0x3000;
  Memory *memory = bx_memory_new();
  returned = false;
  int status = setjmp(escape);
  if (!status) {
    trace(state, 0x1000, memory);
    if (!returned)
      stop(3);
  }
  SimdResult result{};
  std::memcpy(result.xmm0, &state.vec[0], 16);
  std::memcpy(result.xmm1, &state.vec[1], 16);
  result.mxcsr = state.x87.fxsave.mxcsr.flat;
  result.exit = status;
  bx_memory_free(memory);
  return result;
}

static void floating_point() {
  // Literal IEEE binary32 answers, derived before executing the lifted code.
  // 1 + 2^-24 is halfway between adjacent floats; PE is bit 5 of MXCSR.
  struct Case {
    const char *name;
    uint32_t a, b, mxcsr, bits, after;
    unsigned exit;
  };
  const Case cases[] = {
      {"exact", 0x3f800000, 0x40000000, 0x1f80, 0x40400000, 0x1f80, 0},
      {"tie-nearest", 0x3f800000, 0x33800000, 0x1f80, 0x3f800000, 0x1fa0, 0},
      {"tie-down", 0x3f800000, 0x33800000, 0x3f80, 0x3f800000, 0x3fa0, 0},
      {"tie-up", 0x3f800000, 0x33800000, 0x5f80, 0x3f800001, 0x5fa0, 0},
      {"tie-zero", 0x3f800000, 0x33800000, 0x7f80, 0x3f800000, 0x7fa0, 0},
      {"negative-down", 0xbf800000, 0xb3800000, 0x3f80, 0xbf800001, 0x3fa0, 0},
      {"denormal", 0x00000001, 0x00000000, 0x1f80, 0x00000001, 0x1f82, 0},
      {"denormal-daz", 0x00000001, 0x00000000, 0x1fc0, 0x00000000, 0x1fc0, 0},
      {"precision-unmasked", 0x3f800000, 0x33800000, 0x0f80, 0x3f800000, 0x0fa0,
       3},
  };
  const uint8_t code[] = {0xf3, 0x0f, 0x58, 0xc1, 0xc3};
  for (const auto &c : cases) {
    const uint64_t inputs[] = {
        0x1122334400000000ULL | c.a, 0x5566778899aabbccULL,
        0xaabbccdd00000000ULL | c.b, 0x0123456789abcdefULL};
    for (unsigned engine = 0; engine < 3; ++engine) {
      SimdResult actual{};
      if (engine == 2)
        actual = remill_simd(lifted_addss, inputs, c.mxcsr);
      else
        require(
            bx_simd_run(engine, code, sizeof(code), inputs, c.mxcsr, &actual),
            "SIMD baseline available");
      const uint64_t expected = 0x1122334400000000ULL | c.bits;
      bool match = actual.xmm0[0] == expected && actual.xmm0[1] == inputs[1] &&
                   actual.xmm1[0] == inputs[2] && actual.xmm1[1] == inputs[3] &&
                   actual.mxcsr == c.after && actual.exit == c.exit;
      observe(c.name, engine, match, actual.xmm0[0], expected, actual.mxcsr,
              c.after, actual.exit);
    }
  }
}

static void vectors() {
  const uint32_t left[8] = {0xffffffff, 0x7fffffff, 10, 0x80000000, 3, 5, 7, 9};
  const uint32_t right[8] = {1, 1, 20, 0x80000000, 4, 6, 8, 10};
  const uint32_t expected[8] = {0, 0x80000000, 30, 0, 7, 11, 15, 19};
  uint64_t inputs[4];
  std::memcpy(inputs, left, 16);
  std::memcpy(inputs + 2, right, 16);
  const uint8_t code[] = {0x66, 0x0f, 0xfe, 0xc1, 0xc3};
  for (unsigned engine = 0; engine < 3; ++engine) {
    SimdResult actual{};
    if (engine == 2)
      actual = remill_simd(lifted_paddd, inputs, 0x1f80);
    else
      require(bx_simd_run(engine, code, sizeof(code), inputs, 0x1f80, &actual),
              "PADD baseline available");
    observe("paddd128", engine,
            std::memcmp(actual.xmm0, expected, 16) == 0 &&
                std::memcmp(actual.xmm1, inputs + 2, 16) == 0 &&
                actual.mxcsr == 0x1f80 && actual.exit == 0,
            actual.xmm0[0], uint64_t(expected[0]) | uint64_t(expected[1]) << 32,
            actual.mxcsr, 0x1f80, actual.exit);
  }
  const uint8_t vex[2][5] = {{0xc5, 0xfd, 0xfe, 0xc1, 0xc3},
                             {0xc5, 0xf9, 0xef, 0xc0, 0xc3}};
  for (unsigned form = 0; form < 2; ++form)
    for (unsigned engine = 0; engine < 2; ++engine) {
      SimdResult actual{};
      require(bx_simd_run(engine, vex[form], 5, inputs, 0x1f80, &actual),
              "VEX rejection baseline available");
      observe("baseline-vex-rejection", engine,
              actual.exit == 5 && actual.mxcsr == 0x1f80 &&
                  std::memcmp(actual.xmm0, inputs, 16) == 0 &&
                  std::memcmp(actual.xmm1, inputs + 2, 16) == 0,
              actual.xmm0[0], inputs[0], actual.mxcsr, 0x1f80, actual.exit);
    }
  const Trace traces[] = {lifted_paddd, lifted_vpaddd, lifted_vpxor};
  const char *names[] = {"legacy-upper-preserved", "vpaddd256",
                         "vpxor128-upper-zero"};
  for (unsigned i = 0; i < 3; ++i) {
    std::memset(&state, 0, sizeof(state));
    std::memcpy(&state.vec[0], left, 32);
    std::memcpy(&state.vec[1], right, 32);
    state.gpr.rsp.qword = 0x3000;
    Memory *memory = bx_memory_new();
    returned = false;
    int status = setjmp(escape);
    if (!status) {
      traces[i](state, 0x1000, memory);
      if (!returned)
        stop(3);
    }
    uint32_t result[8];
    std::memcpy(result, &state.vec[0], 32);
    const uint32_t zero[8] = {};
    bool match = status == 0 && std::memcmp(&state.vec[1], right, 32) == 0;
    if (i == 0)
      match = match && std::memcmp(result, expected, 16) == 0 &&
              std::memcmp(result + 4, left + 4, 16) == 0;
    else
      match = match && std::memcmp(result, i == 1 ? expected : zero, 32) == 0;
    observe(names[i], 2, match, result[0], i == 2 ? 0 : expected[0], 0, 0,
            status);
    bx_memory_free(memory);
  }
}

static void control_flow() {
  const uint8_t code[] = {0x48, 0xff, 0xc0, 0x48, 0xff, 0xc9,
                          0x75, 0xf8, 0x48, 0x89, 0x03, 0xc3};
  for (unsigned count : {1, 3, 7}) {
    const auto actual = run(lifted_loop, 5, count, 0x2000);
    observe("finite-loop", 2,
            actual.exit == 0 && actual.rax == 5 + count &&
                actual.data == 5 + count && actual.flags == 0x44,
            actual.rax, 5 + count, 0, 0, actual.exit);
    for (unsigned engine = 0; engine < 2; ++engine) {
      ResultState baseline{};
      require(bx_run(engine, code, sizeof(code), 5, count, 0x2000, &baseline),
              "loop baseline available");
      observe("finite-loop", engine,
              baseline.exit == 0 && baseline.rax == 5 + count &&
                  baseline.data == 5 + count && baseline.flags == 0x44,
              baseline.rax, 5 + count, 0, 0, baseline.exit);
    }
  }
  for (unsigned engine = 0; engine < 2; ++engine) {
    ResultState actual{};
    require(bx_run(engine, code, sizeof(code), 5, 0, 0x2000, &actual),
            "budget baseline available");
    observe("baseline-step-budget", engine, actual.exit == 4, actual.rax, 0, 0,
            0, actual.exit);
  }
}

static uint64_t remill_batch(Trace trace, Memory *memory,
                             unsigned repetitions) {
  const auto start = std::chrono::steady_clock::now();
  for (unsigned i = 0; i < repetitions; ++i) {
    std::memset(&state, 0, sizeof(state));
    state.gpr.rax.qword = 1;
    state.gpr.rcx.qword = 1;
    state.gpr.rbx.qword = 0x2000;
    state.gpr.rsp.qword = 0x3000;
    require(bx_write(memory, 0x2000, 0), "benchmark reset");
    returned = false;
    int status = setjmp(escape);
    if (!status)
      trace(state, 0x1000, memory);
    uint64_t data = 0, hi = 0, ro = 0;
    require(!status && returned && state.gpr.rax.qword == 257 &&
                state.gpr.rsp.qword == 0x3008 && state.aflag.cf == 0 &&
                state.aflag.pf == 0 && state.aflag.af == 0 &&
                state.aflag.zf == 0 && state.aflag.sf == 0 &&
                state.aflag.of == 0 && bx_read(memory, 0x2000, &data) &&
                data == 257 && bx_read(memory, 0x2008, &hi) && hi == 0 &&
                bx_read(memory, 0x6000, &ro) && ro == 0xa5a5a5a5a5a5a5a5ULL,
            "benchmark Remill correctness");
  }
  return std::chrono::duration_cast<std::chrono::nanoseconds>(
             std::chrono::steady_clock::now() - start)
      .count();
}

static void benchmark() {
  constexpr unsigned repetitions = 200, samples = 9;
  const Trace traces[] = {lifted_register256, lifted_memory256};
  const char *names[] = {"register256", "memory256"};
  for (unsigned work = 0; work < 2; ++work) {
    std::vector<uint8_t> code;
    for (unsigned i = 0; i < 256; ++i) {
      code.insert(code.end(), {0x48, 0x01, 0xc8});
      if (work)
        code.insert(code.end(), {0x48, 0x89, 0x03});
    }
    if (!work) {
      code.insert(code.end(), {0x48, 0x89, 0x03});
    }
    code.push_back(0xc3);
    Bench *benches[3];
    for (unsigned engine = 0; engine < 3; ++engine) {
      benches[engine] = bx_bench_new(engine, code.data(), code.size());
      require(benches[engine], "benchmark context");
      BenchResult warm{};
      require(bx_bench_run(benches[engine], 1, &warm),
              "benchmark warmup correctness");
      std::printf("{\"kind\":\"warmup\",\"workload\":\"%s\",\"engine\":%u,"
                  "\"native_bytes\":%llu,\"compiled\":%llu}\n",
                  names[work], engine, (unsigned long long)warm.native_bytes,
                  (unsigned long long)warm.compiled);
    }
    Memory *memory = bx_memory_new();
    remill_batch(traces[work], memory, 1);
    for (unsigned sample = 0; sample < samples; ++sample)
      for (unsigned turn = 0; turn < 4; ++turn) {
        const unsigned engine = (sample + turn) % 4;
        BenchResult result{};
        if (engine == 3)
          result.ns = remill_batch(traces[work], memory, repetitions);
        else {
          require(bx_bench_run(benches[engine], repetitions, &result),
                  "benchmark baseline correctness");
          if (engine == 1)
            require(result.compiled == 0 && result.cache_hits > 0,
                    "warm DBT must reuse its cache");
        }
        std::printf("{\"kind\":\"sample\",\"workload\":\"%s\",\"engine\":%u,"
                    "\"sample\":%u,\"repetitions\":%u,"
                    "\"ns\":%llu,\"compiled\":%llu,\"cache_hits\":%llu,"
                    "\"fallbacks\":%llu}\n",
                    names[work], engine, sample, repetitions,
                    (unsigned long long)result.ns,
                    (unsigned long long)result.compiled,
                    (unsigned long long)result.cache_hits,
                    (unsigned long long)result.fallbacks);
      }
    bx_memory_free(memory);
    for (auto *bench : benches)
      bx_bench_free(bench);
  }
}

int main(int argc, char **argv) {
  if (argc == 2 && std::strcmp(argv[1], "--benchmark") == 0) {
    benchmark();
    return 0;
  }
  if (argc == 2 && std::strcmp(argv[1], "--runaway") == 0) {
    run(lifted_loop, 5, 0, 0x2000);
    return 9;
  }
  if (argc == 2 && std::strcmp(argv[1], "--wrong-answer") == 0)
    return integer_checks(argc, argv);
  if (argc != 1)
    return 64;
  integer_checks(argc, argv);
  require(std::fegetround() == FE_TONEAREST,
          "probe requires host round-to-nearest");
  floating_point();
  vectors();
  control_flow();
  std::printf("{\"kind\":\"summary\",\"observations\":%u,\"mismatches\":%u}\n",
              observations, mismatches);
  return mismatches ? 1 : 0;
}

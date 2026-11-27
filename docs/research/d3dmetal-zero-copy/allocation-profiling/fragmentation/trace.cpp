// SPDX-License-Identifier: GPL-3.0-or-later
// Original bounded logical span experiment. No graphics API or physical memory
// claims.
#include <algorithm>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <map>
#include <random>
#include <stdexcept>
#include <vector>
using U = std::uint64_t;
static constexpr U capacity = 32ULL * 1024 * 1024;
static U up(U n, U a) { return (n + a - 1) / a * a; }
struct Span {
  U start, size;
};
struct Item {
  U start, reserved, payload, retire = 0;
};
struct Arena {
  std::vector<Span> free{{0, capacity}};
  std::map<U, Item> live, pending;
  U next = 1, completed = 0;
  U allocate(U bytes, U alignment) {
    U size = up(bytes, 65536), id = next++;
    for (std::size_t i = 0; i < free.size(); ++i) {
      auto old = free[i];
      U offset = up(old.start, alignment);
      if (offset < old.start || offset - old.start > old.size ||
          size > old.size - (offset - old.start))
        continue;
      free.erase(free.begin() + i);
      if (offset > old.start)
        free.push_back({old.start, offset - old.start});
      if (offset + size < old.start + old.size)
        free.push_back({offset + size, old.start + old.size - offset - size});
      live[id] = {offset, size, bytes, 0};
      std::printf("{\"op\":\"allocate\",\"id\":%llu,\"payload\":%llu,\"size\":%"
                  "llu,\"alignment\":%llu,\"offset\":%llu}\n",
                  (unsigned long long)id, (unsigned long long)bytes,
                  (unsigned long long)size, (unsigned long long)alignment,
                  (unsigned long long)offset);
      return id;
    }
    std::printf("{\"op\":\"failed_allocate\",\"id\":%llu,\"payload\":%llu,"
                "\"size\":%llu,\"alignment\":%llu}\n",
                (unsigned long long)id, (unsigned long long)bytes,
                (unsigned long long)size, (unsigned long long)alignment);
    return 0;
  }
  void give(U offset, U size) {
    free.push_back({offset, size});
    std::sort(free.begin(), free.end(),
              [](auto a, auto b) { return a.start < b.start; });
    std::vector<Span> merged;
    for (auto s : free) {
      if (!merged.empty() && merged.back().start + merged.back().size > s.start)
        throw std::runtime_error("overlap");
      if (!merged.empty() &&
          merged.back().start + merged.back().size == s.start)
        merged.back().size += s.size;
      else
        merged.push_back(s);
    }
    free = merged;
  }
  void release(U id, U fence) {
    auto it = live.find(id);
    if (it == live.end())
      throw std::runtime_error("unknown id");
    auto item = it->second;
    item.retire = fence;
    live.erase(it);
    pending[id] = item;
    std::printf("{\"op\":\"request_free\",\"id\":%llu,\"fence\":%llu}\n",
                (unsigned long long)id, (unsigned long long)fence);
  }
  void signal(U value) {
    if (value < completed)
      throw std::runtime_error("regressive fence");
    completed = value;
    for (auto it = pending.begin(); it != pending.end();) {
      if (it->second.retire <= completed) {
        give(it->second.start, it->second.reserved);
        it = pending.erase(it);
      } else
        ++it;
    }
    std::printf("{\"op\":\"signal\",\"completed\":%llu}\n",
                (unsigned long long)value);
  }
  void stats(const char *label) {
    U payload = 0, reserved = 0, held = 0, total = 0, largest = 0, usable2M = 0;
    for (auto [id, x] : live) {
      (void)id;
      payload += x.payload;
      reserved += x.reserved;
    }
    for (auto [id, x] : pending) {
      (void)id;
      held += x.reserved;
    }
    for (auto s : free) {
      total += s.size;
      largest = std::max(largest, s.size);
      U start = up(s.start, 2097152);
      if (start < s.start + s.size)
        usable2M = std::max(usable2M, s.start + s.size - start);
    }
    std::printf(
        "{\"op\":\"stats\",\"label\":\"%s\",\"capacity\":%llu,\"live_payload\":"
        "%llu,\"live_reserved\":%llu,\"pending_reserved\":%llu,\"internal_"
        "waste\":%llu,\"total_free\":%llu,\"largest_free\":%llu,\"largest_2MiB_"
        "aligned_span\":%llu,\"free_spans\":%zu}\n",
        label, (unsigned long long)capacity, (unsigned long long)payload,
        (unsigned long long)reserved, (unsigned long long)held,
        (unsigned long long)(reserved - payload), (unsigned long long)total,
        (unsigned long long)largest, (unsigned long long)usable2M, free.size());
  }
};
int main(int argc, char **argv) {
  try {
    U seed = argc == 2 ? std::strtoull(argv[1], nullptr, 10) : 473;
    std::printf("{\"op\":\"begin\",\"protocol\":\"A-logical-fragmentation-1\","
                "\"seed\":%llu,\"capacity\":%llu}\n",
                (unsigned long long)seed, (unsigned long long)capacity);
    Arena a;
    std::vector<U> ids;
    for (int i = 0; i < 8; ++i)
      ids.push_back(a.allocate(4 * 1024 * 1024, 65536));
    for (int i = 0; i < 8; i += 2)
      a.release(ids[i], 1);
    a.signal(1);
    a.stats("alternating_holes");
    if (a.allocate(8 * 1024 * 1024, 65536))
      throw std::runtime_error("fragmented request unexpectedly fit");
    for (int i = 1; i < 8; i += 2)
      a.release(ids[i], 2);
    a.stats("pending_blocks_reuse");
    if (a.allocate(8 * 1024 * 1024, 65536))
      throw std::runtime_error("pending spans reused");
    a.signal(2);
    a.stats("fully_coalesced");
    U large = a.allocate(8 * 1024 * 1024, 2097152);
    if (!large)
      throw std::runtime_error("coalesced allocation failed");
    a.release(large, 3);
    a.signal(3);
    std::mt19937 rng(static_cast<unsigned>(seed));
    const U sizes[] = {4097, 65535, 65537, 131073, 393220, 1048580, 3145732};
    for (U step = 4; step < 2004; ++step) {
      if (!a.live.empty() && (a.live.size() >= 48 || rng() % 100 < 45)) {
        auto it = a.live.begin();
        std::advance(it, rng() % a.live.size());
        a.release(it->first, step + 1 + rng() % 9);
      } else
        a.allocate(sizes[rng() % 7], rng() % 4 ? 65536 : 2097152);
      a.signal(step);
      a.stats("varied_trace");
    }
    ids.clear();
    for (auto [id, item] : a.live) {
      (void)item;
      ids.push_back(id);
    }
    for (auto id : ids)
      a.release(id, 2016);
    a.signal(2016);
    a.stats("final_empty");
    std::puts("{\"op\":\"end\",\"success\":true}");
    return 0;
  } catch (const std::exception &e) {
    std::fprintf(stderr, "failure: %s\n", e.what());
    return 1;
  }
}

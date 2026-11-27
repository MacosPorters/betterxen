#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Recompute the published #473 figures from raw logs (CPU only, standard library).

Parses the archived stdout and receipts directly; it does not import either
workstream's summarizer. The first expected word of every copy sample is re-derived
from probe.cpp; the first and last
expected words of every compute sample are re-derived from probe.cpp and access.hlsl. Writes data/results.json.
Exits non-zero if any check fails.
"""
import collections
import hashlib
import json
import re
import statistics
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2] / "d3dmetal-zero-copy"
R = ROOT / "results"
M4 = "2026-11-26-apple-m4-arths17-"
M5 = "2026-11-27-apple-m5-atesahmet0-"
OUT = {}
FAILED = []
med = statistics.median


def check(name, ok, detail=""):
    if not ok:
        FAILED.append(f"{name}: {detail}")


def fields(line):
    return dict(f.split("=", 1) for f in line.split()[1:] if "=" in f)


def rows(path, kind):
    return [fields(x) for x in path.read_text().splitlines() if x.startswith(kind + " ")]


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


# ---- Copy sweeps ---------------------------------------------------------
def copy_sweeps():
    cells = {}
    count = collections.Counter()
    for run, seed in (("b-sweep-01", 473), ("b-sweep-02", 947)):
        for case in sorted((R / (M5 + run)).glob("*-sweep-*")):
            _, _, heap, mib, stride, s = case.name.split("-")
            assert int(s) == seed
            receipt = json.loads((case / "case.json").read_text())
            count["processes"] += 1
            check("sweep exit", receipt["exit"] == 0 and receipt["backend"]["verified"], case.name)
            samples = rows(case / "stdout.txt", "COPY")
            for r in samples:
                it = int(r["iter"])
                got, total = map(int, r["matches"].split("/"))
                omit, full_read = r["omit"] == "1", r["full_read"] == "1"
                # probe.cpp: whole buffer 0x27; iteration byte 0x60+iter; omit leaves 0xef poison.
                byte = 0x60 + it
                if omit:
                    first = 0xEFEFEFEF
                else:
                    changed = r["full_write"] == "1" or int(r["dirty"]) > 0  # word 0 is order[0]
                    first = (byte if changed else 0x27) * 0x01010101
                ok = got == total and total == (int(r["bytes"]) // 4 if full_read else 3)
                ok = ok and int(r["expected"].split(",")[0], 16) == first
                ok = ok and r["expected"] == r["actual"]
                check("sweep oracle", ok, f"{case.name} iter {it} dirty {r['dirty']}")
                count["measured" if it >= 0 else "warmup"] += 1
            count["cpu_measured"] += sum(int(r["iter"]) >= 0 for r in rows(case / "stdout.txt", "CPU"))
            count["empty_measured"] += sum(int(r["iter"]) >= 0 for r in rows(case / "stdout.txt", "EMPTY"))
            groups = collections.defaultdict(list)
            for r in samples:
                if int(r["iter"]) >= 0:
                    groups[(int(r["dirty"]), int(r["full_write"]), int(r["full_read"]), int(r["omit"]))].append(r)
            pages = int(mib) * 1048576 // int(stride)
            for (dirty, fw, fr, om), g in groups.items():
                if om:
                    continue
                label = ("full" if fw else {0: "none", 1: "one", 64: "p64", pages: "all"}[dirty])
                values = [float(x["completion_us"]) for x in g]
                cells[f"{seed}|{heap}|{mib}|{stride}|{'fullcopy' if fr else 'small'}|{label}"] = {
                    "n": len(values), "median": med(values), "min": min(values), "max": max(values),
                    "values": values, "cpu_write_median": med(float(x["cpu_write_us"]) for x in g)}
    OUT["copy_counts"] = dict(count)
    OUT["copy_cells"] = cells
    wb = [cells[f"{s}|wb|64|{st}|small|full"]["median"] - cells[f"{s}|wb|64|{st}|small|none"]["median"]
          for s in (473, 947) for st in (4096, 16384)]
    small = [v["median"] for k, v in cells.items() if k.split("|")[2] == "64" and "|small|" in k and k.endswith("|none")]
    full = [v["median"] for k, v in cells.items() if k.split("|")[2] == "64" and "|fullcopy|" in k and k.endswith("|none")]
    OUT["copy_headlines"] = {"wb64_full_rewrite_minus_none_us": wb,
                             "clean_small_copy_64MiB_median_range_us": [min(small), max(small)],
                             "clean_full_copy_64MiB_median_range_us": [min(full), max(full)]}


# ---- Direct shader access ------------------------------------------------
def input_word(index, seed):  # gpu-correctness/coherence-and-tracking/probe.cpp
    return ((index ^ ((seed * 0x01010301) & 0xFFFFFFFF)) + index * 17) & 0xFFFFFFFF


def expected_word(mode, j, seed):  # access.hlsl
    words = 1 << 20
    outputs, stride, reads = {"both": (words, 1, 1), "full_read": (words // 16, 1, 16),
                              "sparse_read_4k": (words // 1024, 1024, 1),
                              "sparse_read_16k": (words // 4096, 4096, 1),
                              "write": (words, 1, 0), "omitted": (words, 1, 1)}[mode]
    if j < 0:
        j = outputs - 1
    if mode == "omitted":
        return 0x6BA8FA03
    total = j * 37 + seed + sum(input_word(stride * (j + n * outputs), seed) for n in range(reads))
    return (total & 0xFFFFFFFF) ^ 0xA5C35A3C


def compute():
    cells, count = {}, collections.Counter()
    for run, seed in (("b-compute-01", 473), ("b-compute-02", 947)):
        for case in sorted((R / (M5 + run)).glob("0*-*")):
            name = case.name[3:]
            receipt = json.loads((case / "case.json").read_text())
            check("compute exit", receipt["exit"] == 0 and receipt["backend"]["verified"], f"{run}/{name}")
            groups = collections.defaultdict(list)
            for r in rows(case / "stdout.txt", "COMPUTE"):
                got, total = map(int, r["matches"].split("/"))
                check("compute oracle", got == total and r["expected"] == r["actual"], f"{run}/{name} {r['mode']}")
                first, last = (int(x, 16) for x in r["expected"].split(","))
                check("compute derivation", (first, last) == (expected_word(r["mode"], 0, int(r["seed"])),
                      expected_word(r["mode"], -1, int(r["seed"]))), f"{run}/{name} {r['mode']}")
                count["derived"] += 1
                count["measured" if int(r["iter"]) >= 0 else "warmup"] += 1
                if int(r["iter"]) >= 0:
                    groups[r["mode"]].append(r)
            for mode, g in groups.items():
                values = [float(x["completion_us"]) for x in g]
                cells[f"{seed}|{name}|{mode}"] = {"n": len(values), "median": med(values), "min": min(values),
                                                  "max": max(values), "values": values,
                                                  "read_bytes": int(g[0]["read_bytes"]),
                                                  "written_bytes": int(g[0]["written_bytes"])}
    OUT["compute_counts"] = dict(count)
    OUT["compute_cells"] = cells


# ---- Fence-interval rates and empty submissions ----------------------------
def rates():
    cpu, copy, empty = collections.defaultdict(list), collections.defaultdict(list), []
    for run in ("b-sweep-01", "b-sweep-02"):
        for case in sorted((R / (M5 + run)).glob("*-sweep-*")):
            mib = int(case.name.split("-")[3])
            path = case / "stdout.txt"
            c = [r for r in rows(path, "CPU") if int(r["iter"]) >= 0]
            for key, field in (("read", "read_us"), ("write", "write_us")):
                cpu[(mib, key)].append(med(int(r["bytes"]) / float(r[field]) / 1000 for r in c))
            g = [r for r in rows(path, "COPY") if int(r["iter"]) >= 0 and r["full_read"] == "1"
                 and r["dirty"] == "0" and r["omit"] == "0" and r["full_write"] == "0"]
            copy[mib].append(med(int(r["gpu_payload"]) / float(r["completion_us"]) / 1000 for r in g))
            empty.append(med(float(r["completion_us"]) for r in rows(path, "EMPTY") if int(r["iter"]) >= 0))
    OUT["fence_rates_GB_s"] = {str(m): {"cpu_read": [min(cpu[(m, "read")]), max(cpu[(m, "read")])],
                                        "cpu_write": [min(cpu[(m, "write")]), max(cpu[(m, "write")])],
                                        "gpu_copy_fence": [min(copy[m]), max(copy[m])]} for m in (1, 16, 64)}
    OUT["empty_total_median_range_us"] = [min(empty), max(empty)]


# ---- Metal System Trace intervals ----------------------------------------
TRACES = {"wb": "b-trace-02", "wc": "b-metal-wc", "upload-wb": "b-metal-upload-wb", "default": "b-metal-default",
          "copy-wb": "b-metal-copy-wb", "copy-wc": "b-metal-copy-wc", "copy-upload": "b-metal-copy-upload-retry"}


def traces():
    out, intervals = {}, collections.Counter()
    for name, run in TRACES.items():
        data = json.loads((R / (M5 + run) / "gpu-samples.json").read_text())
        intervals["copy" if name.startswith("copy") else "shader"] += data["matched_intervals"]
        out[name] = data["distributions"]
        replay = R / (M4 + "final-trace-replay") / f"{run}.stdout.json"
        check("trace replay", json.loads(replay.read_text()) == data, name)
    OUT["trace_distributions"] = out
    OUT["trace_interval_counts"] = dict(intervals)


# ---- Frozen six-check baseline -------------------------------------------
def baseline():
    out = {}
    for label, run in (("M4 session 1", M4 + "initial"), ("M4 session 2", M4 + "runner"),
                       ("M5", M5 + "b-baseline-runner-01")):
        text = (R / run / "stdout.txt").read_text()
        bench = re.findall(r"BENCH_RESULT (\S+) .*?avg_ms_per_submit=([\d.]+) exact_matches=(\d+/\d+) warmup_matches=(\d+/\d+)", text)
        check("baseline matches", len(bench) == 4 and all(b[2] == "40/40" and b[3] == "5/5" for b in bench), run)
        check("baseline E_NOTIMPL", text.count("HRESULT=0x80004001") >= 2, run)
        out[label] = {"avg_ms_per_submit": {b[0]: float(b[1]) for b in bench},
                      "uma_line": re.search(r"UMA=.*", text).group(0)}
    OUT["baseline"] = out


# ---- Memory accounting ---------------------------------------------------
def memory():
    cases = []
    for run in ("a2-memory-partial", "a22-memory-pressure-stop"):
        receipt = json.loads((R / (M4 + run) / "receipt.json").read_text())
        for c in receipt["cases"]:
            phases = {}
            for snap in c.get("snapshots", []):
                data = [s["data"] for s in snap.get("samples", []) if "data" in s]
                if data:
                    phases[snap["phase"]] = {k: med(d[k] for d in data) for k in ("phys_footprint", "resident_size")}
            cases.append({"run": run, "name": c["name"], "exit": c.get("exit"),
                          "stop_reason": c.get("stop_reason"), "phases": phases})
    OUT["memory_cases"] = cases


# ---- Pooling -------------------------------------------------------------
def pooling():
    out = {}
    for case in sorted((R / (M4 + "a22-pool-corrected")).glob("pool-*")):
        _, mode, session = case.name.split("-")
        path = case / "stdout.txt"
        seed = rows(path, "TRACE")[0]["seed"]
        timed = [r for r in rows(path, "ALLOC") if r["warmup"] == "0"]
        setup = rows(path, "POOL_SETUP")
        out[f"{mode}|{seed}"] = {
            "allocations": len(timed),
            "heap_or_suballoc_ms": med(float(r["heap_or_suballoc_ms"]) for r in timed),
            "placement_ms": med(float(r["placement_ms"]) for r in timed),
            "cycle_ms": med(float(r["ms"]) for r in rows(path, "CYCLE") if r["warmup"] == "0"),
            "pool_setup_ms": float(setup[0]["heap_ms"]) if setup else None}
    OUT["pooling"] = out


# ---- Fragmentation -------------------------------------------------------
def fragmentation():
    out = {}
    for seed in (473, 474, 475):
        path = R / (M4 + "logical-fragmentation") / f"seed-{seed}-stdout.jsonl"
        lines = [json.loads(x) for x in path.read_text().splitlines()]
        stats = [x for x in lines if x["op"] == "stats"]
        for s in stats:
            check("fragmentation conservation",
                  s["total_free"] + s["live_reserved"] + s["pending_reserved"] == s["capacity"]
                  and s["largest_free"] <= s["total_free"], f"{seed} {s['label']}")
        oracle = json.loads((path.parent / f"seed-{seed}-oracle.json").read_text())
        out[str(seed)] = {"snapshots": len(stats), "failed_requests": oracle["failed_allocation_requests"],
                          "labels": {s["label"]: s for s in stats
                                     if s["label"] in ("alternating_holes", "pending_blocks_reuse", "fully_coalesced")}}
    OUT["fragmentation"] = out
    OUT["fragmentation_snapshots_total"] = sum(v["snapshots"] for v in out.values())


# ---- Functional logs -----------------------------------------------------
def functional():
    out = {"texture": {}, "alias": {}, "render": {}, "watch_phases": 0, "pointer_mod64k": {}}
    for chip, prefix, tex, ali in (("M4", M4, "a22-texture-matrix", "a22-buffer-inheritance"),
                                   ("M5", M5, "b-a-texture-01", "b-a-alias-01")):
        for path in sorted((R / (prefix + tex)).glob("*/stdout.txt")):
            lines = [x for x in path.read_text().splitlines() if x.startswith("OUTCOME")]
            out["texture"][f"{chip}|{path.parent.name}"] = lines
        for path in sorted((R / (prefix + ali)).glob("*/stdout.txt")):
            r = rows(path, "BUFFER_INHERIT")
            check("alias", bool(r) and all(x[k] == "16384/16384" for x in r for k in
                  ("source_matches", "inherited_matches", "disjoint_guard_matches")), str(path.parent))
            out["alias"][f"{chip}|{path.parent.name}"] = len(r)
    for run in (M4 + "a23-render-fixed-smoke", M5 + "b-a-render-01", M5 + "b-a-render-omit-01"):
        for path in sorted((R / run).glob("*/stdout.txt")):
            text = path.read_text()
            out["render"][f"{run}|{path.parent.name}"] = {
                "verified": re.findall(r"matches=(\d+/\d+)", text),
                "completed": "END success" in text,
                "last_stage": re.findall(r"STAGE (\w+)", text)[-1]}
    for run in ("b-small-02", "b-small-03", "b-small-05"):
        for path in (R / (M5 + run)).glob("*watch*/stdout.txt"):
            for line in path.read_text().splitlines():
                if line.startswith("WATCH phase="):
                    m = re.search(r"actual=(\S*) expected=(\S*)", line)
                    check("watch", m.group(1) == m.group(2), f"{run} {line[:60]}")
                    out["watch_phases"] += 1
    OUT["functional"] = out


# ---- Output hashes recorded by Workstream B ------------------------------
def b_output_hashes():
    total, bad = 0, []
    for case_file in R.glob(M5 + "b-*/*/case.json"):
        receipt = json.loads(case_file.read_text())
        for name, digest in receipt.get("output_hashes", {}).items():
            target = case_file.parent / name
            if target.is_file():
                total += 1
                if sha(target) != digest:
                    bad.append(str(target.relative_to(R)))
    OUT["b_output_hashes"] = {"checked": total, "mismatched": sorted(bad)}


# ---- Source snapshots and archive manifests ------------------------------
def provenance():
    n = files = 0
    bad = []
    for receipt in sorted(R.glob(M5 + "*/receipt.json")):
        for name, digest in json.loads(receipt.read_text()).get("sources", {}).items():
            snap = receipt.parent / "source" / name
            if isinstance(digest, str) and snap.is_file():
                n += 1
                if sha(snap) != digest:
                    bad.append(str(snap.relative_to(R)))
    for manifest in R.glob("*/archive-manifest.json"):
        for e in json.loads(manifest.read_text())["files"]:
            files += 1
            if sha(manifest.parent / e["path"]) != e["archived_sha256"]:
                bad.append(f"{manifest.parent.name}/{e['path']}")
    check("provenance", not bad, str(bad[:5]))
    OUT["provenance"] = {"b_source_snapshots_checked": n, "a_manifest_files_checked": files, "mismatched": bad}


def main():
    copy_sweeps()
    compute()
    traces()
    rates()
    baseline()
    memory()
    pooling()
    fragmentation()
    functional()
    b_output_hashes()
    provenance()
    OUT["failed_checks"] = FAILED
    (HERE.parent / "data" / "results.json").write_text(json.dumps(OUT, indent=1, sort_keys=True) + "\n")
    print("copy", OUT["copy_counts"], OUT["copy_headlines"])
    print("compute", OUT["compute_counts"])
    print("traces", OUT["trace_interval_counts"])
    print("b hashes", OUT["b_output_hashes"])
    for k, v in sorted(OUT["pooling"].items()):
        print("pool", k, v)
    print("fragmentation", OUT["fragmentation_snapshots_total"], {k: v["failed_requests"] for k, v in OUT["fragmentation"].items()})
    f = OUT["functional"]
    print("watch", f["watch_phases"])
    for k, v in f["texture"].items():
        print(k, len(v), v[:1])
    print(f["alias"]); print(json.dumps(f["render"]))
    for c in OUT["memory_cases"]:
        p = c["phases"]; b = p.get("baseline", {}).get("phys_footprint")
        print(c["run"][:3], c["name"], c["exit"], b and round(b/2**20,2), {n: round((p[n]["phys_footprint"]-b)/2**20,2) for n in ("fully_initialized","gpu_complete","retired_settled") if n in p and b})
    print("rates", OUT["fence_rates_GB_s"], OUT["empty_total_median_range_us"])
    for k, v in sorted(OUT["compute_cells"].items()):
        print("cc", k, round(v["median"], 1))
    for n, d in OUT["trace_distributions"].items():
        print("tr", n, {k: (v["median_gpu_us"], v["mad_gpu_us"], {a: round(b, 2) for a, b in v["payload_GB_s"].items()}) for k, v in d.items()})
    print("provenance", OUT["provenance"])
    print("failed checks:", len(FAILED), FAILED[:10])
    return 1 if FAILED else 0


if __name__ == "__main__":
    sys.exit(main())

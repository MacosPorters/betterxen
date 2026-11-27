#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Draw Research Report #001 figures (SVG and PNG) with matplotlib.

Quantitative figures read only data/results.json (written by recompute.py) and
the #473 evidence register. Diagrams are conceptual and labelled as such.
Figures use an opaque white background so they read on light and dark pages.
"""
import json
import textwrap
from pathlib import Path

import matplotlib

matplotlib.use("Agg")
import matplotlib.pyplot as plt  # noqa: E402
from matplotlib.patches import FancyArrowPatch, FancyBboxPatch, Rectangle  # noqa: E402

HERE = Path(__file__).resolve().parent
PUB = HERE.parent
ASSETS = PUB / "assets"
D = json.loads((PUB / "data" / "results.json").read_text())
REG = json.loads((PUB.parents[1] / "d3dmetal-zero-copy" / "allocation-profiling" /
                  "consolidation-draft" / "evidence-register.json").read_text())["findings"]

INK, MUTED, LINE, PAPER = "#1b1f24", "#57606a", "#d0d7de", "#ffffff"
# Okabe-Ito colour-blind-safe hues; shape or text always repeats the meaning.
BLUE, ORANGE, GREEN, RED, PURPLE, SKY = "#0072B2", "#D55E00", "#007A5E", "#B3261E", "#8E4A86", "#2B7FB8"
TINT = {BLUE: "#e7f1f8", ORANGE: "#fbeee6", GREEN: "#e5f3ef", RED: "#fbeae9", PURPLE: "#f4ebf3",
        MUTED: "#f6f8fa", SKY: "#e9f3fa"}

plt.rcParams.update({"font.family": "DejaVu Sans", "font.size": 11, "text.color": INK,
                     "axes.labelcolor": INK, "xtick.color": INK, "ytick.color": INK,
                     "axes.edgecolor": LINE, "axes.titleweight": "bold", "axes.titlesize": 12,
                     "svg.fonttype": "path", "svg.hashsalt": "betterxen-473", "figure.facecolor": PAPER,
                     "savefig.facecolor": PAPER})


def save(fig, name, caption):
    fig.text(0.012, 0.004, "\n".join(textwrap.wrap(caption, int(fig.get_figwidth() * 13.5))),
             fontsize=9, color=MUTED, va="bottom", ha="left")
    for ext in ("svg", "png"):
        meta = {"Date": None} if ext == "svg" else {"Software": None}
        fig.savefig(ASSETS / f"{name}.{ext}", dpi=170, bbox_inches="tight", pad_inches=0.18, metadata=meta)
    svg = ASSETS / f"{name}.svg"
    svg.write_text("".join(line.rstrip() + "\n" for line in svg.read_text().splitlines()))
    plt.close(fig)


def canvas(title, w=10, h=6, subtitle=""):
    fig = plt.figure(figsize=(w, h))
    ax = fig.add_axes([0, 0.07, 1, 0.84])
    ax.set_xlim(0, 100)
    ax.set_ylim(0, 100)
    ax.axis("off")
    fig.text(0.012, 0.975, title, fontsize=15, weight="bold", va="top")
    if subtitle:
        fig.text(0.012, 0.925, subtitle, fontsize=10.5, color=MUTED, va="top")
    return fig, ax


def box(ax, x, y, w, h, head, body="", color=MUTED, dashed=False, size=10.5):
    ax.add_patch(FancyBboxPatch((x, y), w, h, boxstyle="round,pad=0,rounding_size=1.2", linewidth=1.5,
                                edgecolor=color, facecolor=TINT.get(color, "#f6f8fa"),
                                linestyle="--" if dashed else "-"))
    if body:
        ax.text(x + w / 2, y + h - 2.4, head, ha="center", va="top", fontsize=size, weight="bold")
        ax.text(x + w / 2, y + (h - 6) / 2, body, ha="center", va="center", fontsize=size - 1.5, linespacing=1.45)
    else:
        ax.text(x + w / 2, y + h / 2, head, ha="center", va="center", fontsize=size, weight="bold",
                linespacing=1.4)


def arrow(ax, a, b, label="", color=INK, dashed=False, dx=0, dy=1.8):
    ax.add_patch(FancyArrowPatch(a, b, arrowstyle="-|>", mutation_scale=13, linewidth=1.4, color=color,
                                 linestyle="--" if dashed else "-"))
    if label:
        ax.text((a[0] + b[0]) / 2 + dx, (a[1] + b[1]) / 2 + dy, label, ha="center", fontsize=9, color=MUTED)


def style(ax):
    ax.spines[["top", "right"]].set_visible(False)
    ax.grid(axis="y", color=LINE, linewidth=0.6)
    ax.set_axisbelow(True)


def overview():
    acc = sum(f["acceptance_status"] == "Independently accepted" for f in REG)
    tiles = [("2", "collaborating workstreams", "A: memory and resources\nB: copying and coherence", BLUE),
             ("M4 + M5", "Apple Silicon hosts", "24 GiB and 16 GiB\ndifferent macOS versions", SKY),
             (str(len(REG)), "register entries", "each with its own status", PURPLE),
             (str(acc), "independently accepted", "each only within a\nstated scope", GREEN),
             (f"{D['fragmentation_snapshots_total']:,}", "logical snapshots checked", "CPU model, no GPU,\nno real allocation", MUTED),
             (f"{D['copy_counts']['measured']:,}", "measured copy comparisons", "M5 only, 2 sessions\nincludes omitted-copy controls", ORANGE),
             (f"{D['compute_counts']['measured']:,}", "measured compute comparisons", "M5 only, 2 sessions\nincludes omitted controls", ORANGE),
             ("7", "trace extractions replayed", "same sanitized XML;\nno new GPU capture", BLUE)]
    fig, ax = canvas("Research at a glance (#473)", 10, 5.6,
                     "These are different kinds of evidence. They do not add up to a test total.")
    for i, (num, head, body, col) in enumerate(tiles):
        x, y = 1 + (i % 4) * 25, 50 if i < 4 else 3
        box(ax, x, y, 23, 41, "", color=col)
        ax.text(x + 11.5, y + 30, num, ha="center", va="center", fontsize=22, weight="bold", color=col)
        ax.text(x + 11.5, y + 19, "\n".join(textwrap.wrap(head, 22)), ha="center", va="center", fontsize=10, weight="bold")
        ax.text(x + 11.5, y + 7.5, body, ha="center", va="center", fontsize=8.5, color=MUTED, linespacing=1.4)
    save(fig, "01-overview", "Counts recomputed from raw logs and the evidence register at f8810a48. Acceptance "
         "decisions were made by AI research agents acting for the two contributors.")


def unified():
    fig, ax = canvas("Unified memory is hardware. Copying is a software decision.", 10, 6,
                     "Conceptual. Which of these copies D3DMetal makes internally is unknown.")
    box(ax, 12, 72, 28, 15, "CPU cores", "x86-64 game code\nruns through Rosetta 2", BLUE)
    box(ax, 60, 72, 28, 15, "GPU", "D3D12 calls go through\nD3DMetal to Metal", PURPLE)
    box(ax, 6, 40, 88, 20, "", color=MUTED)
    ax.text(50, 56, "One physical memory pool", ha="center", weight="bold")
    box(ax, 10, 42.5, 24, 10, "Allocation A", color=BLUE)
    box(ax, 38, 42.5, 24, 10, "Staging copy?", color=ORANGE, dashed=True)
    box(ax, 66, 42.5, 24, 10, "GPU resource?", color=ORANGE, dashed=True)
    arrow(ax, (26, 72), (22, 53))
    arrow(ax, (74, 72), (78, 53))
    arrow(ax, (34.5, 47.5), (37.5, 47.5), color=ORANGE, dashed=True)
    arrow(ax, (62.5, 47.5), (65.5, 47.5), color=ORANGE, dashed=True)
    box(ax, 6, 6, 42, 24, "What a software layer can still add",
        "a second allocation\nstaging buffers and transfers\nlayout conversion for textures\nsynchronization waits", ORANGE)
    box(ax, 52, 6, 42, 24, "What the hardware does not promise",
        "that your pointer can be wrapped\nthat no copy happens\nthat CPU and GPU may race safely\nthat two views share a page", RED)
    save(fig, "02-unified-memory", "Dashed boxes are possibilities, not observations.")


def architecture():
    fig, ax = canvas("Today and a research-supported candidate", 11, 7,
                     "Left: main at f07a24c2. Right: NOT implemented on main.")
    ax.plot([50, 50], [2, 96], color=LINE, linewidth=1.2)
    ax.text(25, 96, "Current: separate domains", ha="center", weight="bold", fontsize=12)
    ax.text(75, 96, "Candidate for eligible buffers", ha="center", weight="bold", fontsize=12)
    box(ax, 4, 76, 42, 14, "xmem.dll", "VirtualAlloc and file-mapping sections\nCPU pointer owned by xmem", BLUE)
    box(ax, 4, 44, 42, 16, "d3d12_xs.dll", "forwards to the host D3D12\nConfigureVirtualMemoryX is a\nlogged no-op that returns S_OK", PURPLE)
    box(ax, 4, 12, 42, 14, "D3D12 / D3DMetal resource", "filled by whatever transfers\nthe application records", MUTED)
    arrow(ax, (25, 76), (25, 61), "no shared backing", dx=12, dy=0)
    arrow(ax, (25, 44), (25, 27), "explicit copy where needed", dx=14, dy=0)
    box(ax, 54, 76, 42, 14, "D3D12-owned heap", "CUSTOM L0, persistently mapped\nbounded size", PURPLE, True)
    box(ax, 54, 50, 42, 16, "xmem hands out slices", "only for allocations whose address,\nalignment and lifetime fit\nthe returned Map pointer", BLUE, True)
    box(ax, 54, 28, 20, 14, "CPU", "writes through\nthe pointer", BLUE, True)
    box(ax, 76, 28, 20, 14, "GPU", "uses a placed\nbuffer", PURPLE, True)
    box(ax, 54, 4, 42, 15, "Everything else", "keeps the CPU or section path\nwith explicit, synchronized transfer", MUTED)
    arrow(ax, (75, 76), (75, 67), dashed=True)
    arrow(ax, (66, 50), (64, 43), dashed=True)
    arrow(ax, (84, 50), (86, 43), dashed=True)
    save(fig, "03-architecture", "Dashed outlines are proposals. A mapped pointer being usable is not proof that it names "
         "the same physical pages the GPU uses. PR #486 (unmerged) implements part of a related design.")


def decision():
    fig, ax = canvas("Which allocations can use a mapped D3D12 buffer?", 11, 8,
                     "Research-supported guidance, not implemented routing. Validate every returned pointer.")
    box(ax, 28, 89, 44, 8, "What is being allocated?", color=INK)
    rows = [("Texture (UNKNOWN layout)", "No raw pointer. Per-mip subresource calls or upload/readback.", "Mip transfers verified", ORANGE),
            ("Render target", "DEFAULT or custom heap; initialize after aliasing.", "Clear and readback verified", ORANGE),
            ("Depth / stencil", "Same, with depth states. Sampling matrix incomplete.", "Clear verified; draw tests partial", ORANGE),
            ("Existing CPU memory with contents", "Cannot be wrapped (E_NOTIMPL). Keep it; copy to the GPU.", "Unsupported import", RED),
            ("Fixed or requested address", "Map chooses its own address. Keep the CPU path.", "Not satisfiable as tested", RED),
            ("Allocated before a device exists", "No heap to take it from. Keep the CPU path.", "By construction", RED),
            ("Section-backed or scatter mapping", "Keep the section engine; transfer at the boundary.", "Buffer aliasing is not equivalent", RED),
            ("New graphics buffer, known lifetime,\nalignment checked on the real pointer", "CANDIDATE: persistent Map, fenced reuse.", "M4/M5 functional evidence", GREEN)]
    for i, (kind, route, evid, col) in enumerate(rows):
        y = 76 - i * 10.8
        box(ax, 3, y, 31, 9, kind, color=col, size=9.5)
        box(ax, 39, y, 40, 9, "\n".join(textwrap.wrap(route, 40)), color=MUTED, size=9)
        ax.text(81, y + 4.5, "\n".join(textwrap.wrap(evid, 20)), va="center", fontsize=8.8, color=col, weight="bold")
        arrow(ax, (34.5, y + 4.5), (38.5, y + 4.5))
        ax.plot([1, 3], [y + 4.5, y + 4.5], color=LINE)
    ax.plot([1, 1, 28], [4.9, 93, 93], color=LINE)
    save(fig, "04-ownership", "Evidence labels refer to register entries BASE-2, A-5 to A-12 and B-5.")


def hosts():
    rows = [("Chip / RAM", "Apple M4 / 24 GiB", "Apple M5 / 16 GiB"),
            ("macOS", "27.0 (26A428)", "27.2.1 (26B131)"),
            ("CrossOver / D3DMetal", "26.3.0.39832 / 3.0", "26.3.0.39832 / 3.0"),
            ("Rosetta package", "1.0.0.0.1788432274", "1.0.0.0.1793943009"),
            ("Memory pressure", "warning; urgent stops", "warning; urgent stops"),
            ("Five CUSTOM L0 heaps / L1", "created / rejected", "created / rejected"),
            ("Import existing memory", "E_NOTIMPL (both calls)", "E_NOTIMPL (both calls)"),
            ("Fenced persistent Map copies", "matched", "matched"),
            ("Buffer alias inheritance", "matched, 2 sessions", "same source, 1 session"),
            ("Mip transfers, RT/depth clears", "valid cases matched", "valid cases matched"),
            ("Invalid placed buffers", "accepted (gate fails)", "accepted (gate fails)"),
            ("CPU pointer mod 64 KiB", "16384 or 49152 (sampled)", "0, 16384 or 32768"),
            ("Sampling/depth/stencil", "1 smoke dataset", "1 dataset + control;\nsecond stopped at stencil")]
    fig, ax = canvas("Observations on two hosts (not a chip comparison)", 10, 7.4,
                     "OS, Rosetta, pressure, launch protocol and builds differ.")
    for x, head in ((2, "Observation"), (40, "M4 (Arths17 host)"), (70, "M5 (atesahmet0 host)")):
        ax.text(x, 97, head, weight="bold", va="center")
    for i, row in enumerate(rows):
        y = 90 - i * 7.2
        if i % 2 == 0:
            ax.add_patch(Rectangle((1, y - 3.6), 98, 7.2, color="#f6f8fa"))
        for x, cell in zip((2, 40, 70), row):
            ax.text(x, y, cell, va="center", fontsize=9.8, linespacing=1.2)
    save(fig, "05-hosts", "Sources: baseline stdout (the M5 non-timing lines equal the M4 lines), alias, texture, "
         "render and placement logs.")


def copy_latency():
    fig, axes = plt.subplots(2, 2, figsize=(11, 8), sharey=True)
    fig.subplots_adjust(left=0.08, right=0.99, top=0.87, bottom=0.13, hspace=0.42, wspace=0.07)
    labels = [("small", "none", "no\nwrites"), ("small", "one", "1\nword"), ("small", "p64", "64\nwords"),
              ("small", "all", "1 word\nper page"), ("small", "full", "rewrite\nevery byte"),
              ("fullcopy", "none", "FULL COPY\nno writes")]
    for row, stride in enumerate((4096, 16384)):
        for col, seed in enumerate((473, 947)):
            ax = axes[row][col]
            for i, (kind, case, _) in enumerate(labels):
                cell = D["copy_cells"][f"{seed}|wb|64|{stride}|{kind}|{case}"]
                color = ORANGE if kind == "fullcopy" else BLUE
                ax.scatter([i + (j - 5.5) * 0.025 for j in range(cell["n"])], cell["values"], s=12,
                           color=color, alpha=0.55, linewidths=0)
                ax.hlines(cell["median"], i - 0.3, i + 0.3, color=INK, linewidth=2)
                ax.annotate(f"{cell['median']:.0f}", (i, cell["median"]), xytext=(18, 3),
                            textcoords="offset points", fontsize=8.5, ha="center")
            ax.set_yscale("log")
            ax.set_ylim(100, 15000)
            ax.set_xticks(range(len(labels)), [x[2] for x in labels], fontsize=8)
            ax.set_title(f"stride {stride // 1024} KiB, seed {seed}")
            style(ax)
        axes[row][0].set_ylabel("submit to fence, µs (log)")
    fig.suptitle("64 MiB write-back buffer: 12-byte copy vs explicit full copy (M5)", x=0.012, ha="left",
                 weight="bold", fontsize=14)
    save(fig, "06a-copy-latency", "12 measured iterations per column (3 warmups excluded); every sample and outlier "
         "shown; bar = median. End-to-end host time, not GPU time. 'No writes' means no new CPU stores before that submit.")


def trace_rates():
    t = D["trace_distributions"]
    fig, axes = plt.subplots(1, 3, figsize=(12, 5.2))
    fig.subplots_adjust(left=0.06, right=0.995, top=0.8, bottom=0.27, wspace=0.28)
    names = [("wb", "WB→WB"), ("wc", "WC→WC"), ("upload-wb", "UPLOAD\n→WB"), ("default", "DEFAULT\n→DEFAULT")]
    panels = [(axes[0], "Shader: full read (4 MiB)", "full_read", names),
              (axes[1], "Shader: store 4 MiB", "write", names),
              (axes[2], "Copy 64 MiB to READBACK", "copy", [("copy-wb", "WB"), ("copy-wc", "WC"), ("copy-upload", "UPLOAD")])]
    for ax, title, mode, items in panels:
        for i, (key, _) in enumerate(items):
            d = t[key][mode]
            ax.vlines(i, d["minimum_gpu_us"], d["maximum_gpu_us"], color=MUTED, linewidth=1.5)
            ax.scatter([i, i], [d["minimum_gpu_us"], d["maximum_gpu_us"]], marker="_", s=120, color=MUTED)
            ax.scatter(i, d["median_gpu_us"], s=55, color=BLUE, zorder=3)
            rates = d["payload_GB_s"]
            rate = rates["workload_payload"] if mode == "copy" else rates["read_bytes" if mode == "full_read" else "written_bytes"]
            ax.annotate(f"{d['median_gpu_us']:.0f} µs\n{rate:.0f} GB/s", (i, d["median_gpu_us"]), xytext=(8, 0),
                        textcoords="offset points", fontsize=8.3, va="center")
        ax.set_yscale("log")
        ax.set_xlim(-0.4, len(items) - 0.3)
        ax.set_xticks(range(len(items)), [x[1] for x in items], fontsize=8.5)
        ax.set_title(title)
        style(ax)
    axes[0].set_ylabel("GPU encoder duration, µs (log)")
    fig.suptitle("Metal System Trace intervals (M5): median and full min–max range", x=0.012, ha="left",
                 weight="bold", fontsize=14)
    save(fig, "06b-gpu-intervals", "n = 12 measured per point, one profiled capture per case, pressured host. GB/s = useful "
         "payload / duration (decimal); it is not physical memory bandwidth and does not rank heap types. DEFAULT "
         "transfers happen outside these intervals.")


def compute_latency():
    cells = D["compute_cells"]
    fig, axes = plt.subplots(1, 2, figsize=(11, 4.8), sharey=True)
    fig.subplots_adjust(left=0.07, right=0.995, top=0.8, bottom=0.2, wspace=0.05)
    modes = [("full_read", "full\nread"), ("write", "store\nonly"), ("sparse_read_4k", "sparse\n4 KiB"),
             ("sparse_read_16k", "sparse\n16 KiB"), ("omitted", "omitted\ndispatch")]
    heaps = [("wb", "WB", BLUE, "o"), ("wc", "WC", ORANGE, "s"), ("upload-wb", "UPLOAD→WB", GREEN, "^"),
             ("default", "DEFAULT (staged)", PURPLE, "D")]
    for ax, seed in zip(axes, (473, 947)):
        for h, (heap, label, color, marker) in enumerate(heaps):
            xs = [i + (h - 1.5) * 0.18 for i in range(len(modes))]
            data = [cells[f"{seed}|{heap}|{m}"] for m, _ in modes]
            ax.vlines(xs, [d["min"] for d in data], [d["max"] for d in data], color=color, alpha=0.4)
            ax.scatter(xs, [d["median"] for d in data], color=color, marker=marker, label=label, s=34, zorder=3)
        ax.set_xticks(range(len(modes)), [m[1] for m in modes], fontsize=9)
        ax.set_yscale("log")
        ax.set_ylim(30, 6000)
        ax.set_title(f"session seed {seed}")
        style(ax)
    axes[0].set_ylabel("submit to fence, µs (log)")
    axes[1].legend(fontsize=8.5, frameon=False, ncol=2)
    fig.suptitle("Shader access, end-to-end completion (M5): median and min–max", x=0.012, ha="left",
                 weight="bold", fontsize=14)
    save(fig, "06c-compute-latency", "12 measured iterations per point. DEFAULT includes a dependent readback copy. "
         "Sparse and omitted cases are dominated by submission and fence latency.")


PHASES = [("baseline", "baseline"), ("created", "created"), ("mapped", "mapped"), ("fully_initialized", "CPU\nfilled"),
          ("gpu_complete", "GPU\nconsumed"), ("resources_released", "resource\nreleased"),
          ("commands_retired", "commands\nretired"), ("retired_settled", "settled"), ("device_released", "device\nreleased")]


def footprint():
    cases = {c["name"]: c for c in D["memory_cases"] if c["run"] == "a2-memory-partial"}
    fig, axes = plt.subplots(2, 1, figsize=(10.5, 8.2), sharex=True)
    fig.subplots_adjust(left=0.09, right=0.99, top=0.9, bottom=0.14, hspace=0.18)
    series = [("cpu-plain", "CPU only, no D3D12", MUTED, "o", "--"), ("cpu", "CPU, graphics warmed", GREEN, "s", "-"),
              ("wb", "CUSTOM write-back heap", BLUE, "^", "-"), ("upload", "UPLOAD heap", ORANGE, "x", ":")]
    for ax, mib in zip(axes, (64, 1024)):
        for key, label, color, marker, ls in series:
            p = cases[f"memory-{key}-{mib}MiB-r0"]["phases"]
            base = p["baseline"]["phys_footprint"]
            xs = [i for i, (n, _) in enumerate(PHASES) if n in p]
            ys = [(p[PHASES[i][0]]["phys_footprint"] - base) / 2**20 for i in xs]
            ax.plot(xs, ys, color=color, marker=marker, linestyle=ls, label=label, linewidth=1.8)
        if mib == 1024:
            p = cases["memory-wb-1024MiB-r0"]["phases"]
            xs = [i for i, (n, _) in enumerate(PHASES) if n in p]
            ax.plot(xs, [(p[PHASES[i][0]]["resident_size"] - p["baseline"]["resident_size"]) / 2**20 for i in xs],
                    color=PURPLE, marker="D", markersize=4, label="write-back heap: RSS change", linewidth=1.2)
        ax.set_title(f"{mib} MiB requested")
        ax.set_ylabel("change from baseline, MiB")
        style(ax)
    axes[0].legend(fontsize=8.5, frameon=False)
    axes[1].set_ylim(-80, 1650)
    axes[1].legend(fontsize=8.5, frameon=False, loc="upper right", ncol=2)
    axes[1].set_xticks(range(len(PHASES)), [p[1] for p in PHASES], fontsize=8.5)
    fig.suptitle("Process footprint at each checkpoint (M4, first session only)", x=0.012, ha="left",
                 weight="bold", fontsize=14)
    save(fig, "07-footprint", "All series except the one marked RSS are the process physical-footprint ledger (median of 3 "
         "correlated readings at one pause; one process per line). Ledger charge and RSS are not physical residency, and retention is not proof "
         "of a leak. CPU controls run no GPU step. The second session stopped under urgent pressure.")


def pooling():
    p = D["pooling"]
    fig = plt.figure(figsize=(11, 8))
    fig.suptitle("Pooling: cheaper allocation calls, not a demonstrated speedup (M4)", x=0.012, ha="left",
                 weight="bold", fontsize=14)
    seeds = ("473", "474", "475")
    specs = [("heap_or_suballoc_ms", "Heap create / suballocate\n(median ms)"), ("placement_ms", "Placed resource\n(median ms)"),
             ("cycle_ms", "Whole cycle incl. GPU\n(median ms)"), ("pool_setup_ms", "One-time pool setup\n(ms)")]
    for i, (key, title) in enumerate(specs):
        ax = fig.add_axes([0.06 + i * 0.24, 0.6, 0.18, 0.25])
        for j, (mode, color, hatch) in enumerate((("individual", ORANGE, ""), ("pooled", BLUE, "//"))):
            ax.bar([k + (j - 0.5) * 0.38 for k in range(3)], [p[f"{mode}|{s}"][key] or 0 for s in seeds], 0.36,
                   color=color, hatch=hatch, edgecolor=PAPER, label=mode)
        ax.set_xticks(range(3), seeds, fontsize=8.5)
        ax.set_title(title, fontsize=9.5)
        ax.tick_params(axis="y", labelsize=8)
        style(ax)
        if i == 0:
            for k, s in enumerate(seeds):
                ax.annotate(f"{p[f'pooled|{s}'][key]:.4f}", (k + 0.19, 0.0002), ha="center", fontsize=7.5, color=BLUE, rotation=90, va="bottom")
            ax.legend(fontsize=8, frameon=False)
    ax = fig.add_axes([0.012, 0.09, 0.975, 0.42])
    ax.set_xlim(0, 100)
    ax.set_ylim(0, 100)
    ax.axis("off")
    stats = D["fragmentation"]["473"]["labels"]
    ax.text(0, 97, "Logical 32 MiB slab: free bytes are not the same as usable space", weight="bold")
    states = [("1. Alternate slots freed", "L.L.L.L.", "alternating_holes"),
              ("2. Rest freed, GPU fence not done", "PfPfPfPf", "pending_blocks_reuse"),
              ("3. Fence completes, spans coalesce", "ffffffff", "fully_coalesced")]
    for row, (title, pattern, key) in enumerate(states):
        y = 70 - row * 28
        s = stats[key]
        ax.text(0, y + 17, f"{title}: free {s['total_free'] // 2**20} MiB, largest span {s['largest_free'] // 2**20} MiB, "
                           f"pending {s['pending_reserved'] // 2**20} MiB", fontsize=9.5)
        if key == "fully_coalesced":
            ax.add_patch(Rectangle((0, y), 96, 13, facecolor=TINT[GREEN], edgecolor=GREEN))
            ax.text(48, y + 6.5, "one free span: an 8 MiB request now fits", ha="center", va="center", fontsize=9)
            continue
        for i, c in enumerate(pattern):
            col, word = {"L": (BLUE, "live"), ".": (GREEN, "free"), "P": (ORANGE, "pending"), "f": (GREEN, "free")}[c]
            ax.add_patch(Rectangle((i * 12, y), 11.8, 13, facecolor=TINT[col], edgecolor=col,
                                   hatch="//" if word == "pending" else ""))
            ax.text(i * 12 + 6, y + 6.5, f"{word} 4 MiB", ha="center", va="center", fontsize=8.5)
    save(fig, "08-pooling", "Top: three seeded pairs, 360 timed allocations each, same source and workload. Whole-cycle "
         "differences are small and an earlier preserved pair had the opposite ordering. Bottom: CPU-only logical model. "
         "AMD D3D12MA 3.3.0 evaluation: bounded one-block WB/WC buffer pools only; no speedup claim.")


def budget():
    fig, ax = canvas("Combined budget can exceed its cap (unmerged PR #486 at c13f9c7)", 11, 6.6,
                     "Source-derived arithmetic in abstract units. Not a production run. main has no private-heap charge.")

    def bar(y, label, parts):
        ax.text(1, y + 10.5, label, fontsize=10, weight="bold")
        x = 1
        for units, text, color, hatch in parts:
            ax.add_patch(Rectangle((x, y), units * 6.4, 9, facecolor=TINT[color], edgecolor=color, hatch=hatch, linewidth=1.5))
            ax.text(x + units * 3.2, y + 4.5, text, ha="center", va="center", fontsize=9.5, weight="bold")
            x += units * 6.4
        ax.plot([1 + 8 * 6.4] * 2, [y - 1.5, y + 9.4], color=RED, linewidth=2.5, linestyle="--")
        ax.text(1 + 8 * 6.4 + 0.6, y - 3, "cap 8", color=RED, fontsize=9)

    g, c = 6, 3
    bar(74, f"Sequential: d3d12_xs admits {g}, then xmem admits {c} (it cannot see the {g}) = {g + c}",
        [(g, f"private GPU heap {g}", PURPLE, ""), (c, f"CPU {c}", BLUE, "//")])
    ax.text(1, 66, "Reverse order: the heap request is rejected, so the check is one-way.", fontsize=9.5, color=MUTED)
    bar(46, "Concurrent: snapshot taken at xmem = 4, both then commit = 12",
        [(4, "CPU 4", BLUE, ""), (4, "CPU +4", BLUE, "//"), (4, "GPU 4 (stale)", PURPLE, "")])
    box(ax, 2, 6, 27, 22, "xmem.dll", "usage_lock\ncounts heap + virtual\n+ physical only", BLUE)
    box(ax, 71, 6, 27, 22, "d3d12_xs.dll", "g_section lock\nreads xmem totals, adds\nits private heaps", PURPLE)
    box(ax, 36, 6, 28, 22, "Recommended", "one authority that\nchecks and reserves all\nowners atomically", GREEN, True)
    arrow(ax, (71, 17), (65, 17), color=GREEN, dashed=True)
    arrow(ax, (29, 17), (35, 17), color=GREEN, dashed=True)
    save(fig, "09-budget", "Assumes distinct live allocations and successful host allocation. A direction for the owners "
         "to discuss, not a designed or approved fix.")


def sync():
    fig, ax = canvas("When is it safe to read or reuse?", 11, 5.4, "Only after the fence for the LAST GPU use completes.")
    steps = [("CPU writes", "then store fence", BLUE), ("Submit", "ExecuteCommandLists\n+ queue Signal", BLUE),
             ("GPU executes", "CPU must not\noverwrite or free", ORANGE), ("Fence completes", "completed value\n≥ target", GREEN),
             ("Read or reuse", "free, recycle,\noverwrite", GREEN)]
    for i, (head, body, col) in enumerate(steps):
        box(ax, 1 + i * 20, 58, 17, 28, head, body, col)
        if i:
            arrow(ax, (i * 20 - 1.5, 72), (i * 20 + 0.5, 72))
    box(ax, 1, 8, 47, 32, "Completion not proven (research probe)",
        "signal or wait fails, or times out\nprobe calls ExitProcess(2)\nnothing is recycled\nacceptable for a disposable process", ORANGE)
    box(ax, 52, 8, 47, 32, "Completion not proven (production)",
        "NOT qualified by this research\nexiting is not recovery\ndevice loss, multiple queues,\nDLL detach and threads are open", RED, True)
    save(fig, "10-lifetime", "Persistent mapping supplies no scheduling and no atomics. A-13 acceptance covers a "
         "single-threaded disposable guest only.")


def matrix():
    statuses = ["Independently accepted", "Independently reproduced", "Independently source-reviewed", "Author-tested",
                "Failed", "Unsupported", "Blocked", "Unknown"]
    colors = [GREEN, GREEN, BLUE, MUTED, RED, ORANGE, ORANGE, PURPLE]
    marks = ["✓", "✓", "✓", "✓", "✕", "–", "‖", "?"]
    fig, ax = plt.subplots(figsize=(11, 12.5))
    fig.subplots_adjust(left=0.47, right=0.995, top=0.855, bottom=0.05)
    for y, f in enumerate(REG):
        if y % 2 == 0:
            ax.add_patch(Rectangle((-0.5, y - 0.5), 8, 1, color="#f6f8fa", zorder=0))
        for x, s in enumerate(statuses):
            if s in f["status_flags"]:
                ax.add_patch(Rectangle((x - 0.38, y - 0.38), 0.76, 0.76, facecolor=TINT[colors[x]], edgecolor=colors[x]))
                ax.text(x, y, marks[x], ha="center", va="center", color=colors[x], weight="bold")
    q = [f"{f['id']}  {textwrap.shorten(f['research_question'], 62, placeholder='…')}" for f in REG]
    ax.set_yticks(range(len(REG)), q, fontsize=8.6)
    ax.set_xticks(range(8), ["accepted", "reproduced", "source-\nreviewed", "author-\ntested", "failed",
                             "unsupported", "blocked", "unknown"], fontsize=8.6, rotation=60, ha="left")
    ax.xaxis.tick_top()
    ax.set_xlim(-0.5, 7.5)
    ax.set_ylim(len(REG) - 0.5, -0.5)
    ax.spines[:].set_visible(False)
    ax.tick_params(length=0)
    fig.suptitle(f"All {len(REG)} evidence register entries", x=0.012, ha="left", weight="bold", fontsize=15)
    save(fig, "11-evidence-matrix", "Drawn from status_flags in evidence-register.json. One entry can carry several "
         "statuses. 'Independent' means the other workstream, whose work was done by AI agents. No confidence score is implied.")


def direction():
    fig, ax = canvas("Architectural direction: dependencies to discuss", 11, 7,
                     "Research-derived proposal. Not an approved schedule; nothing here is assigned.")
    box(ax, 2, 76, 30, 15, "1. Combined budget\ncorrectness", color=RED)
    box(ax, 35, 76, 30, 15, "2. Lifetime and failure\nsupervision", color=RED)
    box(ax, 68, 76, 30, 15, "3. Placement and\nalignment validation", color=ORANGE)
    box(ax, 28, 52, 44, 13, "4. Eligible mapped-buffer ownership", color=BLUE)
    box(ax, 4, 28, 40, 13, "5. Memory pooling", color=BLUE)
    box(ax, 56, 28, 40, 13, "6. Explicit-copy fallbacks", color=BLUE)
    box(ax, 28, 5, 44, 13, "7. Production benchmarking\nand qualification", color=GREEN)
    for x in (17, 50, 83):
        arrow(ax, (x, 76), (50 + (x - 50) * 0.35, 66))
    arrow(ax, (40, 52), (26, 42))
    arrow(ax, (60, 52), (74, 42))
    arrow(ax, (26, 28), (42, 19))
    arrow(ax, (74, 28), (58, 19))
    box(ax, 76, 3, 22, 17, "Supporting:\nreceipt finalization\nin the research harness", color=PURPLE, dashed=True, size=9)
    arrow(ax, (76, 11), (73, 11), color=PURPLE, dashed=True)
    save(fig, "12-direction", "Arrows show dependencies, not dates. Explicit-copy fallbacks (6) must exist whenever mapped ownership (4) does.")


if __name__ == "__main__":
    ASSETS.mkdir(exist_ok=True)
    for fn in (overview, unified, architecture, decision, hosts, copy_latency, trace_rates, compute_latency,
               footprint, pooling, budget, sync, matrix, direction):
        fn()
    print(len(list(ASSETS.glob("*.png"))), "figures")

#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Derive inspectable summaries from protocol-1 raw logs; never rewrite inputs."""
import argparse
import json
from pathlib import Path
import re
import statistics


def fields(line):
    return dict(re.findall(r"(\w+)=([^ ]+)", line))


def summarize(root):
    result = {"memory": [], "dirty": [], "runs": []}
    for receipt in sorted(root.rglob("receipt.json")):
        doc = json.loads(receipt.read_text())
        if doc.get("protocol") != 1:
            continue
        result["runs"].append({"path": str(receipt.relative_to(root)),
                               "source_hashes": doc["source_hashes"],
                               "baseline_sha256": doc["baseline_sha256"]})
    for path in sorted(root.rglob("case.json")):
        case = json.loads(path.read_text())
        result.setdefault("outcomes", []).append({"path": str(path.relative_to(root)),
                    "exit": case["exit"], "expected_exit": case.get("expected_exit", 0),
                    "timed_out": case["timed_out"]})
        if case["name"].startswith("memory-") and case.get("snapshots"):
            stages = {}
            for snapshot in case["snapshots"]:
                data = [sample["data"] for sample in snapshot["samples"]
                        if sample.get("data", {}).get("return") == 0]
                if data:
                    stages[snapshot["phase"]] = {key: statistics.median(d[key] for d in data)
                            for key in ["phys_footprint", "resident_size", "wired_size", "pageins"]}
            baseline = stages["baseline"]["phys_footprint"]
            result["memory"].append({"name": case["name"], "args": case["args"], "stages": stages,
                 "footprint_delta_bytes": {phase: values["phys_footprint"] - baseline for phase, values in stages.items()}})
        stdout = path.with_name("stdout.txt")
        if stdout.exists():
            lines = stdout.read_text().splitlines()
            samples = [fields(line) for line in lines if line.startswith("DIRTY_SAMPLE ")]
            for line in lines:
                if not line.startswith("DIRTY_RESULT "):
                    continue
                row = fields(line)
                row["case"] = case["name"]
                matching = [s for s in samples if s["k"] == row["k"]]
                cpu = [float(s["dirty_cpu_ms"]) for s in matching if "dirty_cpu_ms" in s]
                row["median_dirty_cpu_ms"] = statistics.median(cpu) if cpu else None
                result["dirty"].append(row)
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("root", type=Path)
    args = parser.parse_args()
    print(json.dumps(summarize(args.root), indent=2))


if __name__ == "__main__":
    main()

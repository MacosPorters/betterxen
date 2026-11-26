#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Compile original HLSL with external DXC, then verify actual Shader Model 6 GPU readback.

RUNNER BOTTLE PROBE_EXE DXC_BIN_DIR OUTPUT_DIR
The dedicated test bottle must have Z: mapped to / (CrossOver's observed default).
Expected float bit patterns below are derived from arithmetic, not the compiler or GPU.
"""
import json
from pathlib import Path
import re
import subprocess
import sys


def windows(path):
    return "Z:" + str(Path(path).resolve()).replace("/", "\\")


def main():
    runner, bottle, probe, dxc, output = sys.argv[1:]
    root = Path(__file__).resolve().parents[2]
    output = Path(output).resolve()
    output.mkdir(parents=True, exist_ok=False)
    dxc = Path(dxc).resolve() / "dxc.exe"
    base = [str(Path(runner).resolve()), "hybrid", "run", "--bottle", bottle,
            "--backend", "d3dmetal", "--timeout", "120", "--keep-output", "true", "--out", str(output / "raw")]

    def run(name, command):
        result = subprocess.run(base + ["--"] + command, capture_output=True, text=True, timeout=180)
        (output / (name + ".json")).write_text(result.stdout)
        (output / (name + ".log")).write_text(result.stderr)
        if result.returncode:
            raise RuntimeError(f"{name} failed; see {output}")
        record = json.loads(result.stdout)
        # Read the complete supervised stdout; a successful exit alone proves no result.
        return Path(record["stdout_path"]).read_text(errors="replace")

    caps = run("caps", [probe, "caps"])
    match = re.search(r"options1: hr=0x0+ wave_ops=1 wave_lane_count_min=(\d+) wave_lane_count_max=(\d+)", caps)
    if not match:
        raise RuntimeError("wave capability query did not succeed; SM6 acceptance unavailable")
    expectations = {"add_f32_4": [0x40000000, 0] * 4, "sum_f32_4": [0x40A00000] * 4}
    results = {}
    for name, words in [("lanes4", 4), ("add_f32_4", 8), ("sum_f32_4", 4)]:
        shader = output / (name + ".dxil")
        run("compile-" + name, [str(dxc), "-T", "cs_6_6", "-E", "main", "-Fo", windows(shader),
                                windows(root / "tests/conformance/test_d3d12_wave/hlsl" / (name + ".hlsl"))])
        disassembly = run("dxil-" + name, [str(dxc), "-dumpbin", windows(shader)])
        operations = {
            "lanes4": [r"\bcall\b[^\n]*@dx\.op\.waveGetLaneIndex", r"\bcall\b[^\n]*@dx\.op\.waveGetLaneCount"],
            "sum_f32_4": [r"\bcall\b[^\n]*@dx\.op\.waveActiveOp\.f32"],
            "add_f32_4": [r"\bfadd\b"],
        }
        if any(not re.search(pattern, disassembly) for pattern in operations[name]):
            raise RuntimeError(f"{name}: compiled DXIL does not contain the operation being tested")
        text = run(name, [probe, "run", windows(shader), "1", str(words)])
        pairs = [(int(i), int(v, 16)) for i, v in re.findall(r"^out (\d+) 0x([0-9a-fA-F]+)$", text, re.M)]
        if [i for i, _ in pairs] != list(range(words)):
            raise RuntimeError(f"{name}: missing, duplicate or unexpected readback words")
        actual = [v for _, v in pairs]
        dispatch = re.search(r"dispatch_caps wave_ops=1 min=(\d+) max=(\d+)", text)
        if not dispatch:
            raise RuntimeError(f"{name}: no wave capabilities for the dispatch device")
        minimum, maximum = map(int, dispatch.groups())
        if name == "lanes4":
            width = actual[0] >> 8
            def valid_lanes(values):
                widths = {v >> 8 for v in values}
                lanes = {v & 255 for v in values}
                return (len(widths) == 1 and minimum <= width <= maximum and 4 <= width <= 128
                        and not width & (width - 1) and len(lanes) == 4 and max(lanes) < width)
            if not valid_lanes(actual):
                raise RuntimeError(f"invalid or duplicate lane IDs or wave width: {actual}")
            wrong = actual.copy()
            wrong[0] ^= 0x80000000
            if valid_lanes(wrong):
                raise RuntimeError("lane invariant negative control failed")
            results[name] = {"actual": actual, "expected_properties": "one supported wave width; four distinct lane IDs below width", "negative_mismatches": 1}
            print("PASS SM6 lanes4: independent lane invariants and one-bit negative control")
            continue
        else:
            expected = expectations[name]
        mismatches = sum(a != b for a, b in zip(actual, expected))
        if mismatches:
            raise RuntimeError(f"{name}: {mismatches} incorrect GPU words: {actual}")
        # The deliberately wrong expectation must detect exactly one mismatch in this readback.
        wrong = expected.copy()
        wrong[0] ^= 1
        if sum(a != b for a, b in zip(actual, wrong)) != 1:
            raise RuntimeError(f"{name}: negative control failed")
        results[name] = {"actual": actual, "expected": expected, "negative_mismatches": 1}
        print(f"PASS SM6 {name}: {words} exact GPU words, one-bit negative control detected")
    (output / "summary.json").write_text(json.dumps({"schema": "betterxen-sm6/0", "passed": True, "results": results}, indent=2) + "\n")
    print("Unsupported double precision is not exercised or counted as passing.")


if __name__ == "__main__":
    try:
        main()
    except (ValueError, OSError, RuntimeError, subprocess.SubprocessError, KeyError) as error:
        sys.exit(f"FAIL SM6: {error}")

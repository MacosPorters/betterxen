#!/bin/sh
# SPDX-License-Identifier: GPL-3.0-or-later
#
# First compiled guest launch gate (#321). See docs/testing/first-launch.md.
#
#   scripts/test-launchable.sh              check the expectations, then report
#                                           every launch checkpoint
#   scripts/test-launchable.sh --self-test  also prove the expectation check
#                                           rejects a mutated copy
#
# Exit status: 0 every checkpoint ran and passed; 1 a check failed or the
# expectations are inconsistent; 2 nothing failed but at least one checkpoint
# could not run yet (PENDING). A pending checkpoint is never a pass.
#
# Each run writes record.json under artifacts/launchable/ (ignored by Git) with
# the host, revision, commands and every checkpoint's status and reason.
set -eu
cd "$(dirname "$0")/.."

fail() { echo "test-launchable: $*" >&2; exit 1; }
[ "$(uname -s)" = Darwin ] || fail "macOS is required"
[ "$(uname -m)" = arm64 ] || fail "a native arm64 host is required"
[ "$(sysctl -n sysctl.proc_translated 2>/dev/null || echo 0)" = 0 ] || fail "running under Rosetta is not supported"
command -v python3 >/dev/null || fail "python3 not found"

self_test=0
case "${1:-}" in
    "") ;;
    --self-test) self_test=1 ;;
    *) fail "unknown option: $1" ;;
esac

out="artifacts/launchable/$(date -u +%Y%m%dT%H%M%SZ)"
mkdir -p "$out"

python3 - "$self_test" "$out" <<'PY'
import json
import platform
import re
import subprocess
import sys
from pathlib import Path

SELF_TEST = sys.argv[1] == "1"
OUT = Path(sys.argv[2])
EXPECTED = Path("tests/launchable/expected.json")
HEADER = Path("crates/ffi/include/xenora.h")


def header_constant(name):
    match = re.search(rf"#define {name} (\d+)u", HEADER.read_text())
    if not match:
        raise SystemExit(f"test-launchable: {name} not found in {HEADER}")
    return int(match.group(1))


def check_expected(doc):
    """Return a list of problems with an expectations document; empty if none.

    The pixel rules are recomputed here from the formulas documented in
    docs/testing/first-launch.md, so a typo in either place is caught."""
    problems = []
    if doc.get("schema") != 1:
        problems.append("schema must be 1")
    frame = doc.get("frame", {})
    width, height = frame.get("width"), frame.get("height")
    if (width, height) != (header_constant("XENORA_FIXTURE_FRAME_WIDTH"),
                           header_constant("XENORA_FIXTURE_FRAME_HEIGHT")):
        problems.append(f"frame {width}x{height} differs from the C interface's fixture frame size")
        return problems
    frames = doc.get("frames_per_session")
    ranges = doc.get("state_by_frame", [])
    covered = []
    for entry in ranges:
        covered.extend(range(entry["first"], entry["last"] + 1))
    if covered != list(range(frames or 0)):
        problems.append("state_by_frame must cover frames 0..frames_per_session-1 once, in order")
    states = doc.get("states", {})
    if sorted(states) != ["A", "B"] or sorted({e["state"] for e in ranges}) != ["A", "B"]:
        problems.append("exactly states A and B must be defined and used")
        return problems
    rules = {
        "A": lambda x, y: [255, 128, 0, 255],
        "B": lambda x, y: [64 * x, 96 * y, 255 - 64 * x, 255],
    }
    for name, rule in rules.items():
        pixels = states[name].get("pixels")
        if not isinstance(pixels, list) or len(pixels) != height or any(len(row) != width for row in pixels):
            problems.append(f"state {name}: pixels must be {height} rows of {width}")
            continue
        for y, row in enumerate(pixels):
            for x, pixel in enumerate(row):
                if not (isinstance(pixel, list) and len(pixel) == 4
                        and all(isinstance(c, int) and 0 <= c <= 255 for c in pixel)):
                    problems.append(f"state {name} pixel ({x},{y}) is not four 0-255 integers")
                elif pixel != rule(x, y):
                    problems.append(f"state {name} pixel ({x},{y}) is {pixel}, rule gives {rule(x, y)}")
    a, b = states["A"].get("pixels"), states["B"].get("pixels")
    if not problems and any(a[y][x] == b[y][x] for y in range(height) for x in range(width)):
        problems.append("states A and B must differ at every pixel")
    if doc.get("guest_exit", {}).get("frames_submitted") != frames:
        problems.append("guest_exit.frames_submitted must equal frames_per_session")
    if doc.get("sessions") != 2:
        problems.append("two sessions (launch and one restart) are required")
    return problems


doc = json.loads(EXPECTED.read_text())
problems = check_expected(doc)
for problem in problems:
    print(f"expectations: {problem}")
if problems:
    sys.exit(1)
print(f"expectations: consistent ({EXPECTED}, status {doc['status']})")

if SELF_TEST:
    mutated = json.loads(json.dumps(doc))
    mutated["states"]["B"]["pixels"][2][3][1] ^= 1
    found = check_expected(mutated)
    if not any("state B pixel (3,2)" in p for p in found):
        print("self-test: a changed expected pixel was not detected")
        sys.exit(1)
    print("self-test: a changed expected pixel is rejected")

# Each checkpoint runs only once the work it needs exists. Until then it is
# PENDING with the reason; it never passes by default.
checkpoints = [
    ("build_identity", "original C source and native build recipe", "#313",
     Path("tests/launchable/program.c")),
    ("admit_load", "admission under the built-in launch worker profile", "#317 (#212 policy)", None),
    ("guest_entry", "PE startup, imports bound and DBT entry with progress records", "#317, #255", None),
    ("first_native_frame", "guest-selected pipeline rendered on Metal and read back", "#314, #315, #316", None),
    ("presentation", "the current session's frame shown in the app window", "#320", None),
    ("repeated_frames", "frames 0-59 match states A then B", "#316, #317", None),
    ("close_restart", "bounded stop, clean close and an identical second session", "#318, #320", None),
]
results = []
for name, what, needs, marker in checkpoints:
    if marker is not None and marker.exists():
        reason = f"{marker} exists, but no runner is wired for this checkpoint yet"
    else:
        reason = f"waits on {needs}"
    results.append({"checkpoint": name, "what": what, "status": "PENDING", "reason": reason})
    print(f"{name:<20} PENDING  {reason}")


def command(*args):
    try:
        return subprocess.run(args, capture_output=True, text=True, check=True).stdout.strip()
    except (OSError, subprocess.CalledProcessError):
        return None


record = {
    "gate": "first compiled guest launch (#321)",
    "revision": command("git", "rev-parse", "HEAD"),
    "dirty": bool(command("git", "status", "--porcelain")),
    "host": {
        "machine": platform.machine(),
        "macos": command("sw_vers", "-productVersion"),
        "chip": command("sysctl", "-n", "machdep.cpu.brand_string"),
    },
    "expectations": {"file": str(EXPECTED), "status": doc["status"]},
    "command": "scripts/test-launchable.sh" + (" --self-test" if SELF_TEST else ""),
    "checkpoints": results,
}
(OUT / "record.json").write_text(json.dumps(record, indent=2) + "\n")
print(f"record: {OUT / 'record.json'}")
statuses = {r["status"] for r in results}
if "FAIL" in statuses:
    sys.exit(1)
sys.exit(2 if "PENDING" in statuses else 0)
PY

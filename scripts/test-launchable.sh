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
# Exit status: 0 every checkpoint and every negative control ran and passed;
# 1 a check failed, a control is missing or misclassified, or the expectations
# are inconsistent; 2 nothing failed but at least one checkpoint or control
# could not run yet (PENDING). A pending checkpoint is never a pass.
#
# Each run writes record.json in its own new directory under
# artifacts/launchable/ (ignored by Git), also when it fails, with the host,
# revision, command, expectation problems and every checkpoint's and control's
# status and reason.
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

mkdir -p artifacts/launchable
# A new directory per run, so two runs in the same second keep both records.
out=$(mktemp -d "artifacts/launchable/$(date -u +%Y%m%dT%H%M%SZ)-XXXXXX") ||
    fail "cannot create a run directory under artifacts/launchable"

python3 - "$self_test" "$out" <<'PY'
import copy
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

# The agreed contract from docs/testing/first-launch.md. expected.json must
# state exactly this, so a change to either place is caught here.
FRAMES = 60
SESSIONS = 2
RANGES = [(0, 29, "A"), (30, 59, "B")]
EXIT_STATUS = int.from_bytes(b"LNCH", "big")  # 0x4C4E4348
CONTROLS = {
    "no_metal": "capability_unavailable",
    "no_frame": "no_frame",
    "unsupported_instruction": "illegal_instruction",
    "unresolved_import": "refused_before_entry",
    "bad_shader_or_binding": "rejected_at_creation_or_binding",
    "wrong_expected_pixel": "pixel_mismatch",
    "low_step_budget": "step_limit_exceeded",
    "worker_timeout": "timeout",
}


def header_constant(name):
    match = re.search(rf"#define {name} (\d+)u", HEADER.read_text())
    if not match:
        raise ValueError(f"{name} not found in {HEADER}")
    return int(match.group(1))


def check_expected(doc):
    """Return a list of problems with an expectations document; empty if none.

    The pixel rules are recomputed here from the formulas documented in
    docs/testing/first-launch.md, so a typo in either place is caught."""
    try:
        return _check_expected(doc)
    except (AttributeError, KeyError, TypeError, ValueError) as error:
        return [f"malformed document: {error!r}"]


def _check_expected(doc):
    problems = []
    if doc.get("schema") != 1:
        problems.append("schema must be 1")
    frame = doc.get("frame", {})
    width, height = frame.get("width"), frame.get("height")
    if (width, height) != (header_constant("XENORA_FIXTURE_FRAME_WIDTH"),
                           header_constant("XENORA_FIXTURE_FRAME_HEIGHT")):
        problems.append(f"frame {width}x{height} differs from the C interface's fixture frame size")
        return problems
    if doc.get("frames_per_session") != FRAMES:
        problems.append(f"frames_per_session must be {FRAMES}")
    ranges = [(e.get("first"), e.get("last"), e.get("state")) for e in doc.get("state_by_frame", [])]
    if ranges != RANGES:
        problems.append(f"state_by_frame must be exactly {RANGES} (first, last, state)")
    if doc.get("sessions") != SESSIONS:
        problems.append(f"sessions must be {SESSIONS} (launch and one restart)")
    guest_exit = doc.get("guest_exit", {})
    if guest_exit.get("status") != EXIT_STATUS:
        problems.append(f"guest_exit.status must be {EXIT_STATUS} (0x{EXIT_STATUS:08X}, 'LNCH')")
    if guest_exit.get("frames_submitted") != FRAMES:
        problems.append(f"guest_exit.frames_submitted must be {FRAMES}")
    controls = [(c.get("name"), c.get("classification")) for c in doc.get("negative_controls", [])]
    if sorted(controls) != sorted(CONTROLS.items()) or len(controls) != len(CONTROLS):
        problems.append("negative_controls must list each of the eight agreed controls once, with its classification")
    if any(not isinstance(c.get("realized_by"), str) or not c["realized_by"] for c in doc.get("negative_controls", [])):
        problems.append("every negative control must say what realizes it (realized_by)")
    states = doc.get("states", {})
    if sorted(states) != ["A", "B"]:
        problems.append("exactly states A and B must be defined")
        return problems
    rules = {
        "A": lambda x, y: [255, 128, 0, 255],
        "B": lambda x, y: [64 * x, 96 * y, 255 - 64 * x, 255],
    }
    pixels_ok = True
    for name, rule in rules.items():
        pixels = states[name].get("pixels")
        if not isinstance(pixels, list) or len(pixels) != height or any(len(row) != width for row in pixels):
            problems.append(f"state {name}: pixels must be {height} rows of {width}")
            pixels_ok = False
            continue
        for y, row in enumerate(pixels):
            for x, pixel in enumerate(row):
                if not (isinstance(pixel, list) and len(pixel) == 4
                        and all(isinstance(c, int) and 0 <= c <= 255 for c in pixel)):
                    problems.append(f"state {name} pixel ({x},{y}) is not four 0-255 integers")
                    pixels_ok = False
                elif pixel != rule(x, y):
                    problems.append(f"state {name} pixel ({x},{y}) is {pixel}, rule gives {rule(x, y)}")
                    pixels_ok = False
    a, b = states["A"]["pixels"], states["B"]["pixels"]
    if pixels_ok and any(a[y][x] == b[y][x] for y in range(height) for x in range(width)):
        problems.append("states A and B must differ at every pixel")
    return problems


def gate(checkpoints, controls):
    """Combine positive checkpoints and negative controls into one result.

    PASS needs every checkpoint PASS and every agreed control PASS with its
    agreed classification. A missing or misclassified control is a FAIL."""
    statuses = [c["status"] for c in checkpoints]
    by_name = {c["control"]: c for c in controls}
    for name, classification in CONTROLS.items():
        control = by_name.get(name)
        if control is None:
            statuses.append("FAIL")
        elif control["status"] == "PASS" and control.get("observed") != classification:
            statuses.append("FAIL")
        else:
            statuses.append(control["status"])
    if any(s not in ("PASS", "PENDING") for s in statuses):
        return "FAIL"
    return "PENDING" if "PENDING" in statuses else "PASS"


def self_test(doc):
    """Each mutation of a consistent document must be rejected for its reason,
    and the gate must not pass with a missing or misclassified control."""
    def swap_states(d):
        for entry in d["state_by_frame"]:
            entry["state"] = {"A": "B", "B": "A"}[entry["state"]]

    def two_frames(d):
        d["frames_per_session"] = 2
        d["guest_exit"]["frames_submitted"] = 2
        d["state_by_frame"] = [{"first": 0, "last": 0, "state": "A"}, {"first": 1, "last": 1, "state": "B"}]

    def drop_control(d):
        d["negative_controls"].pop()

    def reclassify_control(d):
        d["negative_controls"][0]["classification"] = "no_frame"

    def flip_pixel(d):
        d["states"]["B"]["pixels"][2][3][1] ^= 1

    mutations = [
        ("changed expected pixel", flip_pixel, "state B pixel (3,2)"),
        ("states A and B swapped", swap_states, "state_by_frame"),
        ("two frames per session", two_frames, "frames_per_session"),
        ("exit status 1", lambda d: d["guest_exit"].update(status=1), "guest_exit.status"),
        ("one session", lambda d: d.update(sessions=1), "sessions"),
        ("missing negative control", drop_control, "negative_controls"),
        ("wrong control classification", reclassify_control, "negative_controls"),
    ]
    outcomes = []
    for label, mutate, reason in mutations:
        mutated = copy.deepcopy(doc)
        mutate(mutated)
        found = check_expected(mutated)
        outcomes.append({"case": label, "rejected": any(reason in p for p in found)})

    passed = [{"checkpoint": "x", "status": "PASS"}]
    all_controls = [{"control": n, "status": "PASS", "observed": c} for n, c in CONTROLS.items()]
    gate_cases = [
        ("every check passes", gate(passed, all_controls), "PASS"),
        ("a control has no outcome", gate(passed, all_controls[1:]), "FAIL"),
        ("a control is misclassified",
         gate(passed, [dict(all_controls[0], observed="no_frame")] + all_controls[1:]), "FAIL"),
        ("a control is pending", gate(passed, [dict(all_controls[0], status="PENDING")] + all_controls[1:]), "PENDING"),
    ]
    for label, got, want in gate_cases:
        outcomes.append({"case": f"gate: {label}", "rejected": got == want})
    return outcomes


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
    "command": "scripts/test-launchable.sh" + (" --self-test" if SELF_TEST else ""),
    "expectations": {"file": str(EXPECTED)},
    "checkpoints": [],
    "negative_controls": [],
    "result": "FAIL",
}
status = 1
try:
    try:
        doc = json.loads(EXPECTED.read_text())
    except (OSError, ValueError) as error:
        doc, problems = None, [f"cannot read {EXPECTED}: {error}"]
    else:
        problems = check_expected(doc)
        record["expectations"]["status"] = doc.get("status") if isinstance(doc, dict) else None
    record["expectations"]["problems"] = problems
    for problem in problems:
        print(f"expectations: {problem}")
    if problems:
        raise SystemExit(1)
    print(f"expectations: consistent ({EXPECTED}, status {doc['status']})")

    if SELF_TEST:
        outcomes = self_test(doc)
        record["self_test"] = outcomes
        for outcome in outcomes:
            print(f"self-test: {outcome['case']:<32} {'ok' if outcome['rejected'] else 'NOT DETECTED'}")
        if not all(o["rejected"] for o in outcomes):
            raise SystemExit(1)

    # Each checkpoint and control runs only once the work it needs exists.
    # Until then it is PENDING with the reason; it never passes by default.
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
    for name, what, needs, marker in checkpoints:
        if marker is not None and marker.exists():
            reason = f"{marker} exists, but no runner is wired for this checkpoint yet"
        else:
            reason = f"waits on {needs}"
        record["checkpoints"].append({"checkpoint": name, "what": what, "status": "PENDING", "reason": reason})
        print(f"{name:<32} PENDING  {reason}")
    for control in doc["negative_controls"]:
        reason = f"no runner yet; plan: {control['realized_by']}"
        record["negative_controls"].append({
            "control": control["name"], "classification": control["classification"],
            "status": "PENDING", "observed": None, "reason": reason,
        })
        print(f"{'control ' + control['name']:<32} PENDING  {reason}")

    record["result"] = gate(record["checkpoints"], record["negative_controls"])
    status = {"PASS": 0, "FAIL": 1, "PENDING": 2}[record["result"]]
except SystemExit as stop:
    status = stop.code if isinstance(stop.code, int) else 1
finally:
    (OUT / "record.json").write_text(json.dumps(record, indent=2) + "\n")
    print(f"record: {OUT / 'record.json'} ({record['result']})")
sys.exit(status)
PY

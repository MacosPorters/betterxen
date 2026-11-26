#!/bin/sh
# SPDX-License-Identifier: GPL-3.0-or-later
# Run the integrated-workload acceptance sequence through `betterxen hybrid` on this Mac.
# Every line printed is "ok <check>" or "FAIL <check>"; the exit status is non-zero if any
# check failed. Raw JSON records and logs are kept under the output directory.
#
#   scripts/hybrid/accept-integrated.sh <llvm-mingw-dir> <bottle> [soak-frames] [repeats]
#
# Exit 0: every check ran and passed. 1: a check failed. 2: the only deviations are the known
# D3DMetal swapchain retention and/or checks reported NOT-RUN. NOT-RUN is never a pass.
# Shader Model 6 needs Microsoft's DXC (scripts/hybrid/install-dxc.py); point
# BETTERXEN_DXC_DIR at its bin/x64 directory.
# Needs CrossOver, an existing win11_64 bottle, Rust 1.100.0, the pinned llvm-mingw
# (docs/research/hybrid-toolchain.md), and for the on-screen check a GUI session with the
# Screen Recording and Accessibility permissions (the latter for the keyboard check, which
# brings the test window to the front: do not type while it runs). Opens small windows; do not
# run on a busy machine or alongside anything else using the same bottle.
set -u
TC=${1:?usage: $0 <llvm-mingw-dir> <bottle> [soak-frames] [repeats]}
BOTTLE=${2:?name an existing bottle}
SOAK=${3:-20000}
REPEATS=${4:-5}
ROOT=$(cd "$(dirname "$0")/../.." && pwd)
export RUSTUP_TOOLCHAIN=${RUSTUP_TOOLCHAIN:-1.100.0-aarch64-apple-darwin}
mkdir -p "$ROOT/artifacts/acceptance" || exit 1
OUT=$(mktemp -d "$ROOT/artifacts/acceptance/$(date -u +%Y%m%dT%H%M%SZ)-XXXXXX") || exit 1
failed=0
known=0
not_run=0
ok() { echo "ok   $1"; }
bad() { echo "FAIL $1"; failed=1; }

if [ -z "${BETTERXEN_RUNNER:-}" ]; then
    (cd "$ROOT" && cargo build --locked --release -p bx-cli) >"$OUT/build-runner.log" 2>&1 ||
        { bad "build runner"; exit 1; }
fi
if [ -n "${BETTERXEN_WORKLOAD_DIR:-}" ]; then
    mkdir -p "$OUT/bin" || exit 1
    for name in d3d11-integrated d3d12-integrated d3d12-lifetime d3d12-wave-probe; do
        cp "$BETTERXEN_WORKLOAD_DIR/$name.exe" "$OUT/bin/" || exit 1
    done
else
"$ROOT/tests/conformance/test_d3d11_integrated/build.sh" "$TC" "$OUT/bin" >"$OUT/build-workload.log" 2>&1 ||
    { bad "build workload"; exit 1; }
"$ROOT/tests/conformance/test_d3d12_integrated/build.sh" "$TC" "$OUT/bin" >"$OUT/build-workload12.log" 2>&1 ||
    { bad "build D3D12 workload"; exit 1; }
for probe in lifetime wave; do
    "$ROOT/tests/conformance/test_d3d12_$probe/build.sh" "$TC" "$OUT/bin" >"$OUT/build-$probe.log" 2>&1 ||
        { bad "build D3D12 $probe probe"; exit 1; }
done
fi
BX=${BETTERXEN_RUNNER:-"${CARGO_TARGET_DIR:-$ROOT/target}/release/bx-cli"}
HYBRID=${BETTERXEN_HYBRID_RUNNER:-"$(dirname "$BX")/bx-cli"}
EXE12="$OUT/bin/d3d12-integrated.exe"
EXE="$OUT/bin/d3d11-integrated.exe"
# Unique executable names separate lifecycle observations from other bottles. Names are
# never used as kill authority by the runner.
RUN_ID=$(basename "$OUT")
mv "$EXE" "$OUT/bin/d3d11-$RUN_ID.exe" || exit 1
mv "$EXE12" "$OUT/bin/d3d12-$RUN_ID.exe" || exit 1
EXE="$OUT/bin/d3d11-$RUN_ID.exe"
EXE12="$OUT/bin/d3d12-$RUN_ID.exe"
if [ "$(git -C "$ROOT" rev-parse --show-toplevel 2>/dev/null)" = "$ROOT" ]; then
    git -C "$ROOT" rev-parse HEAD >"$OUT/commit.txt"
    git -C "$ROOT" status --porcelain >>"$OUT/commit.txt"
elif [ -n "${BETTERXEN_SOURCE_COMMIT:-}" ]; then
    printf '%s\nprebuilt bundle; source archive SHA-256 verified by test.sh\n' "$BETTERXEN_SOURCE_COMMIT" >"$OUT/commit.txt"
else
    bad "source commit unavailable; use a Git checkout or the verified bundle test.sh"
    exit 1
fi
"$BX" hybrid receipt >"$OUT/receipt.json" 2>&1

if "$BX" hybrid doctor --bottle "$BOTTLE" >"$OUT/doctor.json" 2>&1; then ok "A environment"; else
    bad "A environment (see doctor.json)"; exit 1; fi
if "$BX" hybrid inspect "$EXE" >"$OUT/inspect.json" 2>&1; then ok "B executable identified"; else bad "B inspect"; fi

GATES=runtime.threads,runtime.memory,runtime.exceptions,runtime.time,runtime.files,graphics.init,shader.compile,frame.first,frame.continuous,resources.churn,input,memory.bounded,shutdown

# run <name> <expected-classification> [runner options...] -- [workload args...]
run() {
    name=$1; want=$2; shift 2
    opts=""
    while [ $# -gt 0 ] && [ "$1" != -- ]; do opts="$opts $1"; shift; done
    shift
    # shellcheck disable=SC2086 # options are fixed words from this script
    "$BX" hybrid run --bottle "$BOTTLE" --expect "PASS integrated" --keep-output true --out "$OUT/raw" $opts -- "$EXE" "$@" \
        >"$OUT/$name.json" 2>"$OUT/$name.log"
    got=$(sed -n 's/^  "classification": "\(.*\)",$/\1/p' "$OUT/$name.json")
    case "$got" in
        $want) ok "$name ($got)" ;;
        *) bad "$name: expected $want, got ${got:-no record}" ;;
    esac
}

run C-F-G-H-full passed --require-gates "$GATES" --timeout 180 --stall-timeout 60 -- --frames 600
run BC1-full passed --require-gates "$GATES,textures.bc1" --timeout 180 -- --frames 128 --bc1
run BC1-negative failed_exit_1 --timeout 120 -- --frames 128 --bc1 --fault bc1

# a negative control counts only if the gate the fault targets is the one that failed
intended() {
    case "$1" in
        input) echo input ;;
        stale) echo frame.continuous ;;
        bc1) echo textures.bc1 ;;
        *) echo "frame.first frame.continuous" ;;
    esac
}
detects() {
    # shellcheck disable=SC2046 # gate names are fixed words
    if python3 "$ROOT/scripts/hybrid/check-failed-gates.py" "$OUT/$1.json" $(intended "$2") >"$OUT/$1.gates.log" 2>&1;
    then ok "$1 failed its intended gate ($(cat "$OUT/$1.gates.log"))"
    else bad "$1: $(cat "$OUT/$1.gates.log")"; fi
}
detects BC1-negative bc1
for scene in C-F-G-H-full BC1-full; do
    if python3 "$ROOT/scripts/hybrid/check-image-digest.py" "$OUT/$scene.json" >"$OUT/oracle-$scene.log" 2>&1;
    then ok "independent image oracle $scene"; else bad "independent image oracle $scene (see log)"; fi
done
for fault in color blank stale input depth; do
    run "negative-$fault" failed_exit_1 --timeout 120 -- --frames 128 --fault "$fault"
    detects "negative-$fault" "$fault"
done
# With Wine's crash dialog on, winedbg waits for a click and this times out;
# scripts/hybrid/create-test-bottle.py turns it off in new test bottles.
run negative-crash 'failed_exit_*' --timeout 240 -- --frames 128 --fault crash
run recovery-after-crash passed --require-gates "$GATES" --timeout 120 -- --frames 128
run negative-hang stalled --timeout 90 --stall-timeout 15 -- --frames 128 --fault hang
if pgrep -f "$(basename "$EXE")" >/dev/null; then bad "hung workload still running after stop";
else ok "hung workload removed"; fi
run recovery-after-hang passed --require-gates "$GATES" --timeout 120 -- --frames 128

if python3 "$ROOT/tests/conformance/test_d3d11_integrated/check_window.py" "$HYBRID" "$BOTTLE" "$EXE" \
    --keep "$OUT/window.png" >"$OUT/window.log" 2>&1; then ok "F on-screen frame"; else
    bad "F on-screen frame: $(cat "$OUT/window.log")"; fi
if python3 "$ROOT/tests/conformance/test_d3d11_integrated/check_window.py" "$HYBRID" "$BOTTLE" "$EXE" \
    --fault depth >"$OUT/window-negative.log" 2>&1; then bad "on-screen check accepted a wrong image";
else
    if grep -q 'samples differ' "$OUT/window-negative.log"; then ok "on-screen negative control";
    else bad "on-screen negative control did not reach comparison"; fi
fi

i=1
while [ "$i" -le "$REPEATS" ]; do
    run "I-repeat-$i" passed --require-gates "$GATES" --timeout 180 -- --frames 600
    i=$((i + 1))
done
# Direct3D 12. Under D3DMetal (CrossOver 26.3) a destroyed D3D12 swapchain leaves a device
# reference, so the `shutdown` gate fails. That is reported as KNOWN only when it is the sole
# failing gate; any other failure is a failure.
d12_summary() {
    python3 - "$1" <<'PY'
import json, re, sys
j = json.load(open(sys.argv[1]))
failed = sorted(g["name"] for g in j["gates"] if not g["passed"])
digest = "".join(re.findall(r"digest=([0-9a-f]+)", " ".join(g["detail"] for g in j["gates"])))
backend = j["backend_detected"].split(" ")[0]
print(j["classification"], backend, ",".join(failed) or "-", digest or "-")
PY
}
digest11=$(sed -n 's/.*digest=\([0-9a-f]*\).*/\1/p' "$OUT/C-F-G-H-full.json" | head -1)
BX_SECRET_PROBE=1 "$BX" hybrid run --bottle "$BOTTLE" --keep-output true --out "$OUT/raw" --timeout 180 --stall-timeout 60 -- "$EXE12" \
    --expect-absent-env BX_SECRET_PROBE >"$OUT/d3d12-full.json" 2>"$OUT/d3d12-full.log"
read -r class backend failed12 digest12 <<EOT
$(d12_summary "$OUT/d3d12-full.json")
EOT
case "$class:$backend:$failed12" in
    passed:*:-) ok "D3D12 all gates (backend $backend)" ;;
    failed_exit_1:d3dmetal:shutdown)
        ok "D3D12 all gates but shutdown"
        known=1
        echo "KNOWN D3D12 shutdown: D3DMetal retains a device reference per swapchain" ;;
    *) bad "D3D12: $class backend=$backend failed=$failed12" ;;
esac
if [ -n "$digest11" ] && [ "$digest11" = "$digest12" ]; then ok "D3D11 and D3D12 pixel digests equal ($digest12)";
else bad "pixel digests differ: d3d11=$digest11 d3d12=$digest12"; fi
if python3 "$ROOT/scripts/hybrid/check-image-digest.py" "$OUT/d3d12-full.json" >"$OUT/oracle-d3d12.log" 2>&1;
then ok "independent image oracle D3D12"; else bad "independent image oracle D3D12 (see log)"; fi
if python3 "$ROOT/tests/conformance/test_d3d11_integrated/check_window.py" "$HYBRID" "$BOTTLE" "$EXE12" \
    --keep "$OUT/window12.png" >"$OUT/window12.log" 2>&1; then ok "D3D12 on-screen frame";
else bad "D3D12 on-screen frame: $(cat "$OUT/window12.log")"; fi
for api in d3d11 d3d12; do
    case "$api" in d3d11) first_exe=$EXE ;; d3d12) first_exe=$EXE12 ;; esac
    if python3 "$ROOT/tests/conformance/test_d3d11_integrated/check_window.py" "$HYBRID" "$BOTTLE" "$first_exe" \
        --first-frame --keep "$OUT/first-$api.png" >"$OUT/first-$api.log" 2>&1;
    then ok "$api first frame on screen"; else bad "$api first frame on screen (see log)"; fi
done
for fault in color blank stale input depth upload; do
    "$BX" hybrid run --bottle "$BOTTLE" --keep-output true --out "$OUT/raw" --timeout 90 -- "$EXE12" --frames 128 --fault "$fault" \
        >"$OUT/d3d12-negative-$fault.json" 2>/dev/null
    detects "d3d12-negative-$fault" "$fault"
done

# Minimal swapchain lifetime probe: no application GPU work. The detach control must pass
# and the deliberately retained reference must be caught, or the witness proves nothing.
LIFE="$OUT/bin/d3d12-lifetime.exe"
lifetime() {
    name=$1
    shift
    "$BX" hybrid run --bottle "$BOTTLE" --keep-output true --out "$OUT/raw" --timeout 90 -- "$LIFE" "$@" \
        >"$OUT/$name.json" 2>"$OUT/$name.log"
}
if lifetime lifetime-detach-control --detach-witness; then ok "lifetime witness detaches";
else bad "lifetime detach control (see lifetime-detach-control.log)"; fi
lifetime lifetime-retained-negative --detach-witness --retain-app-reference
if python3 "$ROOT/scripts/hybrid/check-failed-gates.py" "$OUT/lifetime-retained-negative.json" resources.private-data-detach >/dev/null 2>&1;
then ok "lifetime witness catches a retained reference"; else bad "lifetime retained-reference control"; fi
if lifetime lifetime-swapchain; then ok "D3D12 swapchain released"
elif grep -q 'adapter=AMD Compatibility Mode' "$OUT/lifetime-swapchain.json" &&
    python3 "$ROOT/scripts/hybrid/check-failed-gates.py" "$OUT/lifetime-swapchain.json" resources.swapchain-release >/dev/null 2>&1; then
    known=1
    echo "KNOWN D3D12 swapchain retention under D3DMetal (minimal probe)"
else bad "lifetime probe (see lifetime-swapchain.log)"; fi

DXC=${BETTERXEN_DXC_DIR:-$ROOT/artifacts/dependencies/dxc-v1.9.2609/bin/x64}
if [ -f "$DXC/dxc.exe" ]; then
    if python3 "$ROOT/scripts/hybrid/accept-sm6.py" "$BX" "$BOTTLE" "$OUT/bin/d3d12-wave-probe.exe" "$DXC" "$OUT/sm6" \
        >"$OUT/sm6.log" 2>&1; then ok "Shader Model 6 GPU readback"; else bad "Shader Model 6: $(tail -1 "$OUT/sm6.log")"; fi
else
    not_run=1
    echo "NOT-RUN Shader Model 6: no dxc.exe at $DXC"
fi
if python3 "$ROOT/tests/conformance/test_d3d12_integrated/send_keys.py" "$BX" "$BOTTLE" "$EXE12" >"$OUT/keys.log" 2>&1;
then ok "H macOS keyboard events"; else bad "H keyboard: $(tail -1 "$OUT/keys.log")"; fi
if python3 "$ROOT/tests/conformance/test_d3d12_integrated/send_keys.py" "$BX" "$BOTTLE" "$EXE12" --keys up,up \
    --expect fail >"$OUT/keys-negative.log" 2>&1; then ok "keyboard negative control";
else bad "keyboard negative control"; fi
if python3 "$ROOT/scripts/hybrid/accept-lifecycle.py" "$BX" "$BOTTLE" "$EXE" >"$OUT/lifecycle.log" 2>&1;
then ok "lifecycle: signals, kill and stop, repeated launch"; else
    bad "lifecycle (see lifecycle.log)"; grep FAIL "$OUT/lifecycle.log"; fi

# Direct3D 12 is the primary target, so it is soaked first; Direct3D 11 stays as the control.
"$BX" hybrid run --bottle "$BOTTLE" --keep-output true --out "$OUT/raw" --timeout 1800 --stall-timeout 60 -- "$EXE12" \
    --frames "$SOAK" --verify-every 500 >"$OUT/I-soak-d3d12.json" 2>"$OUT/I-soak-d3d12.log"
read -r class backend failed12 digest12 <<EOT
$(d12_summary "$OUT/I-soak-d3d12.json")
EOT
case "$class:$backend:$failed12" in
    passed:*:-) ok "I-soak D3D12 ($SOAK frames)" ;;
    failed_exit_1:d3dmetal:shutdown)
        known=1
        ok "I-soak D3D12 all gates but the known shutdown ($SOAK frames)" ;;
    *) bad "I-soak D3D12: $class backend=$backend failed=$failed12" ;;
esac
run I-soak-d3d11 passed --require-gates "$GATES" --timeout 1800 --stall-timeout 60 -- --frames "$SOAK" --verify-every 500

echo "records: $OUT"
python3 - "$OUT" "$failed" "$known" "$not_run" <<'PY'
import json, pathlib, sys
path = pathlib.Path(sys.argv[1])
failed, known, not_run = map(int, sys.argv[2:])
(path / "summary.json").write_text(json.dumps({
    "schema": "betterxen-integrated-acceptance/0",
    "unexpected_failures": bool(failed), "known_d3d12_swapchain_retention": bool(known),
    "checks_not_run": bool(not_run), "all_checks_passed": not (failed or known or not_run),
}, indent=2) + "\n")
PY
[ "$failed" -eq 0 ] || exit 1
[ "$known" -eq 0 ] && [ "$not_run" -eq 0 ] || exit 2
exit 0

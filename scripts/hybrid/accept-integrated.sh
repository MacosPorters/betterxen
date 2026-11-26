#!/bin/sh
# SPDX-License-Identifier: GPL-3.0-or-later
# Run the integrated-workload acceptance sequence through `betterxen hybrid` on this Mac.
# Every line printed is "ok <check>" or "FAIL <check>"; the exit status is non-zero if any
# check failed. Raw JSON records and logs are kept under the output directory.
#
#   scripts/hybrid/accept-integrated.sh <llvm-mingw-dir> <bottle> [soak-frames] [repeats]
#
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
OUT="$ROOT/artifacts/acceptance/$(date -u +%Y%m%dT%H%M%SZ)"
mkdir -p "$OUT" || exit 1
failed=0
ok() { echo "ok   $1"; }
bad() { echo "FAIL $1"; failed=1; }

(cd "$ROOT" && cargo build --locked --release -p betterxen -p betterxen-hybrid) >"$OUT/build-runner.log" 2>&1 ||
    { bad "build runner"; exit 1; }
"$ROOT/workloads/d3d11-integrated/build.sh" "$TC" "$OUT/bin" >"$OUT/build-workload.log" 2>&1 ||
    { bad "build workload"; exit 1; }
"$ROOT/workloads/d3d12-integrated/build.sh" "$TC" "$OUT/bin" >"$OUT/build-workload12.log" 2>&1 ||
    { bad "build D3D12 workload"; exit 1; }
BX="${CARGO_TARGET_DIR:-$ROOT/target}/release/betterxen"
EXE12="$OUT/bin/d3d12-integrated.exe"
EXE="$OUT/bin/d3d11-integrated.exe"
git -C "$ROOT" rev-parse HEAD >"$OUT/commit.txt"
git -C "$ROOT" status --porcelain >>"$OUT/commit.txt"
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
    "$BX" hybrid run --bottle "$BOTTLE" --expect "PASS integrated" $opts -- "$EXE" "$@" \
        >"$OUT/$name.json" 2>"$OUT/$name.log"
    got=$(sed -n 's/^  "classification": "\(.*\)",$/\1/p' "$OUT/$name.json")
    case "$got" in
        $want) ok "$name ($got)" ;;
        *) bad "$name: expected $want, got ${got:-no record}" ;;
    esac
}

run C-F-G-H-full passed --require-gates "$GATES" --timeout 180 --stall-timeout 60 -- --frames 600
for fault in color blank stale input depth; do
    run "negative-$fault" failed_exit_1 --timeout 120 -- --frames 128 --fault "$fault"
done
run negative-crash 'failed_exit_*' --timeout 60 -- --frames 128 --fault crash
run recovery-after-crash passed --require-gates "$GATES" --timeout 120 -- --frames 128
run negative-hang stalled --timeout 90 --stall-timeout 15 -- --frames 128 --fault hang
if pgrep -f 'd3d11-integrated\.exe' >/dev/null; then bad "hung workload still running after stop";
else ok "hung workload removed"; fi
run recovery-after-hang passed --require-gates "$GATES" --timeout 120 -- --frames 128

if python3 "$ROOT/workloads/d3d11-integrated/check_window.py" "${CARGO_TARGET_DIR:-$ROOT/target}/release/betterxen-hybrid" "$BOTTLE" "$EXE" \
    --keep "$OUT/window.png" >"$OUT/window.log" 2>&1; then ok "F on-screen frame"; else
    bad "F on-screen frame: $(cat "$OUT/window.log")"; fi
if python3 "$ROOT/workloads/d3d11-integrated/check_window.py" "${CARGO_TARGET_DIR:-$ROOT/target}/release/betterxen-hybrid" "$BOTTLE" "$EXE" \
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
BX_SECRET_PROBE=1 "$BX" hybrid run --bottle "$BOTTLE" --timeout 180 --stall-timeout 60 -- "$EXE12" \
    --expect-absent-env BX_SECRET_PROBE >"$OUT/d3d12-full.json" 2>"$OUT/d3d12-full.log"
read -r class backend failed12 digest12 <<EOT
$(d12_summary "$OUT/d3d12-full.json")
EOT
case "$class:$backend:$failed12" in
    passed:*:-) ok "D3D12 all gates (backend $backend)" ;;
    failed_exit_1:d3dmetal:shutdown)
        ok "D3D12 all gates but shutdown"
        echo "KNOWN D3D12 shutdown: D3DMetal retains a device reference per swapchain" ;;
    *) bad "D3D12: $class backend=$backend failed=$failed12" ;;
esac
if [ -n "$digest11" ] && [ "$digest11" = "$digest12" ]; then ok "D3D11 and D3D12 pixel digests equal ($digest12)";
else bad "pixel digests differ: d3d11=$digest11 d3d12=$digest12"; fi
for fault in color blank stale input depth upload; do
    "$BX" hybrid run --bottle "$BOTTLE" --timeout 90 -- "$EXE12" --frames 128 --fault "$fault" \
        >"$OUT/d3d12-negative-$fault.json" 2>/dev/null
    read -r class backend failed12 digest12 <<EOT
$(d12_summary "$OUT/d3d12-negative-$fault.json")
EOT
    # the fault must break a gate other than the known shutdown one
    case "$class:${failed12%shutdown}" in
        failed_exit_1:?*) ok "D3D12 negative-$fault ($failed12)" ;;
        *) bad "D3D12 negative-$fault: $class failed=$failed12" ;;
    esac
done
if python3 "$ROOT/workloads/d3d12-integrated/send_keys.py" "$BX" "$BOTTLE" "$EXE12" >"$OUT/keys.log" 2>&1;
then ok "H macOS keyboard events"; else bad "H keyboard: $(tail -1 "$OUT/keys.log")"; fi
if python3 "$ROOT/workloads/d3d12-integrated/send_keys.py" "$BX" "$BOTTLE" "$EXE12" --keys up,up \
    --expect fail >"$OUT/keys-negative.log" 2>&1; then ok "keyboard negative control";
else bad "keyboard negative control"; fi
if python3 "$ROOT/scripts/hybrid/accept-lifecycle.py" "$BX" "$BOTTLE" "$EXE" >"$OUT/lifecycle.log" 2>&1;
then ok "lifecycle: signals, kill and stop, repeated launch"; else
    bad "lifecycle (see lifecycle.log)"; grep FAIL "$OUT/lifecycle.log"; fi

run I-soak passed --require-gates "$GATES" --timeout 1800 --stall-timeout 60 -- --frames "$SOAK" --verify-every 500

echo "records: $OUT"
exit $failed

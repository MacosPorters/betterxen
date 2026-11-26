#!/bin/sh
# SPDX-License-Identifier: GPL-3.0-or-later
# Run the integrated-workload acceptance sequence through betterxen-hybrid on this Mac.
# Every line printed is "ok <check>" or "FAIL <check>"; the exit status is non-zero if any
# check failed. Raw JSON records and logs are kept under the output directory.
#
#   scripts/hybrid/accept-integrated.sh <llvm-mingw-dir> <bottle> [soak-frames] [repeats]
#
# Needs CrossOver, an existing win11_64 bottle, Rust 1.100.0, the pinned llvm-mingw
# (docs/research/hybrid-toolchain.md), and for the on-screen check a GUI session with the
# Screen Recording permission. Opens a small window; do not run on a busy machine.
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

(cd "$ROOT" && cargo build --locked --release -p betterxen-hybrid) >"$OUT/build-runner.log" 2>&1 ||
    { bad "build runner"; exit 1; }
"$ROOT/workloads/d3d11-integrated/build.sh" "$TC" "$OUT/bin" >"$OUT/build-workload.log" 2>&1 ||
    { bad "build workload"; exit 1; }
R="${CARGO_TARGET_DIR:-$ROOT/target}/release/betterxen-hybrid"
EXE="$OUT/bin/d3d11-integrated.exe"
git -C "$ROOT" rev-parse HEAD >"$OUT/commit.txt"
git -C "$ROOT" status --porcelain >>"$OUT/commit.txt"
"$R" receipt >"$OUT/receipt.json" 2>&1

if "$R" doctor --bottle "$BOTTLE" >"$OUT/doctor.json" 2>&1; then ok "A environment"; else
    bad "A environment (see doctor.json)"; exit 1; fi
if "$R" inspect "$EXE" >"$OUT/inspect.json" 2>&1; then ok "B executable identified"; else bad "B inspect"; fi

GATES=runtime.threads,runtime.memory,runtime.exceptions,runtime.time,runtime.files,graphics.init,shader.compile,frame.first,frame.continuous,resources.churn,input,memory.bounded,shutdown

# run <name> <expected-classification> [runner options...] -- [workload args...]
run() {
    name=$1; want=$2; shift 2
    opts=""
    while [ $# -gt 0 ] && [ "$1" != -- ]; do opts="$opts $1"; shift; done
    shift
    # shellcheck disable=SC2086 # options are fixed words from this script
    "$R" run --bottle "$BOTTLE" --expect "PASS integrated" $opts -- "$EXE" "$@" \
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

if python3 "$ROOT/workloads/d3d11-integrated/check_window.py" "$R" "$BOTTLE" "$EXE" \
    --keep "$OUT/window.png" >"$OUT/window.log" 2>&1; then ok "F on-screen frame"; else
    bad "F on-screen frame: $(cat "$OUT/window.log")"; fi
if python3 "$ROOT/workloads/d3d11-integrated/check_window.py" "$R" "$BOTTLE" "$EXE" \
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
run I-soak passed --require-gates "$GATES" --timeout 1800 --stall-timeout 60 -- --frames "$SOAK" --verify-every 500

echo "records: $OUT"
exit $failed

#!/bin/sh
# SPDX-License-Identifier: GPL-3.0-or-later
# Cross-build the vendored victory-shader test executable for Windows and run it under an
# installed CrossOver through `betterxen-hybrid run`, so every GPU test goes through D3D12 on the
# host Apple GPU (D3DMetal). Writes JSON run records under artifacts/hybrid-runs/.
#
#   scripts/hybrid/run-shader-gpu-tests.sh <llvm-mingw-dir> [bottle] [fixtures|all]
#
# `fixtures` (default) runs only BetterXen's own gfx1030 fixtures through the recompiler and
# D3D12; `all` runs the whole upstream suite, which currently has two known failures on D3DMetal
# (see docs/research/victory-shader-integration.md). Needs: Rust 1.100.0 with the
# x86_64-pc-windows-gnullvm target, an unpacked llvm-mingw, CrossOver, and a bottle.
set -eu
TC=${1:?usage: $0 <llvm-mingw-dir> [bottle] [fixtures|all]}
BOTTLE=${2:-betterxen-test}
WHICH=${3:-fixtures}
ROOT=$(cd "$(dirname "$0")/../.." && pwd)
cd "$ROOT"
# Both executables are taken from the messages of the cargo invocation that built them, and only
# when that invocation succeeded: no path is guessed and nothing left over from an earlier build can
# be run. (Cargo is not piped into the parser: under /bin/sh a pipeline reports only the parser's
# exit status.)
LOG=$(mktemp)
trap 'rm -f "$LOG"' EXIT
# built_exe <target name> <is test: true|false>: the executable named in $LOG.
built_exe() {
    python3 -I -c '
import json, sys
name, test, exe = sys.argv[1], sys.argv[2] == "true", ""
for line in open(sys.argv[3]):
    try:
        m = json.loads(line)
    except ValueError:
        continue
    t = m.get("target", {})
    if (m.get("reason") == "compiler-artifact" and m.get("executable")
            and t.get("name", "").replace("-", "_") == name
            and bool(m.get("profile", {}).get("test")) == test):
        exe = m["executable"]
print(exe)' "$1" "$2" "$LOG"
}

# The host tool first, before the environment is pointed at the Windows toolchain.
rustup run 1.100.0 cargo build -q -p betterxen-hybrid --message-format=json-render-diagnostics >"$LOG" \
    || { echo "building betterxen-hybrid failed" >&2; exit 1; }
B=$(built_exe betterxen_hybrid false)
[ -n "$B" ] && [ -x "$B" ] || { echo "the host build named no betterxen-hybrid executable" >&2; exit 1; }

export PATH="$TC/bin:$PATH"
export CARGO_TARGET_X86_64_PC_WINDOWS_GNULLVM_LINKER="$TC/bin/x86_64-w64-mingw32-clang"
export CARGO_TARGET_DIR="$ROOT/artifacts/victory-win"
rustup run 1.100.0 cargo test -q -p victory-shader --target x86_64-pc-windows-gnullvm --no-run \
    --message-format=json-render-diagnostics >"$LOG" \
    || { echo "building the victory-shader test executable failed" >&2; exit 1; }
EXE=$(built_exe victory_shader true)
[ -n "$EXE" ] && [ -f "$EXE" ] || { echo "the build named no victory-shader test executable" >&2; exit 1; }
file "$EXE" | grep -q "PE32+ executable.*x86-64" || { echo "not an x86-64 PE: $EXE" >&2; exit 1; }
rm -f "$LOG"
if [ "$WHICH" = all ]; then
    exec "$B" run --bottle "$BOTTLE" --timeout 900 --stall-timeout 120 --expect "test result: ok" \
        --out artifacts/hybrid-runs -- "$EXE" --test-threads=1
else
    # The exact count is deliberate: a looser marker would also accept a filter that matches nothing
    # ("0 passed"). Update it when BetterXen adds or removes tests in emit/betterxen_tests.rs.
    exec "$B" run --bottle "$BOTTLE" --timeout 300 --stall-timeout 120 --expect "test result: ok. 10 passed" \
        --out artifacts/hybrid-runs -- "$EXE" betterxen --test-threads=1
fi

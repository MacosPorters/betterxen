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
# Build the host tool first, into the normal target directory, before the Windows build redirects it.
rustup run 1.100.0 cargo build -q -p betterxen-hybrid
export PATH="$TC/bin:$PATH"
export CARGO_TARGET_X86_64_PC_WINDOWS_GNULLVM_LINKER="$TC/bin/x86_64-w64-mingw32-clang"
export CARGO_TARGET_DIR="$ROOT/artifacts/victory-win"
# Build the Windows test executable and take its path from cargo's own build message. It normally
# lands in <target>/x86_64-pc-windows-gnullvm/debug/deps/, but cargo's layout can differ with the
# build configuration, so the path is not guessed; `deps/` is only the fallback.
EXE=$(rustup run 1.100.0 cargo test -q -p victory-shader --target x86_64-pc-windows-gnullvm --no-run \
    --message-format=json | python3 -I -c '
import json, sys
exe = None
for line in sys.stdin:
    try:
        m = json.loads(line)
    except ValueError:
        continue
    if m.get("reason") == "compiler-artifact" and m.get("profile", {}).get("test") \
            and m.get("target", {}).get("name") in ("victory_shader", "victory-shader") and m.get("executable"):
        exe = m["executable"]
print(exe or "")')
if [ -z "$EXE" ]; then
    EXE=$(ls -t "$CARGO_TARGET_DIR"/x86_64-pc-windows-gnullvm/debug/deps/victory_shader-*.exe 2>/dev/null | head -1)
fi
[ -n "$EXE" ] && [ -f "$EXE" ] || { echo "could not find the victory-shader test executable" >&2; exit 1; }
file "$EXE" | grep -q "PE32+ executable.*x86-64" || { echo "not an x86-64 PE: $EXE" >&2; exit 1; }
B="$ROOT/target/debug/betterxen-hybrid"
if [ "$WHICH" = all ]; then
    exec "$B" run --bottle "$BOTTLE" --timeout 900 --stall-timeout 120 --expect "test result: ok" \
        --out artifacts/hybrid-runs -- "$EXE" --test-threads=1
else
    # The exact count is deliberate: a looser marker would also accept a filter that matches nothing
    # ("0 passed"). Update it when BetterXen adds or removes tests in emit/betterxen_tests.rs.
    exec "$B" run --bottle "$BOTTLE" --timeout 300 --stall-timeout 120 --expect "test result: ok. 7 passed" \
        --out artifacts/hybrid-runs -- "$EXE" betterxen --test-threads=1
fi

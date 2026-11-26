#!/bin/sh
# SPDX-License-Identifier: GPL-3.0-or-later
# Produce a native runner and original redistributable workloads without starting CrossOver.
set -eu
ROOT=$(CDPATH='' cd -- "$(dirname -- "$0")/../.." && pwd)
TC=${1:?usage: build-maintainer.sh LLVM_MINGW_DIR}
TC=$(CDPATH='' cd -- "$TC" && pwd)
"$ROOT/scripts/hybrid/check-toolchain.sh" "$TC"
export CARGO_BUILD_JOBS=${CARGO_BUILD_JOBS:-2}
export RUSTUP_TOOLCHAIN=1.100.0-aarch64-apple-darwin
OUT="$ROOT/artifacts/maintainer"
mkdir -p "$OUT/bin" "$OUT/workloads" "$OUT/logs"
cd "$ROOT"
cargo build --locked --release --target aarch64-apple-darwin -p bx-cli \
    >"$OUT/logs/build.log" 2>&1 || { cat "$OUT/logs/build.log" >&2; exit 1; }
for exe in bx-cli; do
    cp "${CARGO_TARGET_DIR:-$ROOT/target}/aarch64-apple-darwin/release/$exe" "$OUT/bin/$exe"
    file "$OUT/bin/$exe"
    [ "$(lipo -archs "$OUT/bin/$exe")" = arm64 ] || { echo "unexpected host architecture" >&2; exit 1; }
    otool -L "$OUT/bin/$exe" >"$OUT/logs/$exe-linkage.txt"
    python3 - "$OUT/logs/$exe-linkage.txt" <<'PY'
from pathlib import Path
import sys
libraries = [line.strip().split(' (', 1)[0] for line in Path(sys.argv[1]).read_text().splitlines()[1:]]
if not libraries or any(not lib.startswith(('/usr/lib/', '/System/Library/')) for lib in libraries):
    sys.exit(f'non-system or missing host linkage: {libraries}')
PY
done
scripts/hybrid/build-windows-workloads.sh "$TC" >"$OUT/logs/build-workloads.log" 2>&1 ||
    { cat "$OUT/logs/build-workloads.log" >&2; exit 1; }
for exe in selfcheck-c selfcheck-rust d3d11-clear d3d11-frameloop; do
    cp "artifacts/hybrid-workloads/x86_64/$exe.exe" "$OUT/workloads/"
done
for api in d3d11 d3d12; do
    "tests/conformance/test_${api}_integrated/build.sh" "$TC" "$OUT/workloads" >>"$OUT/logs/build-workloads.log" 2>&1
done
tests/conformance/test_d3d12_lifetime/build.sh "$TC" "$OUT/workloads" >>"$OUT/logs/build-workloads.log" 2>&1
tests/conformance/test_d3d12_wave/build.sh "$TC" "$OUT/workloads" >>"$OUT/logs/build-workloads.log" 2>&1
python3 scripts/hybrid/collect-build-notices.py "$TC" "$OUT/licenses"
git rev-parse HEAD >"$OUT/commit.txt"
git status --porcelain >"$OUT/source-status.txt"
"$OUT/bin/bx-cli" hybrid receipt >"$OUT/receipt.json"
(cd "$OUT" && shasum -a 256 bin/* workloads/*.exe > SHA256SUMS)
echo "Build and checksums: $OUT"

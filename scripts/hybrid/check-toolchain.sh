#!/bin/sh
# SPDX-License-Identifier: GPL-3.0-or-later
set -eu
TC=${1:?usage: check-toolchain.sh LLVM_MINGW_DIR}
ROOT=$(CDPATH='' cd -- "$(dirname -- "$0")/../.." && pwd)
PIN=$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["llvm_commit"])' "$ROOT/scripts/hybrid/toolchain.json")
[ "$(uname -s):$(uname -m)" = Darwin:arm64 ] || { echo "native Apple Silicon macOS is required" >&2; exit 1; }
for name in clang ld.lld; do
    [ -x "$TC/bin/$name" ] || { echo "missing $TC/bin/$name" >&2; exit 1; }
    lipo "$TC/bin/$name" -verify_arch arm64
done
"$TC/bin/x86_64-w64-mingw32-clang" --version | grep -F "$PIN" >/dev/null
echo "verified ARM64-capable llvm-mingw compiler at $PIN" >&2

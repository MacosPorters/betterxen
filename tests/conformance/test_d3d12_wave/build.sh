#!/bin/sh
# SPDX-License-Identifier: GPL-3.0-or-later
set -eu
TC=${1:?usage: build.sh LLVM_MINGW_DIR [OUTPUT_DIR]}
ROOT=$(CDPATH='' cd -- "$(dirname -- "$0")/../../.." && pwd)
OUT=${2:-"$ROOT/artifacts/hybrid-workloads/x86_64"}
"$ROOT/scripts/hybrid/check-toolchain.sh" "$TC"
mkdir -p "$OUT"
"$TC/bin/x86_64-w64-mingw32-clang" -std=c11 -O2 -Wall -Wextra -Werror -Wl,--no-insert-timestamp \
    "$ROOT/tests/conformance/test_d3d12_wave/d3d12_run.c" -o "$OUT/d3d12-wave-probe.exe" -ld3d12 -ldxgi -ldxguid

#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Build the x86_64 bx-d3dmetal-probe against a local utmapp/d3dmetal-native
# checkout that has already been built with meson (see ../README.md).
#
#   probe/build-probe.sh <d3dmetal-native checkout> [output directory]
#
# The output defaults to target/d3dmetal-harness/ at the repository root.
# Nothing from D3DMetal.framework is linked here; d3dmetal-native loads the
# user-supplied framework at run time.
set -euo pipefail

# utmapp/d3dmetal-native revision evaluated for M1-D2 (docs/research/upstream-registry.md).
PINNED_REVISION="af1ee1710711fadff41dead87960d0f1336c1047"

if [[ $# -lt 1 || $# -gt 2 ]]; then
    echo "usage: $0 <d3dmetal-native checkout> [output directory]" >&2
    exit 64
fi

PROBE_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$PROBE_DIR/../../../.." && pwd)"
DMN_DIR="$(cd "$1" && pwd)"
OUT_DIR="${2:-$REPO_ROOT/target/d3dmetal-harness}"
DMN_LIB_DIR="$DMN_DIR/build"

if [[ "$(uname -s)" != "Darwin" ]]; then
    echo "error: the probe targets macOS only" >&2
    exit 1
fi
if [[ ! -f "$DMN_LIB_DIR/libd3dmetal-native.dylib" ]]; then
    echo "error: $DMN_LIB_DIR/libd3dmetal-native.dylib not found;" \
         "run 'meson setup build --cross-file build-macos-x86_64.txt && meson compile -C build' in $DMN_DIR" >&2
    exit 1
fi
if [[ ! -d "$DMN_DIR/subprojects/directx-native/include" ]]; then
    echo "error: $DMN_DIR/subprojects/directx-native is missing; run meson setup first" >&2
    exit 1
fi

actual_revision="$(git -C "$DMN_DIR" rev-parse HEAD 2>/dev/null || echo unknown)"
if [[ "$actual_revision" != "$PINNED_REVISION" ]]; then
    echo "warning: d3dmetal-native is at $actual_revision, not the evaluated $PINNED_REVISION" >&2
fi

mkdir -p "$OUT_DIR"
clang++ -arch x86_64 -mmacosx-version-min=14.0 -std=c++17 -O2 \
    -Wall -Wextra -Werror \
    -DWIDL_EXPLICIT_AGGREGATE_RETURNS=1 \
    -I "$PROBE_DIR" \
    -I "$DMN_DIR/include" \
    -I "$DMN_DIR/include/native/windows" \
    -I "$DMN_DIR/include/native/directx" \
    -I "$DMN_DIR/subprojects/directx-native/include" \
    "$PROBE_DIR/bx_d3dmetal_probe.cpp" \
    -L "$DMN_LIB_DIR" -ld3dmetal-native \
    -Wl,-rpath,"$DMN_LIB_DIR" \
    -o "$OUT_DIR/bx-d3dmetal-probe"

echo "built $OUT_DIR/bx-d3dmetal-probe ($(lipo -archs "$OUT_DIR/bx-d3dmetal-probe"))"

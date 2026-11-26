#!/bin/sh
# SPDX-License-Identifier: GPL-3.0-or-later
# Build the Direct3D 12 workload with the pinned llvm-mingw (docs/research/hybrid-toolchain.md).
#   tests/conformance/test_d3d12_integrated/build.sh <llvm-mingw-dir> [output-dir]
set -eu
TC=${1:?usage: build.sh LLVM_MINGW_DIR [OUTPUT_DIR]}
src=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)
out=${2:-"$src/../../../artifacts/hybrid-workloads/x86_64"}
mkdir -p "$out"
"$TC/bin/x86_64-w64-mingw32-clang" -std=c11 -O2 -Wall -Wextra -Werror -Wl,--no-insert-timestamp \
    -o "$out/d3d12-integrated.exe" "$src/integrated12.c" \
    -ld3d12 -ldxgi -ldxguid -ld3dcompiler -lpsapi -luser32
description=$(file -b "$out/d3d12-integrated.exe")
echo "$description"
case "$description" in
    *PE32+*x86-64*) ;;
    *) exit 1 ;;
esac

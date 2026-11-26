#!/bin/sh
# SPDX-License-Identifier: GPL-3.0-or-later
set -eu
toolchain=${1:?usage: build.sh LLVM_MINGW_DIR VKD3D_COMPILER [OUTPUT_DIR]}
shader_compiler=${2:?supply the native vkd3d-compiler 2.2 executable}
src=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)
out=${3:-"$src/../../artifacts/hybrid-workloads/x86_64"}
mkdir -p "$out"

version=$(arch -arm64 "$shader_compiler" --version)
case "$version" in
    'vkd3d shader compiler version 2.2 using vkd3d-shader 2.2'*) ;;
    *) printf 'Expected vkd3d-compiler and vkd3d-shader 2.2, got %s\n' "$version" >&2; exit 1 ;;
esac

# Compile original HLSL to D3D11-compatible shader model 5 tokenized bytecode.
arch -arm64 "$shader_compiler" -x hlsl -b dxbc-tpf -p cs_5_0 -e main \
    -o "$out/compute.dxbc" "$src/compute.hlsl"
xxd -i < "$out/compute.dxbc" > "$out/compute_dxbc.inc"
arch -arm64 "$toolchain/bin/x86_64-w64-mingw32-clang" \
    -std=c11 -O2 -Wall -Wextra -Werror -Wl,--no-insert-timestamp \
    -I "$out" "$src/compute.c" -o "$out/d3d11-compute.exe" -ld3d11 -ldxguid
description=$(file -b "$out/d3d11-compute.exe")
echo "$description"
case "$description" in
    *PE32+*x86-64*) ;;
    *) exit 1 ;;
esac

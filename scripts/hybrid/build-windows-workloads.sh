#!/bin/sh
# SPDX-License-Identifier: GPL-3.0-or-later
# Build BetterXen's original Windows workloads (issue #364) with a pinned cross toolchain and
# write the results to artifacts/hybrid-workloads/<arch>/. Nothing is executed; the script ends
# by printing `file` and the PE machine type of every output so a build that produced the wrong
# architecture fails here.
#
#   scripts/hybrid/build-windows-workloads.sh <llvm-mingw-dir>
#
# <llvm-mingw-dir> is an unpacked llvm-mingw release (https://github.com/mstorsjo/llvm-mingw,
# Apache-2.0 with LLVM exceptions); see docs/research/hybrid-toolchain.md for the pinned
# release and checksum. Rust workloads use the x86_64-pc-windows-gnullvm and
# aarch64-pc-windows-gnullvm targets (rustup target add ...) linked with that toolchain.
set -eu
TC=${1:?usage: $0 <llvm-mingw-dir>}
ROOT=$(cd "$(dirname "$0")/../.." && pwd)
OUT="$ROOT/artifacts/hybrid-workloads"
RUSTC_TC=${RUSTUP_TOOLCHAIN_FOR_WORKLOADS:-1.100.0}
[ -x "$TC/bin/x86_64-w64-mingw32-clang" ] || { echo "no llvm-mingw at $TC" >&2; exit 1; }

for arch in x86_64 aarch64; do
    d="$OUT/$arch"; mkdir -p "$d"
    "$TC/bin/$arch-w64-mingw32-clang" -O2 -Wall -Wextra -Werror -Wl,--no-insert-timestamp \
        -o "$d/selfcheck-c.exe" "$ROOT/workloads/win-selfcheck-c/selfcheck.c"
    "$TC/bin/$arch-w64-mingw32-clang" -O2 -Wall -Wextra -Werror -Wl,--no-insert-timestamp \
        -o "$d/d3d11-clear.exe" "$ROOT/workloads/d3d11-clear/d3d11_clear.c" -ld3d11 -ldxgi
    "$TC/bin/$arch-w64-mingw32-clang" -O2 -Wall -Wextra -Werror -Wl,--no-insert-timestamp \
        -o "$d/d3d11-frameloop.exe" "$ROOT/workloads/d3d11-frameloop/frameloop.c" -ld3d11 -ldxgi -ldxguid -ld3dcompiler -lm
    env_arch=$(echo "$arch" | tr a-z A-Z)
    (
        export PATH="$TC/bin:$PATH"
        export "CARGO_TARGET_${env_arch}_PC_WINDOWS_GNULLVM_LINKER=$TC/bin/$arch-w64-mingw32-clang"
        cd "$ROOT/workloads/win-selfcheck-rust"
        CARGO_TARGET_DIR="$OUT/rust-target" rustup run "$RUSTC_TC" cargo build --release --target "$arch-pc-windows-gnullvm" -q
    )
    cp "$OUT/rust-target/$arch-pc-windows-gnullvm/release/win-selfcheck-rust.exe" "$d/selfcheck-rust.exe"
done

fail=0
for f in "$OUT"/x86_64/*.exe "$OUT"/aarch64/*.exe; do
    want=$(basename "$(dirname "$f")")
    desc=$(file -b "$f")
    case "$want:$desc" in
        x86_64:*"PE32+"*"x86-64"*) ok=ok ;;
        aarch64:*"PE32+"*"Aarch64"*|aarch64:*"PE32+"*"ARM64"*) ok=ok ;;
        *) ok=WRONG; fail=1 ;;
    esac
    echo "$ok  $f: $desc"
done
exit $fail

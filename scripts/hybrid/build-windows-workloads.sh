#!/bin/sh
# SPDX-License-Identifier: GPL-3.0-or-later
# Build BetterXen's original Windows workloads (issue #364) with a pinned cross toolchain and
# write the results to artifacts/hybrid-workloads/<arch>/. Nothing is executed; the script ends
# by printing `file` and the PE machine type of every output so a build that produced the wrong
# architecture fails here.
#
#   scripts/hybrid/build-windows-workloads.sh <llvm-mingw-dir>
#
# <llvm-mingw-dir> is an unpacked llvm-mingw release (https://github.com/mstorsjo/llvm-mingw;
# wrappers/scripts ISC, compiler and runtime under their component licences);
# see https://github.com/MacosPorters/betterxen/blob/54264e23a417e861834ee879f810717385b91f58/docs/research/hybrid-toolchain.md for the pinned
# release and checksum. Rust workloads use the x86_64-pc-windows-gnullvm and
# aarch64-pc-windows-gnullvm targets (rustup target add ...) linked with that toolchain.
set -eu
TC=$(CDPATH='' cd -- "${1:?usage: $0 <llvm-mingw-dir>}" && pwd)
ROOT=$(cd "$(dirname "$0")/../.." && pwd)
OUT="$ROOT/artifacts/hybrid-workloads"
RUSTC_TC=${RUSTUP_TOOLCHAIN_FOR_WORKLOADS:-1.100.0}
[ -x "$TC/bin/x86_64-w64-mingw32-clang" ] || { echo "no llvm-mingw at $TC" >&2; exit 1; }
"$ROOT/scripts/hybrid/check-toolchain.sh" "$TC"

for arch in x86_64; do
    d="$OUT/$arch"; mkdir -p "$d"
    "$TC/bin/$arch-w64-mingw32-clang" -O2 -Wall -Wextra -Werror -Wl,--no-insert-timestamp \
        -o "$d/selfcheck-c.exe" "$ROOT/tests/conformance/test_runtime_c/selfcheck.c"
    "$TC/bin/$arch-w64-mingw32-clang" -O2 -Wall -Wextra -Werror -Wl,--no-insert-timestamp \
        -o "$d/d3d11-clear.exe" "$ROOT/tests/conformance/test_d3d11_clear/d3d11_clear.c" -ld3d11 -ldxgi
    "$TC/bin/$arch-w64-mingw32-clang" -O2 -Wall -Wextra -Werror -Wl,--no-insert-timestamp \
        -o "$d/d3d11-frameloop.exe" "$ROOT/tests/conformance/test_d3d11_frameloop/frameloop.c" -ld3d11 -ldxgi -ldxguid -ld3dcompiler -lm
    env_arch=$(echo "$arch" | tr a-z A-Z)
    (
        export PATH="$TC/bin:$PATH"
        export "CARGO_TARGET_${env_arch}_PC_WINDOWS_GNULLVM_LINKER=$TC/bin/$arch-w64-mingw32-clang"
        export CARGO_ENCODED_RUSTFLAGS
        CARGO_ENCODED_RUSTFLAGS=$(printf '%s\037%s\037%s' '-C' 'link-arg=-Wl,--no-insert-timestamp' "--remap-path-prefix=$ROOT=/betterxen")
        cd "$ROOT/tests/conformance/support/win-selfcheck-rust"
        CARGO_TARGET_DIR="$OUT/rust-target" rustup run "$RUSTC_TC" cargo build --locked --release --target "$arch-pc-windows-gnullvm" -q
    )
    cp "$OUT/rust-target/$arch-pc-windows-gnullvm/release/win-selfcheck-rust.exe" "$d/selfcheck-rust.exe"
done

fail=0
for f in "$OUT"/x86_64/*.exe; do
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

#!/bin/sh
# SPDX-License-Identifier: GPL-3.0-or-later
# Install only open-source build dependencies. CrossOver and Apple tools remain external.
set -eu
ROOT=$(CDPATH='' cd -- "$(dirname -- "$0")/../.." && pwd)
[ "$(uname -s):$(uname -m)" = Darwin:arm64 ] || { echo "native Apple Silicon macOS is required" >&2; exit 1; }
command -v python3 >/dev/null
command -v rustup >/dev/null || { echo "install native rustup from https://rustup.rs first" >&2; exit 1; }
xcrun --find clang >/dev/null
TC=$(python3 "$ROOT/scripts/hybrid/install-toolchain.py" "${1:-$HOME/Library/Caches/BetterXen/toolchains}")
"$ROOT/scripts/hybrid/check-toolchain.sh" "$TC"
rustup toolchain install 1.100.0 --profile minimal --component rustfmt --component clippy \
    --target aarch64-apple-darwin --target x86_64-pc-windows-gnullvm >&2
printf '%s\n' "$TC"

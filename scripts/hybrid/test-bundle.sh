#!/bin/sh
# SPDX-License-Identifier: GPL-3.0-or-later
# Installed as test.sh at the root of a private maintainer bundle. No Rust or cross-compiler
# is needed: both runners and original workload binaries were built from the recorded commit.
set -eu
ROOT=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)
BOTTLE=${1:?usage: ./test.sh NEW_DEDICATED_BOTTLE_NAME}
[ "$(uname -s):$(uname -m)" = Darwin:arm64 ] || { echo "native Apple Silicon macOS is required" >&2; exit 1; }
cd "$ROOT"
/usr/bin/shasum -a 256 -c SHA256SUMS
/usr/bin/shasum -a 256 -c SOURCE-SHA256SUMS
BUNDLE_TEST=$(mktemp -d "$ROOT/test-XXXXXX")
tar -xzf "$ROOT/source.tar.gz" -C "$BUNDLE_TEST"
SOURCE="$BUNDLE_TEST/betterxen-source"
BX="$ROOT/bin/bx-cli"
python3 "$SOURCE/scripts/hybrid/create-test-bottle.py" "$BX" "$BOTTLE" >"$BUNDLE_TEST/bottle.json"
"$BX" hybrid doctor --bottle "$BOTTLE" >"$BUNDLE_TEST/doctor.json"
echo "Testing commit $(cat "$ROOT/commit.txt"); diagnostics under $BUNDLE_TEST"
export BETTERXEN_RUNNER="$BX"
export BETTERXEN_HYBRID_RUNNER="$BX"
export BETTERXEN_WORKLOAD_DIR="$ROOT/workloads"
export BETTERXEN_SOURCE_COMMIT
BETTERXEN_SOURCE_COMMIT=$(cat "$ROOT/commit.txt")
exec "$SOURCE/scripts/hybrid/accept-integrated.sh" unused "$BOTTLE"

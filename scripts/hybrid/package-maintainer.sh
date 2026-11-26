#!/bin/sh
# SPDX-License-Identifier: GPL-3.0-or-later
# Private maintainer test bundle: original source, ARM64 runners and original Windows probes.
set -eu
ROOT=$(CDPATH='' cd -- "$(dirname -- "$0")/../.." && pwd)
cd "$ROOT"
[ -z "$(git status --porcelain)" ] || { echo "package requires a clean committed checkout" >&2; exit 1; }
COMMIT=$(git rev-parse HEAD)
OUT="$ROOT/artifacts/maintainer"
[ "$(cat "$OUT/commit.txt")" = "$COMMIT" ] && [ ! -s "$OUT/source-status.txt" ] ||
    { echo "rebuild from this clean commit before packaging" >&2; exit 1; }
(cd "$OUT" && shasum -a 256 -c SHA256SUMS)
mkdir -p "$ROOT/artifacts/packages"
STAGE=$(mktemp -d "$ROOT/artifacts/packages/staging-XXXXXX")
trap 'rm -rf "$STAGE"' EXIT HUP INT TERM
NAME="betterxen-maintainer-$COMMIT"
[ ! -e "$ROOT/artifacts/packages/$NAME.tar.gz" ] || { echo "artifact already exists; preserving it" >&2; exit 1; }
mkdir "$STAGE/$NAME"
mkdir "$STAGE/$NAME/bin"
# The native research CLI has an unresolved redistribution gate in docs/licenses.md.
# The standalone hybrid binary has the same runtime commands and depends only on libc.
cp "$OUT/bin/bx-cli" "$STAGE/$NAME/bin/"
mkdir "$STAGE/$NAME/workloads"
for exe in selfcheck-c selfcheck-rust d3d11-clear d3d11-frameloop d3d11-integrated d3d12-integrated d3d12-lifetime d3d12-wave-probe; do
    cp "$OUT/workloads/$exe.exe" "$STAGE/$NAME/workloads/"
done
cp -R "$OUT/commit.txt" "$OUT/licenses" "$STAGE/$NAME/"
(cd "$STAGE/$NAME" && shasum -a 256 bin/* workloads/*.exe > SHA256SUMS)
cp LICENSE THIRD_PARTY_NOTICES.md docs/maintainer-testing.md "$STAGE/$NAME/"
cp scripts/hybrid/test-bundle.sh "$STAGE/$NAME/test.sh"
chmod +x "$STAGE/$NAME/test.sh"
git archive --format=tar.gz --prefix=betterxen-source/ -o "$STAGE/$NAME/source.tar.gz" "$COMMIT"
(cd "$STAGE/$NAME" && shasum -a 256 source.tar.gz > SOURCE-SHA256SUMS)
tar -czf "$ROOT/artifacts/packages/$NAME.tar.gz" -C "$STAGE" "$NAME"
(cd "$ROOT/artifacts/packages" && shasum -a 256 "$NAME.tar.gz" > "$NAME.tar.gz.sha256")
echo "$ROOT/artifacts/packages/$NAME.tar.gz"

#!/bin/sh
# SPDX-License-Identifier: GPL-3.0-or-later
# Regression test for run-shader-gpu-tests.sh with a stubbed `rustup`/cargo and a stubbed runner:
# no toolchain, CrossOver or GPU is needed. Checks that
#   1. a failed Windows test build stops the script and the runner is never invoked, even when an
#      older test executable is lying in the target directory;
#   2. a successful build runs exactly the executable that build named;
#   3. the host runner is the one the host build named, also with an inherited CARGO_TARGET_DIR.
set -u
HERE=$(cd "$(dirname "$0")" && pwd)
T=$(mktemp -d)
trap 'rm -rf "$T"' EXIT
fails=0
check() { if eval "$2"; then echo "ok   $1"; else echo "FAIL $1"; fails=1; fi; }

# A file `file` reports as an x86-64 PE32+ executable.
make_pe() {
    python3 -I -c '
import struct, sys
b = bytearray(512); b[0:2] = b"MZ"; struct.pack_into("<I", b, 0x3C, 0x80)
b[0x80:0x84] = b"PE\0\0"; struct.pack_into("<HHIIIHH", b, 0x84, 0x8664, 0, 0, 0, 0, 240, 0x22)
struct.pack_into("<H", b, 0x98, 0x20B); struct.pack_into("<H", b, 0x98 + 68, 3)
open(sys.argv[1], "wb").write(b)' "$1"
}

setup() { # $1 = case directory
    R=$1/repo
    mkdir -p "$R/scripts/hybrid" "$1/bin" "$1/tc/bin"
    cp "$HERE/run-shader-gpu-tests.sh" "$R/scripts/hybrid/"
    : > "$1/tc/bin/x86_64-w64-mingw32-clang"; chmod +x "$1/tc/bin/x86_64-w64-mingw32-clang"
    # The stub runner records how it was called.
    runner() { mkdir -p "$(dirname "$1")"; printf '#!/bin/sh\necho "$0 $*" >> "%s"\n' "$2" > "$1"; chmod +x "$1"; }
    # A runner left over from an earlier build at the old hard-coded place.
    runner "$R/target/debug/betterxen-hybrid" "$1/stale-runner.log"
    # An older test executable lying around in the target directory.
    old="$R/artifacts/victory-win/x86_64-pc-windows-gnullvm/debug/deps"
    mkdir -p "$old"; make_pe "$old/victory_shader-0000stale.exe"
    cat > "$1/bin/rustup" <<STUB
#!/bin/sh
# stands in for: rustup run 1.100.0 cargo <build|test> ...
shift 3
cmd=\$1
dir=\${CARGO_TARGET_DIR:-$R/target}
case "\$cmd" in
build)
    mkdir -p "\$dir/debug"
    printf '#!/bin/sh\necho "\$0 \$*" >> "%s"\n' "$1/runner.log" > "\$dir/debug/betterxen-hybrid"
    chmod +x "\$dir/debug/betterxen-hybrid"
    echo "{\"reason\":\"compiler-artifact\",\"target\":{\"name\":\"betterxen-hybrid\",\"kind\":[\"bin\"]},\"profile\":{\"test\":false},\"executable\":\"\$dir/debug/betterxen-hybrid\"}"
    ;;
test)
    [ "\${STUB_TEST_BUILD_FAILS:-0}" = 1 ] && exit 101
    mkdir -p "\$dir/fresh"; cp "$old/victory_shader-0000stale.exe" "\$dir/fresh/victory_shader-1111new.exe"
    echo "{\"reason\":\"compiler-artifact\",\"target\":{\"name\":\"victory_shader\",\"kind\":[\"lib\"]},\"profile\":{\"test\":true},\"executable\":\"\$dir/fresh/victory_shader-1111new.exe\"}"
    ;;
esac
STUB
    chmod +x "$1/bin/rustup"
}

# 1. failed test build
c=$T/fail; setup "$c"
STUB_TEST_BUILD_FAILS=1 PATH="$c/bin:$PATH" sh "$c/repo/scripts/hybrid/run-shader-gpu-tests.sh" "$c/tc" >/dev/null 2>&1; rc=$?
check "failed build: script exits non-zero (got $rc)" "[ $rc -ne 0 ]"
check "failed build: no runner is invoked" "[ ! -e '$c/runner.log' ] && [ ! -e '$c/stale-runner.log' ]"

# 2. successful build
c=$T/ok; setup "$c"
PATH="$c/bin:$PATH" sh "$c/repo/scripts/hybrid/run-shader-gpu-tests.sh" "$c/tc" >/dev/null 2>&1; rc=$?
check "good build: script succeeds (got $rc)" "[ $rc -eq 0 ]"
check "good build: runs the executable this build named" "grep -q 'victory_shader-1111new.exe' '$c/runner.log' 2>/dev/null"

# 3. inherited CARGO_TARGET_DIR
c=$T/dir; setup "$c"
CARGO_TARGET_DIR="$c/elsewhere" PATH="$c/bin:$PATH" sh "$c/repo/scripts/hybrid/run-shader-gpu-tests.sh" "$c/tc" >/dev/null 2>&1; rc=$?
check "inherited target dir: script succeeds (got $rc)" "[ $rc -eq 0 ]"
check "inherited target dir: the freshly built runner is used" "grep -q '$c/elsewhere/debug/betterxen-hybrid' '$c/runner.log' 2>/dev/null"
check "inherited target dir: the stale runner is not used" "[ ! -e '$c/stale-runner.log' ]"
exit $fails

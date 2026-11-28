#!/bin/sh
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Builds bx_shader_bridge.dll (this crate for x86_64-pc-windows-gnu) and the guest check
# check_bridge.exe into one directory (default: <repo>/target/bx-shader-bridge), then checks
# the DLL: its Export Address Table holds exactly the three bx_shader_bridge.h entry points as
# direct RVAs (no extra named or ordinal-only entries, no forwarders), and every imported DLL
# is a system DLL Wine provides. Run the check under CrossOver afterwards, for example:
#   BX_RUN="$(mktemp -d "${TMPDIR:-/tmp}/bx-run.XXXXXX")"
#   target/debug/bx-cli run --prefix "$BX_RUN/prefix" --shims target/bx-shader-bridge \
#       --exe target/bx-shader-bridge/check_bridge.exe
#
# Needs: rustup with the x86_64-pc-windows-gnu target, x86_64-w64-mingw32-gcc / -objdump
# (mingw-w64), and python3.
set -eu

crate=$(cd "$(dirname "$0")/.." && pwd)
root=$(cd "$crate/../../.." && pwd)
out=${1:-$root/target/bx-shader-bridge}
target=x86_64-pc-windows-gnu
cc=${BX_MINGW_CC:-x86_64-w64-mingw32-gcc}
objdump=${BX_MINGW_OBJDUMP:-x86_64-w64-mingw32-objdump}
# Always build here and copy from here, whatever CARGO_TARGET_DIR or build.target-dir say,
# so a stale DLL left elsewhere can never be packaged.
target_dir=$root/target

for tool in "$cc" "$objdump" python3; do
    command -v "$tool" >/dev/null 2>&1 || { echo "build.sh: $tool not found" >&2; exit 1; }
done

# Use one toolchain for both the target check and the build. With rustup, resolve cargo and
# rustc through it (a cargo earlier on PATH, e.g. Homebrew's, has no Windows std).
if command -v rustup >/dev/null 2>&1; then
    cargo=$(cd "$root" && rustup which cargo)
    rustc=$(cd "$root" && rustup which rustc)
else
    cargo=$(command -v cargo) || { echo "build.sh: cargo not found" >&2; exit 1; }
    rustc=$(command -v rustc) || { echo "build.sh: rustc not found" >&2; exit 1; }
fi
libdir=$("$rustc" --print target-libdir --target "$target" 2>/dev/null || true)
if [ -z "$libdir" ] || ! ls "$libdir"/libstd-*.rlib >/dev/null 2>&1; then
    echo "build.sh: $rustc ($("$rustc" --version)) has no std for $target" >&2
    echo "build.sh: install it with: rustup target add $target" >&2
    exit 1
fi
echo "build.sh: using $("$rustc" --version) at $rustc"

RUSTC=$rustc CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER=$cc \
    "$cargo" build --locked --release -p dxbc-accc --target "$target" \
    --target-dir "$target_dir" --manifest-path "$root/Cargo.toml"
mkdir -p "$out"
cp "$target_dir/$target/release/dxbc_accc.dll" "$out/bx_shader_bridge.dll"
dll=$out/bx_shader_bridge.dll

# Same reading of `objdump -p` as scripts/build-shims.sh: every populated Export Address Table
# slot must be a direct RVA owned by one of the expected names.
"$objdump" -p "$dll" | python3 -I -c '
import re, sys
expected = {"bx_shader_inspect", "bx_shader_prepare_for_d3dmetal", "bx_shader_free_buffer"}
# The Windows-provided system DLLs a Rust std cdylib imports; anything else is an error.
allowed = {"kernel32.dll", "ntdll.dll", "bcryptprimitives.dll", "userenv.dll", "ws2_32.dll"}
allowed_prefixes = ("api-ms-win-crt-", "api-ms-win-core-")
eat, names, imports, mode = {}, {}, [], None
for line in sys.stdin:
    line = line.rstrip("\n")
    m = re.match(r"^\s*DLL Name:\s*(\S+)", line)
    if m:
        imports.append(m.group(1))
    if line.startswith("Export Address Table --"):
        mode = "eat"; continue
    if line.startswith("[Ordinal/Name Pointer] Table --"):
        mode = "names"; continue
    if not line.strip():
        mode = None; continue
    if mode == "eat":
        m = re.match(r"^\s*\[\s*\d+\]\s*\+base\[\s*(\d+)\]\s+[0-9a-fA-F]+\s+(Export|Forwarder) RVA", line)
        if m:
            eat[int(m.group(1))] = m.group(2)
    elif mode == "names":
        m = re.match(r"^\s*\[\s*\d+\]\s*\+base\[\s*(\d+)\]\s+[0-9a-fA-F]+\s+(\S+)", line)
        if m:
            names[m.group(2)] = int(m.group(1))
errors = []
if set(names) != expected:
    errors.append("named exports %s, want %s" % (sorted(names), sorted(expected)))
owned = set(names.values())
for ordinal, kind in sorted(eat.items()):
    if ordinal not in owned:
        errors.append("export @%d has no name (ordinal-only or stray entry)" % ordinal)
    if kind != "Export":
        errors.append("export @%d is a forwarder" % ordinal)
for name, ordinal in names.items():
    if ordinal not in eat:
        errors.append("%s @%d missing from the Export Address Table" % (name, ordinal))
for dll in imports:
    low = dll.lower()
    if low not in allowed and not low.startswith(allowed_prefixes):
        errors.append("unexpected import %s (only Wine-provided system DLLs are allowed)" % dll)
if errors:
    sys.exit("build.sh: bx_shader_bridge.dll: " + "; ".join(errors))
print("build.sh: exports %s; imports %s" % (" ".join(sorted(names)), " ".join(imports)))
'

"$cc" -std=c11 -O2 -Wall -Wextra -Werror -static \
    -I"$root/abi/include" -I"$crate/../d3dmetal-harness/probe/shaders" \
    -o "$out/check_bridge.exe" "$crate/guest/check_bridge.c"

echo "built $dll"
echo "built $out/check_bridge.exe"

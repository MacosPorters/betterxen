#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Preserve unmodified notices for the standalone hybrid runner and Windows runtime."""
import json
from pathlib import Path
import shutil
import subprocess
import sys

root = Path(__file__).resolve().parents[2]
toolchain, output = map(lambda p: Path(p).resolve(), sys.argv[1:])
output.mkdir(parents=True, exist_ok=True)
meta = json.loads(subprocess.check_output(["cargo", "metadata", "--locked", "--format-version", "1"], cwd=root))
libc = [p for p in meta["packages"] if p["name"] == "libc"]
if len(libc) != 1:
    sys.exit("expected exactly one locked libc dependency")
package = libc[0]
source = Path(package["manifest_path"]).parent
shutil.copytree(source, output / "libc-source", dirs_exist_ok=True)
notices = [source / "LICENSE-MIT", source / "LICENSE-APACHE"]
for path in notices:
    shutil.copy2(path, output / ("libc-" + path.name))
shutil.copy2(toolchain / "LICENSE.TXT", output / "llvm-mingw-LICENSE.TXT")
for path in (toolchain / "x86_64-w64-mingw32/share/mingw32").glob("COPYING*"):
    shutil.copy2(path, output / ("mingw-" + path.name))
sysroot = Path(subprocess.check_output(["rustc", "--print", "sysroot"], text=True).strip())
docs = sysroot / "share/doc/rust"
for name in ["COPYRIGHT.html", "COPYRIGHT-library.html"]:
    shutil.copy2(docs / name, output / ("rust-" + name))
shutil.copytree(docs / "licenses", output / "rust-licenses", dirs_exist_ok=True)
manifest = {"schema": "betterxen-build-notices/0", "rust": "1.100.0", "libc": {
    "version": package["version"], "license": package["license"], "source": package["source"]},
    "llvm_mingw": json.loads((root / "scripts/hybrid/toolchain.json").read_text()),
    "scope": "Standalone bx-cli plus original Windows workloads; no native research CLI or external proprietary components"}
(output / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")

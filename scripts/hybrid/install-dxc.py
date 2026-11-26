#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Install Microsoft's checksum-pinned open-source Windows DXC as an external guest tool."""
import hashlib
from pathlib import Path
import subprocess
import shutil
import stat
import sys
import zipfile

URL = "https://github.com/microsoft/DirectXShaderCompiler/releases/download/v1.9.2609/dxc_2026_09_29.zip"
SHA256 = "ad31b1fc8443175d204f77a611fdb3ef2ec42759bdc2f1167368de24a4a7e7f1"


def install(destination):
    destination = destination.expanduser().resolve()
    archive = destination / "dxc_2026_09_29.zip"
    destination.mkdir(parents=True, exist_ok=True)
    if any(p != archive for p in destination.iterdir()):
        raise RuntimeError(f"{destination} already contains extracted files; choose a fresh installation directory")
    if not archive.exists():
        subprocess.run(["/usr/bin/curl", "--fail", "--location", "--retry", "3", "--connect-timeout", "30",
                        URL, "-o", str(archive)], check=True)
    with archive.open("rb") as stream:
        digest = hashlib.file_digest(stream, "sha256").hexdigest()
    if digest != SHA256:
        raise RuntimeError(f"DXC checksum mismatch: {digest}, expected {SHA256}")
    with zipfile.ZipFile(archive) as bundle:
        entries = []
        for member in bundle.infolist():
            # Microsoft's Windows zip uses backslashes; normalize before checking containment.
            name = member.filename.replace("\\", "/")
            if Path(name).is_absolute() or ".." in Path(name).parts or ":" in name or stat.S_ISLNK(member.external_attr >> 16):
                raise RuntimeError(f"invalid archive path {member.filename}")
            entries.append((member, name))
        for member, name in entries:
            path = destination / name
            if name.endswith("/"):
                path.mkdir(parents=True, exist_ok=True)
            else:
                path.parent.mkdir(parents=True, exist_ok=True)
                with bundle.open(member) as source, path.open("xb") as target:
                    shutil.copyfileobj(source, target)
    compiler = destination / "bin/x64/dxc.exe"
    if not compiler.is_file():
        raise RuntimeError("pinned archive lacks bin/x64/dxc.exe")
    print(compiler.parent)


if __name__ == "__main__":
    try:
        install(Path(sys.argv[1]) if len(sys.argv) > 1 else Path.home() / "Library/Caches/BetterXen/dxc-v1.9.2609")
    except (OSError, RuntimeError, zipfile.BadZipFile, subprocess.CalledProcessError) as error:
        sys.exit(str(error))

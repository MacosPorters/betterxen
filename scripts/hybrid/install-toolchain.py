#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Download the pinned external llvm-mingw release; print its installed path on stdout.

Only the checksum-verified archive is extracted. No compiler, workload or proprietary
component is executed. Existing installations are never overwritten.
"""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tarfile
import tempfile


def install(destination):
    if not hasattr(tarfile, "data_filter"):
        raise RuntimeError("Python with tarfile.data_filter is required (Python 3.12 or newer)")
    pin = json.loads(Path(__file__).with_name("toolchain.json").read_text())
    destination = destination.expanduser().resolve()
    destination.mkdir(parents=True, exist_ok=True)
    archive = destination / pin["asset"]
    if not archive.exists():
        print(f'Downloading {pin["url"]}', file=sys.stderr)
        with tempfile.NamedTemporaryFile(dir=destination, delete=False) as out:
            temporary = Path(out.name)
            try:
                out.close()
                subprocess.run(["/usr/bin/curl", "--fail", "--location", "--retry", "3",
                                "--connect-timeout", "30", "--output", str(temporary), pin["url"]], check=True)
                verify(temporary, pin["sha256"])
                # Publish without overwriting an archive installed by a concurrent invocation.
                os.link(temporary, archive)
            finally:
                temporary.unlink(missing_ok=True)
    verify(archive, pin["sha256"])
    installed = destination / pin["asset"].removesuffix(".tar.xz")
    if installed.exists():
        raise RuntimeError(f"{installed} already exists; choose a fresh destination to avoid using a modified toolchain")
    with tempfile.TemporaryDirectory(prefix=".llvm-mingw-", dir=destination) as staging:
        with tarfile.open(archive, "r:xz") as bundle:
            # The pinned archive is trusted; also reject names escaping its single root.
            for member in bundle.getmembers():
                parts = Path(member.name).parts
                if not parts or parts[0] != installed.name or ".." in parts or Path(member.name).is_absolute():
                    raise RuntimeError(f"unexpected archive path: {member.name}")
            bundle.extractall(staging, filter="data")
        os.rename(Path(staging) / installed.name, installed)
    print(f'Verified SHA-256 {pin["sha256"]}', file=sys.stderr)
    return installed


def verify(path, expected):
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    if digest.hexdigest() != expected:
        raise RuntimeError(f"checksum mismatch for {path}: got {digest.hexdigest()}, expected {expected}")


if __name__ == "__main__":
    try:
        print(install(Path(sys.argv[1]) if len(sys.argv) > 1 else Path.home() / "Library/Caches/BetterXen/toolchains"))
    except (OSError, RuntimeError, tarfile.TarError, subprocess.CalledProcessError) as error:
        sys.exit(str(error))

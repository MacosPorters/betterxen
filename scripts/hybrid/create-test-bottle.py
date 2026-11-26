#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Create a new dedicated test bottle, refusing to reuse an existing directory.

This uses CrossOver's observed cxbottle interface, not a vendor-supported automation API.
The new bottle also has Wine's crash dialog turned off: with it on, a crashing program
waits for a click and an unattended run only ends at its timeout.

RUNNER BOTTLE
"""
import json
from pathlib import Path
import re
import subprocess
import sys
import tempfile

runner, name = sys.argv[1:]
if not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9._-]*", name):
    sys.exit("use a simple new bottle name with letters, digits, dots, dashes or underscores")
facts = json.loads(subprocess.check_output([runner, "crossover"], text=True))
if not facts["installs"]:
    sys.exit("install CrossOver from CodeWeavers first")
app = Path(facts["installs"][0]["path"])
base = Path(facts.get("bottle_dir_preference") or "~/Library/Application Support/CrossOver/Bottles").expanduser()
prefix = base / name
if prefix.exists() or prefix.is_symlink():
    sys.exit(f"refusing to reuse {prefix}; choose a fresh dedicated name")
subprocess.run([str(app / "Contents/SharedSupport/CrossOver/bin/cxbottle"), "--bottle", name,
                "--create", "--template", "win11_64"], check=True, timeout=180, stdout=sys.stderr)
if not (prefix / "drive_c").is_dir():
    sys.exit(f"CrossOver did not create the expected bottle at {prefix}")
with tempfile.TemporaryDirectory() as records:
    subprocess.run([str(Path(runner).resolve()), "run", "--bottle", name, "--timeout", "180", "--out", records, "--",
                    str(prefix / "drive_c/windows/system32/reg.exe"), "add", r"HKCU\Software\Wine\WineDbg",
                    "/v", "ShowCrashDialog", "/t", "REG_DWORD", "/d", "0", "/f"],
                   check=True, timeout=300, stdout=sys.stderr)
print(json.dumps({"schema": "betterxen-test-bottle/0", "bottle": name, "prefix": str(prefix), "created": True, "crash_dialog": False}))

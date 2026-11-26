#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Build and run the six fixed UMA experiments without altering a source bottle."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import plistlib
import re
import shutil
import signal
import subprocess
import sys
import time


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def command(args, env, cwd):
    result = subprocess.run(args, cwd=cwd, env=env, capture_output=True)
    return {"command": [str(x) for x in args], "exit_code": result.returncode,
            "stdout": result.stdout.decode(errors="replace"),
            "stderr": result.stderr.decode(errors="replace")}


def bottle_value(text, key):
    matches = re.findall(r'^"' + re.escape(key) + r'"\s*=\s*"([^"]*)"', text, re.M)
    return matches[-1] if matches else None


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bottle", required=True, type=Path,
                        help="absolute path to existing CrossOver win64 bottle; copied, never modified")
    parser.add_argument("--scratch", required=True, type=Path,
                        help="new empty directory outside the repository; all generated files go here")
    parser.add_argument("--crossover", type=Path, default=Path("/Applications/CrossOver.app"))
    parser.add_argument("--compiler", default="x86_64-w64-mingw32-g++")
    parser.add_argument("--receipt-only", action="store_true",
                        help="separate diagnostic: device, wrapping, 10-second pause for library inspection; no benchmark")
    args = parser.parse_args()
    source = Path(__file__).resolve().with_name("probe.cpp")
    scratch = args.scratch.expanduser().resolve()
    # The runner itself never writes into a checkout or existing scratch directory.
    repository = Path(__file__).resolve().parents[3]
    if scratch == repository or repository in scratch.parents:
        parser.error("--scratch must be outside this checkout")
    if scratch.exists():
        parser.error("--scratch already exists; use a fresh run directory")
    bottle = args.bottle.expanduser().resolve()
    config = bottle / "cxbottle.conf"
    if not config.is_file():
        parser.error("--bottle must contain cxbottle.conf")
    original = config.read_text()
    if bottle_value(original, "WineArch") != "win64":
        parser.error("baseline requires an existing win64 bottle")
    cx = args.crossover.expanduser().resolve() / "Contents/SharedSupport/CrossOver"
    wine = cx / "bin/wine"
    if not wine.is_file() or not shutil.which(args.compiler):
        parser.error("CrossOver wine or cross-compiler is unavailable")
    scratch.mkdir(parents=True)
    os.chmod(scratch, 0o700)
    # Small allowlist: do not forward host credentials or unrelated Wine overrides.
    env = {key: os.environ[key] for key in
           ("HOME", "USER", "LOGNAME", "PATH", "LANG", "LC_ALL", "DISPLAY") if key in os.environ}
    env["TMPDIR"] = str(scratch) + "/"
    env.update(ROSETTA_ADVERTISE_AVX="1", CX_GRAPHICS_BACKEND="d3dmetal", WINEDEBUG="-all")
    observations = {}
    for name, cmd in {
        "chip": ["sysctl", "-n", "machdep.cpu.brand_string"],
        "ram_bytes": ["sysctl", "-n", "hw.memsize"],
        "macos": ["sw_vers"], "host_arch": ["uname", "-m"],
        "compiler": [args.compiler, "--version"],
        "crossover": [str(wine), "--version"],
        "rosetta": ["pkgutil", "--pkg-info", "com.apple.pkg.RosettaUpdateAuto"],
    }.items():
        observations[name] = command(cmd, env, scratch)
    framework = cx / "lib64/apple_gptk/external/D3DMetal.framework/Versions/A"
    for name in ("Info.plist", "version.plist"):
        p = framework / "Resources" / name
        observations["d3dmetal_" + name] = plistlib.loads(p.read_bytes()) if p.is_file() else None
    backend = cx / "lib64/apple_gptk/wine/x86_64-windows/d3d12.dll"
    observations["installed_backend_dll"] = {"path": str(backend), "sha256": digest(backend)} if backend.is_file() else None
    observations["source_sha256"] = digest(source)
    observations["bottle"] = {"template": bottle_value(original, "Template"),
                              "architecture": bottle_value(original, "WineArch"),
                              "mode": "temporary private clone; --no-update",
                              "runtime_backend_setting": "CX_GRAPHICS_BACKEND=d3dmetal"}
    observations["environment"] = {k: env[k] for k in
                                   ("ROSETTA_ADVERTISE_AVX", "CX_GRAPHICS_BACKEND", "WINEDEBUG")}
    observations["timeout_seconds"] = 30
    observations["mode"] = "diagnostic-only" if args.receipt_only else "six-check-baseline"
    local_source = scratch / "probe.cpp"
    shutil.copy2(source, local_source)
    exe = scratch / "probe.exe"
    compile_cmd = [args.compiler, "-std=c++20", "-O2", "-static", "-static-libgcc",
                   "-static-libstdc++", "-Wall", "-Wextra", "-Werror",
                   str(local_source), "-o", str(exe), "-lkernel32"]
    observations["compile"] = command(compile_cmd, env, scratch)
    if observations["compile"]["exit_code"]:
        (scratch / "receipt.json").write_text(json.dumps(observations, indent=2) + "\n")
        print(observations["compile"]["stderr"], file=sys.stderr)
        return 2
    observations["executable_sha256"] = digest(exe)
    observations["executable_format"] = command(["file", str(exe)], env, scratch)
    objdump = shutil.which("x86_64-w64-mingw32-objdump")
    if objdump:
        pe = command([objdump, "-p", str(exe)], env, scratch)
        pe["stdout"] = "\n".join(line for line in pe["stdout"].splitlines()
                                  if "DLL Name:" in line or "Magic" in line or "file format" in line)
        observations["pe_imports"] = pe
    temporary_bottle = scratch / "bottle"
    shutil.copytree(bottle, temporary_bottle, symlinks=True)
    temporary_config = temporary_bottle / "cxbottle.conf"
    # Apply overrides only to the disposable clone. This original locally built
    # executable is trusted; scan third-party/untrusted launches normally.
    # CrossOver's scanner otherwise aborts if the host lacks a ClamAV database.
    configured = original.replace("[Bottle]", '[Bottle]\n"AllowUntrusted" = "1"', 1)
    configured += '\n[EnvironmentVariables]\n"CX_GRAPHICS_BACKEND" = "d3dmetal"\n'
    configured += '\n[CrossOver]\n"AntiVirusScan" = "untrusted"\n'
    temporary_config.write_text(configured)
    observations["temporary_config_sha256"] = digest(temporary_config)
    launch = [str(wine), "--bottle", str(temporary_bottle), "--no-update", "--no-gui",
              "--workdir", str(scratch)]
    if args.receipt_only:
        launch += ["--debugmsg", "-all,+loaddll,+d3d12"]
    launch += [str(exe)]
    if args.receipt_only:
        launch += ["--receipt"]
    observations["launch"] = launch
    observations["run_exit_code"] = None
    observations["timed_out"] = False
    try:
        start = time.monotonic()
        with (scratch / "stdout.txt").open("wb") as out, (scratch / "stderr.txt").open("wb") as err:
            child = subprocess.Popen(launch, cwd=scratch, env=env, stdout=out, stderr=err,
                                     start_new_session=True)
            try:
                observations["run_exit_code"] = child.wait(timeout=30)
            except subprocess.TimeoutExpired:
                observations["timed_out"] = True
                os.killpg(child.pid, signal.SIGKILL)
                child.wait()
                observations["run_exit_code"] = 124
            except BaseException:
                os.killpg(child.pid, signal.SIGKILL)
                child.wait()
                raise
        observations["process_elapsed_seconds"] = time.monotonic() - start
    finally:
        # Never kill another bottle's server. Prefix disposal follows process exit.
        stop_env = dict(env, WINEPREFIX=str(temporary_bottle))
        observations["stop_private_server"] = command([str(cx / "bin/wineserver"), "-k"], stop_env, scratch)
        observations["source_bottle_config_unchanged"] = config.read_text() == original
        shutil.rmtree(temporary_bottle)
        (scratch / "receipt.json").write_text(json.dumps(observations, indent=2) + "\n")
    print((scratch / "stdout.txt").read_text(errors="replace"), end="")
    print((scratch / "stderr.txt").read_text(errors="replace"), file=sys.stderr, end="")
    print(f"RUN_EXIT={observations['run_exit_code']} TIMEOUT={observations['timed_out']} "
          f"process_elapsed_s={observations['process_elapsed_seconds']:.6f}")
    print("Receipt and raw logs:", scratch)
    return observations["run_exit_code"]


if __name__ == "__main__":
    sys.exit(main())

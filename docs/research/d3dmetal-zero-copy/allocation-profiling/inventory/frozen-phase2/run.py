#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Protocol 1 supplemental suite, separate from all six frozen baseline checks."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import plistlib
import shutil
import signal
import subprocess
import threading
import time
from datetime import datetime, timezone


def sha(p):
    return hashlib.sha256(p.read_bytes()).hexdigest()


def capture(cmd, env, cwd, timeout=15):
    start = time.monotonic()
    try:
        p = subprocess.run([str(x) for x in cmd], env=env, cwd=cwd,
                           capture_output=True, timeout=timeout)
        return {"command": [str(x) for x in cmd], "exit": p.returncode,
                "stdout": p.stdout.decode(errors="replace"),
                "stderr": p.stderr.decode(errors="replace"),
                "elapsed_seconds": time.monotonic() - start}
    except subprocess.TimeoutExpired as e:
        return {"command": [str(x) for x in cmd], "exit": 124, "timeout": True,
                "stdout": (e.stdout or b"").decode(errors="replace"),
                "stderr": (e.stderr or b"").decode(errors="replace")}


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--bottle", required=True, type=Path)
    ap.add_argument("--scratch", required=True, type=Path)
    ap.add_argument("--modes", default="identity,coherence,negative-control,watch,texture,dirty,memory")
    ap.add_argument("--sessions", type=int, default=3, choices=[1, 2, 3])
    ap.add_argument("--memory-repeats", type=int, default=2, choices=[1, 2, 3])
    ap.add_argument("--large", action="store_true", help="also 2 GiB footprint cases; requires >=24 GiB RAM")
    ap.add_argument("--crossover", type=Path, default=Path("/Applications/CrossOver.app"))
    args = ap.parse_args()
    src = Path(__file__).resolve().parent
    repo = src.parents[3]
    base = args.scratch.expanduser().resolve()
    if base.exists() or base == repo or repo in base.parents:
        ap.error("scratch must be a new directory outside the checkout")
    bottle = args.bottle.expanduser().resolve()
    if not (bottle / "cxbottle.conf").is_file():
        ap.error("bottle must be an existing CrossOver bottle")
    original = (bottle / "cxbottle.conf").read_bytes()
    if b'"WineArch" = "win64"' not in original:
        ap.error("requires win64 bottle")
    modes = args.modes.split(",")
    if set(modes) - {"identity", "coherence", "negative-control", "watch", "texture", "dirty", "memory"}:
        ap.error("unknown mode")
    base.mkdir(parents=True, mode=0o700)
    env = {k: os.environ[k] for k in ["HOME", "USER", "LOGNAME", "PATH", "LANG", "LC_ALL", "DISPLAY"] if k in os.environ}
    env.update(TMPDIR=str(base) + "/", ROSETTA_ADVERTISE_AVX="1", CX_GRAPHICS_BACKEND="d3dmetal", WINEDEBUG="-all")
    cx = args.crossover.resolve() / "Contents/SharedSupport/CrossOver"
    wine = cx / "bin/wine"
    exe = base / "phase2.exe"
    observer = base / "observe"
    receipt = {"protocol": 1, "recorded_utc": datetime.now(timezone.utc).isoformat(),
               "modes": modes, "sessions": args.sessions, "memory_repeats": args.memory_repeats,
               "environment": {k: env[k] for k in ["ROSETTA_ADVERTISE_AVX", "CX_GRAPHICS_BACKEND", "WINEDEBUG"]},
               "source_hashes": {p.name: sha(p) for p in src.iterdir() if p.suffix in [".cpp", ".py"]},
               "baseline_sha256": sha(src.parent / "probe.cpp"), "tools": {}, "cases": []}
    commands = {
        "chip": ["sysctl", "-n", "machdep.cpu.brand_string"],
        "ram": ["sysctl", "-n", "hw.memsize"], "macos": ["sw_vers"],
        "host_page_size": ["sysctl", "-n", "hw.pagesize"],
        "crossover": [wine, "--version"], "rosetta": ["pkgutil", "--pkg-info", "com.apple.pkg.RosettaUpdateAuto"],
        "gcc": ["x86_64-w64-mingw32-g++", "--version"],
        "clang": ["xcrun", "clang++", "--version"], "sdk": ["xcrun", "--show-sdk-version"],
        "pressure_before": ["memory_pressure", "-Q"], "vm_before": ["vm_stat"],
        "load_before": ["ps", "-axo", "pid,pcpu,comm"],
        "build_pe": ["x86_64-w64-mingw32-g++", "-std=c++20", "-O2", "-static", "-static-libgcc", "-static-libstdc++", "-Wall", "-Wextra", "-Werror", src / "probe.cpp", "-o", exe, "-lkernel32"],
        "build_observer": ["xcrun", "clang++", "-std=c++20", "-O2", "-arch", "arm64", "-Wall", "-Wextra", "-Werror", src / "observe.cpp", "-o", observer],
        "binary_formats": ["file", exe, observer], "observer_arch": ["lipo", "-archs", observer],
        "observer_links": ["otool", "-L", observer],
    }
    for name, cmd in commands.items():
        receipt["tools"][name] = capture(cmd, env, base, timeout=45)
        if name.startswith("build_") and receipt["tools"][name]["exit"]:
            (base / "receipt.json").write_text(json.dumps(receipt, indent=2))
            raise RuntimeError(receipt["tools"][name])
    receipt["binary_hashes"] = {"phase2.exe": sha(exe), "observe": sha(observer)}
    ram = int(receipt["tools"]["ram"]["stdout"].strip())
    if args.large and ram < 24 * 1024**3:
        ap.error("--large requires >=24 GiB; common profile is identical on 16/24 GiB hosts")
    framework = cx / "lib64/apple_gptk/external/D3DMetal.framework/Versions/A/Resources"
    receipt["framework"] = {p.name: plistlib.loads(p.read_bytes()) for p in framework.glob("*.plist")}
    for p in [cx / "lib64/apple_gptk/wine/x86_64-windows/d3d12.dll", cx / "lib/wine/x86_64-windows/d3dcompiler_47.dll"]:
        receipt.setdefault("installed_dll_hashes", {})[str(p)] = sha(p)
    temp = base / "bottle"
    shutil.copytree(bottle, temp, symlinks=True)
    conf = original.decode().replace("[Bottle]", '[Bottle]\n"AllowUntrusted" = "1"', 1)
    conf += '\n[EnvironmentVariables]\n"CX_GRAPHICS_BACKEND" = "d3dmetal"\n[CrossOver]\n"AntiVirusScan" = "untrusted"\n'
    (temp / "cxbottle.conf").write_text(conf)
    receipt["source_bottle_config_sha256"] = hashlib.sha256(original).hexdigest()
    receipt["temporary_bottle_config"] = {"template": "inherited; inspect source config", "architecture": "win64", "backend": "d3dmetal", "no_update": True, "AntiVirusScan": "untrusted", "AllowUntrusted": 1}
    common = [str(wine), "--bottle", str(temp), "--no-update", "--no-gui", str(exe)]
    backend_seen = False

    def run_case(name, argv, memory=False):
        nonlocal backend_seen
        case = base / name
        case.mkdir()
        metadata = {"expected_exit": 1 if argv[0] == "negative-control" else 0, "name": name, "args": argv, "command": common + argv, "timeout_seconds": 150 if memory else 90, "snapshots": []}
        stdout = case / "stdout.txt"
        stderr = case / "stderr.txt"
        timed_out = threading.Event()
        start = time.monotonic()
        with stdout.open("wb") as out, stderr.open("wb") as err:
            p = subprocess.Popen(common + argv, env=env, cwd=case, stdin=subprocess.PIPE,
                                 stdout=subprocess.PIPE, stderr=err, start_new_session=True)
            def expire():
                timed_out.set()
                try:
                    os.killpg(p.pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
            timer = threading.Timer(metadata["timeout_seconds"], expire)
            timer.start()
            hostpid = None
            try:
                for line in p.stdout:
                    out.write(line)
                    out.flush()
                    if not line.startswith(b"STAGE "):
                        continue
                    if hostpid is None:
                        found = capture(["lsof", "-t", "-a", "-c", "phase2.exe", str(exe)], env, base)
                        candidates = sorted({int(x) for x in found["stdout"].split() if x.isdigit()})
                        if len(candidates) != 1:
                            metadata["pid_lookup"] = found
                            raise RuntimeError("need exactly one process mapping this scratch PE")
                        hostpid = candidates[0]
                        metadata["host_pid"] = hostpid
                    phase = line.decode().split()[1]
                    snap = {"phase": phase, "marker": line.decode().strip(), "samples": []}
                    for _ in range(3):
                        sample = capture([observer, str(hostpid)], env, base)
                        try:
                            sample["data"] = json.loads(sample["stdout"])
                        except json.JSONDecodeError:
                            pass
                        snap["samples"].append(sample)
                    for tool, command in {
                        "footprint": ["footprint", "-p", str(hostpid), "-f", "bytes"],
                        "vmmap": ["vmmap", "-summary", str(hostpid)],
                        "vmstat": ["vm_stat"], "pressure": ["memory_pressure", "-Q"],
                    }.items():
                        result = capture(command, env, base, timeout=12)
                        (case / f"{phase}-{tool}.json").write_text(json.dumps(result, indent=2))
                        snap[tool] = {"exit": result["exit"], "file": f"{phase}-{tool}.json"}
                    if not backend_seen:
                        result = capture(["lsof", "-n", "-p", str(hostpid)], env, base)
                        metadata["backend_paths_excerpt"] = ["/" + line.split("/", 1)[1] for line in result["stdout"].splitlines() if any(key in line for key in ["apple_gptk/wine/x86_64-windows/d3d12.dll", "apple_gptk/wine/x86_64-windows/dxgi.dll", "D3DMetal.framework/Versions/A/D3DMetal", "/rosetta/runtime"])]
                        backend_seen = any("/D3DMetal" in path for path in metadata["backend_paths_excerpt"])
                    metadata["snapshots"].append(snap)
                    p.stdin.write(b"\n")
                    p.stdin.flush()
                p.wait()
            except BaseException:
                try:
                    os.killpg(p.pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
                p.wait()
                raise
            finally:
                timer.cancel()
                metadata.update(exit=p.returncode, timed_out=timed_out.is_set(), elapsed_seconds=time.monotonic() - start)
                (case / "case.json").write_text(json.dumps(metadata, indent=2))
                receipt["cases"].append(metadata)
                (base / "receipt.json").write_text(json.dumps(receipt, indent=2))
        if argv[0] == "negative-control":
            metadata["negative_control_detected"] = b"END failure reason=GPU checksum mismatch" in stdout.read_bytes()
            if not metadata["negative_control_detected"]:
                raise RuntimeError("negative control did not detect an omitted dispatch")
        print(name, "exit", metadata["exit"], "expected", metadata["expected_exit"], "timeout", metadata["timed_out"], flush=True)

    try:
        for mode in ["identity", "coherence", "negative-control", "watch", "texture"]:
            if mode in modes:
                run_case(mode, [mode])
        if "dirty" in modes:
            for session in range(args.sessions):
                for stride in [4096, 16384]:
                    # Alternate heap order; no background OS polling during latency cases.
                    types = ["wb", "wc", "upload"]
                    if session % 2:
                        types.reverse()
                    for heap in types:
                        run_case(f"dirty-{heap}-{stride}-s{session}", ["dirty", heap, str(stride), str(session)])
        if "memory" in modes:
            for repeat in range(args.memory_repeats):
                for size in [64, 256, 1024] + ([2048] if args.large else []):
                    for heap in ["cpu", "wb", "upload"]:
                        run_case(f"memory-{heap}-{size}MiB-r{repeat}", ["memory", heap, str(size * 1024**2)], memory=True)
    finally:
        receipt["tools"]["pressure_after"] = capture(["memory_pressure", "-Q"], env, base)
        receipt["tools"]["vm_after"] = capture(["vm_stat"], env, base)
        receipt["source_bottle_config_unchanged"] = (bottle / "cxbottle.conf").read_bytes() == original
        stop = dict(env, WINEPREFIX=str(temp))
        receipt["server_stop"] = capture([cx / "bin/wineserver", "-k"], stop, base)
        receipt["server_wait"] = capture([cx / "bin/wineserver", "-w"], stop, base)
        shutil.rmtree(temp)
        (base / "receipt.json").write_text(json.dumps(receipt, indent=2) + "\n")
    failures = [c["name"] for c in receipt["cases"] if c["exit"] != c["expected_exit"] or c["timed_out"]]
    print("Completed", len(receipt["cases"]), "cases; failures:", failures)
    return bool(failures)


if __name__ == "__main__":
    raise SystemExit(main())

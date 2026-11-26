#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Process lifecycle checks for the hybrid runner against an installed CrossOver.

Uses the Direct3D 11 workload: `--fault hang` makes a program that never exits, and a normal
run shows recovery. Prints "ok"/"FAIL" per check and exits non-zero on any failure. Do not
run anything else in the same bottle meanwhile: stopping a run stops the whole bottle.

usage: accept-lifecycle.py RUNNER BOTTLE D3D11_EXE [--launches N]
"""
import argparse
import json
import os
import signal
import subprocess
import sys
import tempfile
import time

failed = False


def check(ok, what):
    global failed
    failed |= not ok
    print(("ok   " if ok else "FAIL ") + what, flush=True)


def workload_pids(exe_name):
    out = subprocess.run(["/bin/ps", "-axo", "pid=,command="], capture_output=True, text=True).stdout
    pids = []
    for line in out.splitlines():
        pid, _, command = line.strip().partition(" ")
        command = command.strip()
        # the Windows program appears with a drive-letter command line
        if command[1:3] == ":\\" and exe_name.lower() in command.lower():
            pids.append(int(pid))
    return pids


def wait_for(predicate, seconds):
    end = time.time() + seconds
    while time.time() < end:
        if predicate():
            return True
        time.sleep(0.2)
    return False


def main():
    p = argparse.ArgumentParser()
    p.add_argument("runner")
    p.add_argument("bottle")
    p.add_argument("exe")
    p.add_argument("--launches", type=int, default=20)
    a = p.parse_args()
    name = os.path.basename(a.exe)
    base = [a.runner, "hybrid", "run", "--bottle", a.bottle, "--timeout", "120"]
    if workload_pids(name):
        print(f"FAIL precondition: {name} is already running")
        return 1

    def start(extra, out):
        return subprocess.Popen(base + ["--", a.exe] + extra, stdout=out, stderr=subprocess.DEVNULL,
                                start_new_session=True)

    def healthy(label):
        with tempfile.TemporaryFile("w+") as out:
            run = start(["--frames", "64"], out)
            code = run.wait(timeout=150)
            check(code == 0, f"{label}: next run passes")
        check(not workload_pids(name), f"{label}: nothing left running")

    for sig, to_group in ((signal.SIGINT, True), (signal.SIGTERM, False), (signal.SIGHUP, False)):
        label = sig.name + (" to the process group" if to_group else "")
        with tempfile.TemporaryFile("w+") as out:
            run = start(["--frames", "64", "--fault", "hang"], out)
            if not wait_for(lambda: workload_pids(name), 60):
                check(False, f"{label}: workload never started")
                run.kill()
                continue
            time.sleep(2)
            began = time.time()
            (os.killpg if to_group else os.kill)(run.pid, sig)
            try:
                code = run.wait(timeout=40)
            except subprocess.TimeoutExpired:
                run.kill()
                check(False, f"{label}: supervisor did not exit")
                continue
            out.seek(0)
            try:
                record = json.load(out)
            except json.JSONDecodeError:
                record = {}
            check(code == 1 and record.get("classification") == "interrupted",
                  f"{label}: classified {record.get('classification')} in {time.time() - began:.1f}s")
        check(wait_for(lambda: not workload_pids(name), 10), f"{label}: workload stopped")
        healthy(label)

    with tempfile.TemporaryFile("w+") as out:
        run = start(["--frames", "64", "--fault", "hang"], out)
        started = wait_for(lambda: workload_pids(name), 60)
        time.sleep(2)
        run.kill()
        run.wait()
        # SIGKILL cannot be handled: the program is expected to survive until `stop`.
        check(started and bool(workload_pids(name)), "SIGKILL: program survives, as documented")
        subprocess.run([a.runner, "hybrid", "stop", "--bottle", a.bottle], timeout=60)
        check(wait_for(lambda: not workload_pids(name), 15), "SIGKILL: `hybrid stop` removes it")
    healthy("after stop")

    passed = 0
    for launch in range(a.launches):
        with tempfile.TemporaryFile("w+") as out:
            code = start(["--frames", "64"], out).wait(timeout=150)
            passed += code == 0
            if code:
                out.seek(0)
                try:
                    reason = json.load(out).get("classification")
                except json.JSONDecodeError:
                    reason = "no record"
                print(f"     launch {launch + 1}: {reason}", flush=True)
    check(passed == a.launches, f"repeated launch: {passed} of {a.launches} passed")
    check(not workload_pids(name), "repeated launch: nothing left running")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())

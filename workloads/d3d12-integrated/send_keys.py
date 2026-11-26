#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Drive the D3D12 workload with macOS keyboard events.

Starts the workload through `betterxen hybrid run` with `--external-input`, finds its window,
brings that process to the front and posts key events to that process only
(CGEventPostToPid), so nothing is typed into any other application. The events travel macOS
-> Wine's Mac driver -> the Windows message queue. They are synthesized, not typed on a
keyboard. Needs the Accessibility permission, `swiftc` and a logged-in GUI session.

usage: send_keys.py RUNNER BOTTLE EXE [--keys right,right,down,left,down] [--expect pass|fail]
"""
import argparse
import json
import subprocess
import sys
import tempfile
import time
from pathlib import Path

CODES = {"left": 123, "right": 124, "down": 125, "up": 126}
SENDER = r"""
import AppKit
let title = "BetterXen D3D12 workload"
guard CGPreflightPostEventAccess() else { print("no event-posting permission"); exit(3) }
var pid: pid_t = 0
for _ in 0..<120 {
    let all = CGWindowListCopyWindowInfo([.optionOnScreenOnly], kCGNullWindowID) as? [[String: Any]] ?? []
    if let w = all.first(where: { ($0[kCGWindowName as String] as? String) == title }) {
        pid = pid_t(w[kCGWindowOwnerPID as String] as! Int32); break
    }
    usleep(250_000)
}
guard pid > 0 else { print("window not found"); exit(4) }
sleep(2)
NSRunningApplication(processIdentifier: pid)?.activate(options: [])
sleep(1)
for code in CommandLine.arguments.dropFirst().compactMap({ UInt16($0) }) {
    for down in [true, false] {
        CGEvent(keyboardEventSource: nil, virtualKey: code, keyDown: down)?.postToPid(pid)
        usleep(60_000)
    }
    usleep(250_000)
}
print("posted to pid \(pid)")
"""


def main():
    p = argparse.ArgumentParser()
    p.add_argument("runner")
    p.add_argument("bottle")
    p.add_argument("exe")
    p.add_argument("--keys", default="right,right,down,left,down")
    p.add_argument("--expect", choices=["pass", "fail"], default="pass")
    a = p.parse_args()
    codes = [str(CODES[k]) for k in a.keys.split(",") if k]
    with tempfile.TemporaryDirectory() as tmp:
        source, binary = Path(tmp) / "send.swift", Path(tmp) / "send"
        source.write_text(SENDER)
        subprocess.run(["swiftc", "-O", str(source), "-o", str(binary)], check=True)
        record = open(Path(tmp) / "run.json", "w+")
        run = subprocess.Popen(
            [a.runner, "hybrid", "run", "--bottle", a.bottle, "--timeout", "90", "--",
             a.exe, "--external-input", "--input-timeout-s", "20"],
            stdout=record, stderr=subprocess.DEVNULL)
        sent = subprocess.run([str(binary)] + codes, capture_output=True, text=True, timeout=60)
        print(sent.stdout.strip())
        run.wait(timeout=120)
        record.seek(0)
        gates = {g["name"]: g for g in json.load(record)["gates"]}
    if sent.returncode != 0 or "input" not in gates:
        print("FAIL keyboard: the events were not sent or the workload reported no input gate")
        return 1
    print("workload:", gates["input"]["detail"])
    passed = gates["input"]["passed"]
    if passed == (a.expect == "pass"):
        print(f"PASS keyboard: input gate {'passed' if passed else 'failed'} as expected")
        return 0
    print(f"FAIL keyboard: input gate {'passed' if passed else 'failed'}, expected {a.expect}")
    return 1


if __name__ == "__main__":
    sys.exit(main())

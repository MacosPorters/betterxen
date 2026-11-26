#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Observe the integrated workload's window from macOS and compare what is on screen.

Runs the workload through `betterxen-hybrid run` with a hold period, finds its window with
CGWindowList, captures only that window with `screencapture -l`, converts the capture to sRGB
and samples the client area on a grid. This is evidence from outside the Windows program that
the final frame reached the macOS compositor; the program's own read-back cannot show that.
Needs the Screen Recording permission, `swift` and a logged-in GUI session. A missing
permission or window is a failure, never a pass.

usage: check_window.py RUNNER BOTTLE EXE [--fault NAME] [--keep PNG]
"""
import argparse
import struct
import subprocess
import sys
import tempfile
import time
import uuid
from pathlib import Path

FRAMES = 128
WINDOW_ID_SWIFT = """
import CoreGraphics
let all = CGWindowListCopyWindowInfo([.optionOnScreenOnly], kCGNullWindowID) as? [[String: Any]] ?? []
for w in all where (w[kCGWindowName as String] as? String) == "%s" {
    print(w[kCGWindowNumber as String]!)
    break
}
"""

TEX_A = [(255, 0, 0), (0, 255, 0), (0, 0, 255), (255, 255, 255)]
TEX_B = [(255, 255, 0), (0, 255, 255), (255, 0, 255), (128, 128, 128)]


def expected(x, y, first_frame=False):
    """Final frame after FRAMES frames and the scripted input (quad B moved by +16, +32)."""
    if y < 16:
        marker = 1 if first_frame else FRAMES
        return (marker & 255, marker >> 8, 77)
    bx, by = (96, 96) if first_frame else (96 + 16, 96 + 32)
    colour = None
    if bx <= x < bx + 128 and by <= y < by + 128:
        colour = TEX_B[(y - by) // 64 * 2 + (x - bx) // 64]
    elif 32 <= x < 160 and 32 <= y < 160:
        colour = TEX_A[(y - 32) // 64 * 2 + (x - 32) // 64]
    if colour is None:
        return (0, 0, 0)
    r, g, b = colour
    return (b, r, g)


def exists_off_screen(scratch, title):
    """True when macOS knows the window but is not showing it (another Space, minimised)."""
    script = scratch / "window_any.swift"
    script.write_text((WINDOW_ID_SWIFT % title).replace("[.optionOnScreenOnly]", "[.optionAll]"))
    found = subprocess.run(["swift", str(script)], capture_output=True, text=True).stdout.strip()
    return found.isdigit()


def read_bmp(path):
    data = path.read_bytes()
    if data[:2] != b"BM":
        raise ValueError("not a BMP")
    offset = struct.unpack_from("<I", data, 10)[0]
    width, height = struct.unpack_from("<ii", data, 18)
    bpp = struct.unpack_from("<H", data, 28)[0]
    if bpp not in (24, 32):
        raise ValueError(f"unsupported BMP depth {bpp}")
    step = bpp // 8
    stride = (width * step + 3) & ~3

    def pixel(x, y):
        row = height - 1 - y if height > 0 else y
        at = offset + row * stride + x * step
        return (data[at + 2], data[at + 1], data[at])

    return width, abs(height), pixel


def compare(png, scratch, first_frame=False):
    """Return (samples, mismatches, first mismatch) for one capture."""
    bmp = scratch / "capture.bmp"
    subprocess.run(["/usr/bin/sips", "--matchTo", "/System/Library/ColorSync/Profiles/sRGB Profile.icc",
                    "-s", "format", "bmp", str(png), "--out", str(bmp)],
                   check=True, capture_output=True)
    width, height, pixel = read_bmp(bmp)
    scale = width / 256
    top = height - round(256 * scale)  # the title bar is above the client area
    if scale < 1 or top < 0:
        raise ValueError(f"unexpected capture size {width}x{height}")
    samples, bad, first = 0, 0, None
    for y in range(4, 256, 8):
        for x in range(4, 256, 8):
            if y > 236 and (x < 20 or x > 236):
                continue  # rounded window corners
            got = pixel(int((x + 0.5) * scale), top + int((y + 0.5) * scale))
            want = expected(x, y, first_frame)
            samples += 1
            if any(abs(g - w) > 6 for g, w in zip(got, want)):
                bad += 1
                first = first or (x, y, got, want)
    return samples, bad, first


def main():
    p = argparse.ArgumentParser()
    p.add_argument("runner")
    p.add_argument("bottle")
    p.add_argument("exe")
    # only faults that change the rendered image are meaningful here; "color" corrupts the
    # program's expectation and leaves the screen correct
    p.add_argument("--fault", choices=["blank", "depth", "stale"])
    p.add_argument("--keep")
    p.add_argument("--first-frame", action="store_true")
    a = p.parse_args()
    with tempfile.TemporaryDirectory() as tmp:
        scratch = Path(tmp)
        script = scratch / "window_id.swift"
        title = "BetterXen capture " + uuid.uuid4().hex
        script.write_text(WINDOW_ID_SWIFT % title)
        workload = [a.exe, "--frames", str(FRAMES), "--hold-ms", "12000", "--window-title", title]
        if a.first_frame:
            workload = [a.exe, "--frames", str(FRAMES), "--hold-first-ms", "12000", "--window-title", title]
        if a.fault:
            workload += ["--fault", a.fault]
        log = open(scratch / "run.json", "w+")
        run = subprocess.Popen([a.runner, "run", "--bottle", a.bottle, "--timeout", "90", "--"] + workload,
                               stdout=log, stderr=subprocess.DEVNULL)
        result, off_screen = None, False
        deadline = time.time() + 60
        while time.time() < deadline and run.poll() is None:
            wid = subprocess.run(["swift", str(script)], capture_output=True, text=True).stdout.strip()
            if wid.isdigit():
                png = scratch / "window.png"
                if subprocess.run(["/usr/sbin/screencapture", "-x", "-o", "-l" + wid, str(png)],
                                  capture_output=True).returncode == 0 and png.exists():
                    result = compare(png, scratch, a.first_frame)
                    if a.keep:
                        Path(a.keep).write_bytes(png.read_bytes())
                    if result[1] == 0:
                        break
            elif result is None and not off_screen:
                off_screen = exists_off_screen(scratch, title)
            time.sleep(0.5)
        run.wait(timeout=120)
        if result is None and off_screen:
            print("FAIL window: the window exists but is not on the active Space; leave a desktop "
                  "(not a full-screen app) frontmost and rerun")
            return 1
        if result is None:
            print("FAIL window: no window was captured (permission, GUI session or window missing)")
            return 1
        samples, bad, first = result
        if bad:
            print(f"FAIL window: {bad} of {samples} samples differ, first (x, y, got, want) = {first}")
            return 1
        which = "first" if a.first_frame else "final"
        print(f"PASS window: {samples} of {samples} on-screen samples match the expected {which} frame")
        return 0


if __name__ == "__main__":
    sys.exit(main())

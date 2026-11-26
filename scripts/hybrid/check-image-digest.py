#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Independent image oracle for the fixed scripted D3D11/D3D12 scene.

Build images by painting whole rectangles, independently of the workloads' pixel
functions and GPU shaders. The fixed protocol is: far A at (32,32), near B at
(96,96), 128-square quads with four 64-square colours; five 16-pixel key moves;
rotate RGB to BRG; overwrite the top 16 rows with the frame number. Hash only the
specified verification frames, in row-major RGBA order. No renderer or C oracle
is called. The legacy digest seed is intentionally preserved (it differs from
the standard FNV offset basis).
"""
import json
from pathlib import Path
import re
import sys

PALETTES = (((255, 0, 0), (0, 255, 0), (0, 0, 255), (255, 255, 255)),
            ((255, 255, 0), (0, 255, 255), (255, 0, 255), (128, 128, 128)))
POSITIONS = ((96, 96), (112, 96), (128, 96), (128, 112), (112, 112), (112, 128))


def image(frame, interval):
    pixels = bytearray(b"\x00\x00\x00\xff" * (256 * 256))

    def rectangle(x, y, width, height, rgba):
        row = bytes(rgba) * width
        for line in range(y, y + height):
            start = (line * 256 + x) * 4
            pixels[start:start + width * 4] = row

    # Painter's order is analytically equivalent to the scene's depth test.
    steps = min(frame // interval, 5)
    for palette, (x, y) in zip(PALETTES, ((32, 32), POSITIONS[steps])):
        for quadrant, (r, g, b) in enumerate(palette):
            rectangle(x + (quadrant % 2) * 64, y + (quadrant // 2) * 64, 64, 64, (b, r, g, 255))
    marker = (frame + 1) & 65535
    rectangle(0, 0, 256, 16, (marker & 255, marker >> 8, 77, 255))
    return pixels


def expected_digest(frames, verify_every):
    interval = frames // 8
    selected = set(range(0, frames, verify_every)) | {0, frames - 1}
    selected.update(k * interval + 1 for k in range(1, 6) if k * interval + 1 < frames)
    digest = 1469598103934665603
    for frame in sorted(selected):
        for byte in image(frame, interval):
            digest = ((digest ^ byte) * 1099511628211) & 0xffffffffffffffff
    return f"{digest:016x}", len(selected)


def main():
    record = json.loads(Path(sys.argv[1]).read_text())
    args = record["args"]
    if any(flag in args for flag in ["--fault", "--duration-s", "--external-input", "--interactive"]):
        raise ValueError("oracle accepts only the fixed scripted scene, without injected faults")
    frames = int(args[args.index("--frames") + 1]) if "--frames" in args else 600
    verify = int(args[args.index("--verify-every") + 1]) if "--verify-every" in args else 30
    if frames < 64 or verify < 1:
        raise ValueError("invalid verification schedule")
    gate = next(g for g in record["gates"] if g["name"] == "frame.continuous")
    if not gate["passed"]:
        raise ValueError("continuous rendering gate did not pass")
    observed = re.search(r"verified=(\d+) mismatched=0 digest=([0-9a-f]{16})", gate["detail"])
    expected, count = expected_digest(frames, verify)
    if not observed or observed.groups() != (str(count), expected):
        raise ValueError(f"independent digest mismatch: expected {count} verified frames, digest={expected}; {gate['detail']}")
    print(f"PASS independent image oracle: {count} frames digest={expected}")


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, KeyError, StopIteration) as error:
        sys.exit(f"FAIL independent image oracle: {error}")

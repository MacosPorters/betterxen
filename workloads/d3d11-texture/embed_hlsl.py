#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Embed sample.hlsl as a C string for D3DCompile at runtime."""

import sys


def main() -> None:
    source_path, header_path = sys.argv[1], sys.argv[2]
    text = open(source_path, "r", encoding="utf-8").read()
    lines = ["/* Generated from sample.hlsl. Do not edit. */\n", "static const char k_shader_source[] =\n"]
    for line in text.splitlines():
        escaped = line.replace("\\", "\\\\").replace('"', '\\"')
        lines.append(f'    "{escaped}\\n"\n')
    lines.append("    ;\n")
    with open(header_path, "w", encoding="utf-8", newline="\n") as out:
        out.write("".join(lines))


if __name__ == "__main__":
    main()

#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""A negative control counts only when the injected fault failed the gate it targets.

check-failed-gates.py RECORD.json GATE [GATE...]

Passes when the record is a non-zero workload exit and every named gate was reported
and failed. A crash, a missing gate or an unrelated failure is not a detection.
"""
import json
from pathlib import Path
import sys


def main():
    path, *wanted = sys.argv[1:]
    if not wanted:
        raise ValueError("name at least one gate")
    record = json.loads(Path(path).read_text())
    if record["classification"] != "failed_exit_1":
        raise ValueError(f"expected failed_exit_1, got {record['classification']}")
    reported = {gate["name"]: gate["passed"] for gate in record["gates"]}
    for name in wanted:
        if name not in reported:
            raise ValueError(f"gate {name} was never reported")
        if reported[name]:
            raise ValueError(f"gate {name} passed despite the injected fault")
    failed = sorted(name for name, passed in reported.items() if not passed)
    print(",".join(failed))


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, KeyError) as error:
        sys.exit(f"FAIL negative control: {error}")

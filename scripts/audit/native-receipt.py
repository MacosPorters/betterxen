#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Native ARM64 and Metal capability receipt for BetterXen.

A read-only contributor report, not a gate and not a benchmark suite. It joins
one host's actual evidence into a single JSON file: candidate identity and
dirty state, host architecture and translation status, OS and tool versions,
the selected Metal device and what the host can report about it, validation
settings, the exact commands that were run, per-dependency
``file``/``lipo -archs``/``otool -L`` evidence, and the outcome of one
explicitly selected CPU fixture and one optional native Metal fixture run in
bounded processes.

The tool never installs anything, never posts results anywhere, never
modifies CI or build scripts, and never invokes a host binary it knows to be
Intel-only. Fixture outcomes are recorded as one of pass, fail, skip,
timeout, unavailable or unsupported; a skip or unavailable Metal result is
recorded honestly and is never GPU verification. One receipt describes one
host: it does not certify any other Apple Silicon generation.

Usage:

    python3 scripts/audit/native-receipt.py [--cpu-command CMD ...]
        [--metal-command CMD ...] [--output PATH] [--no-matrix]

The default output path is ``artifacts/native-receipt.json`` (ignored by
git). The exit status is 0 whenever the report was written, whatever the
fixture outcomes recorded in it; the ``overall`` field inside the report is
the evidence. A usage or I/O failure exits non-zero without claiming
anything about the host.

Fixture commands are run from the repository root with a wall-clock timeout
(the default is 300 seconds). The CPU fixture should be an existing original
test target, for example::

    cargo test --locked -p xenora-loader --test cpu_known_answers

and the Metal fixture should be an existing native suite entry, for example
``scripts/test-gpu.sh`` or a bounded ``cargo test`` invocation of a
Metal-gated target. Commands are recorded verbatim in the report.
"""

from __future__ import annotations

import argparse
import json
import platform
import re
import subprocess
import sys
from dataclasses import asdict, dataclass, field
from pathlib import Path
from typing import Any, Optional

REPO_ROOT = Path(__file__).resolve().parents[2]
DEFAULT_OUTPUT = REPO_ROOT / "artifacts" / "native-receipt.json"
DEFAULT_TIMEOUT_SECONDS = 300
DEVICE_MATRIX_KEYS = ("M1", "M2", "M3", "M4", "M5", "M6+")

# Outcomes are recorded, never silently upgraded. "pass" requires exit 0 from
# the fixture command; everything else keeps its own name.
PASS = "pass"
FAIL = "fail"
SKIP = "skip"
TIMEOUT = "timeout"
UNAVAILABLE = "unavailable"
UNSUPPORTED = "unsupported"


@dataclass
class DependencyEvidence:
    """file/lipo/otool evidence for one native host dependency."""

    path: str
    file: str = ""
    archs: str = ""
    linkage: str = ""
    ok: bool = False
    error: str = ""

    def to_dict(self) -> dict[str, Any]:
        return asdict(self)


@dataclass
class FixtureResult:
    """Outcome of one bounded fixture process."""

    name: str
    command: list[str] = field(default_factory=list)
    outcome: str = UNAVAILABLE
    returncode: Optional[int] = None
    stdout_tail: str = ""
    stderr_tail: str = ""
    detail: str = ""

    def to_dict(self) -> dict[str, Any]:
        return asdict(self)


@dataclass
class Receipt:
    """One host's native capability receipt. Schema version 1."""

    schema_version: int = 1
    candidate_sha: str = "unknown"
    dirty: bool = False
    dirty_detail: str = ""
    host: dict[str, Any] = field(default_factory=dict)
    tools: dict[str, str] = field(default_factory=dict)
    metal: dict[str, Any] = field(default_factory=dict)
    validation: dict[str, Any] = field(default_factory=dict)
    commands: list[str] = field(default_factory=list)
    dependencies: list[dict[str, Any]] = field(default_factory=list)
    fixtures: list[dict[str, Any]] = field(default_factory=list)
    matrix: dict[str, str] = field(default_factory=dict)
    overall: str = UNAVAILABLE

    def to_dict(self) -> dict[str, Any]:
        return asdict(self)


def run_capture(
    command: list[str], timeout: int = 30, cwd: Optional[Path] = None
) -> tuple[int, str, str]:
    """Run a command and capture its output. Never raises for tool failures."""
    try:
        proc = subprocess.run(
            command,
            capture_output=True,
            text=True,
            timeout=timeout,
            cwd=str(cwd) if cwd else None,
        )
        return proc.returncode, proc.stdout or "", proc.stderr or ""
    except FileNotFoundError as exc:
        return 127, "", str(exc)
    except subprocess.TimeoutExpired as exc:
        stdout = exc.stdout.decode() if isinstance(exc.stdout, bytes) else (exc.stdout or "")
        stderr = exc.stderr.decode() if isinstance(exc.stderr, bytes) else (exc.stderr or "")
        return 124, stdout, stderr


def first_line(text: str, fallback: str) -> str:
    text = text.strip()
    return text.splitlines()[0] if text else fallback


_CARGO_SUMMARY = re.compile(
    r"test result: (ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored"
)


def outcome_for(returncode: int, combined_output: str) -> str:
    """Classify a fixture result from its exit status and captured output.

    A cargo-style summary is parsed when present: any failure fails, and a
    zero-pass run (nothing ran, or every test was ignored) is a skip, never
    a pass. Exit 0 without a summary is a pass. A missing command is
    unavailable, a timeout is a timeout, and output that says a Metal
    device is required is unsupported. Anything else fails. A skip,
    timeout, unavailable or unsupported result is never reported as a pass.
    """
    if returncode == 124:
        return TIMEOUT
    if returncode == 127:
        return UNAVAILABLE
    summaries = list(_CARGO_SUMMARY.finditer(combined_output))
    if summaries:
        # A suite prints one summary per test target: any failed target
        # fails the fixture, and a nonzero exit is a failure however the
        # first summary read. A run where nothing passed anywhere is a
        # skip: it is never GPU success.
        if returncode != 0:
            return FAIL
        if any(int(match.group(3)) > 0 or match.group(1) == "FAILED" for match in summaries):
            return FAIL
        if all(int(match.group(2)) == 0 for match in summaries):
            return SKIP
        return PASS
    if returncode == 0:
        return PASS
    lowered = combined_output.lower()
    # The repo's ignored-test phrasing names the device; a crate that merely
    # contains "metal" (for example an MSRV error naming xenora-metal) is a
    # build failure, not an unsupported-device skip.
    if (
        "requires a native metal device" in lowered
        or "needs a native metal device" in lowered
        or "needs native metal device" in lowered
    ):
        return UNSUPPORTED
    if "ignored" in lowered and "0 passed" in lowered:
        return SKIP
    return FAIL


def inspect_dependency(path: str) -> DependencyEvidence:
    """Collect file/lipo/otool evidence for one path, or record the failure."""
    evidence = DependencyEvidence(path=path)
    target = Path(path)
    if not target.exists():
        evidence.error = "path does not exist"
        return evidence
    rc, out, err = run_capture(["file", "-b", path])
    if rc == 0:
        evidence.file = out.strip()
    else:
        evidence.file = f"file failed: {first_line(err, 'no output')}"
    rc, out, err = run_capture(["lipo", "-archs", path])
    if rc == 0:
        evidence.archs = out.strip()
    else:
        evidence.archs = ""
        evidence.error = evidence.error or first_line(err, "lipo failed")
    rc, out, err = run_capture(["otool", "-L", path])
    if rc == 0:
        evidence.linkage = out.strip()
    else:
        evidence.linkage = ""
        evidence.error = evidence.error or first_line(err, "otool failed")
    if not evidence.error:
        if evidence.archs != "arm64":
            evidence.error = f"non-arm64 architectures: {evidence.archs!r}"
        elif any(
            name in evidence.linkage.casefold()
            for name in ("vulkan", "moltenvk")
        ):
            evidence.error = "unexpected graphics linkage in otool -L"
        else:
            evidence.ok = True
    return evidence


def run_fixture(name: str, command: list[str], timeout: int) -> FixtureResult:
    """Run one fixture command in a bounded process and classify it."""
    result = FixtureResult(name=name, command=list(command))
    if not command:
        result.outcome = UNAVAILABLE
        result.detail = "no command specified"
        return result
    rc, stdout, stderr = run_capture(command, timeout=timeout, cwd=REPO_ROOT)
    result.returncode = rc
    result.stdout_tail = stdout[-500:]
    result.stderr_tail = stderr[-500:]
    result.outcome = outcome_for(rc, stdout + stderr)
    result.detail = f"exit {rc}"
    return result


def host_info() -> dict[str, Any]:
    """Host architecture, translation status and OS version."""
    info: dict[str, Any] = {
        "uname_machine": platform.machine(),
        "uname_system": platform.system(),
        "uname_release": platform.release(),
        "python_version": platform.python_version(),
    }
    rc, out, _ = run_capture(["sysctl", "-n", "sysctl.proc_translated"])
    info["proc_translated"] = out.strip() if rc == 0 else "unknown"
    info["rosetta_active"] = info["proc_translated"] not in ("0",)
    rc, out, _ = run_capture(["sw_vers", "-productVersion"])
    info["macos_version"] = out.strip() if rc == 0 else "unknown"
    rc, out, _ = run_capture(["sysctl", "-n", "machdep.cpu.brand_string"])
    info["cpu_brand"] = out.strip() if rc == 0 else "unknown"
    return info


def tool_versions() -> dict[str, str]:
    """Versions of the native tools the report depends on, or explicit gaps."""
    tools: dict[str, str] = {}
    for name, command in (
        ("rustc", ["rustc", "-vV"]),
        ("cargo", ["cargo", "--version"]),
        ("python3", ["python3", "--version"]),
        ("clang", ["clang", "--version"]),
        ("swift", ["swift", "--version"]),
    ):
        rc, out, err = run_capture(command)
        text = (out or err).strip()
        tools[name] = first_line(text, "unavailable") if rc == 0 else "unavailable"
    return tools


def metal_info() -> dict[str, Any]:
    """What this host reports about its Metal device and toolchain.

    This records runtime-observable host state only. The Apple feature-set
    table is a documented reference, not proof of execution on this host;
    device execution is proven only by a Metal fixture outcome above.
    """
    info: dict[str, Any] = {}
    rc, out, _ = run_capture(["xcrun", "--find", "metal"])
    info["metal_toolchain"] = out.strip() if rc == 0 else "not found"
    rc, out, _ = run_capture(["xcrun", "metal", "--version"])
    info["metal_version"] = first_line(out, "not available") if rc == 0 else "not available"
    rc, out, _ = run_capture(["xcodebuild", "-version"])
    info["xcodebuild"] = first_line(out, "not available") if rc == 0 else "not available"
    rc, out, _ = run_capture(
        ["system_profiler", "SPDisplaysDataType"], timeout=60
    )
    info["displays_report"] = out.strip() if rc == 0 else "unavailable"
    info["metal_shader_toolchain_available"] = info["metal_toolchain"] != "not found"
    return info


def device_matrix(cpu_brand: str) -> dict[str, str]:
    """An M1-M5+ matrix with unknown cells, marking this host where matched."""
    matrix = {key: "unknown" for key in DEVICE_MATRIX_KEYS}
    for key in DEVICE_MATRIX_KEYS:
        if key.rstrip("+") in cpu_brand:
            matrix[key] = "this host"
            break
    return matrix


def build_receipt(
    cpu_command: Optional[list[str]] = None,
    metal_command: Optional[list[str]] = None,
    extra_dependencies: Optional[list[str]] = None,
    include_matrix: bool = True,
    timeout: int = DEFAULT_TIMEOUT_SECONDS,
) -> Receipt:
    """Collect one host's native receipt."""
    receipt = Receipt()

    rc, out, _ = run_capture(["git", "rev-parse", "HEAD"], cwd=REPO_ROOT)
    receipt.candidate_sha = out.strip() if rc == 0 else "unknown"
    rc, out, _ = run_capture(["git", "status", "--porcelain"], cwd=REPO_ROOT)
    if rc == 0:
        lines = [line for line in out.splitlines() if line.strip()]
        receipt.dirty = bool(lines)
        receipt.dirty_detail = "\n".join(lines[:50])
    else:
        receipt.dirty_detail = "git status unavailable"

    receipt.host = host_info()
    receipt.tools = tool_versions()
    receipt.metal = metal_info()
    receipt.validation = {
        "metal_validation_layers": "not enabled by this receipt; recorded from the fixture command",
        "fixture_timeout_seconds": timeout,
    }

    commands: list[str] = []
    fixtures: list[FixtureResult] = []
    if cpu_command:
        commands.append(" ".join(cpu_command))
        fixtures.append(run_fixture("cpu", list(cpu_command), timeout))
    if metal_command:
        commands.append(" ".join(metal_command))
        fixtures.append(run_fixture("metal", list(metal_command), timeout))
    receipt.commands = commands

    dependencies: list[DependencyEvidence] = []
    for path in extra_dependencies or []:
        dependencies.append(inspect_dependency(path))
    binary = REPO_ROOT / "target" / "debug" / "betterxen"
    if binary.exists():
        dependencies.append(inspect_dependency(str(binary)))
    release = REPO_ROOT / "target" / "release" / "betterxen"
    if release.exists():
        dependencies.append(inspect_dependency(str(release)))
    receipt.dependencies = [dep.to_dict() for dep in dependencies]
    receipt.fixtures = [fixture.to_dict() for fixture in fixtures]

    if include_matrix:
        receipt.matrix = device_matrix(str(receipt.host.get("cpu_brand", "")))

    outcomes = [fixture["outcome"] for fixture in receipt.fixtures]
    if not outcomes:
        receipt.overall = UNAVAILABLE
    elif any(outcome == FAIL for outcome in outcomes):
        receipt.overall = FAIL
    elif all(outcome == PASS for outcome in outcomes):
        receipt.overall = PASS
    else:
        receipt.overall = "partial"
    return receipt


def write_receipt(receipt: Receipt, output: Path) -> None:
    """Write the report JSON, creating the ignored artifacts directory."""
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(receipt.to_dict(), indent=2) + "\n", encoding="utf-8")


def main(argv: Optional[list[str]] = None) -> int:
    parser = argparse.ArgumentParser(
        description="Record one native ARM64/Metal capability receipt."
    )
    parser.add_argument(
        "--cpu-command",
        default=None,
        help="CPU fixture command (shlex-split); e.g. 'cargo test --locked -p xenora-loader --test cpu_known_answers'",
    )
    parser.add_argument(
        "--metal-command",
        default=None,
        help="native Metal fixture command (shlex-split); a skip/unavailable result is recorded, never counted as a pass",
    )
    parser.add_argument(
        "--dependency",
        action="append",
        default=[],
        help="additional executable or loadable path to inspect with file/lipo/otool",
    )
    parser.add_argument(
        "--output",
        default=str(DEFAULT_OUTPUT),
        help=f"output JSON path (default: {DEFAULT_OUTPUT})",
    )
    parser.add_argument("--no-matrix", action="store_true", help="omit the M1-M5+ matrix")
    parser.add_argument(
        "--timeout",
        type=int,
        default=DEFAULT_TIMEOUT_SECONDS,
        help=f"fixture wall-clock timeout in seconds (default: {DEFAULT_TIMEOUT_SECONDS})",
    )
    args = parser.parse_args(argv)

    cpu_command = args.cpu_command.split() if args.cpu_command else None
    metal_command = args.metal_command.split() if args.metal_command else None
    try:
        receipt = build_receipt(
            cpu_command=cpu_command,
            metal_command=metal_command,
            extra_dependencies=args.dependency,
            include_matrix=not args.no_matrix,
            timeout=args.timeout,
        )
        write_receipt(receipt, Path(args.output))
    except OSError as exc:
        print(f"could not write the receipt: {exc}", file=sys.stderr)
        return 2
    print(f"wrote {args.output}")
    print(f"overall: {receipt.overall}")
    for fixture in receipt.fixtures:
        print(f"{fixture['name']}: {fixture['outcome']} ({fixture['detail']})")
    return 0


if __name__ == "__main__":
    sys.exit(main())

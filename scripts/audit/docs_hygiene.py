#!/usr/bin/env python3
"""Fast, deterministic documentation and repository hygiene checks.

Checks only tracked files:
  1. no merge-conflict markers in tracked text files;
  2. every relative Markdown link points at a tracked file or directory;
  3. every workflow file under .github/workflows parses as YAML.

External URLs and in-page anchors are not fetched or validated, so the result
does not depend on the network. Exit status is 1 when a problem is found. Pass --require-yaml to fail, not skip,
when PyYAML is missing.
"""
import re
import subprocess
import sys
from pathlib import Path, PurePosixPath
from urllib.parse import unquote

ROOT = Path(__file__).resolve().parents[2]
MARKER = re.compile(r"^(<{7}( |$)|={7}$|>{7}( |$))")
LINK = re.compile(r"(?<!\!)\[[^\]]*\]\(([^)\s]+)(?:\s+\"[^\"]*\")?\)")
FENCE = re.compile(r"^\s*(```|~~~)")


def tracked():
    out = subprocess.run(["git", "ls-files", "-z"], cwd=ROOT, check=True,
                         capture_output=True).stdout.decode("utf-8", "replace")
    return [p for p in out.split("\0") if p and (ROOT / p).is_file()]


def text(path):
    data = (ROOT / path).read_bytes()
    if b"\0" in data:
        return None
    return data.decode("utf-8", "replace")


def check_markers(files, problems):
    for path in files:
        if path.endswith((".patch", ".diff")):
            continue
        body = text(path)
        if body is None:
            continue
        for number, line in enumerate(body.splitlines(), 1):
            if MARKER.match(line):
                problems.append(f"{path}:{number}: merge conflict marker")


def check_links(files, problems):
    known = set(files)
    dirs = {str(d) for f in files for d in PurePosixPath(f).parents}
    for path in files:
        if not path.endswith(".md") or path.startswith(("artifacts/", "third_party/")):
            continue
        in_fence = False
        for number, line in enumerate((text(path) or "").splitlines(), 1):
            if FENCE.match(line):
                in_fence = not in_fence
                continue
            if in_fence:
                continue
            line = re.sub(r"`[^`]*`", "", line)
            for target in LINK.findall(line):
                if re.match(r"^([a-z][a-z0-9+.-]*:|#|//)", target, re.I):
                    continue
                target = unquote(target.split("#", 1)[0].split("?", 1)[0])
                if not target:
                    continue
                base = PurePosixPath(path).parent
                resolved = PurePosixPath(target.lstrip("/")) if target.startswith("/") else base / target
                parts = []
                for part in resolved.parts:
                    if part == "..":
                        if parts:
                            parts.pop()
                    elif part != ".":
                        parts.append(part)
                norm = "/".join(parts)
                if norm and norm not in known and norm not in dirs:
                    problems.append(f"{path}:{number}: broken relative link '{target}'")


def check_workflows(files, problems, require_yaml):
    try:
        import yaml
    except ImportError:
        if require_yaml:
            problems.append("PyYAML is not installed but --require-yaml was given")
        else:
            print("note: PyYAML not installed; workflow YAML parse check skipped", file=sys.stderr)
        return
    for path in files:
        if path.startswith(".github/workflows/") and path.endswith((".yml", ".yaml")):
            try:
                yaml.safe_load((ROOT / path).read_text(encoding="utf-8"))
            except yaml.YAMLError as err:
                problems.append(f"{path}: invalid YAML: {str(err).splitlines()[0]}")


def main():
    files = tracked()
    problems = []
    check_markers(files, problems)
    check_links(files, problems)
    check_workflows(files, problems, "--require-yaml" in sys.argv[1:])
    for problem in problems:
        print(problem)
    print(f"docs hygiene: {len(files)} tracked files checked, {len(problems)} problem(s)")
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main())

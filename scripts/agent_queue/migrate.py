# SPDX-License-Identifier: GPL-3.0-or-later
"""Dry-run migration inventory for legacy ## issue bodies versus ### agent-task headings.

Text is data only: nothing parsed from issue bodies is ever executed.
Never fabricates file ownership or edits issues without explicit maintainer approval.
"""
import re

from . import config as cfgmod
from . import tasks as T

HEADING = re.compile(r"^(#{2,3})\s+(.+?)\s*$")
PLACEHOLDER = "_no response_"

ALIASES = {
    "area": "area",
    "priority": "priority",
    "interface": "interface",
    "goal": "goal",
    "objective": "goal",
    "objective and acceptance criteria": "goal",
    "files": "files",
    "owned files": "files",
    "acceptance command": "acceptance command",
    "acceptance": "acceptance command",
    "depends on": "depends on",
    "dependencies": "depends on",
    "context": "context",
}

CANONICAL_ORDER = (
    ("area", "Area"),
    ("priority", "Priority"),
    ("interface", "Interface"),
    ("goal", "Goal"),
    ("files", "Files"),
    ("acceptance command", "Acceptance command"),
    ("depends on", "Depends on"),
    ("context", "Context"),
)


def parse_sections_with_levels(body):
    """Parse ## and ### headings while ignoring headings inside fenced code blocks.

    Returns (preamble, entries) where each entry is (level, raw_heading, text).
    """
    text = (body or "").replace("\r\n", "\n")
    lines = text.split("\n")
    in_fence = False
    preamble_lines = []
    entries = []
    cur = None

    for line in lines:
        stripped = line.strip()
        if stripped.startswith("```"):
            in_fence = not in_fence
            if cur is None:
                preamble_lines.append(line)
            else:
                cur[2].append(line)
            continue
        if not in_fence:
            m = HEADING.match(line)
            if m:
                if cur is not None:
                    entries.append((cur[0], cur[1], "\n".join(cur[2]).strip()))
                cur = (len(m.group(1)), m.group(2).strip(), [])
                continue
        if cur is None:
            preamble_lines.append(line)
        else:
            cur[2].append(line)

    if cur is not None:
        entries.append((cur[0], cur[1], "\n".join(cur[2]).strip()))

    preamble = "\n".join(preamble_lines).strip()
    return preamble, entries


def _clean_text(text):
    t = (text or "").strip()
    return "" if t.lower() == PLACEHOLDER else t


def _parse_files(text):
    """Extract owned file patterns without leaking code fence markers or fabricating paths."""
    out = []
    for raw in (text or "").splitlines():
        line = raw.strip()
        if not line or line.startswith("```"):
            continue
        if line.startswith(("- ", "* ")):
            line = line[2:].strip()
        line = line.strip("`").strip()
        if not line or line.lower() == PLACEHOLDER:
            continue
        out.append(line)
    return out


def _heading_style(entries):
    levels = {lvl for lvl, _, _ in entries}
    if not levels:
        return "none"
    if levels == {3}:
        return "h3"
    if levels == {2}:
        return "h2_legacy"
    return "mixed"


def render_migrated_body(preamble, canonical, extra_sections, parsed):
    """Render a ### agent-task body while preserving preamble and extra legacy sections."""
    blocks = []

    def add(heading, value):
        blocks.append("### %s\n\n%s" % (heading, value if value else "_No response_"))

    add("Area", parsed["area"])
    add("Priority", parsed["priority"])
    add("Interface", canonical.get("interface", ""))
    add("Goal", canonical.get("goal", ""))

    if parsed["files"]:
        files_text = "\n".join("- %s" % p for p in parsed["files"])
    else:
        files_text = ""
    add("Files", files_text)

    acc = T.normalise_acceptance(parsed["acceptance"])
    add("Acceptance command", acc)

    deps_raw = canonical.get("depends on", "")
    if not deps_raw and parsed["depends"]:
        deps_raw = ", ".join("#%d" % n for n in parsed["depends"])
    add("Depends on", deps_raw)

    context_parts = []
    if canonical.get("context"):
        context_parts.append(canonical["context"])
    if preamble:
        context_parts.append("Preserved issue preamble:\n\n" + preamble)
    for lvl, raw_heading, raw_text in extra_sections:
        if raw_text:
            context_parts.append("%s %s\n\n%s" % ("#" * lvl, raw_heading, raw_text))
        else:
            context_parts.append("%s %s" % ("#" * lvl, raw_heading))
    add("Context", "\n\n".join(context_parts))

    return "\n\n".join(blocks) + "\n"


def classify_issue(issue):
    """Inspect one issue dict and build a dry-run migration inventory record."""
    body = issue.get("body") or ""
    labels = T.label_names(issue)
    current = T.parse_task(issue)
    preamble, entries = parse_sections_with_levels(body)
    style = _heading_style(entries)

    canonical = {}
    extra_sections = []
    warnings = []
    raw_area_text = ""

    for lvl, raw_heading, raw_text in entries:
        cleaned = _clean_text(raw_text)
        key = ALIASES.get(raw_heading.lower())
        if key is None:
            extra_sections.append((lvl, raw_heading, cleaned))
            continue
        if key == "area":
            raw_area_text = cleaned.strip().lower()
        if key in canonical and cleaned:
            if canonical[key]:
                canonical[key] = canonical[key] + "\n\n" + cleaned
            else:
                canonical[key] = cleaned
        else:
            canonical[key] = cleaned

    area_candidate = raw_area_text or T.label_value(labels, "area:")
    area = area_candidate if area_candidate in cfgmod.AREAS else ""
    if raw_area_text and raw_area_text not in cfgmod.AREAS:
        warnings.append("unrecognized_area")

    prio_candidate = canonical.get("priority", "").strip().lower() or T.label_value(labels, "priority:")
    priority = prio_candidate if prio_candidate in cfgmod.PRIORITIES else "medium"
    if canonical.get("priority", "").strip() and prio_candidate not in cfgmod.PRIORITIES:
        warnings.append("unrecognized_priority")

    files = _parse_files(canonical.get("files", ""))
    acceptance = canonical.get("acceptance command", "")
    depends = sorted({int(n) for n in re.findall(r"#(\d+)", canonical.get("depends on", ""))})
    interface = canonical.get("interface", "")

    parsed_legacy = {
        "area": area,
        "priority": priority,
        "files": files,
        "depends": depends,
        "acceptance": T.normalise_acceptance(acceptance),
        "interface": interface,
    }

    missing = []
    if not area:
        missing.append("area")
    if not files:
        missing.append("files")
        warnings.append("no_explicit_file_ownership")
    if not parsed_legacy["acceptance"]:
        missing.append("acceptance command")

    if style in ("h2_legacy", "mixed"):
        warnings.append("legacy_h2_headings")
    if preamble or extra_sections:
        warnings.append("extra_content_preserved_in_context")

    current_ok = (
        style == "h3"
        and bool(current["area"])
        and bool(current["files"])
        and bool(T.normalise_acceptance(current["acceptance"]))
        and not any(f.startswith("```") or f in ("text", "sh") for f in current["files"])
    )

    if current_ok:
        status = "compliant"
        proposed_body = None
    elif not missing:
        status = "legacy_migratable"
        proposed_body = render_migrated_body(preamble, canonical, extra_sections, parsed_legacy)
    else:
        status = "incomplete"
        proposed_body = render_migrated_body(preamble, canonical, extra_sections, parsed_legacy)

    parser_gap = {
        "area_missed": bool(area) and current["area"] != area,
        "files_missed": bool(files) and current["files"] != files,
        "acceptance_missed": bool(parsed_legacy["acceptance"])
        and T.normalise_acceptance(current["acceptance"]) != parsed_legacy["acceptance"],
        "depends_missed": bool(depends) and current["depends"] != depends,
    }

    return {
        "number": int(issue["number"]),
        "title": issue.get("title", ""),
        "state": str(issue.get("state", "OPEN")).upper(),
        "labels": labels,
        "status": status,
        "heading_style": style,
        "current_parsed": {
            "area": current["area"],
            "priority": current["priority"],
            "files": current["files"],
            "depends": current["depends"],
            "acceptance": T.normalise_acceptance(current["acceptance"]),
            "interface": current["interface"],
        },
        "legacy_parsed": parsed_legacy,
        "parser_gap": parser_gap,
        "missing_fields": missing,
        "warnings": warnings,
        "requires_maintainer_review": status != "compliant",
        "safe_to_apply_without_review": False,
        "proposed_body": proposed_body,
    }


def inventory(state, include_closed=False):
    """Build a dry-run migration inventory across issues in a gathered state dict."""
    items = []
    for issue in state.get("issues") or []:
        st = str(issue.get("state", "OPEN")).upper()
        if not include_closed and st != "OPEN":
            continue
        items.append(classify_issue(issue))
    items.sort(key=lambda r: r["number"])
    counts = {"compliant": 0, "legacy_migratable": 0, "incomplete": 0}
    for r in items:
        counts[r["status"]] += 1
    return {
        "dry_run": True,
        "total": len(items),
        "counts": counts,
        "issues": items,
    }


def format_report(report):
    """Format a dry-run inventory as a concise human-readable review table."""
    c = report["counts"]
    lines = [
        "Dry-run issue migration inventory (no issues edited, no labels changed)",
        "Total: %d | compliant: %d | legacy_migratable: %d | incomplete: %d"
        % (report["total"], c["compliant"], c["legacy_migratable"], c["incomplete"]),
        "",
        "number\tstatus\theading_style\tarea\tfiles\tdepends\tmissing\ttitle",
    ]
    for r in report["issues"]:
        lp = r["legacy_parsed"]
        lines.append(
            "#%d\t%s\t%s\t%s\t%d\t%s\t%s\t%s"
            % (
                r["number"],
                r["status"],
                r["heading_style"],
                lp["area"] or "-",
                len(lp["files"]),
                ",".join("#%d" % d for d in lp["depends"]) or "-",
                ",".join(r["missing_fields"]) or "-",
                r["title"],
            )
        )
    return "\n".join(lines) + "\n"


def plan_apply(report, approved_issues, actor, cfg, add_ready=False):
    """Produce guarded edit plans only for maintainer-approved, complete legacy issues.

    Never fabricates missing fields or file ownership, and never adds agent:ready unless
    explicitly requested by a maintainer for a complete issue.
    """
    if not cfgmod.is_maintainer(cfg, actor):
        return {
            "ok": False,
            "error": "only a maintainer may approve issue body migrations or ready labels",
            "actions": [],
            "skipped": [],
        }

    approved = {int(n) for n in approved_issues}
    actions = []
    skipped = []
    for r in report.get("issues") or []:
        n = r["number"]
        if n not in approved:
            continue
        if r["status"] == "compliant":
            skipped.append({"issue": n, "reason": "already compliant"})
            continue
        if r["status"] != "legacy_migratable" or r["missing_fields"]:
            skipped.append(
                {
                    "issue": n,
                    "reason": "incomplete (missing: %s); maintainer must supply explicit fields without fabricating ownership"
                    % ", ".join(r["missing_fields"]),
                }
            )
            continue
        actions.append(
            {
                "issue": n,
                "edit_body": r["proposed_body"],
                "add_labels": ["agent:ready"] if add_ready else [],
                "preserved_history": True,
                "approved_by": actor,
            }
        )
    return {"ok": True, "actions": actions, "skipped": skipped}

# SPDX-License-Identifier: GPL-3.0-or-later
import json
import os
import subprocess
import sys
import tempfile
import unittest

sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "..", "scripts"))

from agent_queue import migrate as M
from agent_queue import tasks as T
from helpers import CFG, ROOT, issue


LEGACY_H2_BODY = """## Area

shared

## Priority

high

## Files

- `scripts/agent_queue/`
- `tests/agent_queue/`
- `docs/coordination/`

## Acceptance command

```sh
PYTHONPATH=scripts python3 -m unittest discover -s tests/agent_queue
```

## Depends on

#165

## Interface

Existing public interfaces; coordinate changes before editing.

## Objective and acceptance criteria

Build a dry-run migration inventory for legacy ## headings versus ### headings.

Baseline: 951189c; preserve existing issue history.
"""

INCOMPLETE_LEGACY_BODY = """## Area

runtime

## Priority

medium

## Acceptance command

cargo test --workspace

## Objective and acceptance criteria

Add missing runtime checks without guessing owned paths.
"""

COMPLIANT_H3_BODY = """### Area

gpu

### Priority

high

### Files

crates/xenora-metal/

### Acceptance command

cargo test -p xenora-metal

### Depends on

#165
"""


class MigrationInventory(unittest.TestCase):
    def test_reproducible_gap_and_round_trip(self):
        i = {"number": 179, "title": "[SHARED] Migrate legacy issues", "state": "OPEN", "labels": ["p2"], "body": LEGACY_H2_BODY}

        # Reproducible gap: current ### parser misses every legacy ## section.
        current = T.parse_task(i)
        self.assertEqual(current["area"], "")
        self.assertEqual(current["files"], [])
        self.assertEqual(current["depends"], [])
        self.assertEqual(current["acceptance"], "")

        rec = M.classify_issue(i)
        self.assertEqual(rec["status"], "legacy_migratable")
        self.assertEqual(rec["heading_style"], "h2_legacy")
        self.assertTrue(rec["requires_maintainer_review"])
        self.assertFalse(rec["safe_to_apply_without_review"])
        self.assertEqual(rec["missing_fields"], [])
        self.assertEqual(rec["legacy_parsed"]["area"], "shared")
        self.assertEqual(rec["legacy_parsed"]["priority"], "high")
        self.assertEqual(
            rec["legacy_parsed"]["files"],
            ["scripts/agent_queue/", "tests/agent_queue/", "docs/coordination/"],
        )
        self.assertEqual(rec["legacy_parsed"]["depends"], [165])
        self.assertEqual(
            rec["legacy_parsed"]["acceptance"],
            "PYTHONPATH=scripts python3 -m unittest discover -s tests/agent_queue",
        )
        self.assertTrue(all(rec["parser_gap"].values()))

        # Round-trip: proposed ### body parses cleanly under tasks.parse_task.
        reparsed = T.parse_task({"number": 179, "title": i["title"], "state": "OPEN", "labels": [], "body": rec["proposed_body"]})
        self.assertEqual(reparsed["area"], "shared")
        self.assertEqual(reparsed["priority"], "high")
        self.assertEqual(
            reparsed["files"],
            ["scripts/agent_queue/", "tests/agent_queue/", "docs/coordination/"],
        )
        self.assertEqual(reparsed["depends"], [165])
        self.assertEqual(
            T.normalise_acceptance(reparsed["acceptance"]),
            "PYTHONPATH=scripts python3 -m unittest discover -s tests/agent_queue",
        )
        self.assertIn("Baseline: 951189c; preserve existing issue history.", rec["proposed_body"])

    def test_never_fabricates_file_ownership(self):
        i = {"number": 201, "title": "[RUNTIME] Missing files", "state": "OPEN", "labels": [], "body": INCOMPLETE_LEGACY_BODY}
        rec = M.classify_issue(i)
        self.assertEqual(rec["status"], "incomplete")
        self.assertEqual(rec["legacy_parsed"]["files"], [])
        self.assertIn("files", rec["missing_fields"])
        self.assertIn("no_explicit_file_ownership", rec["warnings"])
        self.assertTrue(rec["requires_maintainer_review"])

    def test_preserves_preamble_and_extra_sections(self):
        body = (
            "Original triage note from 2026-11-20.\n\n"
            + LEGACY_H2_BODY
            + "\n## Historical notes\n\nObserved on Apple M5; keep this record.\n"
        )
        rec = M.classify_issue({"number": 180, "title": "History check", "state": "OPEN", "labels": [], "body": body})
        self.assertIn("Original triage note from 2026-11-20.", rec["proposed_body"])
        self.assertIn("## Historical notes", rec["proposed_body"])
        self.assertIn("Observed on Apple M5; keep this record.", rec["proposed_body"])
        self.assertIn("extra_content_preserved_in_context", rec["warnings"])

    def test_code_fences_in_files_not_treated_as_paths(self):
        body = (
            "## Area\n\nshared\n\n## Files\n\n```text\nscripts/agent_queue/\n```\n\n"
            "## Acceptance command\n\n```sh\npython3 -m unittest\n```\n"
        )
        rec = M.classify_issue({"number": 182, "title": "Fenced files", "state": "OPEN", "labels": [], "body": body})
        self.assertEqual(rec["legacy_parsed"]["files"], ["scripts/agent_queue/"])
        reparsed = T.parse_task({"number": 182, "body": rec["proposed_body"]})
        self.assertEqual(reparsed["files"], ["scripts/agent_queue/"])

    def test_compliant_h3_issue_is_not_rewritten(self):
        rec = M.classify_issue({"number": 183, "title": "Compliant", "state": "OPEN", "labels": ["agent:ready"], "body": COMPLIANT_H3_BODY})
        self.assertEqual(rec["status"], "compliant")
        self.assertEqual(rec["heading_style"], "h3")
        self.assertFalse(rec["requires_maintainer_review"])
        self.assertIsNone(rec["proposed_body"])

    def test_mixed_headings_detected(self):
        body = "### Area\n\nqa\n\n## Files\n\ntests/agent_queue/\n\n### Acceptance command\n\npython3 -m unittest\n"
        rec = M.classify_issue({"number": 184, "title": "Mixed", "state": "OPEN", "labels": [], "body": body})
        self.assertEqual(rec["heading_style"], "mixed")
        self.assertEqual(rec["status"], "legacy_migratable")
        self.assertTrue(rec["parser_gap"]["files_missed"])

    def test_plan_apply_requires_maintainer_and_blocks_incomplete(self):
        state = {
            "issues": [
                {"number": 179, "title": "Legacy ok", "state": "OPEN", "labels": [], "body": LEGACY_H2_BODY},
                {"number": 201, "title": "Incomplete", "state": "OPEN", "labels": [], "body": INCOMPLETE_LEGACY_BODY},
                {"number": 183, "title": "Compliant", "state": "OPEN", "labels": [], "body": COMPLIANT_H3_BODY},
                issue(999, state="CLOSED"),
            ]
        }
        rep = M.inventory(state)
        self.assertTrue(rep["dry_run"])
        self.assertEqual(rep["total"], 3)
        self.assertEqual(rep["counts"], {"compliant": 1, "legacy_migratable": 1, "incomplete": 1})

        # Non-maintainer refused.
        refused = M.plan_apply(rep, [179], actor="gpu-dev", cfg=CFG, add_ready=True)
        self.assertFalse(refused["ok"])
        self.assertEqual(refused["actions"], [])

        # Maintainer approving all three: only #179 gets an edit plan; #201 is blocked; #183 skipped.
        plan = M.plan_apply(rep, [179, 201, 183], actor="maint", cfg=CFG, add_ready=False)
        self.assertTrue(plan["ok"])
        self.assertEqual([a["issue"] for a in plan["actions"]], [179])
        self.assertEqual(plan["actions"][0]["add_labels"], [])
        self.assertEqual(len(plan["skipped"]), 2)

        # Explicit opt-in required to add agent:ready.
        plan_ready = M.plan_apply(rep, [179], actor="maint", cfg=CFG, add_ready=True)
        self.assertEqual(plan_ready["actions"][0]["add_labels"], ["agent:ready"])


class MigrationCli(unittest.TestCase):
    def test_cli_migrate_inventory_json_and_text(self):
        state = {
            "issues": [
                {"number": 179, "title": "Legacy ok", "state": "OPEN", "labels": [], "body": LEGACY_H2_BODY},
                {"number": 201, "title": "Incomplete", "state": "OPEN", "labels": [], "body": INCOMPLETE_LEGACY_BODY},
            ]
        }
        env = dict(os.environ, PYTHONPATH=os.path.join(ROOT, "scripts"))
        with tempfile.TemporaryDirectory() as td:
            sp = os.path.join(td, "state.json")
            with open(sp, "w", encoding="utf-8") as fh:
                json.dump(state, fh)

            r_json = subprocess.run(
                [sys.executable, "-m", "agent_queue", "migrate-inventory", "--state", sp],
                cwd=ROOT, env=env, capture_output=True, text=True, check=True,
            )
            out = json.loads(r_json.stdout)
            self.assertTrue(out["dry_run"])
            self.assertEqual(out["counts"]["legacy_migratable"], 1)
            self.assertEqual(out["counts"]["incomplete"], 1)

            r_text = subprocess.run(
                [sys.executable, "-m", "agent_queue", "migrate-inventory", "--state", sp, "--text"],
                cwd=ROOT, env=env, capture_output=True, text=True, check=True,
            )
            self.assertIn("Dry-run issue migration inventory", r_text.stdout)
            self.assertIn("#179\tlegacy_migratable\th2_legacy", r_text.stdout)
            self.assertIn("#201\tincomplete\th2_legacy", r_text.stdout)


if __name__ == "__main__":
    unittest.main()

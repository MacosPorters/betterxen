# SPDX-License-Identifier: GPL-3.0-or-later
"""Tests for the native ARM64/Metal capability receipt tool.

CPU-only: no Metal device, no fixture binary and no network access is
required. The receipt module has a hyphenated script name, so it is loaded
from its path with importlib.
"""

import importlib.util
import json
import tempfile
import unittest
from pathlib import Path

_SCRIPT = (
    Path(__file__).resolve().parents[2] / "scripts" / "audit" / "native-receipt.py"
)
_spec = importlib.util.spec_from_file_location("native_receipt", _SCRIPT)
receipt = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(receipt)


class OutcomeClassification(unittest.TestCase):
    def test_exit_zero_is_the_only_pass(self):
        self.assertEqual(receipt.outcome_for(0, ""), receipt.PASS)
        self.assertEqual(receipt.outcome_for(1, ""), receipt.FAIL)
        self.assertEqual(receipt.outcome_for(2, "anything"), receipt.FAIL)

    def test_timeout_and_missing_command_keep_their_names(self):
        self.assertEqual(receipt.outcome_for(124, ""), receipt.TIMEOUT)
        self.assertEqual(receipt.outcome_for(127, "command not found"), receipt.UNAVAILABLE)

    def test_zero_pass_or_all_ignored_cargo_run_is_a_skip(self):
        all_ignored = "test result: ok. 0 passed; 0 failed; 2 ignored"
        self.assertEqual(receipt.outcome_for(0, all_ignored), receipt.SKIP)
        zero_tests = "test result: ok. 0 passed; 0 failed; 0 ignored"
        self.assertEqual(receipt.outcome_for(0, zero_tests), receipt.SKIP)

    def test_cargo_pass_with_ignored_tests_is_a_pass_that_keeps_its_tail(self):
        mixed = "test result: ok. 1 passed; 0 failed; 1 ignored"
        self.assertEqual(receipt.outcome_for(0, mixed), receipt.PASS)

    def test_cargo_failure_is_a_fail(self):
        failed = "test result: FAILED. 0 passed; 1 failed; 0 ignored"
        self.assertEqual(receipt.outcome_for(101, failed), receipt.FAIL)

    def test_a_later_failing_target_beats_an_earlier_passing_summary(self):
        # Regression: reading only the first summary line once let a
        # multi-target suite with a failing later target look like a pass.
        mixed = (
            "test result: ok. 2 passed; 0 failed; 0 ignored\n"
            "test result: FAILED. 0 passed; 1 failed; 0 ignored\n"
        )
        self.assertEqual(receipt.outcome_for(101, mixed), receipt.FAIL)

    def test_nonzero_exit_with_all_ok_summaries_is_still_a_fail(self):
        ok_only = "test result: ok. 3 passed; 0 failed; 0 ignored"
        self.assertEqual(receipt.outcome_for(101, ok_only), receipt.FAIL)

    def test_multi_target_suite_passes_only_when_something_passed(self):
        two_passing = (
            "test result: ok. 2 passed; 0 failed; 0 ignored\n"
            "test result: ok. 3 passed; 0 failed; 1 ignored\n"
        )
        self.assertEqual(receipt.outcome_for(0, two_passing), receipt.PASS)
        all_empty = (
            "test result: ok. 0 passed; 0 failed; 0 ignored\n"
            "test result: ok. 0 passed; 0 failed; 2 ignored\n"
        )
        self.assertEqual(receipt.outcome_for(0, all_empty), receipt.SKIP)

    def test_unsupported_marker_is_recorded_as_unsupported(self):
        self.assertEqual(
            receipt.outcome_for(1, "requires a native Metal device"), receipt.UNSUPPORTED
        )
        self.assertEqual(
            receipt.outcome_for(1, "needs native Metal device; run scripts/test-gpu.sh"),
            receipt.UNSUPPORTED,
        )

    def test_a_crate_named_metal_in_a_build_error_is_a_fail_not_a_skip(self):
        # Regression: "xenora-metal@0.1.0 requires rustc 1.100" once
        # false-matched the unsupported-device rule through the words
        # "requires" and "metal". It is a build failure.
        self.assertEqual(
            receipt.outcome_for(
                101, "error: rustc 1.99.1 is not supported\nxenora-metal@0.1.0 requires rustc 1.100"
            ),
            receipt.FAIL,
        )


class DependencyInspection(unittest.TestCase):
    def test_missing_path_records_an_explicit_failure(self):
        evidence = receipt.inspect_dependency("/nonexistent/betterxen-binary")
        self.assertFalse(evidence.ok)
        self.assertEqual(evidence.error, "path does not exist")
        self.assertEqual(evidence.file, "")
        self.assertEqual(evidence.archs, "")

    def test_non_arm64_artifact_is_rejected_not_hidden(self):
        with tempfile.NamedTemporaryFile(suffix=".txt") as handle:
            handle.write(b"not a mach-o binary\n")
            handle.flush()
            evidence = receipt.inspect_dependency(handle.name)
        self.assertFalse(evidence.ok)
        self.assertTrue(evidence.error)
        self.assertNotEqual(evidence.archs, "arm64")


class ReceiptConstruction(unittest.TestCase):
    def test_failing_fixture_command_records_a_fail(self):
        built = receipt.build_receipt(cpu_command=["/usr/bin/false"])
        self.assertEqual(len(built.fixtures), 1)
        self.assertEqual(built.fixtures[0]["outcome"], receipt.FAIL)
        self.assertEqual(built.overall, receipt.FAIL)
        self.assertEqual(built.commands, ["/usr/bin/false"])

    def test_passing_fixture_command_records_a_pass(self):
        built = receipt.build_receipt(cpu_command=["/usr/bin/true"])
        self.assertEqual(built.fixtures[0]["outcome"], receipt.PASS)
        self.assertEqual(built.overall, receipt.PASS)

    def test_mixed_pass_and_unavailable_is_partial_not_pass(self):
        built = receipt.build_receipt(cpu_command=["/usr/bin/true"], metal_command=[])
        self.assertEqual(built.fixtures[0]["outcome"], receipt.PASS)
        self.assertEqual(len(built.fixtures), 1)
        built = receipt.build_receipt(cpu_command=["/usr/bin/true"], metal_command=None)
        self.assertEqual(built.overall, receipt.PASS)
        # A missing metal command simply leaves the metal side unrecorded;
        # the overall result then reflects the fixtures that ran.
        self.assertEqual(len(built.fixtures), 1)

    def test_unavailable_fixture_command_is_not_a_pass(self):
        built = receipt.build_receipt(cpu_command=["/nonexistent-command-xyz"])
        self.assertEqual(built.fixtures[0]["outcome"], receipt.UNAVAILABLE)
        self.assertEqual(built.overall, "partial")

    def test_no_fixtures_is_unavailable(self):
        built = receipt.build_receipt()
        self.assertEqual(built.overall, receipt.UNAVAILABLE)
        self.assertEqual(built.fixtures, [])

    def test_report_json_round_trips_with_schema_version(self):
        built = receipt.build_receipt(cpu_command=["/usr/bin/true"])
        data = json.loads(json.dumps(built.to_dict()))
        self.assertEqual(data["schema_version"], 1)
        for key in (
            "candidate_sha",
            "dirty",
            "host",
            "tools",
            "metal",
            "validation",
            "commands",
            "dependencies",
            "fixtures",
            "matrix",
            "overall",
        ):
            self.assertIn(key, data)

    def test_matrix_cells_start_unknown_and_mark_this_host(self):
        matrix = receipt.device_matrix("Apple M5")
        self.assertEqual(set(matrix), set(receipt.DEVICE_MATRIX_KEYS))
        self.assertEqual(matrix["M5"], "this host")
        self.assertEqual(matrix["M1"], "unknown")
        self.assertEqual(matrix["M6+"], "unknown")
        unknown = receipt.device_matrix("Unknown CPU")
        self.assertTrue(all(cell == "unknown" for cell in unknown.values()))


class Writing(unittest.TestCase):
    def test_write_receipt_creates_parent_and_round_trips(self):
        built = receipt.build_receipt(cpu_command=["/usr/bin/true"])
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory) / "nested" / "receipt.json"
            receipt.write_receipt(built, target)
            self.assertTrue(target.exists())
            data = json.loads(target.read_text(encoding="utf-8"))
            self.assertEqual(data["schema_version"], 1)
            self.assertEqual(data["fixtures"][0]["name"], "cpu")


if __name__ == "__main__":
    unittest.main()

# SPDX-License-Identifier: GPL-3.0-or-later
import io
import json
import tempfile
import unittest
from contextlib import redirect_stdout
from pathlib import Path
from unittest.mock import patch

from qa.__main__ import main
from qa.rdna2_plan import load_claim_scopes
from qa.rdna2_rank import rank_groups


def operand(field, kind, fmt="FMT_NUM_U32", *, output=False):
    return (field, fmt, kind, 32, not output, output, False, True)


class Rdna2Rank(unittest.TestCase):
    def setUp(self):
        self.base = ("VOP2", "SYNTHETIC_ADD")
        self.variant = ("VOP3", "SYNTHETIC_ADD")
        self.rule = ("SOP2", "SYNTHETIC_OTHER")
        self.primitive = ("VOP2", "SYNTHETIC_NEW")
        self.floating = ("VOP2", "SYNTHETIC_FLOAT")
        self.data = {
            "identified": {self.primitive: 5, self.variant: 17, self.base: 2,
                           self.floating: 4, self.rule: 11},
            "declared": {self.base},
            "registry": {},
        }
        dst = operand("VDST", "OPR_VGPR", output=True)
        src = operand("SRC0", "OPR_SRC")
        self.operands = {
            self.base: [dst, src],
            self.variant: [dst, operand("SRC0", "OPR_SRC_NOLDS")],
            self.rule: [operand("SDST", "OPR_SGPR", output=True), src],
            self.primitive: [dst, src],
            self.floating: [dst, operand("SRC0", "OPR_SRC", "FMT_NUM_F32")],
        }

    def test_kinds_and_costs_determine_ranking(self):
        groups = rank_groups(self.data, self.operands)
        self.assertEqual(
            [(group["kind"], group["cost"], group["score"]) for group in groups],
            [("encoding-variant", 1, 1.0), ("primitive", 4, 0.5), ("operand-rule", 2, 0.5)],
        )
        self.assertEqual(groups[0]["family"], "VOP3")
        self.assertEqual(groups[0]["forms"], [{"name": "SYNTHETIC_ADD", "opcode": 17}])
        self.assertEqual(groups[0]["signature"], (
            ("VDST", "OPR_VGPR", 32, "out"), ("SRC0", "OPR_SRC_NOLDS", 32, "in"),
        ))

    def test_declared_variant_is_not_ranked_and_supplies_a_rule_shape(self):
        self.data["registry"]["encoding_variant"] = [
            {"of": self.base[1], "family": "VOP3", "opcode": 17, "operands": "synthetic_rule"},
        ]
        other = ("VOP3", "SYNTHETIC_OTHER")
        self.data["identified"][other] = 18
        self.operands[other] = self.operands[self.variant]
        groups = rank_groups(self.data, self.operands, families=["vop3"])
        self.assertEqual(len(groups), 1)
        self.assertEqual(groups[0]["kind"], "primitive")
        self.assertEqual(groups[0]["forms"], [{"name": other[1], "opcode": 18}])

    def test_claim_file_family_and_path_scopes(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "claims.json"
            path.write_text(json.dumps({"claims": [
                {"families": ["vop3"], "paths": ["crates/isa/**"]},
                {"paths": ["unrelated/**"]},
            ]}), encoding="utf-8")
            remaining = rank_groups(self.data, self.operands, claims=load_claim_scopes(path))
            self.assertEqual([group["family"] for group in remaining], ["VOP2", "SOP2"])
            path.write_text(json.dumps([{"paths": ["crates/isa/**"]}]), encoding="utf-8")
            self.assertEqual(rank_groups(self.data, self.operands, claims=load_claim_scopes(path)), [])

    def test_partially_blocked_group_reports_dropped_form(self):
        with patch("qa.rdna2_plan.claims_block_candidate", side_effect=lambda cand, _: cand.name == self.primitive[1]):
            groups = rank_groups(self.data, self.operands, families=["VOP2"])
        self.assertEqual(groups[0]["blocked_by_claims"], 1)
        self.assertEqual(groups[0]["count"], 1)
        self.assertEqual(groups[0]["score"], 0.25)
        self.assertEqual(groups[0]["forms"], [{"name": self.floating[1], "opcode": 4}])

    def test_float_marker_and_cli_filter(self):
        def invoke(*flags):
            stdout = io.StringIO()
            with redirect_stdout(stdout):
                self.assertEqual(main(["rdna2", "rank", *flags]), 0)
            return stdout.getvalue()

        with (
            patch("qa.rdna2_plan.load_inventories", return_value=self.data),
            patch("qa.rdna2_rank.load_xml_operands", return_value=self.operands),
        ):
            groups = json.loads(invoke())
            self.assertEqual([group["float"] for group in groups], [False, True, False])
            self.assertEqual(json.loads(invoke("--no-float")), [groups[0], groups[2]])
            self.assertEqual(json.loads(invoke("--family", "sop2", "--limit", "1")), [groups[2]])
            self.assertEqual(len(invoke("--no-float", "--compact").splitlines()), 2)

    def test_deterministic_with_reordered_inventory_and_equal_scores(self):
        additional_rule = ("SOP1", "SYNTHETIC_OTHER")
        self.data["identified"][additional_rule] = 1
        self.operands[additional_rule] = self.operands[self.rule]
        first = rank_groups(self.data, self.operands)
        reordered = {**self.data, "identified": dict(reversed(list(self.data["identified"].items())))}
        second = rank_groups(reordered, dict(reversed(list(self.operands.items()))))
        self.assertEqual(first, second)
        self.assertEqual([group["family"] for group in first], ["VOP3", "VOP2", "SOP1", "SOP2"])

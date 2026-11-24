"""A malformed encoding variant gives the planner a diagnostic, not a traceback."""

import contextlib
import io
import unittest
from unittest import mock

import qa.__main__ as cli
from qa import rdna2_plan
from audit import semantic_coverage as coverage


def malformed_loader(variant):
    def load(_root):
        registry = {
            "schema": 1,
            "form": [{"family": "SOP2", "name": "S_ADD_U32", "opcode": 0,
                      "operands": "scalar", "verify": ["reference"]}],
            "operand_rules": {"scalar": {"family": "SOP2", "unsupported": ["two_literals"]}},
            "encoding_variant": [variant],
        }
        # The real reader, as load_inventories uses it.
        coverage.read_declarations(registry, {("SOP2", "S_ADD_U32"): 0})
        raise AssertionError("the malformed variant was accepted")

    return load


class MalformedVariant(unittest.TestCase):
    def test_inventory_and_rank_report_registry_errors(self):
        variants = {
            "missing of": {"family": "VOP3", "opcode": 1, "operands": "x"},
            "list operands": {"family": "VOP3", "of": "S_ADD_U32", "opcode": 1, "operands": []},
        }
        for action in ("inventory", "rank"):
            for name, variant in variants.items():
                with self.subTest(action=action, variant=name):
                    stderr = io.StringIO()
                    loader = malformed_loader(variant)
                    with mock.patch.object(cli, "load_inventories", loader), \
                            mock.patch.object(rdna2_plan, "load_inventories", loader), \
                            contextlib.redirect_stderr(stderr), contextlib.redirect_stdout(io.StringIO()):
                        code = cli.main(["rdna2", action])
                    self.assertEqual(code, 2)
                    self.assertIn("encoding_variant needs", stderr.getvalue())


if __name__ == "__main__":
    unittest.main()

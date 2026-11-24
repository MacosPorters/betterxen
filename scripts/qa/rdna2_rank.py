# SPDX-License-Identifier: GPL-3.0-or-later
"""Rank registry expansion by XML identity and operand shape."""
from __future__ import annotations

import json
import runpy
from argparse import Namespace
from dataclasses import asdict, dataclass, field
from pathlib import Path
from typing import Any, Sequence

from . import rdna2_plan

# Relative planning weights, not measurements of implementation effort.
ENCODING_VARIANT_COST = 1
OPERAND_RULE_COST = 2
PRIMITIVE_COST = 4

FLOAT_FORMATS = {"FMT_NUM_F16", "FMT_NUM_F32", "FMT_NUM_F64"}
DIRECTIONS = {(False, False): "none", (True, False): "in",
              (False, True): "out", (True, True): "inout"}

FormKey = tuple[str, str]
Operand = tuple[str, str, str, int, bool, bool, bool, bool]
Signature = tuple[tuple[str, str, int, str], ...]


@dataclass
class RankGroup:
    key: str
    kind: str
    family: str
    signature: Signature
    cost: int
    forms: list[dict[str, str | int]] = field(default_factory=list)
    float: bool = False
    blocked_by_claims: int = 0

    def payload(self) -> dict[str, Any]:
        count = len(self.forms)
        return {**asdict(self), "count": count, "score": count / self.cost}


def load_xml_operands(root: Path, identified: dict[FormKey, int]) -> dict[FormKey, list[Operand]]:
    # Use the same base-row selection as the planner, including literal-only
    # forms and relative moves with non-default conditions.
    metadata = runpy.run_path(str(root / "crates/isa/generate_rdna2_meta.py"))
    snapshot = (root / "crates/isa/data/rdna2.jsonl").read_text(encoding="utf-8")
    _, _, forms = metadata["forms_from_snapshot"](snapshot)
    return {
        key: form["operands"]
        for form in forms
        if (key := (form["encoding"].upper(), form["name"])) in identified
    }


def rank_groups(
    data: dict[str, Any],
    operands: dict[FormKey, list[Operand]],
    *,
    families: Sequence[str] = (),
    claims: Sequence[rdna2_plan.ClaimScope] = (),
    no_float: bool = False,
    limit: int | None = None,
) -> list[dict[str, Any]]:
    identified = data["identified"]
    declared = set(data["declared"]) | {
        (variant["family"], variant["of"])
        for variant in data["registry"].get("encoding_variant", [])
    }
    signatures = {
        key: tuple((field, kind, size, DIRECTIONS[is_input, is_output])
                   for field, _, kind, size, is_input, is_output, _, _ in rows)
        for key, rows in operands.items()
    }
    formats = {
        key: tuple((row[1], row[3]) for row in rows)
        for key, rows in operands.items()
    }
    # The planner validates each form's rule. Its XML signature supplies the
    # kinds, widths and directions that the registry's rule alone lacks.
    declared_names = {name for _, name in declared}
    rule_shapes = {(key[0], signatures[key]) for key in declared if key in identified}
    declared_formats = {formats[key] for key in declared if key in identified}

    selected = {family.upper() for family in families}
    claimed_families = {family for scope in claims for family in scope.families}
    claimed_paths = {path for scope in claims if not scope.families for path in scope.paths}
    groups: dict[str, RankGroup] = {}
    for family, name in sorted(identified.keys() - declared):
        if selected and family not in selected:
            continue
        form = (family, name)
        signature = signatures[form]
        if name in declared_names:
            kind, cost = "encoding-variant", ENCODING_VARIANT_COST
        elif (family, signature) not in rule_shapes and formats[form] in declared_formats:
            kind, cost = "operand-rule", OPERAND_RULE_COST
        else:
            kind, cost = "primitive", PRIMITIVE_COST
        shape = ",".join(f"{field}/{kind}/{size}/{direction}" for field, kind, size, direction in signature)
        key = f"{kind}:{family}:{shape}"
        if key not in groups:
            groups[key] = RankGroup(key, kind, family, signature, cost)
        group = groups[key]
        group.float |= any(fmt in FLOAT_FORMATS for fmt, _ in formats[form])
        opcode = identified[form]
        candidate = rdna2_plan.Candidate(family, name, opcode, None, None, None)
        if family in claimed_families or rdna2_plan.claims_block_candidate(candidate, claimed_paths):
            group.blocked_by_claims += 1
        else:
            group.forms.append({"name": name, "opcode": opcode})

    ranked = [group.payload() for group in groups.values()
              if group.forms and not (no_float and group.float)]
    ranked.sort(key=lambda group: (-group["score"], -group["count"], group["key"]))
    return ranked[:limit]


def run_rank(root: Path, args: Namespace) -> int:
    data = rdna2_plan.load_inventories(root)
    operands = load_xml_operands(root, data["identified"])
    claims = rdna2_plan.load_claim_scopes(Path(args.claims)) if args.claims else []
    groups = rank_groups(data, operands, families=args.family, claims=claims,
                         no_float=args.no_float, limit=args.limit or None)
    if args.out:
        rdna2_plan.write_json(Path(args.out), groups)
    if args.compact or args.out:
        for group in groups:
            print(f"{group['key']} count={group['count']} cost={group['cost']} "
                  f"score={group['score']:g} float={str(group['float']).lower()} "
                  f"blocked_by_claims={group['blocked_by_claims']}")
    else:
        print(json.dumps(groups, indent=2, sort_keys=True))
    return 0

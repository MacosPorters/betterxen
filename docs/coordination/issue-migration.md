# Legacy Issue Migration Inventory

The agent task queue parser (`scripts/agent_queue/tasks.py`) reads GitHub issue-form fields from
level-3 Markdown headings (`### Area`, `### Priority`, `### Files`, `### Acceptance command`,
`### Depends on`, `### Interface`). Earlier backlog issues in Xenora use level-2 headings (`## Area`,
`## Files`, `## Acceptance command`, `## Depends on`, `## Objective and acceptance criteria`), so the
parser sees empty `area`, `files`, `acceptance`, and `depends` fields until those issues are
migrated to the agent task form.

## Safety rules

1. **Dry-run by default:** `migrate-inventory` is read-only. It never edits GitHub issues and never
   adds `agent:ready` labels.
2. **Explicit maintainer review required:** Every non-compliant issue is flagged with
   `requires_maintainer_review: true` and `safe_to_apply_without_review: false`. Maintainers must
   review each proposed body before applying edits or adding `agent:ready`.
3. **Never fabricate file ownership:** Issues without an explicit `Files` section are classified as
   `incomplete` with `missing_fields: ["files"]`. `plan_apply` refuses to generate an edit or
   `agent:ready` label for any incomplete issue until a maintainer explicitly supplies owned paths.
4. **Preserve issue history:** Proposed migrated bodies map recognized legacy `##` sections to the
   canonical `###` headings and preserve any original preamble text and unrecognized `##` sections
   verbatim under `### Context`.
5. **Text is data only:** Issue text is parsed with fixed string and regular-expression rules and is
   never executed. Code fences inside `Files` sections are stripped so fence markers (such as
   `text`) are never treated as owned paths.

## Running the dry-run inventory

Gather open issues into a local state snapshot, then inspect the inventory as JSON or a review table:

```sh
PYTHONPATH=scripts python3 -m agent_queue gather --repo ranveersoni98/Xenora --no-prs > state.json
PYTHONPATH=scripts python3 -m agent_queue migrate-inventory --state state.json --text
PYTHONPATH=scripts python3 -m agent_queue migrate-inventory --state state.json > migration-inventory.json
```

Each issue in the inventory is classified into one of three states:

- `compliant` — already uses `###` headings with a valid `Area`, explicit `Files`, and a non-empty
  `Acceptance command`.
- `legacy_migratable` — uses `##` or mixed `##`/`###` headings and already declares explicit `Area`,
  `Files`, and `Acceptance command`; includes a non-destructive `proposed_body` for maintainer review.
- `incomplete` — missing one or more required fields (`area`, `files`, or `acceptance command`);
  blocked from automated edits and `agent:ready` labeling until completed by a maintainer.

## Tests

```sh
PYTHONPATH=scripts python3 -m unittest discover -s tests/agent_queue -v
```

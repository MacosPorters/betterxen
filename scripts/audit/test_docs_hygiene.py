#!/usr/bin/env python3
"""Unit tests for scripts/audit/docs_hygiene.py anchor validation.

Run with:
    python3 -m unittest scripts/audit/test_docs_hygiene.py
"""
import importlib.util
import sys
import unittest
from pathlib import Path

_MODULE_PATH = Path(__file__).resolve().parent / "docs_hygiene.py"
_spec = importlib.util.spec_from_file_location("docs_hygiene", _MODULE_PATH)
docs_hygiene = importlib.util.module_from_spec(_spec)
sys.modules["docs_hygiene"] = docs_hygiene
_spec.loader.exec_module(docs_hygiene)


class SlugTests(unittest.TestCase):
    def test_basic_slug(self):
        slugs = docs_hygiene.markdown_slugs("# Hello World\n")
        self.assertIn("hello-world", slugs)

    def test_strips_punctuation(self):
        slugs = docs_hygiene.markdown_slugs("## What's new? (2024)\n")
        self.assertIn("whats-new-2024", slugs)

    def test_keeps_underscore_and_hyphen(self):
        slugs = docs_hygiene.markdown_slugs("### foo_bar-baz\n")
        self.assertIn("foo_bar-baz", slugs)

    def test_duplicate_headings(self):
        body = "# Setup\n\n# Setup\n\n# Setup\n"
        slugs = docs_hygiene.markdown_slugs(body)
        self.assertIn("setup", slugs)
        self.assertIn("setup-1", slugs)
        self.assertIn("setup-2", slugs)

    def test_fenced_code_block_ignored(self):
        body = "```\n# Not A Heading\n```\n\n# Real Heading\n"
        slugs = docs_hygiene.markdown_slugs(body)
        self.assertIn("real-heading", slugs)
        self.assertNotIn("not-a-heading", slugs)

    def test_tilde_fence_ignored(self):
        body = "~~~\n# Not A Heading\n~~~\n"
        slugs = docs_hygiene.markdown_slugs(body)
        self.assertNotIn("not-a-heading", slugs)

    def test_atx_levels(self):
        body = "# One\n## Two\n### Three\n#### Four\n##### Five\n###### Six\n"
        slugs = docs_hygiene.markdown_slugs(body)
        for s in ("one", "two", "three", "four", "five", "six"):
            self.assertIn(s, slugs)


class CheckLinksTests(unittest.TestCase):
    """Exercise check_links with a synthetic in-memory file set."""

    def _run(self, files):
        """files: dict mapping repo-relative path -> Markdown body."""
        problems = []
        original_text = docs_hygiene.text

        def fake_text(path):
            return files.get(path)

        docs_hygiene.text = fake_text
        try:
            docs_hygiene.check_links(list(files), problems)
        finally:
            docs_hygiene.text = original_text
        return problems

    def test_existing_heading_passes(self):
        problems = self._run({
            "docs/a.md": "# Hello World\n\n[link](b.md#hello-world)\n",
            "docs/b.md": "# Hello World\n",
        })
        self.assertEqual(problems, [])

    def test_in_page_anchor_passes(self):
        problems = self._run({
            "docs/a.md": "# Hello World\n\n[link](#hello-world)\n",
        })
        self.assertEqual(problems, [])

    def test_nonexistent_heading_fails(self):
        problems = self._run({
            "docs/a.md": "[link](b.md#no-such-heading)\n",
            "docs/b.md": "# Hello World\n",
        })
        self.assertEqual(len(problems), 1)
        self.assertIn("#no-such-heading", problems[0])
        self.assertIn("heading not found", problems[0])

    def test_in_page_nonexistent_heading_fails(self):
        problems = self._run({
            "docs/a.md": "# Real\n\n[link](#this-heading-does-not-exist)\n",
        })
        self.assertEqual(len(problems), 1)
        self.assertIn("#this-heading-does-not-exist", problems[0])

    def test_duplicate_heading_addressable(self):
        problems = self._run({
            "docs/a.md": "[link](b.md#setup-1)\n",
            "docs/b.md": "# Setup\n\n# Setup\n",
        })
        self.assertEqual(problems, [])

    def test_fenced_heading_not_a_slug(self):
        problems = self._run({
            "docs/a.md": "[link](b.md#not-a-heading)\n",
            "docs/b.md": "```\n# Not A Heading\n```\n",
        })
        self.assertEqual(len(problems), 1)
        self.assertIn("#not-a-heading", problems[0])

    def test_external_url_with_fragment_not_checked(self):
        problems = self._run({
            "docs/a.md": "[link](https://example.com/page#nonexistent)\n",
        })
        self.assertEqual(problems, [])

    def test_mailto_not_checked(self):
        problems = self._run({
            "docs/a.md": "[link](mailto:foo@example.com#frag)\n",
        })
        self.assertEqual(problems, [])

    def test_protocol_relative_not_checked(self):
        problems = self._run({
            "docs/a.md": "[link](//example.com/page#frag)\n",
        })
        self.assertEqual(problems, [])

    def test_bare_hash_skipped(self):
        problems = self._run({
            "docs/a.md": "[link](#)\n",
        })
        self.assertEqual(problems, [])

    def test_missing_file_still_reported(self):
        problems = self._run({
            "docs/a.md": "[link](nonexistent.md#foo)\n",
        })
        self.assertEqual(len(problems), 1)
        self.assertIn("broken relative link", problems[0])

    def test_fragment_on_non_markdown_not_checked(self):
        problems = self._run({
            "docs/a.md": "[link](image.png#foo)\n",
            "docs/image.png": None,
        })
        # image.png is binary (text returns None) but it exists in the
        # file list, so the link itself resolves; fragment is not checked.
        self.assertEqual(problems, [])


if __name__ == "__main__":
    unittest.main()

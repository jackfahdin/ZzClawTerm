#!/usr/bin/env python3
"""Guard the docs-site invariants that a Docusaurus build does not catch.

The site builds fine with a page that exists in only one locale, with a
Chinese page whose English mirror has lost half its sections, and with a doc
that no sidebar references. This script fails on all three, plus on authoring
notes that were never meant to ship.

Run from the repository root:

    python3 scripts/ci/check_docs_translations.py
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
SITE = REPO_ROOT / "docs-site"
ZH_ROOT = SITE / "docs"
EN_ROOT = SITE / "i18n/en/docusaurus-plugin-content-docs/current"
SIDEBARS = SITE / "sidebars.ts"

# Authoring scaffolding that must not reach a published page. Each entry is a
# (pattern, explanation) pair; the explanation is what the failure prints.
FORBIDDEN = [
    (
        re.compile(r":::tip\s+Screenshot suggestion", re.IGNORECASE),
        "internal screenshot-planning note",
    ),
    (
        re.compile(r"/img/docs/"),
        "reference to the /img/docs/ tree, which does not exist in static/",
    ),
    (
        re.compile(r"scripts/demo-[\w-]+\.sh"),
        "reference to a scripts/demo-*.sh helper, which does not exist",
    ),
    (
        re.compile(r"^\s*(TODO|FIXME|XXX)\b", re.MULTILINE),
        "unresolved TODO/FIXME marker",
    ),
]

HEADING = re.compile(r"^#{1,6}\s", re.MULTILINE)
FENCE = re.compile(r"^```.*?^```", re.MULTILINE | re.DOTALL)


def headings(text: str) -> list[str]:
    """Count headings outside fenced code blocks, so a `# comment` line in a
    shell example is not mistaken for a section."""
    without_code = FENCE.sub("", text)
    return [line for line in without_code.splitlines() if HEADING.match(line + "\n")]


def relative_docs(root: Path) -> set[str]:
    return {p.relative_to(root).as_posix() for p in root.rglob("*.md")}


def main() -> int:
    failures: list[str] = []

    zh_docs = relative_docs(ZH_ROOT)
    en_docs = relative_docs(EN_ROOT)

    for missing in sorted(zh_docs - en_docs):
        failures.append(f"{missing}: has no English mirror under {EN_ROOT.relative_to(REPO_ROOT)}")
    for orphan in sorted(en_docs - zh_docs):
        failures.append(f"{orphan}: English page has no Chinese source under {ZH_ROOT.relative_to(REPO_ROOT)}")

    # Heading counts are a cheap proxy for "the translation lost whole
    # sections". It does not verify wording, only that neither locale silently
    # dropped or gained a section relative to the other.
    for name in sorted(zh_docs & en_docs):
        zh_count = len(headings((ZH_ROOT / name).read_text(encoding="utf-8")))
        en_count = len(headings((EN_ROOT / name).read_text(encoding="utf-8")))
        if zh_count != en_count:
            failures.append(
                f"{name}: heading count differs (zh={zh_count}, en={en_count}); "
                "a section is missing from one locale"
            )

    for root, label in ((ZH_ROOT, "zh-CN"), (EN_ROOT, "en")):
        for path in sorted(root.rglob("*.md")):
            text = path.read_text(encoding="utf-8")
            for pattern, explanation in FORBIDDEN:
                if pattern.search(text):
                    failures.append(f"{label} {path.relative_to(root)}: {explanation}")

    # Every doc must be reachable from a sidebar; an unreferenced page builds
    # successfully but is only findable through search.
    sidebar_text = SIDEBARS.read_text(encoding="utf-8")
    referenced = set(re.findall(r"'([\w/-]+)'", sidebar_text))
    for name in sorted(zh_docs):
        doc_id = name[: -len(".md")]
        if doc_id not in referenced:
            failures.append(f"{name}: not referenced in sidebars.ts")

    if failures:
        print(f"docs-site checks failed ({len(failures)}):", file=sys.stderr)
        for failure in failures:
            print(f"  - {failure}", file=sys.stderr)
        return 1

    print(f"docs-site ok: {len(zh_docs)} pages, both locales in step")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

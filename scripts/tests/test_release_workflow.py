"""Evaluate workflow job gates without dispatching publishing operations."""

from __future__ import annotations

import re
from pathlib import Path


WORKFLOW = Path(__file__).resolve().parents[2] / ".github/workflows/release.yml"


def jobs() -> dict[str, str]:
    parts = re.split(r"^  ([a-z_][a-z_0-9]*):\s*$", WORKFLOW.read_text(encoding="utf-8"), flags=re.M)
    return dict(zip(parts[1::2], parts[2::2]))


def gate(job: str, values: dict[str, object]) -> bool:
    condition = re.search(r"^    if: (.+(?:\n      [^\n]+)*)", job, flags=re.M).group(1)
    condition = condition.removeprefix(">-").strip().replace("\n", " ")
    condition = condition.replace("always()", "True")
    for name, value in sorted(values.items(), key=lambda item: -len(item[0])):
        condition = condition.replace(name, repr(value))
    condition = re.sub(r"\btrue\b(?=(?:[^']*'[^']*')*[^']*$)", "True", condition)
    condition = condition.replace("&&", " and ").replace("||", " or ")
    if re.search(r"[^\w\s'().=!-]", condition):
        raise ValueError(f"unexpected gate expression: {condition}")
    return bool(eval(condition, {"__builtins__": {}}, {}))

"""Which recipes a `just` recipe reaches, read from just's own dump.

Extracted from `check-prebuilt-artifact-producers.py` when a second gate
(`check-workflow-pin-baseline.py`, issue 1959) needed the same answer. Two
spellings of a closure drift apart in the direction that reads green: a walk
that misses an edge reports a recipe as NOT reaching something it does.

The walk follows both kinds of edge just has:

  * dependencies (`fast: a b`), resolved inside the recipe's own module;
  * `just <recipe>` calls in a recipe BODY, including `just <module> <recipe>`
    and `just <module>::<recipe>`. A call through a variable module
    (`just "$platform" build-fixtures`) counts as that recipe in EVERY module
    that has it, which over-approximates on purpose.

It does not evaluate arguments. `just build linux` and `just build tier1` are
the same recipe here, so a closure is an upper bound on what an invocation can
reach, never a lower one.
"""

from __future__ import annotations

import json
import re
import subprocess
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent.parent

JUST_CALL = re.compile(
    r"""(?:^|[;&|(\s])just\s+(?:--?[A-Za-z][\w-]*(?:[ =]\S+)?\s+)*"""
    r"""("[^"]*"|'[^']*'|\S+)(?:\s+("[^"]*"|'[^']*'|\S+))?"""
)


def dump(root: Path | None = None) -> dict:
    """`just --dump --dump-format json` from the repo root."""
    out = subprocess.run(
        ["just", "--dump", "--dump-format", "json"],
        cwd=root or REPO,
        capture_output=True,
        text=True,
    )
    if out.returncode != 0:
        raise SystemExit(f"just --dump failed:\n{out.stderr}")
    return json.loads(out.stdout)


def _line_text(frags) -> str:
    out = []
    for f in frags:
        out.append(f if isinstance(f, str) else "{{x}}")
    return "".join(out)


# A body line that PRINTS rather than runs: a comment, a help-text line inside a
# multi-line `printf` (each continuation starts with a quote), or an `echo` /
# `printf` whose argument names a recipe. `setup`'s usage text says
# `just build zephyr`, and reading that as a call put the whole fixture build in
# `setup`'s closure.
_TEXT_LINE = re.compile(r"""^\s*(?:#|["']|@?(?:echo|printf)\b)""")


def _is_text_line(text: str) -> bool:
    return bool(_TEXT_LINE.match(text))


def recipe_table(dump) -> dict[str, dict]:
    """`mod::recipe` (or `recipe` at the root) -> recipe json."""
    table = {}

    def walk(node, prefix):
        for name, r in node.get("recipes", {}).items():
            table[prefix + name] = r
        for mname, m in node.get("modules", {}).items():
            walk(m, prefix + mname + "::")

    walk(dump, "")
    return table


def resolve_call(a: str, b: str, prefix: str, table, modules) -> list[str]:
    """The recipe(s) one `just A [B]` call names, from inside module `prefix`."""
    a = a.strip("\"'")
    b = (b or "").strip("\"'")
    if a.startswith("$") or a.startswith("{{"):
        # A variable module: that recipe, in every module with it.
        return [f"{mod}::{b}" for mod in modules]
    if "::" in a:
        return [a]
    if a in modules and b:
        return [f"{a}::{b}"]
    return [prefix + a if prefix and prefix + a in table else a]


def closure(dump, root: str) -> set[str]:
    """Every recipe `root` reaches, `root` included. Empty if `root` is unknown."""
    table = recipe_table(dump)
    modules = set(dump.get("modules", {}))
    seen: set[str] = set()
    todo = [root]
    while todo:
        q = todo.pop()
        if q in seen or q not in table:
            continue
        seen.add(q)
        r = table[q]
        prefix = q.rsplit("::", 1)[0] + "::" if "::" in q else ""
        for d in r.get("dependencies", []):
            dn = d["recipe"]
            todo.append(dn if "::" in dn or not prefix else prefix + dn)
        for frags in r.get("body", []):
            text = _line_text(frags)
            if _is_text_line(text):
                continue
            for m in JUST_CALL.finditer(text):
                todo.extend(resolve_call(m.group(1), m.group(2), prefix, table, modules))
    return seen

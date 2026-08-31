#!/usr/bin/env python3
"""A gate registry holds ONE name per line, sorted.

`just/check.just` names every gate twice: once as a recipe, and once as a
dependency of the lane that runs it. That dependency list is the shape issues
0883/0884 are about — a file EVERY pull request appends to — and it had the
worse variant of it: up to fifteen names packed onto a line, with new gates
appended at the tail. Two agents adding unrelated gates edited the same
physical line, so git conflicted on changes that do not overlap in meaning, and
the merge queue ejected them.

0884 fixed the same shape for `docs/issues/open.md` with `merge=union`, and that
IS working (measured: it conflicts in none of the open PRs). Union is not
available here — `.gitattributes` says so in as many words, "NEVER apply this to
an authored file" — because a union of two recipe bodies is not a recipe, it is
both of them.

One name per line, sorted, is the fix that does not need a merge driver: two
gates with different names land at different, usually non-adjacent, lines, and
git merges them without help. It costs nothing, because ORDER IN THIS LIST HAS
NEVER BEEN LOAD-BEARING: `fast` runs the same gates concurrently, and
`run-gates-parallel.sh` `sort -u`s the list it derives from this very block. A
list whose consumer sorts it cannot have been ordered on purpose.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
JUSTFILE = REPO / "just" / "check.just"

# The lane recipes whose dependencies are a gate REGISTRY. A lane with a
# handful of names on one line (`default: cli-fresh fast build api-parity`) is
# not one: nobody appends to it, so it is not a conflict site.
REGISTRIES = ("fast-serial", "build")


def dep_block(lines: list[str], name: str) -> tuple[int, int]:
    """(start, end) of `name:`'s dependency lines, end EXCLUSIVE.

    A recipe's dependencies run to the first line that does not end in a
    backslash — that last line is a dependency too, not the body.
    """
    for i, line in enumerate(lines):
        if line.startswith(f"{name}:"):
            j = i
            while lines[j].rstrip().endswith("\\"):
                j += 1
                if j >= len(lines):
                    raise SystemExit(f"{name}: unterminated continuation")
            return i, j + 1
    raise SystemExit(f"check-gate-lists: no recipe `{name}` in {JUSTFILE}")


def names_per_line(lines: list[str], s: int, e: int) -> list[list[str]]:
    """The dependency names, grouped by the line they sit on."""
    out = []
    for k in range(s, e):
        text = lines[k].strip().rstrip("\\").strip()
        if k == s:
            text = text.split(":", 1)[1]
        out.append([n for n in text.split() if n])
    return out


def check(lines: list[str], name: str) -> list[str]:
    s, e = dep_block(lines, name)
    per_line = names_per_line(lines, s, e)
    flat = [n for group in per_line for n in group]
    problems = []

    crowded = sum(1 for group in per_line if len(group) > 1)
    if crowded:
        problems.append(
            f"  {name}: {crowded} line(s) carry more than one gate.\n"
            f"      Two agents adding unrelated gates then edit the SAME line and\n"
            f"      git conflicts on changes that do not overlap in meaning."
        )

    if flat != sorted(flat):
        first = next(
            (a for a, b in zip(flat, sorted(flat)) if a != b), "?"
        )
        problems.append(
            f"  {name}: not sorted (first out of order: `{first}`).\n"
            f"      Sorted order is what makes a new gate's line position\n"
            f"      predictable, and so unlikely to abut someone else's."
        )

    dupes = sorted({n for n in flat if flat.count(n) > 1})
    if dupes:
        problems.append(f"  {name}: listed twice: {', '.join(dupes)}")

    return problems


def self_test() -> None:
    """Both directions, on synthetic input.

    `check-gate-selftests` requires this on the normal path: a control nobody
    runs decays into a comment.
    """
    good = ["build: \\", "    alpha \\", "    beta \\", "    gamma", "    @echo hi"]
    assert check(good, "build") == [], "selftest: rejected a well-formed list"

    crowded = ["build: \\", "    alpha beta \\", "    gamma", "    @echo hi"]
    assert any("more than one gate" in p for p in check(crowded, "build")), (
        "selftest: missed two names sharing a line"
    )

    unsorted = ["build: \\", "    gamma \\", "    alpha", "    @echo hi"]
    assert any("not sorted" in p for p in check(unsorted, "build")), (
        "selftest: missed an out-of-order list"
    )

    dup = ["build: \\", "    alpha \\", "    alpha", "    @echo hi"]
    assert any("listed twice" in p for p in check(dup, "build")), (
        "selftest: missed a duplicate"
    )

    # The block must end at the first line with no continuation, so a BODY line
    # that happens to look like a name is not read as a dependency.
    body = ["build: \\", "    alpha", "    zzz-not-a-dep", "    @echo hi"]
    s, e = dep_block(body, "build")
    assert (s, e) == (0, 2), f"selftest: block boundary wrong: {(s, e)}"


def main() -> int:
    lines = JUSTFILE.read_text(encoding="utf-8").split("\n")
    problems = [p for name in REGISTRIES for p in check(lines, name)]
    if problems:
        print("check-gate-lists: a gate registry is a conflict site again\n")
        print("\n".join(problems))
        print(
            "\nOne gate per line, sorted. `just/check.just` is the file every PR\n"
            "that adds a gate must touch, and packing names onto shared lines\n"
            "turns that into a merge conflict for changes that do not overlap\n"
            "(issues 0883/0884, same shape, no union merge available here --\n"
            "a union of two authored recipe bodies is both of them, not one)."
        )
        return 1

    total = sum(
        len([n for g in names_per_line(lines, *dep_block(lines, name)) for n in g])
        for name in REGISTRIES
    )
    print(
        f"check-gate-lists OK — {len(REGISTRIES)} registry(ies), {total} gate(s), "
        "one per line and sorted."
    )
    return 0


if __name__ == "__main__":
    self_test()
    sys.exit(main())

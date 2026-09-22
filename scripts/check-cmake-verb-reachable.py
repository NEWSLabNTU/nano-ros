#!/usr/bin/env python3
"""A cmake module defining a public `nano_ros_*` verb must be reachable.

Phase-451's first acceptance criterion, which had no check. Issue 1451.

Issue 1218 was `packages/api/nros-c/cmake/NanoRosLink.cmake`: it defined the
public verb `nano_ros_link_rmw`, nothing included it, and it **misled two of
four independent readers in one session** because it held a fourth closed RMW
list and a force-link that does not happen. It could not even have worked — the
`find_package` targets it named were deleted years apart.

phase-451 W1 deleted that file and stated the rule as an acceptance criterion:

    grep-reachable: no cmake module defining a public `nano_ros_*` verb is
    unreachable from any `include()`.

Nothing enforced it. The phase's own W1 note says why that matters: the same
dead file was found and deleted TWICE on one day by two sessions looking for
different things, because "nothing in this tree asks whether a declaration is
REACHABLE".

## Why a gate and not a grep — measured the hard way

Asked by hand on 2026-09-22, this reported `cmake/NanoRosProviders.cmake` as a
second dead module. It is not. It is included by
`packages/testing/nros-tests/tests/provider_index_gate.sh`, which calls
`nano_ros_load_providers()` directly — a real consumer the hand check missed
because its output was piped through `head -6` and the first six matches were
all the module's own lines. The truncation was read as the answer.

So this searches EVERY tracked file, not just `*.cmake` and `CMakeLists.txt`,
and counts a test's include as reachability. A module exercised only by its own
test is reachable: whether that is enough reason to keep it is a judgement for
a reader, and this gate exists to make sure a reader is looking at a true list.

Usage: check-cmake-verb-reachable.py [--self-test] [--list]
"""

from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
# A PUBLIC verb: `nano_ros_*`, no leading underscore. `_nros_*` / `_nano_ros_*`
# are internal by this tree's convention and carry no promise to a reader.
VERB_RE = re.compile(r"^\s*(?:function|macro)\(\s*(nano_ros_[a-z0-9_]+)", re.M)


def tracked(pattern: str) -> list[str]:
    out = subprocess.run(
        ["git", "ls-files", pattern], cwd=ROOT, capture_output=True, text=True, check=True
    )
    return [l for l in out.stdout.splitlines() if l and not l.startswith("third-party/")]


def modules_with_verbs() -> dict[str, set[str]]:
    found: dict[str, set[str]] = {}
    for rel in tracked("*.cmake"):
        try:
            text = (ROOT / rel).read_text(errors="replace")
        except OSError:
            continue
        verbs = set(VERB_RE.findall(text))
        if verbs:
            found[rel] = verbs
    return found


def referenced_from_elsewhere(rel: str, verbs: set[str]) -> list[str]:
    """Tracked files OTHER than `rel` naming the module or one of its verbs."""
    base = Path(rel).name
    needles = [base] + sorted(verbs)
    hits: set[str] = set()
    for needle in needles:
        r = subprocess.run(
            ["git", "grep", "-l", "-F", needle, "--", ".", ":(exclude)third-party"],
            cwd=ROOT,
            capture_output=True,
            text=True,
        )
        for line in r.stdout.splitlines():
            if line and line != rel:
                hits.add(line)
    return sorted(hits)


def run(list_only: bool) -> int:
    mods = modules_with_verbs()
    if not mods:
        print(
            "check-cmake-verb-reachable: found NO cmake module defining a public "
            "`nano_ros_*` verb.\n"
            "  That is not a pass — this tree has many. Discovery broke.",
            file=sys.stderr,
        )
        return 1

    dead: list[tuple[str, set[str]]] = []
    for rel, verbs in sorted(mods.items()):
        refs = referenced_from_elsewhere(rel, verbs)
        if list_only:
            print(f"  {rel}: {len(refs)} referrer(s) — verbs {sorted(verbs)}")
        if not refs:
            dead.append((rel, verbs))

    if list_only:
        return 0

    if dead:
        print("check-cmake-verb-reachable: FAILED (phase-451 / issue 1451)", file=sys.stderr)
        for rel, verbs in dead:
            print(
                f"  - {rel} defines {sorted(verbs)} and NOTHING tracked references it.\n"
                f"      A public verb nobody can reach still reads as authoritative, which is\n"
                f"      what issue 1218 cost two of four readers in one session. Delete it, or\n"
                f"      include it from whatever should have been using it.",
                file=sys.stderr,
            )
        return 1

    print(
        f"check-cmake-verb-reachable: OK — {len(mods)} module(s) define a public "
        f"`nano_ros_*` verb, every one referenced from somewhere else in the tree."
    )
    return 0


def self_test() -> bool:
    ok = True

    def chk(label: str, cond: bool, detail: str = "") -> None:
        nonlocal ok
        print(f"  {'ok ' if cond else 'FAIL'} {label}" + ("" if cond else f" — {detail}"))
        if not cond:
            ok = False

    chk(
        "a public verb is detected",
        VERB_RE.findall("function(nano_ros_link_rmw target)") == ["nano_ros_link_rmw"],
    )
    chk(
        "a macro form is detected",
        VERB_RE.findall("macro(nano_ros_thing)") == ["nano_ros_thing"],
    )
    chk(
        "an INTERNAL verb is not a public one",
        VERB_RE.findall("function(_nano_ros_watch_provider_inputs index)") == [],
    )
    chk(
        "a call site is not mistaken for a definition",
        VERB_RE.findall("    nano_ros_link_rmw(app)") == [],
    )
    # The live tree must contain at least one module with a public verb, or the
    # discovery half is broken and every run would pass over nothing.
    chk("discovery finds modules in this tree", len(modules_with_verbs()) > 0)
    return ok


if __name__ == "__main__":
    if "--self-test" in sys.argv:
        sys.exit(0 if self_test() else 1)
    if not self_test():
        print("check-cmake-verb-reachable: SELF-TEST FAILED", file=sys.stderr)
        sys.exit(1)
    sys.exit(run("--list" in sys.argv))

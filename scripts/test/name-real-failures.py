#!/usr/bin/env python3
"""Print the ids of the REAL (non-``[SKIPPED]``) failures in a JUnit XML.

Issue 0527. ``_count-real-failures`` tells you *how many* real failures a sweep
had; until now nothing told you *which*, and the XML that knew was routinely
gone by the time anyone looked — any later ``cargo nextest`` invocation writes
the same ``target/nextest/default/junit.xml``, including the ones you run while
triaging. A count without names sends you back to re-running suites one at a
time to rediscover what the sweep already knew.

So this prints them, and ``_rewrite-skipped-junit`` snapshots the rewritten file
to ``junit-real.xml`` — a path no nextest run writes — so the names survive both
the doctest phase and the triage commands that follow it.

A ``<failure>`` is a real failure unless its message or body starts with
``[SKIPPED]`` (the ``nros_tests::skip!`` marker). Normally the rewrite has
already converted those to ``<skipped>``, so this is belt-and-braces for a junit
that was never rewritten — the same defence-in-depth ``_count-real-failures``
keeps, and for the same reason: the two must not disagree about what "real"
means.

Usage::

    name-real-failures.py [junit.xml] [--limit N]

Exit status is 0 whether or not failures were found: this is a reporting aid on
an error path that has already decided to fail, and it must never turn a
diagnosable failure into a confusing one of its own.
"""

import re
import sys

import skip_marker
import xml.etree.ElementTree as ET
from pathlib import Path

# Matches both the bare marker and a classed one (`[SKIPPED:lane]`, issue 0584).
# Anchored at the start because a real failure may legitimately mention the word.
SKIP_RE = re.compile(r"^\[SKIPPED(?::[a-z_]+)?\]")


def is_skip(node: ET.Element, case: ET.Element | None = None) -> bool:
    """True when this failure is a `skip!` marker rather than a real failure.

    The marker does not always reach the `<failure>` payload — for some harness
    invocations it lands only in the sibling `<system-err>`, which cost tier 1 a
    permanent red on a test that was skipping (see `skip_marker`). `case` is the
    owning `<testcase>`; without it only the payload forms are checked.
    """
    streams = skip_marker.testcase_streams(case) if case is not None else ()
    return (
        skip_marker.skip_class_in(
            (node.get("message"), node.text), streams
        )
        is not None
    )


def real_failures(path: Path) -> list[str]:
    try:
        root = ET.parse(path).getroot()
    except (OSError, ET.ParseError):
        return []
    out = []
    for case in root.iter("testcase"):
        for tag in ("failure", "error"):
            node = case.find(tag)
            if node is not None and not is_skip(node, case):
                cls = case.get("classname") or "?"
                out.append(f"{cls} {case.get('name') or '?'}")
                break
    return out


def main(argv: list[str]) -> int:
    args = [a for a in argv[1:] if not a.startswith("--")]
    limit = 40
    for a in argv[1:]:
        if a.startswith("--limit="):
            limit = int(a.split("=", 1)[1])

    # Prefer the snapshot: the live junit is whatever ran most recently.
    candidates = (
        [Path(args[0])]
        if args
        else [
            Path("target/nextest/default/junit-real.xml"),
            Path("target/nextest/default/junit.xml"),
        ]
    )
    path = next((p for p in candidates if p.is_file()), None)
    if path is None:
        print("  (no junit.xml found — cannot name the failures)")
        return 0

    names = real_failures(path)
    if not names:
        # Worth saying explicitly rather than printing nothing: "the count said
        # N but the file names none" is itself the 0527 symptom, and silence
        # reads as "no output implemented".
        print(f"  (no real failures recorded in {path} — it may describe a different run)")
        return 0

    for n in names[:limit]:
        print(f"  {n}")
    if len(names) > limit:
        print(f"  … and {len(names) - limit} more (see {path})")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))

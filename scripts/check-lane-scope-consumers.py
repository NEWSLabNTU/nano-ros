#!/usr/bin/env python3
"""issue 0571 — a matrix consumer must narrow its own cell list by lane.

Consolidation (phase-329 W1) made some consumers ONE test each, generically
named, iterating every platform's cell in a single process. Nothing outside the
test can deselect a cell inside it, so on a host lane those cells boot whatever
images exist — and the cells whose images do NOT exist vanish into a green
verdict. That is issue 0571: `realtime_tiers` reported a 12-second PASS having
run 1 of its 16 rows, and a genuinely broken NuttX cell (issue 0572) sat behind
it.

The fix those consumers carry is `nros_tests::lane_scope::admits`, applied to
their cell list. This gate keeps it there, and requires it of the NEXT one.

Rule
----
A file under `packages/testing/nros-tests/tests/` that iterates `matrix::CELLS`
by PLATFORM must either select the host platform only, or NARROW its cells with
`lane_scope::admits` (a positive `filter(|c| admits(…))`, or `if !admits(…)
{ … continue | return }`).

Issue 1900 — this rule used to have a third arm: "(a) be reachable by the lane
filter — its FILE name carries a platform token, so `binary(~<token>)`
excludes it". That arm assumed `scripts/test/lane-filter.sh linux` narrowed the
run, through `NROS_TEST_SCOPE`. No runner has set that variable since phase-395
W19 moved every lane to coordinate scoping, so the arm exempted consumers on
the strength of a filter that never ran. The coordinates are the ONE mechanism
now, and a consumer is narrowed by them or not at all.

Buildless: reads sources only.
"""

import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
TESTS = os.path.join(ROOT, "packages/testing/nros-tests/tests")

# Consumers that read CELLS purely as DATA — they assert about the table, never
# boot a fixture, so no lane can narrow them and none should. Kept explicit
# rather than inferred: "does this test boot anything" is not something a regex
# should be trusted to answer.
DATA_ONLY = {
    "matrix_fixture_coverage.rs",  # G1–G4 coverage gates over the table itself
    "no_local_axis_tables.rs",  # asserts no consumer re-declares the axes
}


_PLATFORM_USE = re.compile(r"\.platform\b")
_HOST_PRED = re.compile(
    r"matches!\(\s*\w+\.platform\s*,\s*(?:[\w:]*::)?PlatformId::Linux\s*\)"
    r"|\w+\.platform\s*==\s*(?:[\w:]*::)?PlatformId::Linux\b")


def host_only(src):
    """Every `.platform` use is a predicate selecting `PlatformId::Linux`."""
    sys.path.insert(0, os.path.join(ROOT, "scripts", "lib"))
    import comments

    code = comments.strip_comments(src, "rust")
    uses = len(_PLATFORM_USE.findall(code))
    return uses > 0 and uses == len(_HOST_PRED.findall(code))


# issue 1737 — rule (b) was "the file CALLS admits", and `entry_e2e.rs`
# dropped its narrowing `filter(admits)` while keeping the out-of-lane REPORT
# (`filter(|c| !admits(…))`) and passed. A call that only COLLECTS the
# excluded cells narrows nothing. What narrows: a POSITIVE `filter(|c| admits(…))`
# on the cell list, or `if !admits(…) { … continue | return }` in the loop.
_ADMITS = r"(?:[\w:]*::)?admits\("
_NARROW_FILTER = re.compile(r"\.filter\(\s*\|[^|]*\|\s*" + _ADMITS)
_SKIP_IF = re.compile(r"\bif\s+!\s*" + _ADMITS)


def narrows(src):
    """Does this source NARROW its cell list by lane (not merely report)?"""
    sys.path.insert(0, os.path.join(ROOT, "scripts", "lib"))
    import comments
    import per_item
    code = comments.strip_comments(src, "rust")
    if _NARROW_FILTER.search(code):
        return True
    for _m, a, b in per_item.blocks(code, _SKIP_IF):
        if re.search(r"\b(?:continue|return)\b", code[a:b]):
            return True
    return False


def main():
    # Controls on the normal path (phase-472 W8): the host-only shape is exempt,
    # its neighbours — another platform, or a mix — are not.
    assert host_only("fn f(c: &Cell) -> bool { matches!(c.platform, PlatformId::Linux) }")
    assert not host_only("fn f(c: &Cell) -> bool { matches!(c.platform, PlatformId::Linux) "
                         "|| matches!(c.platform, PlatformId::Zephyr) }")
    assert not host_only("fn f(c: &Cell) -> bool { c.platform == PlatformId::Nuttx }")
    report_only = ("let skipped: Vec<_> = CELLS.iter().filter(|c| !lane_scope::admits(c.platform)).collect();\n"
                   "for c in CELLS.iter() { run(c); }\n")
    assert not narrows(report_only), "a REPORT-only admits call read as narrowing"
    assert narrows(report_only + "let run: Vec<_> = CELLS.iter().filter(|c| lane_scope::admits(c.platform)).collect();\n")
    assert narrows("for c in CELLS { if !nros_tests::lane_scope::admits(c.platform) { note(c); continue; } go(c); }\n")
    assert not narrows("for c in CELLS { if !lane_scope::admits(c.platform) { note(c); } go(c); }\n")
    assert not narrows("// .filter(|c| lane_scope::admits(c.platform))\n")

    offenders, checked, host = [], 0, 0
    for name in sorted(os.listdir(TESTS)):
        if not name.endswith(".rs") or name in DATA_ONLY:
            continue
        path = os.path.join(TESTS, name)
        with open(path, encoding="utf-8") as fh:
            src = fh.read()
        if "matrix::CELLS" not in src:
            continue
        # Only consumers that branch on the platform axis can be out of lane.
        if "c.platform" not in src and ".platform" not in src:
            continue
        checked += 1
        # A consumer whose EVERY platform predicate selects the host platform
        # iterates no out-of-lane cell, so there is nothing to narrow — keyed on
        # that exact shape (phase-472 W8), not on the file name.
        if host_only(src):
            host += 1
            continue
        if narrows(src):
            continue
        offenders.append(name)

    if offenders:
        sys.stderr.write(
            "check-lane-scope-consumers: FAILED — matrix consumer(s) no lane can narrow:\n"
        )
        for o in offenders:
            sys.stderr.write(f"  packages/testing/nros-tests/tests/{o}\n")
        sys.stderr.write(
            "\n  This test iterates matrix::CELLS across platforms in one test, so\n"
            "  nothing outside it can deselect a cell (issues 0571, 1900 — a name\n"
            "  filter narrows no run). Narrow the cell list itself:\n\n"
            "      if !nros_tests::lane_scope::admits(c.platform) { /* record + skip */ }\n\n"
            "  and REPORT what did not run — a silently absent cell is a green\n"
            "  that ran nothing (issue 0445).\n"
        )
        return 1

    print(
        f"lane-scope consumers: OK ({checked} platform-iterating consumer(s); "
        f"{host} select the host platform only, the rest narrow their own cells)"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())

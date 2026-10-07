#!/usr/bin/env python3
"""issue 0571 — a matrix consumer the lane filter cannot reach must narrow itself.

`scripts/test/lane-filter.sh native` scopes a tier-1 run by EXCLUDING names: a
test binary whose name carries a platform family token (`freertos_qemu`,
`zephyr_cortex_m_qemu`, …) and a test whose own name carries one
(`case_05_zephyr_rust`, `Platform__Freertos`). Issue 0357 added the second half
after the first proved insufficient.

Consolidation (phase-329 W1) defeats BOTH halves for four consumers: they are
ONE test each, generically named, iterating every platform's cell in a single
process. No name filter can reach inside a test, so on a tier-1 host those
cells boot whatever images exist — and the cells whose images do NOT exist
vanish into a green verdict. That is issue 0571: `realtime_tiers` reported a
12-second PASS having run 1 of its 16 rows, and a genuinely broken NuttX cell
(issue 0572) sat behind it.

The fix those consumers carry is `nros_tests::lane_scope::admits`, applied to
their cell list. This gate keeps it there, and requires it of the NEXT one.

Rule
----
A file under `packages/testing/nros-tests/tests/` that iterates `matrix::CELLS`
by PLATFORM must either

  (a) be reachable by the lane filter — its FILE name contains a platform
      family token, so `binary(~<token>)` excludes it; or
  (b) call `lane_scope::admits`.

Buildless: reads sources, plus `PlatformId::just_module` in matrix.rs for the
token list — the same derivation `lane-filter.sh` uses, so a new platform
extends both with no third edit.
"""

import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
TESTS = os.path.join(ROOT, "packages/testing/nros-tests/tests")
MATRIX = os.path.join(ROOT, "packages/testing/nros-tests/src/matrix.rs")

# Consumers that read CELLS purely as DATA — they assert about the table, never
# boot a fixture, so no lane can narrow them and none should. Kept explicit
# rather than inferred: "does this test boot anything" is not something a regex
# should be trusted to answer.
DATA_ONLY = {
    "matrix_fixture_coverage.rs",  # G1–G4 coverage gates over the table itself
    "no_local_axis_tables.rs",  # asserts no consumer re-declares the axes
}


def platform_tokens():
    """Family tokens from `PlatformId::just_module`, as lane-filter.sh derives them."""
    with open(MATRIX, encoding="utf-8") as fh:
        src = fh.read()
    body = re.search(
        r"pub const fn just_module\(self\) -> &'static str \{(.*?)\n    \}",
        src,
        re.S,
    )
    if not body:
        sys.exit("check-lane-scope-consumers: cannot find PlatformId::just_module")
    return {m for m in re.findall(r'"([a-z0-9_]+)"', body.group(1))}


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


def lane_excluded_tokens():
    """The binary-name tokens `lane-filter.sh native` ACTUALLY excludes.

    phase-472 W8 — rule (a) is "the lane filter excludes this binary by name",
    so its exemption must be keyed on the filter's own output, not on
    `just_module`'s tokens: those include `native`, which the host lane RUNS,
    so every `native_*` consumer was exempt from narrowing its cells while being
    exactly the binary the tier-1 lane executes.
    """
    import subprocess

    out = subprocess.run(["bash", os.path.join(ROOT, "scripts/test/lane-filter.sh"), "native"],
                         capture_output=True, text=True, check=True).stdout
    return set(re.findall(r"not binary\(~([a-z0-9_]+)\)", out))


def main():
    tokens = lane_excluded_tokens()
    if not tokens:
        sys.exit("check-lane-scope-consumers: lane-filter.sh native excluded no binary token")
    # Controls on the normal path (phase-472 W8): the host-only shape is exempt,
    # its neighbours — another platform, or a mix — are not.
    assert host_only("fn f(c: &Cell) -> bool { matches!(c.platform, PlatformId::Linux) }")
    assert not host_only("fn f(c: &Cell) -> bool { matches!(c.platform, PlatformId::Linux) "
                         "|| matches!(c.platform, PlatformId::Zephyr) }")
    assert not host_only("fn f(c: &Cell) -> bool { c.platform == PlatformId::Nuttx }")
    # The neighbour the name exemption must not cover: the host lane's own
    # family is run, never excluded.
    report_only = ("let skipped: Vec<_> = CELLS.iter().filter(|c| !lane_scope::admits(c.platform)).collect();\n"
                   "for c in CELLS.iter() { run(c); }\n")
    assert not narrows(report_only), "a REPORT-only admits call read as narrowing"
    assert narrows(report_only + "let run: Vec<_> = CELLS.iter().filter(|c| lane_scope::admits(c.platform)).collect();\n")
    assert narrows("for c in CELLS { if !nros_tests::lane_scope::admits(c.platform) { note(c); continue; } go(c); }\n")
    assert not narrows("for c in CELLS { if !lane_scope::admits(c.platform) { note(c); } go(c); }\n")
    assert not narrows("// .filter(|c| lane_scope::admits(c.platform))\n")
    if "native" in tokens or "linux" in tokens:
        sys.exit("check-lane-scope-consumers: the host family is in the exclusion set — "
                 "rule (a) would exempt the binaries the host lane runs")

    offenders, checked, exempt, host = [], 0, 0, 0
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
        stem = name[:-3]
        if any(tok in stem for tok in tokens):
            exempt += 1  # (a) the lane filter excludes this binary by name
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
            "\n  This test iterates matrix::CELLS across platforms, and neither its\n"
            "  binary name nor its test name carries a platform token — so\n"
            "  `scripts/test/lane-filter.sh native` cannot exclude its embedded\n"
            "  cells (issues 0357, 0571). Narrow the cell list itself:\n\n"
            "      if !nros_tests::lane_scope::admits(c.platform) { /* record + skip */ }\n\n"
            "  and REPORT what did not run — a silently absent cell is a green\n"
            "  that ran nothing (issue 0445).\n"
        )
        return 1

    print(
        f"lane-scope consumers: OK ({checked} platform-iterating consumer(s); "
        f"{exempt} excluded by binary name, {host} select the host platform only, "
        f"the rest narrow their own cells)"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())

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


def main():
    tokens = platform_tokens()
    if not tokens:
        sys.exit("check-lane-scope-consumers: no platform tokens parsed from matrix.rs")

    offenders, checked, exempt = [], 0, 0
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

        stem = name[:-3]
        if any(tok in stem for tok in tokens):
            exempt += 1  # (a) the lane filter excludes this binary by name
            continue
        if "lane_scope::admits" in src:
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
        f"{exempt} excluded by binary name, the rest narrow their own cells)"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())

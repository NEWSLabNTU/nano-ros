#!/usr/bin/env python3
"""A fixture stamp may not claim what the build did not achieve — phase-407 W1.

THE DEFECT

`nros_fixtures_stamp_write` recorded the lane's NOMINAL coordinates, read from
the manifest, regardless of what the build produced. A module that skipped for a
missing prerequisite (`nros_lane_skip`, exit 78 — issue 0599) still appeared as
covered, so `nros_fixtures_stamp_require` answered "yes, covered", the run
proceeded, and its tests skipped for absent binaries. Green sweep, nothing run.

The skip was laundered into a coverage claim, and every consumer downstream read
the claim rather than the reality. Same shape as three defects fixed this week
one level up: `check-submodule-pins` skipping on an unresolvable baseline,
`check-feature-set-ssot` matching a spelling no site used, and `post-submit`
reporting success with its only expensive job skipped.

WHAT IS REQUIRED

  H1  the writer consults the fan-out's joblog, so the stamp records what was
      ACHIEVED — a `skipped_module=` line per module that did not build;
  H2  the reader exposes it (`nros_fixtures_stamp_skipped`);
  H3  the `have = all` early return — "a build of everything covers every
      lane" — is not reached while modules are recorded as skipped. That line
      is where the laundering happened, so it is the line that must be guarded.

Buildless: reads one shell file.
"""

import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
LANE = os.path.join(ROOT, "scripts", "build", "fixture-lane.sh")


def analyze(text):
    """[] when the honesty rules hold, else a list of problems."""
    problems = []

    if "nros_fixtures_skipped_modules" not in text:
        problems.append(
            "H1: no `nros_fixtures_skipped_modules` — the writer has no way to "
            "know what the build achieved, so the stamp can only record what it "
            "was asked for.")
    if "skipped_module=" not in text:
        problems.append(
            "H1: the stamp never writes a `skipped_module=` line, so a skipped "
            "module is indistinguishable from a built one.")
    if "nros_fixtures_stamp_skipped" not in text:
        problems.append(
            "H2: no reader for `skipped_module=`; a fact nothing can read is "
            "not a fact the gate can act on.")

    # H3 — the guard must come BEFORE the `have = all` early return, not after.
    # After it, `lane=all` (the common case, and the one that laundered) returns
    # 0 without ever consulting the skip list.
    guard = text.find("nros_fixtures_stamp_skipped 2>/dev/null")
    early = text.find('if [ "$have" = "all" ]; then')
    if guard == -1 or early == -1:
        problems.append(
            "H3: could not locate both the skip guard and the `have = all` "
            "early return in nros_fixtures_stamp_require — the check cannot "
            "confirm their order, which is the whole invariant.")
    elif guard > early:
        problems.append(
            "H3: the skip guard sits AFTER the `have = all` early return, so a "
            "full-lane build returns `covered` without consulting it — exactly "
            "the laundering this exists to stop.")
    return problems


def selftest():
    """Exercise both verdicts on every run — phase-395."""
    good = ('nros_fixtures_skipped_modules() { :; }\n'
            'nros_fixtures_stamp_skipped() { :; }\n'
            'echo "skipped_module=$m"\n'
            'stamp_skipped="$(nros_fixtures_stamp_skipped 2>/dev/null)"\n'
            'if [ "$have" = "all" ]; then\n')
    assert not analyze(good), f"a correct file must pass: {analyze(good)}"

    # The ORDER is the invariant, so swapping it must be caught — a file with
    # every required symbol present and the guard one line too late.
    swapped = ('nros_fixtures_skipped_modules() { :; }\n'
               'nros_fixtures_stamp_skipped() { :; }\n'
               'echo "skipped_module=$m"\n'
               'if [ "$have" = "all" ]; then\n'
               'stamp_skipped="$(nros_fixtures_stamp_skipped 2>/dev/null)"\n')
    assert any("H3" in p for p in analyze(swapped)), \
        "a guard after the early return must be caught — that is the defect"

    assert any("H1" in p for p in analyze("nothing here\n")), \
        "a file with no achievement tracking must fail H1"


def main():
    selftest()
    with open(LANE, encoding="utf-8") as fh:
        problems = analyze(fh.read())
    if problems:
        sys.stderr.write("check-fixture-stamp-honesty: FAILED\n")
        for p in problems:
            sys.stderr.write(f"  {p}\n")
        return 1
    print("fixture stamp honesty: OK (records achieved modules; the "
          "`have = all` shortcut is guarded)")
    return 0


if __name__ == "__main__":
    sys.exit(main())

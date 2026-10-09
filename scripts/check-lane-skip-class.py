#!/usr/bin/env python3
"""An out-of-lane skip must SAY `lane`, not default to `capability`.

Issue 0584 gave skips a class: `skip_class!(lane, …)` emits `[SKIPPED:lane]`,
while plain `skip!` emits `[SKIPPED]`, which the junit rewriter reads as
`capability`. The two are counted separately in every sweep summary, and they
mean different things:

  * `lane`       — the fixture was DELIBERATELY not built for this run's lane.
                   Nothing is wrong; a broader lane would run it.
  * `capability` — this machine or this build cannot do it.

Three matrix aggregators (`entry_matrix`, `multihost`, `roundtrip_xprocess`)
skipped with the reason "every cell is out of this run's lane" through a plain
`skip!`, so a purely lane-scoped skip was counted as a missing capability. Tier
1 on 2026-08-21 reported `capability=2 lane=1` when the honest split was 1 and
2 — small, but the summary exists to be trusted, and `baremetal_run_plan_runtime`
carries a comment about getting this exact classification wrong once already.

THE RULE — a precondition panic whose message names being out of lane must be
the lane skip. Since issue 1758 the spellings are `nros_tests::unmet!` (a
FAILURE: in scope and unmet) and `nros_tests::lane_skip!` (out of scope), so an
`unmet!` that says "out of lane" is now worse than a miscount — it turns a
deselection into a red.

DELIBERATELY NARROW. Only the unambiguous phrasing is matched. Two aggregators
(`realtime_tiers`, `sched_dims_applied`) report "N skipped, M out of lane" from
a mix of BOTH kinds; no single class is right for those, and forcing one would
trade a small miscount for a confident wrong answer. They keep the
`capability` default, which is the safe side.

Run:  python3 scripts/check-lane-skip-class.py [--self-test]
"""

import os
import re
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

# Phrases that mean "out of lane" and nothing else.
LANE_PHRASES = (
    "out of this run's lane",
    "out of the run's lane",
    "not selected by this lane",
)
SKIP_CALL = re.compile(r"\bunmet!\s*\(", re.M)


def offenders(paths):
    bad = []
    for rel in paths:
        try:
            with open(os.path.join(ROOT, rel), encoding="utf8", errors="replace") as fh:
                lines = fh.read().split("\n")
        except OSError:
            continue
        for n, line in enumerate(lines):
            if not SKIP_CALL.search(line):
                continue
            window = "\n".join(lines[n : n + 6]).lower()
            if any(p in window for p in LANE_PHRASES):
                bad.append((rel, n + 1, " ".join(lines[n].split())[:70]))
    return bad


def tracked_tests():
    out = subprocess.run(
        ["git", "ls-files", "packages/*/*/tests/*.rs", "packages/*/*/*/tests/*.rs"],
        cwd=ROOT,
        capture_output=True,
        text=True,
        check=True,
    ).stdout.split()
    return [p for p in out if p.endswith(".rs")]


def self_test():
    import tempfile

    tmp = os.path.join(ROOT, "tmp")
    os.makedirs(tmp, exist_ok=True)
    with tempfile.TemporaryDirectory(dir=tmp) as d:
        probe = os.path.join(d, "p.rs")
        rel = os.path.relpath(probe, ROOT)

        def write(t):
            with open(probe, "w") as fh:
                fh.write(t)

        write('fn t(){ nros_tests::unmet!("every cell is out of this run\'s lane:\\n{}", x); }\n')
        assert offenders([rel]), "an out-of-lane `unmet!` was NOT reported"
        write('fn t(){ nros_tests::lane_skip!("every cell is out of this run\'s lane"); }\n')
        assert not offenders([rel]), "the lane spelling was reported"
        write('fn t(){ nros_tests::unmet!("qemu not installed"); }\n')
        assert not offenders([rel]), "an ordinary unmet precondition was reported"
        write('fn t(){ nros_tests::unmet!("no rows RAN (2 skipped, 3 out of lane)"); }\n')
        assert not offenders([rel]), "a MIXED-reason message must not be forced to `lane`"
    sys.stdout.write("check-lane-skip-class self-test: OK\n")


def main():
    if "--self-test" in sys.argv:
        self_test()
        return
    self_test()
    files = tracked_tests()
    bad = offenders(files)
    if bad:
        sys.stderr.write("error: %d out-of-lane skip(s) not classed `lane`:\n\n" % len(bad))
        for rel, line, snip in bad:
            sys.stderr.write(f"  {rel}:{line}\n      {snip}\n")
        sys.stderr.write(
            "\nUse `nros_tests::lane_skip!(…)`. `unmet!` is a FAILURE (issue 1758),\n"
            "so a cell the lane deliberately did not select would read as red.\n"
        )
        sys.exit(1)
    sys.stdout.write("lane-skip-class OK — %d test file(s).\n" % len(files))


if __name__ == "__main__":
    main()

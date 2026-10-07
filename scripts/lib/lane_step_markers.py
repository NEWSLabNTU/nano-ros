"""Did a `just ci <lane>` run reach its CELLS, or die in its preflight? — issue 1754.

A staged lane's cells step is ONE workflow step (`just ci matrix`,
`just ci matrix-nightly`, `just ci tier1 run`), and inside it
`scripts/ci/run-lane-steps.sh` runs several recipes in order — `check::default`
first, the cells (`test-all`, …) after. So the step's outcome cannot say which
of them failed: run 37685900447 died in `check fast`'s
`zephyr-workspace-foreign-checkout` gate 2m13s in, ran 0 cells, and the stage
reporter printed `VERDICT: cells ran and FAILED`.

The runner already prints a POSITIVE marker per inner step, and those are what
this reads — never the absence of `PASS` lines:

    ==> ci tier2 [1/4] check::default — started 21:51:44Z
    <== ci tier2 [1/4] check::default — FAILED after 2m13s (at 21:53:57Z)

One parser for every reader: `lane-stage.py --report` (the in-workflow half,
over the file `NROS_LANE_STEP_RECORD` names, which the runner appends the same
lines to), `lane-stage.py --history` and `nightly-triage.py` (over the job log).
The runner's format and this parser are one contract, and `lane-stage.py
--selftest` runs the real runner against a stub `just` to hold it.
"""

import collections
import re

# The inner recipes whose running IS the cells — the runtime verdict a staged
# lane exists to produce. Everything else a lane's `steps=(…)` names
# (`check::default`, `rust-rtos-link-check`, `check::weak-symbols-image`) is a
# gate or a build: its failure BEFORE a cell starts means no cell ran.
# AUTHORED; `lane-stage.py --selftest` checks the tier-2 lane's array in
# `just/ci.just` still starts with a non-cell and contains a cell.
CELL_STEPS = frozenset({"test-all", "test-ignored", "zephyr::tier3-cell"})

# `re.search`, not `match`: a GitHub job log prefixes every line with the job
# name, the step and a timestamp.
_START = re.compile(r"==> ci (\S+) \[(\d+)/(\d+)\] (\S+) — started")
_END = re.compile(r"<== ci (\S+) \[(\d+)/(\d+)\] (\S+) — (ok|FAILED) after")

Reached = collections.namedtuple("Reached", "cells_ran failed_step started")


def reached_cells(text):
    """Read the runner's markers out of `text`.

    `cells_ran` is
      True   a CELL step started (whatever happened after),
      False  a NON-cell step reported FAILED and no cell step started — the
             preflight stopped the lane, positively,
      None   no runner markers, or a non-cell step started and never ended
             (a killed or truncated log): nobody said, so callers keep the
             step-outcome classification.
    `failed_step` is the first inner step that reported FAILED, or "".
    """
    started, failed = [], []
    for line in (text or "").splitlines():
        m = _START.search(line)
        if m:
            started.append(m.group(4))
            continue
        m = _END.search(line)
        if m and m.group(5) == "FAILED":
            failed.append(m.group(4))
    first_failed = failed[0] if failed else ""
    if any(s in CELL_STEPS for s in started):
        return Reached(True, first_failed, started)
    if failed:
        return Reached(False, first_failed, started)
    return Reached(None, first_failed, started)

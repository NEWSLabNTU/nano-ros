---
id: 1754
title: "The tier-2 stage reporter read a failed `check fast` preflight as `VERDICT: cells ran and FAILED` with 0 cells run: a cells STEP is a whole `just ci <lane>`, and its outcome cannot say which inner step failed"
status: resolved
type: bug
severity: medium
area: [ci, testing]
related: [1158, 1500, 0952, 1387]
found: 2026-10-08
---

## Measured

`run-matrix.yml` run 37685900447, job 113013514754 (`tier 2 (1-wise matrix)`,
2026-10-07, workflow_dispatch). The stage summary read:

```
tier 2 (1-wise matrix): VERDICT: cells ran and FAILED
  first failing step: just ci matrix
```

No cell ran. The job log has 0 `PASS` lines. It does have these:

```
==> ci tier2 [1/4] check::default — started 21:51:44Z
check-zephyr-workspace-foreign-checkout: FAILED (issue 1387)
check-fast (parallel): 1 of 405 gate(s) FAILED
<== ci tier2 [1/4] check::default — FAILED after 2m13s (at 21:53:57Z)
ci tier2 FAILED at step 1 of 4 (check::default).
```

So the `check fast` preflight failed 2m13s into `just ci matrix`. The
`rust-rtos-link-check`, `test-all` and `check::weak-symbols-image` steps never
started. That is a lane failure, not a code verdict, and issue 1158 built the
reporter to tell those two apart.

## Cause

`scripts/ci/lane-stage.py` classifies by workflow STEP outcome. The workflow
hands it four steps, and `just ci matrix` maps to `cells`. That step is not the
cells, though. It is `scripts/ci/run-lane-steps.sh tier2 check::default
rust-rtos-link-check test-all check::weak-symbols-image`, so the step's outcome
reads `failure` the same way when the preflight fails as when a cell fails.

phase-441 W4 added a fourth axis, `NROS_LANE_CELLS_RAN`. Only `live-peer.yml`
supplies it, and tier 2 and tier 1 never did.

`just nightly-triage` had the same defect. `just ci matrix-nightly` matches
none of its role words and defaults to "verdict", and its only log
reclassifier looked for the jobserver-stall line.

## Fixed

The fix reads a positive marker. It never infers anything from missing `PASS`
lines.

- `run-lane-steps.sh` already prints `==> … started` / `<== … FAILED` for each
  inner step. When `NROS_LANE_STEP_RECORD` names a file, it now also appends
  those lines to that file.
- `scripts/lib/lane_step_markers.py` is the one parser.
  - `CELL_STEPS` is `test-all`, `test-ignored` and `zephyr::tier3-cell`.
  - If a cell step started, `cells_ran` is True.
  - If a non-cell step reported FAILED and no cell started, `cells_ran` is
    False.
  - If there are no markers, `cells_ran` is None: nobody said, and the old
    answer stands.
- `lane-stage.py` reads that parser in each mode:
  - `--report` reads the record file. The workflow sets it for both
    `just ci matrix` and `just ci tier1 run`, and on the reporter.
  - `--history` reads the job's failed-step log for each `verdict-fail`.
  - Either way, a red cells step whose preflight failed now reads as
    `NO VERDICT: preflight failed before any cell`, at
    `just ci matrix -> check::default`.
- `nightly-triage.py`'s `reclassify_from_log` uses the same parser.

Gate: `check-lane-stage-reporting`. It now also runs `nightly-triage.py
--selftest`. New rows:

- the real run's verbatim log lines must not read `cells ran and FAILED`, and
  without the markers they still do (the defect, pinned);
- a red `test-all` stays a verdict;
- the REAL `run-lane-steps.sh` is executed against a stub `just`, so the marker
  format and the parser cannot drift apart;
- every `just ci` cells step must set `NROS_LANE_STEP_RECORD` in its env, and
  its reporter must read the same value;
- `_matrix-run`'s `steps=(…)` must start with a non-cell and contain a cell.

Mutations, each confirmed applied with `git diff` and then reverted:

| Mutation | Result |
| --- | --- |
| Classifier branch disabled | 2 FAIL |
| Reporter env removed | 1 FAIL ("reads the SAME record") |
| Runner record write removed | 2 FAIL |
| `nightly-triage` branch disabled | 1 FAIL |

Live replay: `lane-stage.py --history --runs 4 --job "tier 2 (1-wise matrix)"`
reads 37685900447 from GitHub's log as `NO VERDICT: preflight failed before any
cell`.

## Not covered

`_matrix-run` runs `_require-lane-router`, `_lane-gate` and
`_require-owned-provisioned-roots` BEFORE the step runner starts. A failure
there writes no record, so the result is still None and the step-outcome answer
stands (`verdict-fail`). The runner should own those guards as steps too.

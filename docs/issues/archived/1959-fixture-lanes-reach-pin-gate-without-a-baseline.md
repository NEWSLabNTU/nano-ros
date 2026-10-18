---
id: 1959
title: "The self-hosted fixture lanes reach `check-submodule-pins` with no baseline step, so every one went red the night issue 1912 landed"
status: resolved
type: bug
area: [ci]
severity: medium
found: 2026-10-18
related: [1912, 1043, 1941, phase-505]
resolved_in: "branch fix/run-matrix-pin-baseline"
---

## Measured

Scheduled `run-matrix` run **39103947641** (main, 2026-10-18 06:24Z). Both
tiers stopped in `just build <lane>` before any cell ran:

```
submodule-pins: FAILED — baseline '695abb5eca8525511a55a4e50a7ff5b5de9f59f9' does not resolve, in CI.
check-fast (parallel): 1 of 456 gate(s) FAILED
error: recipe `build` failed with exit code 1
```

The tier-2 nightly (`nightly.yml` `matrix-nightly`, run **39106651485**, 07:10Z)
failed the same way against baseline `cb39d47bce48`.

The previous scheduled tier-1 run (**39009801514**, 2026-10-17) was red for a
different reason: `nros ws check-lock` found 234 drifts in the six Zephyr Rust
leaf locks. That is issue 1941, fixed on main by `b28d9cd8e4` at 01:25Z on
2026-10-18. Today's run never got far enough to show whether that fix holds.

## Cause

`build-test-fixtures` depends on `check::fast`, so `just build <lane>` runs
the whole fast line, `check-submodule-pins` included. That gate is
network-free and fails in CI when its baseline does not resolve.

Issue 1912 (`50d8d52206`, merged 03:50Z on 2026-10-18) changed the baseline
from the live `origin/main` to the tested commit's own. On a schedule or
dispatch event that is the commit's first parent. It also added
`pin-baseline.sh provide` to every job in `gate.yml` and `host-tests.yml`,
which is where that gate had been seen.

The fixture lanes reach the gate through `just build`, not `just check fast`,
and none of them got the step:

- `run-matrix.yml` `tier1` and `matrix` (tier 2);
- `nightly.yml` `matrix-nightly`;
- `live-peer.yml` `board` and `board-ci`.

They build in the persistent fixture tree (phase-489 W6), which mirrors the
job checkout's `refs/remotes/origin/*`. So `origin/main` had resolved there
and the old baseline worked. The depth-1 tree does not hold HEAD's first
parent, so the new baseline did not.

## Fix

- Each of those five jobs now runs the same three steps as a `gate.yml` job
  before its build: `pin-baseline.sh provide`, `init-moved`, `deepen`.
- A new gate, `check-workflow-pin-baseline` (fast line), checks the class by
  reach rather than by file. Any workflow job with a `just` invocation whose
  recipe closure contains `check::fast`, `check::fast-serial` or
  `check::submodule-pins` must run `pin-baseline.sh provide` in an earlier
  step.
- The closure is read from just's own dump through
  `scripts/lib/just_closure.py`. That walk was extracted from
  `check-prebuilt-artifact-producers.py`, which now uses it too.
- The walk now skips body lines that print rather than run: comments,
  `echo`/`printf`, and quoted help-text continuations. `setup`'s usage text
  names `just build zephyr`, which put the whole fixture build into `setup`'s
  closure. Unskipped, that flagged 14 false sites.
- The gate's selftest covers eight cases. The text-line skip is mutation-tested:
  disabling it turns the help-text case red.

Sweep: `python3 scripts/check-workflow-pin-baseline.py`. With the steps
removed, it lists exactly the five jobs above.

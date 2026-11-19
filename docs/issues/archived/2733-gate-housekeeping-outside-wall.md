---
id: 2733
title: "`just ci gate` can pass every step and still exit 1: post-verdict housekeeping runs inside the host lock's wall limit"
status: resolved
type: bug
severity: medium
area: [ci, gates]
related: [phase-489, 2602, phase-543]
found: 2026-11-19
resolved: 2026-11-19
resolved_in: "the change that resolved it"
---

**Home:** [phase-489](../../roadmap/phase-489-ci-build-speedup.md), beside
[2602](2602-gate-host-lock.md), whose host lock this corrects.

## What

`just ci gate` is `gate-host-lock.sh just ci::_gate-inner`. The lock script
runs its whole command under the wall limit (`timeout`, 9000 s at 16+ cores)
and the 45-min hang detector. `_gate-inner` runs the verdict steps, then the
post-verdict housekeeping: `scripts/build/trim-caches.py --apply` (phase-543 W4)
and `scripts/worktree-prune.py --reminder` (issue 2611). The housekeeping is
inside the timed region. When the steps are slow, it crosses the limit, the
process group is SIGTERMed, and the gate reports red although every verdict
step passed.

## Evidence

newslab-175, PR #4300, head 830fc49a5b, 2026-11-19 03:53Z to 06:23Z. The
summary table showed 11 of 11 steps ok, total 2h29m22s. Then:

```
trim-caches: corrosion-cargo: 5 target dir(s), 2.8 G / 60 G budget -> within budget
trim-caches: sizes-probe: 57 target dir(s), 16.9 G / 40 G budget -> within budget
error: recipe `_gate-inner` was terminated by signal 15
ci gate: gate timed out after 9000 s; its process group was killed, and the host's gate lock is free (issue 2602).
error: recipe `gate` failed on line 72 with exit code 1
```

## Fix direction (relayed by the nros 241 manager, 2026-11-19)

- Housekeeping runs outside the wall limit and the hang detector, after the
  verdict, under its own bounded limit.
- It reports, and never decides the gate's exit code: that comes from the
  verdict steps alone. A housekeeping failure or timeout is a warning.
- One place defines "verdict region" and "housekeeping".
- Tests: slow housekeeping leaves rc 0 when the steps passed; a failing step is
  still non-zero; a housekeeping failure or timeout is rc 0 with a warning.

## Resolution (2026-11-19)

`scripts/ci/gate-host-lock.sh` defines the two regions, and the `gate` recipe
in `just/ci.just` names each one on its single line:

```
bash scripts/ci/gate-host-lock.sh --housekeeping 'just ci::_gate-housekeeping' just ci::_gate-inner
```

- **Verdict**: `_gate-inner`, the steps and their summary. It alone runs under
  the wall limit and the hang detector, and its status alone is the gate's.
- **Housekeeping**: `_gate-housekeeping` (the cache trim and the worktree
  reminder, moved out of `_gate-inner`). It runs after the verdict has ended,
  however it ended, outside both limits, under its own 900 s deadline
  (`NROS_DEADLINE`, so expiry kills its process group). A failure or a timeout
  prints `ci gate: WARNING: housekeeping ...; the gate's verdict is
  unaffected`, and the exit code stays the verdict's.
- The housekeeping **keeps the lock**: the trim deletes target dirs, and the
  lock's promise is one gate's work per host at a time. The waiter cap is now
  wall + housekeeping + 30 min, so a waiter still outlasts any live holder.
  The start line prints the housekeeping limit beside the others.
- It now also runs after a docs-only gate. That is cheap: nothing new was
  built, so the trim only reports.

Tests (`just check gate-host-lock`), added to the existing harness, each with
a negative control or a case that fails without the property:

| case | control |
| --- | --- |
| slow housekeeping (silent, past the wall and the hang limit) leaves a passed gate at rc 0 | the same sleep at the tail of the verdict, the old layout, is killed red at the wall |
| a failing verdict keeps its status (3) after housekeeping | the passing verdict is 0 |
| a housekeeping failure is rc 0 with a warning | |
| a housekeeping past its own limit is killed, warns, rc 0, and its tree is gone | |
| the wall limit and the hang detector still kill the verdict red, and housekeeping runs after | |
| housekeeping holds the lock: the next gate waits for it | |
| `just/ci.just`: the `gate` line names both regions, and `_gate-inner` runs no trim or reminder | |

Run against the old script and `ci.just`, 13 checks failed (the new cases plus
the waiter-cap line). Mutation: running the housekeeping without its deadline
and returning its status fails the failure and timeout cases.

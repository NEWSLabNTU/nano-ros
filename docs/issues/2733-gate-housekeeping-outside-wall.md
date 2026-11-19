---
id: 2733
title: "`just ci gate` can pass every step and still exit 1: post-verdict housekeeping runs inside the host lock's wall limit"
status: open
type: bug
severity: medium
area: [ci, gates]
related: [phase-489, 2602, phase-543]
found: 2026-11-19
---

**Home:** [phase-489](../roadmap/phase-489-ci-build-speedup.md), beside
[2602](archived/2602-gate-host-lock.md), whose host lock this corrects.

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

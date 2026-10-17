---
id: 1842
title: "`tier-priority-plan-image` fails the Zephyr fixture build in two scheduled lanes, and neither lane shows its verdict"
status: open
type: bug
severity: medium
area: [ci, zephyr]
related: [1253, 1583, 1158, phase-466]
found: 2026-10-12
---

**Home: [phase-466](../roadmap/phase-466-scheduled-lanes-produce-verdicts.md)** — homed 2026-10-17; listed in that phase's "Issues homed here (2026-10-17)".

## Measured

Two scheduled lanes on 2026-10-12 stopped in the Zephyr fixture build at
the same gate, after `west fixtures: 5/5 ok (0 reused, 5 built)`:

```
  ✗ tier-priority-plan-image FAILED (exit 1) — see its verdict above
error: recipe `build-fixtures` failed with exit code 1
```

| run | job | lane | runner |
| --- | --- | --- | --- |
| nightly 38218729431 (07:36Z, `ba86ca6cf`) | 114709540728 | tier 2 nightly (pairwise cover) | self-hosted |
| run-matrix 38213993313 (06:44Z, `3df774a34`) | 114731935020 | tier 1 (cells) | nano-ros-runner-newslab-server-241 |

Both lanes then report `NO VERDICT: stopped in the build`.

**The verdict itself is unseen.** "See its verdict above" refers to
`tmp/build-test-fixtures-<stamp>/zephyr.log` on the runner. The job log
prints only that file's first-error line (`3469:error: recipe
`build-fixtures` failed`) and a short tail, and no artifact uploads it.
So neither run says which image, pin or band failed.

`python3 scripts/check-tier-priority-plan-image.py --selftest` (the
buildless half, issue 1583, on the fast line) passes on the same main.

## Lead, not a diagnosis

`728aedf62d` (phase-484 W5a, merged 2026-10-12) moved the 58 knob blocks in
`zephyr/Kconfig` into one rendered section. This gate judges tier
priorities against the bands in that file. The commit says no value
moved, and the buildless half still passes, so this is only where to look
first, not a cause. Issue 1253 recorded this gate failing on this lane
before, for a different reason (another checkout's module); its fix should
be ruled out too.

## What it is not

- Not issue 1811 (store-source paths): the west fixtures build 5/5 first.
- Not issue 1353 (disk) or a runner death: the gate exits 1 with a verdict.
- Not issue 1841 (play_launch pin): these jobs got past `just setup`.

## What would close it

1. Make the lane show the verdict: print the failing gate's own output,
   or upload `tmp/build-test-fixtures-*/` as an artifact on failure. A red
   that names no cause is a lane with no signal capacity.
2. Then fix what that verdict names, proven by a tier-2 or tier-1-cells
   run getting past the Zephyr fixture build.

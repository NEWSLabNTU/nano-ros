---
id: 1747
title: "`check sched-matrix` was red on main for four weeks and no lane ran it — the generator matched `sched_caps_for()` arms by a spelling the realizer had retired"
status: resolved
type: bug
area: [gates, docs, scheduling]
severity: low
found: 2026-10-07
related: [1071, 1226]
---

## What was measured

`python3 scripts/gen-sched-matrix.py --check` on `origin/main` exits 1:

```
gen-sched-matrix: no arm matching 'f.contains("freertos")' in sched_caps_for — update TARGETS in the same commit.
```

The generator renders `book/src/reference/sched-matrix.md` from
`sched_caps_for()` in `nros-orchestration-ir/src/rtos_realizer.rs`, finding
each platform's arm by its match-pattern text. Commit `14a97ff35`
(2026-09-11, "no RTOS is ever read from a key's spelling") changed the
FreeRTOS, ThreadX and NuttX arms from `f if f.contains("freertos")` to the
exact keys `"freertos"`, `"threadx"` and `"nuttx"`. The generator's `TARGETS`
table kept the old patterns, so it has refused to run since then.

Nobody noticed, because `sched-matrix` was listed in
`.config/gate-lane-exempt.txt` as "generator `--check`; in no lane and no
caller found". The exemption kept it off the fast lane, so the gate never ran.
This is issue 1071's class: a gate that works, that nothing runs. Its sibling
`rmw-feature-matrix` is not exempt and runs on the fast lane.

Found while deleting `SchedClass::TimeTriggered` (phase-482 W6), when the page
needed regenerating and the generator refused.

## Resolution (2026-10-07)

- `TARGETS` names the three arms by their current patterns. `--check` passes,
  and the generated page is unchanged by the fix.
- `sched-matrix` is removed from `.config/gate-lane-exempt.txt`, so it is a
  fast-lane gate like `rmw-feature-matrix`. The next arm rename now fails the
  pull request that makes it.

---
id: 1889
title: "`ci gate` check::build is intermittently red: every jobserver pool in a tree took ONE guard name, so `native-check` and `source-gates` refused each other"
status: resolved
type: bug
area: [build, testing]
severity: medium
found: 2026-10-14
resolved: 2026-10-14
related: [1882, 0762, 1157, 1887, 1886]
---

## Measured

`just ci gate` on a linked worktree failed in step 4 (`check::build`) twice
in a row. Each time `native-check` failed within about one second:

```
check-examples: 49 unit(s), jobserver pool=32 (fifo)
subtree-guard: a 'jobserver-pool' build is already running (pgid 171000, 1 process(es)).
  Holding tree: .../worktrees/i1872
  This tree:    .../worktrees/i1872
  Two builds in one tree corrupt each other's artifacts, so this one refuses to start.
```

An earlier run of the same gate on the same tree did not hit it.

## Cause

`nros_pool_run` (`scripts/build/jobserver-pool.sh`) wraps every pool in
`nros_guard_exec "jobserver-pool"`. `subtree-guard.sh` keeps one lock per
name per tree. So any two pools in one tree refuse each other, whatever they
build.

`check::build` runs its gates in parallel (`run-gates-parallel.sh`), and two
of them run pools:
- `source-gates` runs `compile-check-fixtures.sh` as the `compile-check`
  pool.
- `native-check` runs the `check-examples` pool. It has been in the
  parallel `build-serial` lane since issue 1882.

Whichever of the two starts second fails whenever they overlap, so the red
depends on timing.

Reproduced directly: two pools of two units each, started 2 s apart in one
tree.

| | labels `aaa` + `bbb` | labels `aaa` + `aaa` |
|---|---|---|
| before | second refused, rc=1 | refused |
| after | both run, rc=0 | refused (`jobserver-pool-aaa`) |

## Fix

Name the guard per pool label: `jobserver-pool-<label>`. Issue 0762 added
the guard to stop a re-launched build from racing the leftover processes of
its own earlier run. That is a per-label question: a second `check-examples`
pool is still refused, and different gates' pools no longer block each other.

`runner-sweep.sh` reaps locks by path, not by name, so it needs no change.

## Landed as issue 1886

The same fix landed on main first, as issue 1886 (a549fd0c05). There `nros_pool_guard_name` gives `jobserver-pool-<label>`, and the `subtree_guard.sh` selftest gained path 5. When this branch was rebased onto that, its own `jobserver-pool.sh` hunk was dropped, along with its changelog fragment, which would have claimed a fix this branch no longer carries. This issue stays archived as a duplicate record of 1886, and the measurement above is a second independent confirmation of it.

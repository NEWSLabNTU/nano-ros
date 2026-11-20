---
id: 2763
title: "The gate-lock tests asserted wall-clock bounds that host load breaks, and one of them hid a gate that waited for its orphan"
status: resolved
type: flaky-test
severity: medium
area: [ci, testing, gates]
found: 2026-11-20
resolved: 2026-11-20
resolved_in: "the change that filed it"
related: [issue-2602, issue-2733, issue-2221]
---

## What happened

A full `just ci gate` on another host (load about 12) went red at step 4,
`check::fast`, on one case of `tests/gate-host-lock-tests.sh`: "an orphaned
child does not hold the lock". Run alone on the same tree, it passed. Steps 5
to 11 never ran, so the gate gave no verdict on the branch it was gating. The
test is on the fast line, so any lane under load could hit it.

## Why

Five cases asserted a wall-clock bound, or slept a fixed time and then
checked absence:

- Case 1 compared the next gate's start with the time the first gate's
  WRAPPER returned. The lock is released before that wrapper returns, so the
  next gate may legitimately start first.
- Case 2 required the waiter to start within 1.5 s of the holder's kill.
  Its negative control slept 0.5 s and checked that the waiter had not
  started, which passes for a waiter that has not even launched yet.
- Case 3 required the next gate to start within 1.5 s.
- Case 4 slept 1 s and checked absence, the same vacuous shape as case 2's
  control.
- Cases 5 and 6 held the lock for 4 s and 3 s, and expected a waiter
  launched afterwards to find it still held. Cases 8 and 12 bounded a
  killed gate by 15 s against a 30 s or 60 s alternative. Case 14 slept
  0.5 s and assumed the holder had taken the lock by then.

Case 3 was also hiding a product defect. The orphan inherits the gate's
output FIFO, so the holder's `wait "$tee_pid"` saw no EOF until the orphan
exited, and the gate held the host's lock for the orphan's whole life.
Measured: the first gate took 30.03 s, exactly its orphan's `sleep 30`. The
case passed only because that orphan had already exited when the next gate
started. A real gate that leaves a process holding its stdout would have
kept the lock with no limit, since this wait sits outside the wall limit and
the hang detector.

## Fix

- `scripts/ci/gate-host-lock.sh`: after the gate exits, `tee` gets a 5 s
  drain grace for what the gate already wrote, and is then stopped. Anything
  an orphan writes later is not the gate's output.
- `tests/gate-host-lock-tests.sh` measures each property instead of a time:
  - Ordering checks compare against the holder's command END, recorded
    inside the lock.
  - Waits are on states the scripts print: the holder's `pid=` line, and a
    waiter's "gate lock is held".
  - A holder runs (300 s) until the waiter is seen waiting, and is then
    released.
  - Case 4's control is a LOWER bound, which load can only widen.
  - The remaining upper bounds have a margin many times the nominal time.
  - Case 3 now checks, while the orphan still runs, that the gate has
    returned, the lock is free, and the next gate does not wait.
  - `release` matches the sleeper's argv0 only. An unanchored `pkill -f`
    also matched the stand-in wrappers' own command lines, which would kill
    a waiting gate.

## Measured

- The new test passes solo, and 3 of 3 runs with the test and 40 busy loops
  confined to one CPU.
- Against main's `gate-host-lock.sh`, the new case 3 fails ("the gate returns
  while its orphan still runs"): the gate returned only when its 300 s
  orphan did.
- The old test did not reproduce the red in 6 runs under that load either,
  so the reported red came from a load this probe does not model (I/O, fork
  pressure). The rewrite removes the bound rather than tuning it, so no load
  shape can turn it red.

---
id: 1945
title: "Two timer-overrun tests sleep exactly one timer period on the real clock, so a preempted step reads as a second stall"
status: resolved
type: bug
area: [testing, node, monitors]
severity: low
found: 2026-10-18
related: [1939, 1105]
---

## Measured

`just ci gate` on the phase-462 W4c branch (2026-10-18), step `check::build`,
recipe `node-std-tests`, host load average 40:

```
executor::tests::a_stalled_timer_reports_a_timer_overrun_violation --- FAILED
panicked at packages/core/nros-node/src/executor/tests.rs:9393:5:
no repeat without a new drop: ["timer-overrun-runtime"]
```

8 of 8 green solo. The test is on `main` unchanged; the branch does not
reach it.

## Why

The test registers a 10 ms timer, stalls 120 ms, then takes a "quiet" step
of `elapse_then_spin_once(…, 10)` and asserts no new overrun. That helper is
a real `platform_sleep`, and the default test clock is real time on `std`.
A quiet step one period long has no margin: any preemption past 10 ms drops
another period, and the rule correctly reports it.

`an_installed_violation_sink_sees_every_violation` has the same step after
each stall, and asserts the sink saw ONE verdict whose `measured` equals the
timer's running overrun total — a dropped period in the quiet step adds to
that total and leaves no verdict equal to it.

Issue 1939 fixed this class for `max_latency_ms` tests; its sweep keyed on a
nonzero `max_latency_ms`, which neither of these declares, so it could not
see them.

## Resolution (2026-10-18)

Both tests run on a `private_test_clock!` (`stalled_timer_clock`,
`violation_sink_clock`) with a local `elapse_then_spin_once` that advances
it by exactly what it names, so every elapsed time is a constant of the test.

Sweep: every test body in `executor/tests.rs` that uses real elapsed time
(`elapse_then_spin_once`, `Instant::now`, `thread::sleep`,
`platform_sleep`), asserts on a verdict (`drain_violations`, a
`*-runtime` rule, `timer_overruns`, a nonzero `max_latency_ms`) and has no
`executor_with_clock_fn`. Four remained besides these two; each asserts a
LOWER bound that load can only strengthen (`deadline_skip_*` /
`deadline_warn_*` sleep 5 ms against a 1 ms deadline;
`a_violation_is_logged_and_still_drainable` asserts only that the overrun
rule fired after a 120 ms stall), so they stay on the real clock.

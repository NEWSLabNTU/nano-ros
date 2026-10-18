---
id: 1944
title: "nros-node timer unit tests sleep on the wall clock, so a loaded host fires a timer one spin early"
status: open
type: bug
severity: low
area: [executor, testing]
found: 2026-10-19
related: [1105]
---

## Measured

`just ci gate`, `check::build`, gate `node-std-tests`, in a worktree with
issue 1940's change (none of it in the timer path), at `-P80`:

```
executor::tests::test_timer_delta_accumulates_when_trigger_fails --- FAILED
panicked at packages/core/nros-node/src/executor/tests.rs:7388:5:
  left: 1
 right: 0
```

The same tree, the test run on its own: 8 of 8 pass. The full
`cargo test -p nros-node --lib --features std,sim-time,param-services` run
passes too (669 passed).

## Cause

`elapse_then_spin_once(executor, ms)` (`executor/tests.rs`) calls
`spin::platform_sleep(ms)` and then `spin_once(0)` against `test_clock_us`,
which is a real monotonic clock on `std`. The test registers a 100 ms timer,
sleeps 50 ms, and asserts the timer has NOT fired. A host busy enough to
oversleep 50 ms by another 50 ms fires the timer in the first spin, and the
`== 0` assertion fails. Only an upper bound on the sleep makes the test hold,
and nothing guarantees one.

36 call sites go through `elapse_then_spin_once`, so this is a class, not one
test. Issue 1105's `private_test_clock!` fixed shared-epoch interference
between tests. It does not fix this: the clock is still wall time.

## Fix direction

Give these tests a clock they ADVANCE rather than wait for, with the step
equal to `ms`, so "elapsed = 50" is a fact rather than a lower bound. Keep
the lower-bound assertions on real sleeps only where the thing under test is
the sleep itself (CLAUDE.md: assert lower bounds on timing).

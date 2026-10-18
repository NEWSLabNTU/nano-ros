---
id: 1950
title: "A timer's remaining-time test asserts a lower bound that load breaks, because the spin's real cost is credited on top"
status: open
type: bug
area: [testing, node]
severity: low
found: 2026-10-18
related: [1945, 1939]
---

## Measured

The scheduled `gate` on `main` failed its build tier on one test:

- Run: 39088672707 (schedule, 2026-10-18 02:02 UTC, head `b28d9cd8e`).
- Job: 117560552579, `check / build tier + no-std`.
- Gate: `node-std-tests`, which reported `1 of 26 gate(s) FAILED` and
  `627 passed; 1 failed`.

```
thread 'executor::tests::time_until_next_call_goes_negative_once_a_timer_is_overdue'
  panicked at packages/core/nros-node/src/executor/tests.rs:5067:5:
40ms of a 100ms period gone, so ~60ms remain: 48916000ns
```

The test registers a 100 ms one-shot timer, then calls
`elapse_then_spin_once(&mut executor, 40)` on the REAL clock
(`executor_with_clock`). It asserts that the remaining time is in
`50_000_000..=60_000_000` ns. Its own doc comment says that `spin_once`
"credits the real wall-clock cost of the spin on top of the injected delta".
That cost only ever adds elapsed time, so it only ever removes remaining time.
Under load the spin cost about 11 ms, and remaining time fell to 48.9 ms,
below the bound.

The doc comment calls the 50 ms floor "the assertion", because an upper-bound
check alone passes a counter that never advanced. That reasoning is right, but
it does not make the floor load-proof here. The quantity being bounded is
REMAINING time, and load moves it DOWN, toward the floor.

## Why issue 1945's sweep did not catch it

Issue 1945 moved two timer-overrun tests onto a private clock. Its sweep then
judged the real-clock tests that remained to be safe. The sweep only selected
test bodies that assert on a VERDICT (`drain_violations`, a `*-runtime` rule,
`timer_overruns`, a nonzero `max_latency_ms`). This test asserts on
`timer_time_until_next_call_ns`, so the sweep never looked at it. 1945's
conclusion ("each asserts a LOWER bound that load can only strengthen") holds
for the four tests it named. It does not hold for a lower bound on a quantity
that load decreases.

## What it is not

- Not a regression in the timer arena. The other accessor assertions in the
  same test (exact `Some(100_000_000)` when fresh, the overdue sign) did not
  fail, and the value is about 11 ms off in the direction load pushes it.
- Not issue 1353 (out of disk) or a runner death. The step reached a verdict.

## What would close it

Drive this test, and any sibling that bounds remaining time from below, on a
`private_test_clock!` (issue 1939's helper, used by 1945). That makes the
40 ms step exactly 40 ms and the floor exact.

Alternatively, keep the real clock and derive the floor from a measured spin
cost instead of a constant. But that re-creates the problem this class keeps
hitting.

Re-run the sweep keyed on what the assertion bounds, not on whether it reads a
verdict: every real-clock test with a lower bound on a remaining or budget
quantity. Put that sweep command in the fix's commit message.

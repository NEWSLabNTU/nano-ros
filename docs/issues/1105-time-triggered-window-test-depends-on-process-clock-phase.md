---
id: 1105
title: "`test_time_triggered_dispatch_active_window` asserts a 1 ms window off a FREE-RUNNING process clock, so it passes or fails on how long the tests before it took"
status: open
type: bug
area: testing
severity: medium
related: [1104, phase-425]
found: 2026-09-05
---

# The test measures the suite's runtime, not the scheduler

## Symptom

`executor::tests::test_time_triggered_dispatch_active_window` fails whenever the
suite runs `--test-threads=1`, and passes solo or in the default parallel mode:

```
panicked at src/executor/tests.rs:1052:
assertion `left == right` failed: entry bound to window-0 should dispatch inside
its active slot
  left: 0
 right: 1
```

Stable across runs — 3 of 3, each finishing in an identical 3.49 s — so it is
reproducible rather than flaky. It is reproducible for the wrong reason.

## Cause

The executor under test is built with `executor_with_clock`, whose clock is

```rust
fn test_clock_us() -> u64 {
    use std::{sync::OnceLock, time::Instant};
    static EPOCH: OnceLock<Instant> = OnceLock::new();
    EPOCH.get_or_init(Instant::now).elapsed().as_micros() as u64
}
```

a **free-running wall clock shared by every test in the process** — the epoch is
a `OnceLock` initialised by whichever test runs first.

The test then builds a 2 ms cycle of two 1 ms windows and asserts that an entry
bound to window 0 dispatches:

```rust
TimeTriggeredSchedule::<2>::new_full(2_000_000,
    [TimeTriggeredWindow::new(0, 1_000_000, "w0"),
     TimeTriggeredWindow::new(1_000_000, 1_000_000, "w1")]);
```

Which window "now" falls in is `elapsed % 2ms` — a function of how much work ran
before this test, not of anything the scheduler did. Solo, elapsed is near zero
and it lands in window 0. After ~340 other tests it lands wherever it lands; the
single-threaded ordering happens to put it in window 1, every time, because the
preceding work is deterministic.

The parallel mode passes for an equally accidental reason: a different amount of
preceding work, landing on the other side of the modulus.

**This is not issue 1104.** That one was a genuine shared-state coupling on
`time_source`'s process-global and is fixed by serializing the tests that touch
it. This test touches neither `time_source` nor the ROS-time override; it was
merely the test that surfaced when the threading mode changed. Filing them
together would have hidden a real defect behind a fixed one — the first draft of
1104 did exactly that, and its single-threaded explanation was wrong.

## Fix — not attempted

The test needs a clock it controls. The suite already has the shape: several
neighbours drive time with `elapse_then_spin_once`, and `Clock`'s ROS-time
override exists precisely so a test can say what time it is. Either

1. **Phase-align before asserting** — advance to a known offset within the cycle
   so window 0 is current by construction, or
2. **Give this test its own clock** rather than the shared free-running one, so
   `elapsed` starts at zero for it.

(2) is the one that removes the class: any future window test written against
`executor_with_clock` inherits the same defect, and there is nothing in the
helper's name to warn the author.

## Not covered

* Whether other tests using `executor_with_clock` assert anything phase-dependent.
  Only the one that failed was traced.
* Whether the parallel-mode pass is stable or merely likely — it has passed every
  observed run, but nothing makes it deterministic.

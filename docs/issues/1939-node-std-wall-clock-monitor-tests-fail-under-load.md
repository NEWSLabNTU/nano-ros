---
id: 1939
title: "Two nros-node monitor tests judge a 5 ms latency budget against the wall clock, so they fail when the gate's parallel build preempts them"
status: open
type: bug
area: [testing, node, monitors]
severity: low
found: 2026-10-17
related: [1105]
---

## Measured

`just ci gate` on the phase-462 W3b branch (2026-10-16), step `check::build`,
recipe `node-std-tests` (`cargo test -p nros-node --lib`). Two runs, two
different reds, neither reaching the code the branch changed:

- `executor::tests::i7_a_slow_report_breeds_no_overrun_or_jitter_verdict`
  panicked at `tests.rs:14232`:
  `the report itself bred a verdict: [("max-latency-runtime", "/i7/handler/mrm_state", 27), ("max-latency-runtime", "/i7/handler/takeover_request_state", 27), ("timer-overrun-runtime", "timer", 1)]`
- `executor::tests::t4_an_overrun_after_arming_is_the_one_stored_violation`
  failed in the other run (its message was not kept).

Both pass 5/5 solo and 3/3 in the full `nros-node` suite on an idle host.

## Why

Both tests declare `max_latency_ms: 5` and measure it with the executor's
default test clock, `test_clock_us`, which on `std` is real time. The "slow"
work is a `busy_ms(n)` spin on `std::time::Instant`, and the gaps between
spins are `elapse_then_spin_once`, a real `platform_sleep`. So whether a tick
that is supposed to be quick stays under 5 ms depends on whether the OS
scheduled the test thread during it. Under the gate's 32-way build it did not:
i7 measured 27 ms on two publishers that do no work at all.

The assertion is about the monitor rule (a slow REPORT must not be charged to
the callbacks around it; one overrun after arming is stored exactly once), not
about the host's scheduler. Issue 1105 is the same shape for window phase, and
its fix, `private_test_clock!`, is a clock the test SETS.

## Fix direction

Give both tests a private clock, and make `busy_ms` advance that clock instead
of spinning, so the measured latency is a constant of the test. Then sweep the
other tests in `executor/tests.rs` that pair a millisecond `max_latency_ms`
with real elapsed time (16 tables declare one) and move each that asserts an
exact verdict set.

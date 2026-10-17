---
id: 1939
title: "Two nros-node monitor tests judge a 5 ms latency budget against the wall clock, so they fail when the gate's parallel build preempts them"
status: resolved
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

## Resolution (2026-10-17)

The two tests, and the two siblings a sweep found (`max_latency_ms` declared,
real elapsed time, no private clock), run on a `private_test_clock!` now:
`t4_an_overrun_after_arming_is_the_one_stored_violation`,
`i7_a_slow_report_breeds_no_overrun_or_jitter_verdict`,
`t4_a_timer_overrun_beside_an_idle_subscription_is_judged` and
`b3_a_late_tick_exceeds_the_route_while_every_callback_passes`. A
callback's "work" and each spin's elapsed time advance the clock by exactly
what they name, so every measured latency is a constant of the test.

Running them under load found a SECOND cause in t4, the one whose message was
not kept. Its log sink is process-global and stays installed after it returns,
and libtest reuses worker threads, so a test that ran earlier on the same
thread had logged into t4's thread-local buffer through it. Measured:
`nothing reported: ["contract violation: route-latency-runtime /b3/handler/state
measured=260 declared=149", ...]`, b3's verdict. t4 now clears its buffers
after installing the sink.

Sweep: a scan of `executor/tests.rs` for a test body with a nonzero
`max_latency_ms`, real elapsed time (`elapse_then_spin_once`, `Instant::now`
or `thread::sleep`) and no `executor_with_clock_fn`. It returns nothing now.

Measured: the four tests, 15 of 15 runs green with 32 busy loops on the host
(3 of 10 before the buffer fix, all t4); the full `nros-node` lib suite under
`std,sim-time,param-services` is 667 passed and takes 1.5 s instead of ~5 s.

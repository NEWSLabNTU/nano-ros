---
id: 1105
title: "`test_time_triggered_dispatch_active_window` asserts a 1 ms window off a FREE-RUNNING process clock, so it passes or fails on how long the tests before it took"
status: resolved
type: bug
area: testing
severity: medium
related: [1104, phase-425]
found: 2026-09-05
resolved: 2026-09-07
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

The test then builds a 2 s cycle of two 1 s windows and asserts that an entry
bound to window 0 dispatches:

```rust
TimeTriggeredSchedule::<2>::new_full(2_000_000,
    [TimeTriggeredWindow::new(0, 1_000_000, "w0"),
     TimeTriggeredWindow::new(1_000_000, 1_000_000, "w1")]);
```

(The title and the first draft of this paragraph said **ms**; the fields are
`_us`, so `2_000_000` is 2 **seconds** and each window is 1 second. The
mechanism is unchanged — only the scale of "how long the suite has to run before
it crosses a boundary" — but the numbers below are seconds.)

Which window "now" falls in is `elapsed % 2s` — a function of how much work ran
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

## Fix

**(2), the class fix**: `private_test_clock!(<name>)` in `executor/tests.rs`.
Each invocation expands to its own module holding its own `AtomicU64` and its
own `fn() -> u64`, handed to the new `executor_with_clock_fn`. So a window test
gets a clock whose value it SETS — no lock, no ordering with siblings, and
nothing shared for the rest of the suite to disturb. `claim_at_us` panics on a
second claim (two tests on one clock is the coupling this removes, and it must
not be silent) and `set_us` refuses to go backwards.

The shared `test_clock_us` is untouched, so the ~98 neighbours that only need a
monotonic source keep it. That was the point of preferring (2) over a one-off
phase alignment: nothing in `executor_with_clock`'s name warns an author, and
`private_test_clock!`'s does.

Both TT-window tests moved onto private clocks, and both now assert BOTH
directions — the clock is the only thing that changes between the two halves:

* `test_time_triggered_dispatch_active_window` — phase 0.5 s fires window-0 and
  suppresses window-1; phase 1.5 s does the complement, on the same executor
  with the same entries.
* `test_tt_window_gate_suppresses_outside_window` — the other one the issue's
  "Not covered" asked about. It IS phase-dependent, just at 1-in-60 000 rather
  than reliably: window `[50s, 50.001s)` of a 60 s frame, asserted only from the
  suite's own elapsed time. Now pinned at 10 s (outside) then 50.0005 s (inside).

### Measured

Lane: `cargo test -p nros-node --lib --features std,sim-time,param-services`.

| | runs | result |
| --- | --- | --- |
| `--test-threads=1` | 5 | 371 passed, 0 failed (each) |
| default parallel | 5 | 371 passed, 0 failed (each) |
| `--test-threads=2` | 3 | 371 passed, 0 failed (each) |

**The defect reproduces on the pre-fix file, 3 of 3** at `--test-threads=1`,
exactly as reported (`left: 0, right: 1`).

**Mutation — the tests still catch a broken scheduler, in three directions.**
Each mutation is on the TT gate in `spin.rs`; both tests fail each time:

| mutation | what fails |
| --- | --- |
| `in_window = true` (gate always open) | the window-1 / outside-slot suppression assertions |
| `in_window = false` (gate always shut) | the in-slot dispatch assertions |
| `now_us = 0` (gate ignores the clock) | the second half of each test |

The third is the one that matters: **the OLD tests PASS a gate that ignores the
clock entirely** (verified — 2 passed, with the mutation in place), because
phase-0 happens to be inside window 0. So the old assertions were not merely
order-dependent, they had no purchase on the clock at all. The new ones do.

## Not covered

* `cargo test -p nros-node --lib --no-default-features --features alloc` does not
  compile — 37 pre-existing errors, identical with and without this change. So
  the `not(feature = "std")` arm of the private clock is compiled by no lane
  here; it is written for the same shape as `test_clock_us`'s.
* Whether any NON-window test reads a phase off the shared clock. The two that
  compute `now_us % major_frame_us` are both fixed; nothing else in the module
  registers a TT dispatcher.

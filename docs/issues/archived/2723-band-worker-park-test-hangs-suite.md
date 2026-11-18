---
id: 2723
title: "`a_band_worker_parks_until_its_timer_is_due` could hang the nros-node suite, with one thread spinning and no message"
status: resolved
type: bug
severity: medium
area: [executor, testing]
found: 2026-11-19
resolved: 2026-11-19
resolved_in: "test(nros-node, issue 2723): the band-park test bounds every wait and cannot miss the park"
related: [2696, phase-509]
---

## What happened

One full `just ci gate` on another host hung `node-std-tests` until its
2400 s timeout. 870 of 876 tests had passed; the last line was
`executor::tests::p9b_windows::a_band_worker_parks_until_its_timer_is_due
has been running for over 60 seconds`, then nothing. The same head passed
two other full gates that day. Host load was moderate (1-min median 27 on
80 CPUs, minimum 5.7). For the whole 38 minutes the thread samples read
the same state, with ONE thread on a CPU in every sample: a spin, not a
park or a deadlock.

## Cause: the test, not the executor

The test spawns a helper inside `std::thread::scope` that waits for the band
worker to park, then takes the executor lock to prove the park released it.
That wait was unbounded: `while !waiting { yield_now() }`. `waiting` is true
only during the park, about 50 ms. Two ways to miss it, and both spin
forever:

1. The helper is not scheduled during that window. The main loop then serves
   the timer and leaves, and `scope` joins the helper, which never sees a
   park.
2. The main loop fails first: its `assert!(elapsed < 1 s)` panics. A
   panic inside `scope` is propagated only after every scoped thread is
   joined, so the failure message never printed, and the helper spun.

Either way the suite hung with one busy thread and no message, which matches
the thread profile.

Reproduced: the original test with a 300 ms sleep before the helper's wait
loop (a helper scheduled late) hangs (`timeout 30`: exit 124, every run).

The executor's park is not implicated by the evidence:
- `band_prelude` parks for the band timer's remainder and the next pass
  fires the timer.
- 300 solo runs and 80 runs 16-way on one pinned CPU of the original test all
  passed.
- #4266 (issue 2696) changed `BandWaiter::new` (the interrupt-safe give)
  but not `wait_us` or the park, and the hung head predates it.

Which of the two paths the gate hit is not recoverable from its log. The
fixed test now says which.

## Fix

- Every wait is bounded and fails with what it waited for. A `Drop` guard
  releases the helper on any exit from the main loop, a panic included, so
  a failure propagates. The main loop is bounded at 10 s.
- The helper cannot miss the park. The test's semaphore holds its FIRST park
  open, after the park's own wait ends, until the helper has taken the lock
  (at most 10 s). The park the executor asked for is measured separately
  (`first_wait_us`).
- The assertions then name the failure:
  - `the worker never parked: N pass(es)`;
  - the timer not served within the deadline;
  - the park outside 40 ms to 1 s, a LOWER bound too: a park that ends
    early passes an upper-bound-only check;
  - the lock not taken during the park.

Checked:
- A helper scheduled 300 ms late now passes in 0.35 s.
- A band prelude that never parks fails in about 10 s with
  `the worker never parked: 48476 pass(es)`, instead of hanging.
- 300 solo runs, 80 runs 16-way on one pinned CPU, and 20 runs of the whole
  `p9b_windows` module all pass.

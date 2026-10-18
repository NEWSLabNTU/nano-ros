---
id: 1947
title: "The cross-thread wake-latency test judged a 10 ms bound on ONE real-time trial, so a preempted main thread failed it"
status: resolved
type: bug
area: [testing, node]
severity: low
found: 2026-10-18
related: [1939, 1945]
---

## Measured

`just ci gate` on the issue-1943 branch (2026-10-18), step `check::build`,
recipe `required-features-tests`:

```
nros-tests::wake_latency wake_latency_cross_thread_trigger
panicked at packages/testing/nros-tests/tests/wake_latency.rs:120:5:
Trigger-to-spin-exit latency 22.831153ms exceeds 10ms bound — cv wake not firing
```

The spin's own bound (trigger delay + 10 + 50 ms) held, so the guard's wake
did fire; the branch touches only `nros-macros`. 3 of 3 green solo.

## Why

The test measures a REAL latency — a condvar wake from a worker thread — so
issue 1939's private clock does not apply: the clock is the subject. But it
judged that latency on one trial, measured as `fire.elapsed()` AFTER
`handle.join()`, so any preemption of the main thread between the trigger and
the measurement (a 32-way gate build) was charged to the wake.

## Resolution (2026-10-18)

Five trials, judged on the best. A wake that does not fire misses every
trial at the 1000 ms spin timeout, and each trial still asserts the spin
returned well before it (< 500 ms), so the defect the test exists for fails
on any trial; preemption inflates only some trials and no longer decides the
verdict. Latency is now `spin_end - fire`, taken before the join.

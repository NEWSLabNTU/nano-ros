---
id: 2588
title: "Timing tests read their start clock after spawning the thread whose sleep they bound, so a lower bound can fail by the spawn's cost"
status: resolved
type: flaky-test
severity: low
area: [testing]
related: [issue-2221]
found: 2026-11-15
resolved: 2026-11-15
resolved_in: "the change that filed it"
---

## What

On 2026-11-15 `just ci gate` step 7 (`check::build`, lane
`required-features-tests`) went red on a branch that changes only CI YAML:

    nros-tests::readiness_park a_notify_session_wakes_the_park_from_its_wake_callback
    the spin returned after 99.917488ms, before the arrival at 100ms

`spin_across_an_arrival` spawned the writer thread, which sleeps
`WRITE_AFTER` and then writes, and only then took `let start =
Instant::now()`. The writer's sleep starts when the thread starts, which can
be before `spawn` returns. So `start` is late by up to the spawn's cost, and
the arrival can land less than `WRITE_AFTER` after it. The assertion
`elapsed >= WRITE_AFTER` has no slack, so it failed by 83 µs. This is a
start race, not load: no retry or wider bound is the fix (CLAUDE.md, "No
retries").

## Fix

Read the reference clock BEFORE the spawn, so the trigger's delay can only
start after it and the lower bound holds by construction.

Sweep (every lower-bound timing assertion in tests, Rust and C):

    git grep -n -E 'elapsed >= |elapsed\(\) >= |_ms >= ' -- 'packages/**/*.rs' 'packages/**/tests/**/*.c'

Each hit was classified by whether its reference instant is read after a
trigger thread's delay begins.

- No slack, so it could fail: `readiness_park.rs` (`spin_across_an_arrival`,
  which serves two tests, and `cancel_from_another_thread_ends_a_long_park`),
  `signal_fd_wake.rs` (both tests), and `nros-cli-core` `cmd/prepare.rs`.
- Same shape with 10–100 ms slack, moved anyway so that the rule has no
  exceptions: `nros-node` `executor/spin.rs` (cancel test),
  `c_port_posix_wake.rs` (4), `wake_wrapper.rs`, `nros-tests`
  `port_lease.rs`, `nros-c` `executor_backend_wake.c` (4) and
  `node_guard_condition.c` (the parked wake).
- Not the shape: the reference is read before the spawn, there is no second
  thread, or the delay runs on the measuring thread.

## Resolution

Fixed by the change that filed it: 17 reference reads, each moved above its
spawn. No bound changed.

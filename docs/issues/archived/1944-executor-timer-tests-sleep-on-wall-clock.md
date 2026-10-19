---
id: 1944
title: "nros-node timer unit tests sleep on the wall clock, so a loaded host fires a timer one spin early"
status: resolved
type: bug
severity: low
area: [executor, testing]
found: 2026-10-19
related: [1105, 1945, 1950, 1579]
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

## Resolution (2026-10-19)

Fixed together with issue 1950, which is the same helper seen from a
different assertion.

**The helper is gone.** There is no module-level `elapse_then_spin_once` in
`executor/tests.rs` any more. `private_test_clock!` now generates two more
functions per clock:

- `<clock>::elapse_then_spin_once(executor, ms)` advances that clock by
  exactly `ms`, then calls `spin_once(0)`.
- `<clock>::advance_ms(ms)` is what a callback calls to COST time. The
  executor measures callback runtime on the same injected clock, so a cost
  is exact, where a real sleep is "at least `ms`".

Every test that stepped time moved onto its own clock, so a "50 ms" step is
50 ms and the spin itself costs nothing on that clock. Assertions that were
ranges or one-sided bounds became equalities, which give both bounds at once.
The lower bound is kept, not dropped. Removing the shared spelling is the
structural part: a new test cannot reach the wall-clock version by habit,
because it no longer exists. No production code changed. `MockSession` now
records the timeout its last `drive_io` was asked for (`last_drive_timeout_ms`).

### Sweep

Static, over what a test reads time from:

```
git grep -nE 'elapse_then_spin_once\(|thread::sleep|platform_sleep\(|Instant::now' \
  -- 'packages/core/*/src/**/tests.rs' 'packages/core/*/src/**/*_tests.rs' \
     'packages/core/*/src/**/tests/*.rs' 'packages/core/*/tests/*.rs'
```

Empirical, over what load does to every test. Each of the binary's 689 tests
ran 3 times. Each run was SIGSTOPped for 120 ms at a time, with 0 to 19 ms of
running between stops. The script signals only the pid it started.

```
bin=target/debug/deps/nros_node-<hash>   # cargo test -p nros-node --lib --features std,sim-time,param-services --no-run
for t in $($bin --list 2>/dev/null | sed -n 's/: test$//p'); do
  for _ in 1 2 3; do
    $bin --exact "$t" --test-threads=1 >out 2>&1 & pid=$!
    while kill -0 $pid 2>/dev/null; do sleep 0.0$((RANDOM%20/10))$((RANDOM%10));
      kill -STOP $pid 2>/dev/null || break; sleep 0.12; kill -CONT $pid 2>/dev/null || break; done
    wait $pid; grep -q '^test result: ok. 1 passed' out || echo "RED $t"
  done
done
```

Plain `yes` CPU hogs reproduced this issue 0 times in 80 runs (40 hogs,
P=4). CFS gives a waking sleeper its slice back almost at once, so a hog
rarely stretches a 50 ms sleep by another 50 ms. A process that is
descheduled outright is the condition the `-P80` gate produced, and the stop
loop reproduces it on demand.

Before the fix, 5 tests went red, each 3 of 3:
`test_timer_delta_accumulates_when_trigger_fails` (this issue),
`test_add_timer_and_fire`,
`time_until_next_call_goes_negative_once_a_timer_is_overdue` (1950),
`ros_time_timer_follows_the_simulated_clock`, and
`a_node_level_ros_time_timer_follows_the_simulated_clock`. The last two
held the wall tick count to an UPPER bound (`<= 1`), so they failed for the
same reason. After the fix, none went red. (The 3 tests that are `#[ignore]`d
read as red under this harness's `1 passed` verdict both times.)

The static sweep found more tests that read real time. The stop loop did not
turn them red, but each moved anyway where the clock was not the subject:

- **Exact now, were bounds that load could only strengthen:**
  `test_atomic_sporadic_per_callback_runtime_consumed`. This one also had an
  UPPER bound of 500 ms on a 10 ms sleep. The others:
  `test_atomic_overrun_exceeds_budget`,
  `test_sporadic_budget_exhaustion_suppresses_dispatch`,
  `deadline_skip_*`, `deadline_warn_*`,
  `a_budget_skipped_ros_timer_does_not_advance_on_wall_time`,
  `a_tier_slower_than_its_declared_cadence_is_late`, and
  `violations_beyond_the_ring_are_counted` (now exactly 4 dropped).
- **Load-breakable, but the 120 ms stop did not land in the window:**
  - `test_spin_once_does_not_credit_timeout_to_timer_delta`: its bound came
    from an `Instant` taken after the executor's delta had started. It is
    now exactly 0 fires, plus a positive control of 1 fire after one period.
  - `test_spin_one_period_timed_no_overrun`: "no overrun" was an upper bound
    on a spin's real cost. Both directions are now exact, with a 60 ms
    guard-condition callback as the overrun case.
  - `test_timer_does_not_fire_at_zero_delta`: its "zero" was the time
    registration took.
- **`test_wake_short_circuits_drive_timeout`** timed a mock whose
  `drive_io` returns at once. It could not fail on the executor, only on the
  host. It now asserts what the executor asked the transport for
  (`last_drive_timeout_ms == Some(0)`). The rmw-cffi wake arm it is named
  after is covered end to end by `nros-c`'s
  `tests/run/node_guard_condition.c`.
- **Kept on real time, because the wait IS the subject:**
  - `test_spin_timeout` gained the LOWER bound it lacked. It is 49,999 µs,
    not 50 ms. The executor's clock truncates to whole µs, so 50,000 µs
    between two reads proves only > 49,999 µs of real time. A `>= 50 ms`
    floor failed 8 of 20 parallel runs at 49.9993 to 49.9999 ms.
  - `spin_period_counts_*`, the `open_threaded`/halt smoke tests,
    `graph_wait_tests` (`PROMPT` is 1 s against microseconds), the
    `spin.rs` `is_spinning` tests, `wake_fanout`, and
    `backing::TierSetupGate`. All of these assert liveness or a
    load-strengthened floor.
- **Other crates:** `nros-platform-cffi`'s `c_port_posix*` tests time the
  platform's own sleep, timer and wake, so the clock is their subject (their
  module note already explains why none sleeps a fixed window).
  `packages/api` and `packages/rmw` have no Rust unit test that bounds a real
  sleep.

Fourteen tests needed `std` only for the clock, so they lost the gate:
`test_add_timer_and_fire`, `test_timer_repeats`,
`test_timer_with_subscriptions`, `test_trigger_all_with_mixed_handles`,
`test_timer_delta_accumulates_when_trigger_fails`,
`a_capturing_timer_dispatches_with_the_arenas_copy`,
`a_violation_is_logged_and_still_drainable`,
`violations_beyond_the_ring_are_counted`,
`test_sporadic_budget_exhaustion_suppresses_dispatch`, both `test_atomic_*`,
`a_tier_slower_than_its_declared_cadence_is_late`,
`test_spin_one_period_timed_no_overrun`, and
`test_spin_once_does_not_credit_timeout_to_timer_delta`. They now run in the
`alloc`-only test build as well.

That build had never compiled. On `origin/main` it fails with 30 errors, all
`dead_code` in private clocks whose test is gated off, so `mod $name` now
carries `#[allow(dead_code)]`. It runs 542 tests, all green.

Issue 1579 (open) named four sites of the same helper. All four were
already on private clocks (`timer_readiness_clock`, `exchange_period_clock`),
and the helper it quotes is now deleted.

### Mutation check

Each mutation was applied, run, and restored; `cmp` against the pre-mutation
copy was clean. The suite is `--features std,sim-time,param-services`.

| Mutation | Result |
| --- | --- |
| M1: private clock follows the WALL clock again (sleep, credit what was slept) | quiet: 2 red (`time_until_next_*`, `test_spin_timeout`). Stop loop: the four timer tests above went red 10 of 10. |
| M2: `spin_once` credits +1 ms "spin cost" on top of the clock delta | new tests: 6 red. Origin's tests: 3 red. `time_until_next_*`, `test_timer_does_not_fire_at_zero_delta` and `test_spin_timeout` catch it only after the fix, because origin's 50..=60 ms range admits 59 ms. |
| M3: the phase-100 bug (requested timeout credited to timers) | `test_spin_once_does_not_credit_timeout_to_timer_delta` red |

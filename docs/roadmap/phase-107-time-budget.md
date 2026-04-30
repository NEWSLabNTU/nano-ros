# Phase 107 — Wall-clock time budget per `drive_io`

**Goal:** add an optional `time_budget_ms` parameter to
`Session::drive_io` so time-triggered cyclic apps (avionics,
functional-safety) can budget a fixed CPU slice for ROS work per
schedule cycle. The backend checks elapsed wall-clock time after
each callback and returns when the budget is exhausted, even if more
work is pending.

**Status:** Not Started.
**Priority:** Low. Driven only by use cases like DO-178C-flavoured
deployments that allocate a fixed time slice per cycle. None
currently in tree.
**Depends on:** Phase 105 (`max_callbacks` cap), Phase 106
(timer / GC interleaving) — both shape the drive_io loop in which
the time check fits.

## Background

Time-triggered cyclic apps run a fixed schedule. Each cycle does a
fixed amount of work in a fixed time slot:

```
cycle 0:    sensor_read (2ms) → control_law (3ms) → ros_work (5ms) → idle (10ms)
cycle 1:    sensor_read (2ms) → control_law (3ms) → ros_work (5ms) → idle (10ms)
...
```

The 5 ms ROS slot must finish or yield. If it overruns, the next
cycle's sensor read is delayed, missing its deadline.

Phase 105's `max_callbacks` cap bounds how many callbacks fire per
`drive_io`, but each callback's WCET still varies — fast callbacks
fit the budget, slow ones don't. A wall-clock check after each
callback gives precise budget enforcement.

## Design

Add an optional `time_budget_ms` to the `drive_io` signature:

```rust
pub trait Session {
    fn drive_io(
        &mut self,
        timeout_ms: u32,
        max_callbacks: usize,
        time_budget_ms: Option<u32>,         // new
        timers: &mut dyn TimerScheduler,
        gcs: &mut dyn GuardConditionScheduler,
    ) -> Result<DriveStats, Self::Error>;
}
```

`None` = no budget check, current behaviour. `Some(ms)` = check
elapsed wall-clock after each fired callback; return when exceeded
even if `max_callbacks` not reached.

Backend's drive_io loop adds the check:

```rust
let start = clock_ms();
while fired < cap && now < deadline {
    if let Some(budget) = time_budget_ms {
        if clock_ms() - start >= budget { break; }
    }
    /* fire one callback (sub / service / timer / gc) */
    fired += 1;
}
```

Cost: one clock read per iteration. ARM has DWT cycle counter
(constant-time read). ESP32 has `esp_timer_get_time` (microsecond
resolution, fast). POSIX has `clock_gettime(CLOCK_MONOTONIC)`. All
fast enough not to be the bottleneck.

C vtable: `time_budget_ms` parameter added (use 0 for "no budget"
since the C ABI doesn't naturally express `Option<u32>`; or use a
separate flag).

## Work Items

- [ ] **107.1 — Add `time_budget_ms` parameter to
      `Session::drive_io`.** Optional in Rust trait
      (`Option<u32>`), use 0 sentinel in C vtable for "no budget".
      **Files:** `packages/core/nros-rmw/src/traits.rs`,
      `packages/core/nros-rmw-cffi/include/nros/rmw_vtable.h`,
      `packages/core/nros-rmw-cffi/src/lib.rs`.

- [ ] **107.2 — Backend impls of the time check.**
      Per-backend `clock_ms()` source. `nros-platform-api::PlatformClock`
      already provides this — backends thread it through.
      **Files:** all four backends.

- [ ] **107.3 — `ExecutorConfig::time_budget_per_spin_ms`.**
      Optional. Default `None` (no budget). Apps that want
      time-triggered behaviour set it.
      **Files:** `packages/core/nros-node/src/executor/config.rs`.

- [ ] **107.4 — Test coverage.**
      Configure `time_budget_per_spin_ms = 5`, register a
      slow-callback subscription (artificial 3 ms work), publish 10
      messages. Assert that two `spin_once` calls each fire one
      callback (since 1 callback = 3 ms < 5 ms budget; 2 callbacks
      = 6 ms > 5 ms budget). Drains over multiple iterations.
      **Files:** `packages/testing/nros-tests/tests/`.

- [ ] **107.5 — Book updates.**
      Add to `book/src/concepts/rtos-cooperation.md` the
      time-triggered cyclic profile + time-budget configuration.

## Acceptance Criteria

- [ ] Default behaviour (`time_budget = None`) identical to Phase 106
      behaviour — no regression.
- [ ] `time_budget = Some(5)` test demonstrates spin_once respects
      the budget across multiple iterations.
- [ ] No measurable overhead on the `time_budget = None` path
      (no clock reads when feature unused).

## Notes

- **Why optional?** Most apps don't need it. The clock-read
  overhead is real (10–50 ns per read on typical embedded CPUs);
  apps that don't need time-triggered behaviour shouldn't pay.
- **Combine with `max_callbacks`?** Yes — `drive_io` returns when
  *either* cap is reached. Time budget AND max_callbacks together
  give belt-and-suspenders bounding.
- **Why a wall-clock check, not a deadline timer?** Wall-clock check
  is a polling read — cheap and bounded. Setting a hardware
  deadline timer requires per-call timer setup which can cost more
  than the check itself.
- **What if a single callback exceeds the budget?** The budget is
  checked *after* the callback. A 10 ms callback on a 5 ms budget
  still runs to completion; drive_io returns afterwards. Apps that
  need preempt-during-callback need a different mechanism (cooperative
  yield API exposed to user callbacks; out of scope for this phase).

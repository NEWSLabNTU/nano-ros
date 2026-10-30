---
id: 2143
title: "A host that drives the executor cannot ask when it next needs to run: `next_wake_bound_us` is `pub(crate)`"
status: resolved
resolved: 2026-10-30
type: enhancement
area: [core, executor]
severity: medium
found: 2026-10-29
related: [2144, 2145]
---

## What

RFC-0112 D2's Tier 0 contract needs a **Time out** part: a host loop (tokio,
GLib, Arduino `loop()`, a PX4 work item, a plain C `while`) that calls
`spin_some` must know how long it may sleep before the executor next needs to
run.

The executor already computes exactly that. `next_wake_bound_us` (RFC-0106) is
the minimum over the caller's budget, the next timer, the backend's next event,
a supervised route's deadline and every registered wake source. But it is
`pub(crate)`
(`packages/core/nros-node/src/executor/spin.rs:4395`, and
`next_wake_bound_attributed_us` at `:4486`).

What is public covers only parts of that minimum:

- `route_next_due_us()` (`:4332`) gives supervised routes only.
- `timer_time_until_next_call_ns(id)` (`:11798`) gives one timer per call, so
  the host must enumerate the timers itself and still misses the backend and
  the wake sources.

So today a host either polls on a fixed period, which oversleeps a timer or
wakes the CPU for nothing, or calls `spin_once(t)` with a guess and gives the
executor the thread.

## Direction (RFC-0112 D2.2)

Add `Executor::next_deadline_us() -> Option<u64>`, a public wrapper over
`next_wake_bound_us` with no caller budget. `None` means nothing is scheduled,
so the host waits on readiness alone (issue 2144). Export it through the C and
C++ APIs in the same change. Add a test that registers a 10 ms timer and checks
for a value of at most 10 ms, plus a lower-bound check, as CLAUDE.md requires
for timing.

## Resolution

`Executor::next_deadline_us() -> Option<u64>` is public
(`packages/core/nros-node/src/executor/spin.rs`). It is
`next_wake_bound_attributed_us` with a `u64::MAX` budget and the backend's
`next_deadline_ms`, so it is the same `min` the park takes: next wall timer,
backend event, supervised route, registered wake sources. `None` when the
budget wins, i.e. nothing named a deadline. It works on `no_std` (no `alloc`
needed).

Exported in the same change:

- C: `rclc_executor_get_next_deadline_ns(executor, &next_ns)` — nanoseconds
  like rclc's spin budgets, `UINT64_MAX` for none.
- C++: `rclcpp::Executor::next_deadline_us(uint64_t&)` over
  `nros_cpp_executor_next_deadline_us`, `UINT64_MAX` for none.

**One defect fixed on the way, in the shared bound.** A timer's remainder
(`period - elapsed`) is as of the last spin's END, where `elapsed_us` was last
credited. A host asking after its own work would get a value that overstates
the wait by the time since that spin; and `spin_once`'s own park had the same
gap, so a caller that slept between spins had the next park oversleep a timer
by exactly that long (the band prelude already corrected for it, the
single-worker path did not). The timer member of
`next_wake_bound_attributed_us` now subtracts the time since the last spin
ended (`since_last_spin_end_us`), so both the public answer and the park are
measured from now. Without a clock the correction is 0 and the value is an
upper bound, as before.

Tests (`packages/core/nros-node/src/executor/tests.rs`): a 10 ms timer answers
exactly 10 ms on a held clock (upper and lower bound), 6 ms after the clock
moves 4 ms with no spin, 0 when overdue; `None` with nothing scheduled and
with only a cancelled timer; a wake source reaches the answer;
`spin_once_park_counts_time_spent_between_spins` pins the park fix. Seven
existing exact-bound tests moved to private clocks, since a free-running
clock now shaves microseconds off a measured-from-now bound. C:
`nros-c/tests/run/timer_clock_source.c` section 5 (ROS-time timers alone give
`UINT64_MAX`; a 10 ms wall timer is in [1 ms, 10 ms]; NULLs refused). C++:
`nros-cpp/tests/compile/executor_node_members_runtime.cpp`, same bounds.

Left for issue 2144: the readiness half. `None` tells the host to wait on
readiness, and there is no executor-level notifier yet.

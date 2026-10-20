---
id: 1974
title: "Jazzy timer additions: call info (expected vs actual time) and autostart (init2) have no counterpart"
status: resolved
type: enhancement
area: api
severity: low
found: 2026-10-19
related: [rfc-0036, issue-1975]
resolved_in: 2026-10-20
---

## What

Jazzy's rcl/rclc/rclcpp timer surface grew, recorded in
`docs/reference/api-surface/jazzy/`:

- `rcl_timer_call_info_t {expected_call_time, actual_call_time}` and
  `rcl_timer_call_with_info`; rclcpp `TimerInfo` and `TimerInfoCallbackType`
  (a timer callback that receives that struct);
- `rcl_timer_init2(..., bool autostart)` and `rclc_timer_init_default2`
  (create a timer CANCELLED, start it later with reset).

## Ruling: ADOPT

Both fit the embedded executor with no allocation. Expected-vs-actual call
time is the jitter number a real-time user measures by hand today (the
executor already computes the due time — `route_next_due_us` — it just does
not hand it to the callback). Autostart is a boolean on an init we already
have; `reset` exists (`cpp:Timer::reset`).

Ruled separately and DECLINED (ledger rows, not this issue):
`rcl_timer_set_on_reset_callback` / `Timer::set_on_reset_callback` — plumbing
for the EventsExecutor to learn a timer was reset; our executor recomputes the
next due time from the timer itself.

## Work

1. A `*_with_info` timer callback flavour in C/C++/Rust carrying both times.
2. An autostart flag on the timer init entry points.
3. Delete the `gap` rows (`c:timer_call_info_t`, `c:timer_call_with_info`,
   `c:timer_init2`, `c:timer_init_default2`, `cpp:TimerInfo`,
   `cpp:TimerInfoCallbackType`).

## Resolution

**One producer.** The executor's timer dispatcher computes the call info at the
moment it decides an activation is due (`arena::timer_call_info`):
`actual` = now on the timer's own clock (the reading `timer_clock_step` just took
for a `Ros`/`System` timer; the platform steady clock for `Steady`), `expected` =
`actual - (elapsed_us - period_us)`. So `actual >= expected` always, and under
`CatchUp` each replay reports its own deadline. Every timer entry stores an
`FnMut(&TimerCallInfo)`; a plain `FnMut()` is adapted by ignoring it, so there is
one entry type, one dispatch and no new arena cost (`TIMER_ENTRY` unchanged).
C, C++ and Rust copy the two numbers (`From<TimerCallInfo>`); none computes its
own. `TimerCallInfo` also names the clock, which is how C++ builds
`rclcpp::Time(ns, clock_type)` without growing the capture.

**Autostart** is `cancelled = !autostart` at registration; `reset_timer` starts
it. `reset_timer` now re-seeds a clock timer's last reading — without that a
`Ros`/`System` timer reset after any cancelled interval credited the whole
interval on its first poll and fired at once (a latent bug for every
cancel/reset, not just autostart).

| Layer | Call info | Autostart |
|---|---|---|
| Rust | `TimerCallInfo {expected_call_time, actual_call_time, clock}` (+`lateness_ns()`); `Executor::register_timer_with_info[_on]`, `Node::create_timer_with_info` | `autostart: bool` on those verbs |
| C | `rcl_timer_call_info_t` (rcl's name and fields); `rclc_timer_callback_with_info_t` + `rclc_timer_init_with_info(timer, clock\|NULL, period, cb, ctx, autostart)` | `rclc_timer_init_with_context2` / `rclc_timer_init_on_clock2` (our `rclc_timer_init_default2` / `rcl_timer_init2`) |
| C++ | `rclcpp::TimerInfo`; any callable invocable as `void(const TimerInfo&)` passed to `create_wall_timer` / `rclcpp::create_timer`; `TimerInfoCallbackType` = `InplaceFn<void(const TimerInfo&)>` | Jazzy's trailing `(group, autostart)` on both verbs |

Divergences (ledger `timer.json`): C names stay off `rcl_timer_init2` /
`rclc_timer_init_default2` for the callback-contract reason
`rcl_compat.h` §4 gives; `rcl_timer_call_with_info` is DECLINED (a dispatch
call, like `rcl_timer_call`) and the info rides on the callback instead;
`TimerInfoCallbackType` is move-only (`InplaceFn`, no allocator).
`rcl_timer_t` gained `info_callback` APPENDED (offsets unchanged). C
`rclc_executor_add_timer[_in_group]` are now one body, which also fixed the
in-group verb silently dropping the timer's clock (a ROS-time timer added to a
group ran on the wall). The C++ FFI adds
`nros_cpp_timer_create_capturing_with_info`; the old entry points are kept.

Tests: `nros-node` executor tests (lateness equalities on a private spin clock,
autostart, ROS-clock info + reset re-seed, node-level verb);
`nros-c/tests/run/timer_clock_source.c` §4 (info equalities on the ROS clock,
autostart, grouped ROS timer stays quiet while `/clock` is paused);
`nros-cpp/tests/compile/timer_call_info_runtime.cpp` (TimerInfo equalities on
the ROS clock, autostart, wall lower bound, `TimerInfoCallbackType`).

Not covered: the declarative `nros::Node` (`DeclarativeNode`) timer, which
dispatches through `ExecutableNode::on_callback` rather than a closure; nothing
asked for it, and it would need its own `Callback` payload.

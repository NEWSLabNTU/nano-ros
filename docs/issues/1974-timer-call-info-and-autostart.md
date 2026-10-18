---
id: 1974
title: "Jazzy timer additions: call info (expected vs actual time) and autostart (init2) have no counterpart"
status: open
type: enhancement
area: api
severity: low
found: 2026-10-19
related: [rfc-0036]
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

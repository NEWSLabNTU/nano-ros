---
id: 2039
title: "No interop cell drives the C API's matched and status-event callbacks against a stock ROS 2 peer"
status: resolved
resolved: 2026-10-25
type: enhancement
area: [api, testing, interop]
severity: low
found: 2026-10-25
related: [1972, 1993, 2060, phase-488]
---

## What

`packages/api/nros-c/src/event.rs` exports nine per-event setters:

- `rclc_publisher_set_matched_callback` / `rclc_subscription_set_matched_callback`;
- `..._set_incompatible_type_callback`;
- the QoS status events (`..._liveliness_changed`, `..._requested_deadline_missed`,
  `..._message_lost`, `..._liveliness_lost`, `..._offered_deadline_missed`).

What exercises them today, measured on main:

- **C, no peer.** `packages/api/nros-c/tests/compile/matched_events_entry_points.c`
  (declarations) and `tests/run/status_event_setters.c` (stub backend).
- **Backend only.** `packages/rmw/zenoh/nros-rmw-zenoh/tests/matched_events_live.rs`.
  Its peer is a second nano-ros process at the RMW trait level, not a stock
  ROS 2 node, and not through the C API.
- **Cyclone C++ unit test.** `nros_rmw_cyclonedds_matched_events`.
- **Interop, Rust only.** `interop::CELLS` has `native-advertised-state-rust-cyclone-bidir`
  (matched COUNTS, Rust) and `native-qos-event-rust-zenoh-r2n` (`LivelinessChanged`,
  Rust).

No cell has a stock `ros2` peer (rclcpp / rclpy) raise a matched or status
event that a C application receives through these callbacks. The C path adds
nros-c's process-wide `status_event_slots` table and the generation-tagged
slot reference between the backend and the user's callback. That layer has
never met a live foreign peer.

## Done when

An `interop::CELLS` row (C, zenoh and/or cyclonedds) whose test starts a
stock ROS 2 publisher and subscriber and asserts:

- the C matched callbacks fire with `current_count` rising and falling;
- at least one status event (e.g. `LivelinessChanged`) reaches a C callback.

The cell runs in the lane that already runs the Rust QoS-event cell.

## Resolution

Fixed 2026-10-25. There is a new interop cell, `native-qos-event-c-zenoh-bidir`
(`c(Linux, C, Zenoh, QosEvents, Interop, Runtime)`, `BiDir`). It lives in the
binary that already runs the Rust QoS-event cell, `tests/qos_event_interop.rs`,
so it runs in the same lane. Both of that binary's cases now state both
coordinates in `interop::assert_test_bound`, and `interop::CASE_CELLS` maps
each case to its own cell.

**The probe.** `bins/matched-events-probe-c` is a C leaf with fixture row
`matched-events-probe-c`. It holds a publisher on `/i2039_out` and a
subscription on `/i2039_in`, both `std_msgs/String`. It registers three
callbacks through the public setters:

- `rclc_publisher_set_matched_callback`;
- `rclc_subscription_set_matched_callback`;
- `rclc_subscription_set_liveliness_changed_callback`.

It prints each status (`nros_tests::output::MATCHED_PROBE_*`).

**The case.** `c_matched_and_liveliness_callbacks_follow_stock_ros2_peers` runs
in two phases.

- Phase 1 starts a stock `ros2 topic echo /i2039_out` and a stock
  `ros2 topic pub /i2039_in`. It asserts that the publisher's matched
  `current` is at least 1 with a positive `change`, that the subscription's
  matched `current` is at least 1 with a positive `change`, and that a
  `LivelinessChanged` arrives with `alive_change > 0`.
- Phase 2 stops both peers and asserts that the publisher's matched `current`
  falls to 0 with a negative `change`.

The case ran LIVE in the `ros2` distrobox: ROS humble with `rmw_zenoh_cpp`,
using `rmw_zenohd`. It passed 3 times out of 3, in about 3.5 s each run.

**Negative control (measured).** Making the C status-event slot table's
generation check always fail (`!= generation.wrapping_add(1)` in
`nros-c/src/event.rs`'s trampoline) stops anything reaching the callbacks. The
case then fails at phase 1.

## Still open

- **[issue 2060](../2060-subscription-events-unpolled-without-arrivals.md).**
  The SUBSCRIPTION's matched count never falls when the stock publisher leaves.
  The zenoh shim polls subscription statuses inside `has_data`, and the
  executor skips `has_data` for a wake-attributed slot with no arrival. The case
  prints whether that fall arrived instead of asserting it. Until 2060 is fixed,
  the subscription half of "rising and falling" is only "rising".
- **Cyclone.** It still delivers no status event to an application (issue
  1164, the carved cell), so this cell is zenoh only.

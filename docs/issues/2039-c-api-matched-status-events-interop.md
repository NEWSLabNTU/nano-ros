---
id: 2039
title: "No interop cell drives the C API's matched and status-event callbacks against a stock ROS 2 peer"
status: open
type: enhancement
area: [api, testing, interop]
severity: low
found: 2026-10-25
related: [1972, 1993, phase-488]
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

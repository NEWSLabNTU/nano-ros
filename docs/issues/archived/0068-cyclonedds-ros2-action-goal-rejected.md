---
id: 68
title: CycloneDDS ROS2 action interop — nano C action server rejects the ROS2 client goal ("Goal was rejected")
status: resolved
type: bug
area: rmw
related: [phase-248, phase-249, issue-0067]
resolved_in: "2026-06-15 — remove the stale goal_id len-prefix insert in service.cpp split_wire_header (233.6 completion)"
---

> **RESOLVED (2026-06-15).** Root cause: an incomplete Phase-233.6 migration.
> 233.6 switched the action core to read/write `goal_id` as a bare `uint8[16]`
> (no `uint32(16)` length prefix) and dropped the `subscriber.cpp` insert mirror —
> but **missed** the service-request receive path. `service.cpp::split_wire_header`
> still called `insert_goal_id_len_at(...)` unconditionally for `_SendGoal_Request_`
> / `_GetResult_Request_`, re-inserting a `10 00 00 00` prefix before the UUID. A
> real `rcl_action` (rmw_cyclonedds_cpp) client sends the bare array, so after the
> 16-byte request-header strip the payload already matched — the insert shifted
> `order` 4 bytes (read the UUID tail as `order` → out-of-range → "Goal was
> rejected").
>
> Captured the wire bytes to confirm: `00 01 00 00 | 10 00 00 00 | <16-byte uuid> |
> 05 00 00 00` — the `10 00 00 00` is the spurious prefix; the uuid matched the
> client's reported goal ID and `order=5` sat right after it.
>
> **Fix:** drop the `insert_goal_id_len_at` call in `split_wire_header` (pass the
> stripped payload through unchanged) and delete the now-unused helper. The send
> path was already bare-correct (the framework writes no prefix post-233.6, so the
> mirror `strip_goal_id_len_at` on send is a no-op). Symmetric on both sides now:
> nano↔nano and nano↔rcl agree on the bare `uint8[16]` goal_id.
>
> **Validated:** `cyclonedds_ros2_interop` 5/5 PASS (incl.
> `test_cyclonedds_action_nano_server_ros2_client`); manual repro shows the server
> accepts `order=5`, executes, succeeds, and the stock `ros2 action send_goal
> --feedback` client gets feedback + the Fibonacci result.

## Symptom

`cyclonedds_ros2_interop::test_cyclonedds_action_nano_server_ros2_client` fails
(3/3 retries) with the stock ROS 2 client reporting:

```
Sending goal:
Goal was rejected.
```

The test puts a nano-ros **C** CycloneDDS Fibonacci action server (`c_action_server`,
`build_native_c_example_rmw("action-server", …, Rmw::Cyclonedds)`) against a stock
`ros2 action send_goal /fibonacci … "{order: 5}"` over `rmw_cyclonedds_cpp`. The
client reaches the server (it gets a goal-response) but the response is REJECT.

## Scope — distinct from #67

This is **not** the rust-typed-publisher bug ([#67](0067-rust-typed-cyclonedds-publisher-creation-fails.md),
fixed): the server here is the **C** binary using the static `descriptors.cpp`
table, unaffected by the rust descriptor-hook marker. CycloneDDS ROS 2 **pub/sub**
interop passes (`test_cyclonedds_ros2_to_nano_pubsub` PASS), so discovery + the
basic wire path work — the failure is action-goal-handling specific.

CI-invisible: the host-integration light lane does not build the cyclone extras
(`c_action_server`), so this `skip!`s there; it only surfaces with the extras built
locally + ROS 2 sourced.

## History

`cyclonedds_ros2_interop.rs`'s header records this test as PASSING at Phase 177.36
(the action server publishes `ros_discovery_info` + uses ROS 2 action QoS so
`rcl_action_server_is_available` succeeds). So it has **regressed** since — a
candidate is the phase-248/249 churn around QoS / action-channel wiring.

## Direction (not started)

1. Capture the C action server's stderr during the test (does its `handle_goal`
   run + return accept, or does the goal request never deserialize?).
2. Check the goal/result/cancel service + feedback/status pub QoS against the ROS 2
   action client's expectations (the 177.36 fix was status=RELIABLE+TRANSIENT_LOCAL).
3. Bisect across the phase-248/249 cyclone QoS/registration commits on this repro
   (`just cyclonedds setup` or `just native build-fixture-extras` + ROS 2 Humble).

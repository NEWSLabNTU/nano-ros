---
id: 68
title: CycloneDDS ROS2 action interop — nano C action server rejects the ROS2 client goal ("Goal was rejected")
status: open
type: bug
area: rmw
related: [phase-248, phase-249, issue-0067]
---

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

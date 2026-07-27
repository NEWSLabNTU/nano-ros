---
id: 292
title: "nano-ros zpico action SERVER not discovered by a rmw_zenoh_cpp 0.2.9 client (action liveliness/graph tokens mismatch)"
status: open
type: bug
severity: medium
area: rmw
related: [phase-311, phase-41, 291]
---

## Summary

A nano-ros zpico **action server** does not appear in a stock jazzy
`rmw_zenoh_cpp` (0.2.9) client's graph: `ros2 action list` is empty and
`ros2 action info /fibonacci` reports **0 action servers** while the nano-ros
server is running and its session is open. Consequently `ros2 action send_goal`
finds no server and hangs.

Every OTHER zenoh interop direction works against the same stock jazzy peer
(verified in the phase-311 `ros_editions_zenoh` lane):

- pub/sub both directions ✅
- service both directions ✅
- action CLIENT (nano-ros → ROS server) ✅
- action SERVER (ROS → nano-ros) ❌ — this issue

## Root cause (hypothesis)

This is NOT the #0291 type-hash keyexpr problem — that is fixed and proven for
pub/sub + service + the action-client direction (the RIHS01 tail matches live
jazzy). The action-server direction additionally needs the **action-entity
liveliness/graph tokens** that `rmw_zenoh_cpp` 0.2.x uses to build the action
graph: the send_goal / get_result service queryables + the status/feedback
pub/sub, each with its own liveliness token and RIHS01 service hash (an action
carries nine hashes). zpico's action-server does not emit these in the form
0.2.9's graph_cache recognizes, so the server is invisible to discovery and the
client's `send_goal` query resolves to no queryable.

Reference: `rmw_zenoh_cpp` graph construction —
`rmw_zenoh_cpp/src/detail/graph_cache.cpp` + `liveliness_utils.cpp` (studied at
tag 0.2.10 during the #0291 investigation).

## How it surfaced

phase-311 W5 (the zenoh × ROS-edition interop lane). Five of the six
`ros_editions_zenoh` tests pass against a live jazzy `rmw_zenoh_cpp` peer;
`ros_client_to_nano_action_server_zenoh` is `#[ignore]`d pending this fix.

## Impact

- A ROS 2 (jazzy `rmw_zenoh_cpp`) node cannot drive a nano-ros zenoh action
  server. The reverse (nano-ros action client → ROS server) works.
- Scope is zenoh-only: the same direction over cyclone and XRCE passes
  (phase-310 / phase-311 xrce lanes green).

## Fix direction

Align the zpico action-server liveliness/graph tokens with rmw_zenoh 0.2.x:
emit the send_goal/get_result service entities + status/feedback entities with
the `@ros2_lv/…` liveliness schema and the per-entity RIHS01 hashes the engine
already computes (phase-41/304 W1c produced all nine action hashes). Cross-check
against `graph_cache.cpp`'s expected entity set. Then un-ignore
`ros_client_to_nano_action_server_zenoh`.

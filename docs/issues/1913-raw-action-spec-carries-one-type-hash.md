---
id: 1913
title: "A C or C++ action carries ONE type hash for its five endpoints, so no iron/jazzy action server or client can match a ROS 2 peer"
status: open
type: bug
severity: medium
area: [api, codegen, rmw-zenoh]
related: [issue-1909, issue-1893, issue-0292]
found: 2026-10-15
---

## Measured

Issue 1909 made the C and C++ emitters compute the real REP-2011 hash for every
message and service. Actions were left on the phase-304 placeholder
(`RIHS01_0000…`), because no single value would be right.

An action is five endpoints, and an iron/jazzy `rmw_zenoh_cpp` peer keys each
one on a DIFFERENT hash: the `SendGoal` and `GetResult` service hashes, the
standard `action_msgs/srv/CancelGoal` hash, the `FeedbackMessage` hash and the
standard `action_msgs/msg/GoalStatusArray` hash. The Rust typed path carries
them separately (`RosAction::SEND_GOAL_SERVICE_HASH` /
`GET_RESULT_SERVICE_HASH`, issue 0292). The raw path the C and C++ APIs reach
does not:

* `nros_node::RawActionServerSpec` has one `type_hash`, and
  `register_action_server_raw_sized` (`packages/core/nros-node/src/executor/action.rs`)
  passes it to all five `ServiceInfo` / `TopicInfo`s.
* `nros_cpp_action_server_create` / `nros_cpp_action_client_create` /
  `nros_cpp_action_server_init_polling` take one `type_hash`, and the C++
  headers hand them `A::Goal::TYPE_HASH`.
* The C action type support has one `.type_hash` (`packs/c/action.c.jinja`).

So the placeholder is wrong for all five endpoints, and any one computed hash
would still be wrong for four of them.

## What would close it

Carry the per-endpoint hashes from codegen to the raw spec, the way the Rust
typed path already does: the C/C++ action templates emit the five (the
generator already computes `ActionTypeHashes` through `ResolvedAction`), the
FFI entry points take them, and `RawActionServerSpec` / the client spec stop
reusing one value. That changes the C ABI of three entry points, so it is its
own change rather than part of 1909. Acceptance: a C++ action server built for
jazzy accepts a goal from a Jazzy `ros2 action send_goal`
(`just ros_editions image jazzy` gives one on a Humble host).

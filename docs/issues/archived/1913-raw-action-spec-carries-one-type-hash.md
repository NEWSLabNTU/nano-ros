---
id: 1913
title: "A C or C++ action carries ONE type hash for its five endpoints, so no iron/jazzy action server or client can match a ROS 2 peer"
status: resolved
type: bug
severity: medium
area: [api, codegen, rmw-zenoh]
related: [issue-1909, issue-1893, issue-0292, issue-1918]
found: 2026-10-15
resolved: 2026-10-15
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

## Resolution (2026-10-15)

**The defect was wider than C and C++.** Measured first: the Rust
`ros_editions_e2e` zenoh action cells, against a Jazzy peer
(`just ros_editions image jazzy`, `rmw_zenoh_cpp` 0.2.11), failed BOTH
directions. The Rust server advertised the action hash `RIHS01_9508051d…` on all
five endpoints, although the generated `Fibonacci` carries the correct
`SEND_GOAL_SERVICE_HASH` (`RIHS01_d1a57fb2…`) and `GET_RESULT_SERVICE_HASH`. The
public `nros` API (`node.rs`, `node_runtime.rs`) now registers typed actions
through `RawActionServerSpec` / `RawActionClientSpec` with `A::ACTION_HASH`, so
issue 0292's per-endpoint fix survived only in an executor path users no longer
reach. And no path, 0292's included, keyed `cancel_goal` / `status` on their own
types.

One value, `nros_rmw::ActionEndpointHashes` (send_goal, cancel_goal, get_result,
feedback, status), now runs from codegen to every endpoint builder:

* **Codegen.** `rosidl-resolve` computes the two standard hashes from embedded
  canonical `action_msgs` definitions — `cancel_goal_service_hash()`,
  `goal_status_array_hash()` — pinned to Jazzy's shipped
  `action_msgs/{srv/CancelGoal,msg/GoalStatusArray}.json` by
  `action_msgs_cancel_and_status_match_jazzy`. `ActionTypeHashes` carries them
  and gained `uniform()`, which replaced four hand-spelled literals. The C/C++
  emitters compute every action hash (`c_action_type_hashes`, via
  `ResolvedAction`) where they wrote the placeholder.
* **Rust.** `RosAction` gained `CANCEL_GOAL_SERVICE_HASH` / `STATUS_MESSAGE_HASH`
  (default `ACTION_HASH`, so a hand-written impl still compiles); the template
  emits them. The raw specs gained a required `hashes`; the entity metadata the
  `nros` API records carries `action_hashes`, and replaying an action without it
  is a `Runtime` error rather than a silent single hash. Every builder — typed
  and raw, server and client, `executor/action.rs` and `executor/node.rs` —
  keys each endpoint on its own entry.
* **C.** `rclc_action_type_t` gained a trailing `endpoint_hashes`
  (`rclc_action_endpoint_hashes_t`); a NULL member falls back to `type_hash`,
  which is what an older generator's designated initializer leaves. The server
  and client copy it at init. **The C examples hand-built their descriptor from
  the type name and hash alone**, so they left it NULL and still failed: all 12
  standalone `examples/*/c/action-*` and `action-raw-goal-probe` now pass
  `ROSIDL_GET_ACTION_TYPE_SUPPORT(example_interfaces, action, Fibonacci)`, as
  the workspace examples already did (the three `*_serialized_size_max` fields
  they set are read by nothing but two Kani fixtures).
* **C++.** The four entry points that build endpoints take a
  `const nros_cpp_action_hashes_t*`; generated action types carry five
  `<Endpoint>_HASH` members, and `rclcpp::detail::action_endpoint_hashes<A>()`
  detects them, so an older generated type still compiles. The untyped
  component helpers pass NULL — they never had a hash.
* **Codegen version 16** (min stays 14): a new tree names members an older
  runtime lacks; an older tree still runs here on its single hash.

Gate: `raw_action_endpoints_are_keyed_on_their_own_hashes` (nros-node, via a
`MockSession` that records each entity's hash), mutation-checked by putting the
raw server's status back on `type_hash`.

**Receipts** (Jazzy router and peer in `nano-ros-ros:jazzy`, nano-ros on the
host, everything built for jazzy):

```
ros_editions_e2e zenoh (Rust, 6 cells)        6 passed   (actions failed both ways before)
C++ action server  <- ros2 action send_goal   Goal finished with status: SUCCEEDED
C   action server  <- ros2 action send_goal   Goal finished with status: SUCCEEDED
C++ action client  -> rclpy Fibonacci server  Result received: [0, 1, 1, 2, 3, 5, 8, 13, 21, 34, 55]
C   action client  -> rclpy Fibonacci server  Result received: [0, 1, 1, 2, 3, 5, 8, 13, 21, 34, 55]
```

The `zenoh_action_nano_to_ros` cell kept failing after the hashes were right,
for an unrelated reason: the nano-ros client sends its goal once, before the
containerised rclpy server is up, and logs "Goal accepted" for a send. The cell
now waits for the server's `SERVER READY` (as `xrce_ros2_interop.rs` does); the
client-side defect is issue 1918.

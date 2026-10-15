---
id: 1918
title: "The Rust action-client example logs \"Goal accepted by server\" when it has only SENT the goal, and sends once with no wait for the server, so a goal fired before the server exists is silently lost"
status: resolved
type: bug
severity: low
area: [api, examples, testing]
related: [issue-1913]
found: 2026-10-15
resolved: 2026-10-15
---

## Measured

Found while verifying issue 1913 against a Jazzy `rclpy` Fibonacci server
(`nano-ros-ros:jazzy`, `rmw_zenoh_cpp` 0.2.11, nano-ros client on the host).

`examples/linux/rust/action-client/src/lib.rs` calls
`ctx.send_goal_for_name::<FibonacciGoal, 32>("/fibonacci", &goal)` once and,
on `Ok`, logs `Goal accepted by server, waiting for result`. `send_goal_for_name`
(`packages/api/nros/src/node_runtime.rs`) serializes the goal and hands it to
`send_goal_raw`; `Ok` means the request was SENT, not that a server accepted it.

With the server started 6 s before the client, the result arrives. With no
wait, the client prints the same `Goal accepted by server` line and then waits
for a result that can never come — the goal went out before the server had
declared its queryables, and nothing retries:

```
[INFO] Goal accepted by server, waiting for result     # no server had seen it
(client killed at 30 s, no "Result received")
```

That is how `ros_editions_e2e`'s `zenoh_action_nano_to_ros` cell failed on every
run (consistently ~63 s) until 1913 made it wait for the server's
`SERVER READY` line, the same fix `xrce_ros2_interop.rs` already had. The test
now passes, but only because the test waits; a user's client does not.

## What would close it

The log line should say what happened (sent), and the declarative client should
not report progress it has not observed: either log acceptance from the
goal-response callback, or wait for the action server (as rclcpp's
`wait_for_action_server` does) before the one send. The second is the one a
user needs, since a single unretried send is lost whenever the client starts
first.

## Resolution (2026-10-15)

The declarative client now waits for the server before its one send, and says
what it did:

* `TickCtx::action_server_is_ready_for_name` (and `action_server_is_ready`) —
  the action twin of phase-428 W13's `service_is_ready`, with the same three
  answers, read off the action's `send_goal` service
  (`ActionClientCore::server_readiness`). `ClientDispatch` gained
  `action_server_is_ready` with the same "cannot say" default, so an
  implementor that predates it keeps its shape. A tick checks once and returns,
  the rclcpp `wait_for_action_server` idiom with the tick as the cadence.
* All eight `nros::node!` action-client examples (linux, mps2-an385 baremetal
  rtic + freertos, qemu-armv7a-nuttx, rv-virt-threadx, threadx-linux, zephyr,
  and the rust workspace package) gate the send on `Ok(false)` — `Err`, a
  backend with no discovery, still sends — log `Waiting for action server...`
  once, and log `Goal sent, waiting for result` instead of claiming acceptance.
  The C and C++ clients keep `Goal accepted by server`: their `send_goal` waits
  for the server's answer and branches on rejection (issue 0868), so for them it
  is true. No test asserts the accepted marker on a declarative Rust client.

**Regression test.** `ros_editions_e2e`'s `zenoh_action_nano_to_ros` cell no
longer waits for the rclpy server's `SERVER READY` (the wait 1913 added): the
client is spawned at once, before the containerised server exists. Before this
fix that cell failed on every run (~63 s); now 3 of 3 pass in ~1.5 s. By hand,
same race, no wait:

```
[INFO] Waiting for action server...
[INFO] Sending goal
[INFO] Goal sent, waiting for result
[INFO] Result received: [0, 1, 1, 2, 3, 5, 8, 13, 21, 34, 55]
```

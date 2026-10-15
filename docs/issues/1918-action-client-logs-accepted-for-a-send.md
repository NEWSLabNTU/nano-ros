---
id: 1918
title: "The Rust action-client example logs \"Goal accepted by server\" when it has only SENT the goal, and sends once with no wait for the server, so a goal fired before the server exists is silently lost"
status: open
type: bug
severity: low
area: [api, examples, testing]
related: [issue-1913]
found: 2026-10-15
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

---
id: 2483
title: "A nano-ros node with parameter services does not declare `use_sim_time`, which every stock node does"
status: open
type: bug
severity: medium
area: [params, interop]
found: 2026-11-13
related: [phase-556]
---

## What

Measured on 2026-11-13 (phase-556 first measurement, Humble,
`rmw_zenoh_cpp`, `--no-daemon`), nano-ros fixture `param-two-node-talker`
node `/alpha` against a stock node declaring the same `rate` and `stepped`:

```
$ ros2 param list /alpha            # nano-ros
  rate
  stepped
$ ros2 param list /alpha_stock      # stock
  rate
  stepped
  use_sim_time

$ ros2 param get /alpha use_sim_time
Parameter not set.
$ ros2 param set /alpha use_sim_time true
Setting parameter failed: Parameter was not declared and undeclared parameters are not allowed
```

A stock rclcpp or rclpy node declares `use_sim_time` (bool, default false) on
every node, and a launch file or `ros2 param set` switching a whole graph to
simulated time relies on that. On a nano-ros node the set is refused, so
the node stays on its own clock while the rest of the graph moves to `/clock`.
`ros2 param dump` differs by the same line.

rclcpp also declares four `qos_overrides./parameter_events.publisher.*`
read-only parameters; those are not part of this issue.

## Question for the fix

nano-ros has ROS time. Whether `use_sim_time` should switch it at runtime,
or be declared read-only with its build-time value, is a design choice. A
read-only declaration at least makes `list`, `get` and `dump` agree with
stock, and turns the set refusal into an honest "read-only" answer.

## Acceptance

A phase-556 test compares `ros2 param list` and `ros2 param get use_sim_time`
for a nano-ros node and a stock node and finds the same answer. Until fixed,
that test is held in the strict expected-failure form.

---
id: 2484
title: "The parameter services answer a request for an undeclared name with a filler entry, and refusal reasons differ from stock"
status: open
type: bug
severity: medium
area: [params, interop]
found: 2026-11-13
related: [phase-556]
---

## What

Measured on 2026-11-13 (phase-556 first measurement, Humble,
`rmw_zenoh_cpp`, `--no-daemon`): nano-ros node `/alpha` from fixture
`param-two-node-talker`, against stock rclcpp (`demo_nodes_cpp` talker) and
an rclpy node declaring the same parameters.

### Undeclared names get a filler entry instead of none

| request | nano-ros | stock (rclcpp and rclpy) |
|---|---|---|
| `GetParameters{names:[nope]}` | `values=[ParameterValue(type=0, ...)]` | `values=[]` |
| `DescribeParameters{names:[nope]}` | `descriptors=[ParameterDescriptor(name='nope', type=0, ...)]` | `descriptors=[]` |
| `ros2 param get /alpha nope` | `Parameter not set.`, exit 0 | `Parameter not set`, exit 1 |
| `ros2 param describe /alpha nope` | prints `Parameter name: nope`, `Type: not set` | prints nothing |

A script that checks `ros2 param get`'s exit status reads an undeclared name
as present. A client that pairs reply entries with requested names gets one
entry where stock gives none.

### Refusal reasons

| request | nano-ros | stock |
|---|---|---|
| `ros2 param delete /alpha rate` (statically typed) | `Type mismatch` | rclcpp: `cannot undeclare an statically typed parameter` |
| `ros2 param set /alpha stepped 7` (range 0..100, step 5) | `Value out of range` | rclpy: `... is not a valid step. Min: 0, max: 100, value: 7, step: 5` |

The verdicts agree (each is refused); the reasons do not. The off-step value
reported as "out of range" is the one a user is misled by.

## Acceptance

Phase-556 tests compare the raw `GetParameters` and `DescribeParameters`
replies and the `ros2 param get` exit status for an undeclared name, and the
delete and off-step refusal reasons, against stock. Until fixed, each is held
in the strict expected-failure form.

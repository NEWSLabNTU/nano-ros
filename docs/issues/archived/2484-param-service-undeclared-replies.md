---
id: 2484
title: "The parameter services answer a request for an undeclared name with a filler entry, and refusal reasons differ from stock"
status: resolved
type: bug
severity: medium
area: [params, interop]
found: 2026-11-13
resolved: 2026-11-18
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

## Resolution

Fixed in phase-556 W12, in `nros-node`'s parameter services
(`packages/core/nros-node/src/parameter_services.rs`).

- **Undeclared names, as a class.** `GetParameters`, `DescribeParameters` and
  `GetParameterTypes` all answered each unknown name with a filler entry
  (`GetParameterTypes` with type 0). Stock answers a request that names ANY
  undeclared parameter with an empty reply, in both client libraries:
  rclcpp's `NodeParameters::get_parameters`, `describe_parameters` and
  `get_parameter_types` throw `ParameterNotDeclaredException` and its
  parameter service catches it and replies with nothing; rclpy's
  `ParameterService` replies with the empty list on the same exception. The
  three streaming handlers now read the names once first
  (`names_an_undeclared_parameter`) and write the empty sequence when one is
  undeclared. A node that allows undeclared parameters keeps one `NOT_SET`
  entry per unknown name, which is what both libraries answer for such a
  node. `ListParameters` names only what is stored, so it had no filler
  shape.
- **Refusal reasons.** `refusal_reason` gives the stock reason where it says
  more than the store's verdict. The compatibility test's stock peer is
  rclpy, so the wording is rclpy's: a delete of a statically typed parameter
  reads `Static parameter cannot be undeclared` (rclcpp: `cannot undeclare an
  statically typed parameter`); an off-step value reads `The parameter value
  for <name> is not a valid step. Min: <min>, max: <max>, value: <v>, step:
  <step>` (float: "is not close enough to a valid step"), and an out-of-range
  value `Parameter <name> out of range. Min: <min>, Max: <max>, value: <v>`
  (float: rclpy's form without the `.` after "range"). rclcpp says
  `Parameter {<name>} doesn't comply with integer range.` for both range
  cases. Floats print in their shortest round-trip form, which matches
  Python's except in the exponent forms (`1e16` against `1e+16`).
- The reply buffer's bound prices the longer reasons: a range reason carries
  its parameter's name and four numbers, so `LONGEST_SET_REASON` counts the
  fixed text and four 24-character numbers, and the set reply adds every
  declared name once (`param_sizing::node_bound`).

Not changed: a refused `SetParametersAtomically` still carries the one
reason `One or more parameters could not be set`, where both stock libraries
return the first refused parameter's own reason. No `ros2 param` verb sends
it.

Plain tests in `ros2_cli_parity_interop`:
`get_parameters_reply_for_an_undeclared_name_answers_as_stock_does`,
`describe_parameters_reply_for_an_undeclared_name_answers_as_stock_does`,
`delete_of_a_statically_typed_parameter_is_refused_as_stock_does`,
`off_step_set_is_refused_as_stock_does`.

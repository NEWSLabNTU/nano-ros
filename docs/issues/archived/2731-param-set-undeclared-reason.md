---
id: 2731
title: "A set of an undeclared parameter is refused with rclrs's reason, where the stock peer the compatibility tests use (rclpy) gives its own"
status: resolved
type: bug
severity: low
area: [params, interop]
found: 2026-11-19
resolved: 2026-11-19
related: [phase-556]
---

## What

Measured on 2026-11-19 (phase-556 W2, Humble, `rmw_zenoh_cpp`,
`--no-daemon`), nano-ros fixture `param-two-node-talker` node `/alpha`
against the stock rclpy node `fixtures/ros2-param-peer/param_peer.py`:

```
$ ros2 param set /alpha nope 1          # nano-ros
Setting parameter failed: Parameter was not declared and undeclared parameters are not allowed
$ ros2 param set /param_peer nope 1     # stock rclpy
Setting parameter failed: ('Invalid access to undeclared parameter(s)', [])
```

Both exit 0, and both refuse the set. Only the reason differs. The three stock
client libraries word it three ways:

| client | reason |
|---|---|
| rclpy (measured above) | `('Invalid access to undeclared parameter(s)', [])` |
| rclcpp (`demo_nodes_cpp talker`, measured the same day on Fast DDS) | `parameter 'nope' cannot be set because it was not declared` |
| rclrs (`validate_parameter_setting`) | `Parameter was not declared and undeclared parameters are not allowed` |

nano-ros took rclrs's words on purpose (issue 1151,
`nros-node` `param_sizing.rs`, `UNDECLARED_REASON`). Every other refusal
reason the compatibility tests compare (a delete of a statically typed
parameter, a value out of range or off its step, a value of the wrong type)
now follows rclpy, because the tests' stock peer is rclpy (issue 2484,
phase-556 W12 and W2).

## Question for the fix

Which wording a set of an undeclared name should carry:

- rclpy's, which is the repr of a Python exception's arguments (note the
  empty list: rclpy's own formatting, not a name list), for consistency with
  the other reasons; or
- rclcpp's, which names the parameter and is what most stock nodes a user
  meets print; or
- rclrs's, as now, recorded in RFC-0036 as a by-design difference.

The reason is one constant; the decision is which client to copy.

## Acceptance

`ros2_cli_parity_interop` compares the refusal with a stock node's and finds
the same answer.

## Resolution

The user ruled on 2026-11-19 that every parameter refusal reason nano-ros
sends follows rclcpp's wording, and that this one is rclcpp's text, which
names the parameter. Fixed in phase-556 W2, in `refusal_reason`
(`nros-node`, `parameter_services.rs`):

```
$ ros2 param set /alpha nope 1          # nano-ros
Setting parameter failed: parameter 'nope' cannot be set because it was not declared
```

The same ruling moved the other reasons to rclcpp's words, each measured
against a running Humble rclcpp node: a read-only parameter (`parameter
'<name>' cannot be set because it is read-only`), a value of the wrong type
(`Wrong parameter type, parameter {<name>} is of type {integer}, setting it
to {string} is not allowed.`), a delete of a statically typed parameter
(`cannot undeclare an statically typed parameter`), and a value out of range
or off its step (`Parameter {<name>} doesn't comply with integer range.`,
`floating point range.` for a float).

rclrs's text stays as the fallback a result carries when the reply buffer has
no room for the named reason (a long parameter name; the buffer is priced at
the fallback's length).

The test was held as `set_of_an_undeclared_name_compared_with_stock` and is
now the plain `set_of_an_undeclared_name_is_refused_as_stock_does`, against
`turtlesim_node` (a shipped rclcpp node).

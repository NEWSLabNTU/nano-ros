---
id: 2903
title: "`ros2 lifecycle list` on a nano-ros managed node prints the primary state a transition settles in as its goal, where stock ROS 2 prints the transition state"
status: resolved
type: bug
severity: medium
area: [lifecycle, interop]
related: [phase-554]
found: 2026-11-23
---

## What happens

The lifecycle CLI parity cell (`lifecycle_cli_answers_as_stock_lifecycle_talker`)
asks the same `ros2 lifecycle list` of a nano-ros managed node and of stock
humble's `lifecycle_talker`, both unconfigured:

```text
- configure [1]
Start: unconfigured
- Goal: inactive
+ Goal: configuring
- shutdown [5]
Start: unconfigured
- Goal: finalized
+ Goal: shuttingdown
```

(`-` nano-ros, `+` stock.)

## Why

Stock `rcl_lifecycle` registers each public transition as an edge from a
primary state into its TRANSITION state (`configure`: unconfigured ->
configuring, id 10). The callback's result is a second, internal edge out of
that state. So `~/get_available_transitions` and `~/get_transition_graph`
advertise `configuring`. `nros-node`'s `write_transition_desc` wrote
`Transition::goal_state()`, the primary state a succeeding callback reaches.

## Resolution

`lifecycle_services::transition_goal_wire` names the transition state for the
seven public transitions: configuring (10), cleaningup (11), shuttingdown (12),
activating (13), deactivating (14). `ErrorRecovery` keeps `unconfigured`,
because it IS the internal `TRANSITION_ON_ERROR_SUCCESS` edge. The streaming
writer and the by-value oracle both use it, and
`available_transitions_advertise_the_transition_state_as_goal` pins the table.

Not changed: `rclc_lifecycle_transition_goal_state` in the C API still returns
the success destination, which is what its callers use it for.

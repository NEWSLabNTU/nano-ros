---
id: 2828
title: "No C lifecycle example or interop cell checks that a stock `ros2 lifecycle` client drives a C image"
status: resolved
type: test-gap
severity: medium
area: [c-api, lifecycle, testing, interop]
related: [issue-2429, issue-2626, phase-554]
found: 2026-11-22
resolved: 2026-11-22
resolved_in: "test(interop, issue 2828): a C lifecycle example, driven by stock `ros2 lifecycle`"
---

**Home: [phase-554](../../roadmap/phase-554-per-node-lifecycle.md) W9** (the
phase B acceptance cells), for the C row.

## What is missing

Issue 2429's fix names the C executor-level lifecycle machine after the
image's primary node. A C program's REP-2002 services are now
`/<node>/get_state` and so on, not `//get_state`. Mock-session unit tests in
`nros-node` (`lifecycle_services`) hold the five names and
`~/transition_event` to what `ros2 lifecycle` calls.

A name match is not proof that a stock ROS 2 client works against the image.
The request and reply types, the QoS, the discovery of the node and the
transition replies all have to agree with rclcpp's. Nothing runs that on a C
image:

- the tree has no C lifecycle example (`examples/linux/c/` has none);
- the zenoh lifecycle interop cell (`interop_e2e.rs`, `ZenohLifecycle`)
  drives the Rust example, which declares no node and keeps its configured
  `/lifecycle_demo`, so it exercises the zero-node path only.

## Wanted

- A Linux C lifecycle example: one node bound with `rclc_executor_node_init`,
  `rclc_executor_register_lifecycle_services`, and transition callbacks, in
  the shape of the Rust `lifecycle-node` example.
- An `interop::CELLS` row (zenoh first, then Cyclone DDS) that runs
  `ros2 lifecycle nodes`, `get`, `list` and `set` against it and compares
  each with the expected REP-2002 answers. It should also run
  `ros2 node list` to confirm there is no phantom node.

## Acceptance

The cell is green on zenoh and Cyclone DDS. With the executor-identity
naming put back (issue 2429's defect), it fails with "Node not found".

## Resolved, 2026-11-22

- `examples/linux/c/lifecycle-node` binds one node, registers the five
  services through `rclc_executor_register_lifecycle_services`, and prints
  a line from each transition callback.
- `tests/c_lifecycle_interop.rs` drives it with stock `ros2 lifecycle`. It
  is bound to the `interop::CELLS` row `native-lifecycle-c-zenoh` and
  checks two things:
  - `lifecycle nodes` and `node list` each show exactly
    `/c_lifecycle_node`;
  - `get`, `set` and `list` follow configure, activate, deactivate and
    cleanup, each running the example's own callback.

Green on zenoh with issue 2429's fix (2 of 2, 15.6 s). Negative control,
measured: on `main` before that fix the image served `//change_state`,
`//get_state` and the rest, and the test failed at its first step
(`ros2 lifecycle nodes` listed nothing).

Not done: the Cyclone DDS row. The example carries a `linux_cyclonedds`
image, and its cell waits for phase-554 W9's Cyclone rows.

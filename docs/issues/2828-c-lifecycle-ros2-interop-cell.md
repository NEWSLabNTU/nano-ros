---
id: 2828
title: "No C lifecycle example or interop cell checks that a stock `ros2 lifecycle` client drives a C image"
status: open
type: test-gap
severity: medium
area: [c-api, lifecycle, testing, interop]
related: [issue-2429, issue-2626, phase-554]
found: 2026-11-22
---

**Home: [phase-554](../roadmap/phase-554-per-node-lifecycle.md) W9** (the
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

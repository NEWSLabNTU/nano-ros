---
id: 2428
title: "A managed node keeps publishing while `unconfigured`: its publishers do not follow its lifecycle state, and no component can see that state"
status: open
type: bug
severity: medium
area: [runtime, lifecycle, book, education]
related: [2269, phase-537, rfc-0045, rfc-0128, phase-554]
found: 2026-11-11
---

**Home: [phase-554](../roadmap/phase-554-per-node-lifecycle.md) W6** (managed
publishers follow the node's state), narrowed 2026-11-22 to defect 3 below;
first filed under [phase-537](../roadmap/phase-537-book-user-guide-organization.md). **Blocks** the lifecycle lab of issue 2269 (phase-537 W5, absorbed from
phase-523 W1 lab 4). A lab that drives a node through REP-2002 transitions
and shows nothing change in what the node does would teach the wrong thing.

## Measured

2026-11-11, `main` 62f543fe33, CLI and `nros-launch-resolve` built from it.
The Tutorial's workspace through chapter 3 (`nros new sensor_ws --board
mps2-an385-baremetal --template talker --lang rust`, plus `monitor_node
--template listener --to sensor_node_bringup`), with one addition to the
bringup's `system.toml`:

```toml
[lifecycle]
autostart = "none"
```

`nros build` pulls in `nros-lifecycle-msgs` and the image boots under
`nros run`. From the host (`RMW_IMPLEMENTATION=rmw_zenoh_cpp`, router started
by `nros run`):

```text
$ ros2 lifecycle nodes
/nros_app
$ ros2 lifecycle get /sensor_node
Node not found
$ ros2 node list --no-daemon
/monitor_node
/nros_app
/sensor_node
$ ros2 lifecycle get /nros_app
unconfigured [1]
$ ros2 lifecycle set /nros_app configure
Transitioning successful
$ ros2 lifecycle set /nros_app activate
Transitioning successful
$ ros2 lifecycle get /nros_app
active [3]
```

Meanwhile the board's console shows `sensor_node` publishing
`Hello World: 1`, `2`, ... from boot, while the machine reads `unconfigured`.

So the transport is fine: the five REP-2002 services reach the board and
round-trip. Three things are wrong:

1. **One machine per image, not per node.** `NodeRuntime::apply_lifecycle`
   (`packages/api/nros/src/node_runtime.rs`) calls
   `executor.register_lifecycle_services()` once, so the image gets one
   `LifecyclePollingNodeCtx` (`packages/core/nros-node/src/lifecycle_services.rs`).
   A ROS 2 user manages a NODE (`ros2 lifecycle set /sensor_node ...`); a
   launch file with several managed nodes gives each its own state.
2. **The machine is named `/nros_app`,** a node the launch file does not
   declare, which then appears in `ros2 node list` beside the real ones.
   RFC-0045 names `/nros_app` showing up instead of the launch name as a
   symptom of the boot-config split it resolved; this site was not swept.
3. **No component can observe the state.** A component's surface is
   `Component::register(&mut NodeContext)` and
   `ExecutableNode::on_callback(.., &mut CallbackCtx)`. Neither offers the
   lifecycle state, transition callbacks or a managed publisher. The
   `LifecycleCallbacks` trait exists, but only at the `Executor` level
   (`Executor::register_lifecycle_node`), which the generated entry owns. So
   nothing in a node gates on `active`, and the runtime does not gate
   publishers either. The same holds for a C component: the features
   workspace's `c_lifecycle_talker_pkg/src/Talker.c` says the state machine
   "is handled by the generated entry ... this component has no lifecycle".
   C++ has `rclcpp_lifecycle::LifecycleNode` with `on_*` overrides and an
   activated publisher (`examples/workspaces/managed/.../ManagedTalker.cpp`),
   but that node drives its own machine from its constructor. It is not
   measured here whether `ros2 lifecycle set` reaches it.

Coverage: the only embedded lifecycle cell is `workspace-zephyr-rs-lifecycle`
(Zephyr native_sim, `autostart = "active"`), which asserts the services
answer, not that a node's behaviour follows the state.

## What a fix has to decide

- Per-node machines on the workspace road: one `LifecyclePollingNodeCtx` per
  component the launch file marks managed (ROS 2's own spelling is the
  component's class deriving from a lifecycle node; a `[[component]]` row or
  the launch `<node>` could carry it), each serving the five services under
  its own node name. What that costs in queryable slots: five per managed
  node, against `ZPICO_MAX_QUERYABLES`.
- The component surface for each language: transition callbacks on the
  component (`on_configure`/`on_activate`/... with rclcpp's defaults, as
  `LifecycleCallbacks` already states), and the state on `CallbackCtx`, or a
  managed publisher that drops while inactive, as rclcpp's
  `LifecyclePublisher` does.
- Whether `autostart` stays image-wide or moves per node.

That is a design decision (RFC-0045's boot config, or a new RFC beside it),
so this issue states the gap and the measurements, and does not choose.

## Design (2026-11-12)

User direction 2026-11-12: "I'd like to ensure the interface compatibility
among nano-ros and stock ROS 2" -- option A, REP-2002 as stock ROS 2 does it.
The design is [RFC-0128](../design/0128-per-node-lifecycle.md) (Draft): one
machine per managed node under its own name, publisher gating, per-node
`autostart`, five queryables per managed node, and callbacks in each language.
Phase B (machines and gating) unblocks the issue 2269 lab; phase C adds the
callbacks. The work is tracked in
[phase-554](../roadmap/phase-554-per-node-lifecycle.md).

## Next

Decide the design. The lifecycle lab of issue 2269 waits on it: its grading
line is a node that stays silent until `ros2 lifecycle set /<node> activate`
and then publishes.

## Status (2026-11-22): narrowed to defect 3

Defects 1 and 2 are resolved on `main` by phase-554 W1 and W2 (RFC-0128 D1,
D2 and D8):

- Each node the launch file marks `<lifecycle_node>` (or whose contract or
  C++ code makes it managed) has its own machine, served under its own name.
- Nothing registers on the executor's own identity, so there is no
  `/nros_app`.

Evidence: the workspace-features lifecycle cell
(`workspace_features_e2e.rs`, `Proof::LifecycleActive`, for Rust, C and
C++) requires `ros2 lifecycle nodes` to list exactly `/talker`, and
`ros2 lifecycle get /talker` to answer `active`.

Defect 3 stands, and is what this issue now tracks: a managed node's
publishers send in every state, `unconfigured` included, and a Rust or C
component cannot see the state or supply transition callbacks. The
exception is C++'s `LifecyclePublisher`. The book's
[Run managed nodes](../../book/src/guide/deploy/run-managed-nodes.md) page
already states both limits. The publisher half is phase-554 W6, and its
acceptance is W9's silent-until-active cell. The callback half is phase C
(W10 onward).

## Status (2026-11-27): the publisher half is accepted on embedded

The publisher half of defect 3 (phase-554 W6, in code since 2026-11-23) now
has an embedded cell. `tests/freertos_lifecycle_e2e.rs` boots a managed
talker on FreeRTOS QEMU over zenoh, in C, C++ and Rust
(`bins/lifecycle-freertos-mps2`, `[lifecycle] autostart = "none"`). A native
nano-ros lifecycle client (`bins/lifecycle-driver`) drives `configure`,
`activate` and `deactivate`. The cell asserts:
- `/chatter` is silent while `unconfigured` and while `inactive`;
- it publishes once `active` (4 messages in 4 s, measured);
- it is silent again after `deactivate`.

With the gate forced open, the unconfigured node published 3 messages and the
cell went red. It needs no ROS 2 install, so it is a baked `matrix::CELLS`
cell that the tier lanes run, not an interop row.

What this issue still tracks is the callback half: a Rust or C component
cannot see its node's state or supply transition callbacks (phase-554 phase
C, W10 onward).

---
id: 2626
title: "The executor-level lifecycle registration still names the executor's own identity, not the primary node"
status: resolved
type: tech-debt
area: [node, lifecycle, c-api, cpp-api]
severity: low
found: 2026-11-16
resolved: 2026-11-22
resolved_in: "fix(lifecycle, issues 2429 and 2626): the executor-level machine is served on the primary node"
related: [phase-554]
---

**Owner:** unassigned. **Home:** phase-554 (RFC-0128 D8). This was kept out
of W3 slice B on purpose, because it needs its own review.

## What happens

RFC-0128 D8 moves every executor-level entity to the image's primary node
(`Executor::entity_identity(None)`). Slice A moved the registration sites and
slice B moved the `/diagnostics` reporter and remap resolution. A generated
entry registers each lifecycle machine on its own component's node
(`nros_cpp_lifecycle_register_node`, `register_lifecycle_services_on`).

The hand-written executor-level entry point does not:
`Executor::register_lifecycle_services()` builds the five REP-2002 servers and
`~/transition_event` from `self.node_name` / `self.namespace`, which is the
executor's configured identity. Its callers are:

- `rclc_executor_register_lifecycle_services` (`packages/api/nros-c/src/lifecycle.rs`)
- `nros_cpp_register_lifecycle_services` (`packages/api/nros-cpp/src/lifecycle_shim.rs`)
- `Executor::register_lifecycle_node` (Rust)

In a hand-written multi-node image, these serve `/<configured>/change_state`
beside the declared nodes. That is the phantom node D8 removes.

## Why it is not a one-line change

The machine is keyed `None` (the executor's own). The executor-scoped
accessors (`lifecycle_state_machine_mut`, the transition-event accessor) look
up `None` first, and otherwise fall back to "the only machine there is".
Re-keying it to `Some(NodeId::PRIMARY)` changes what those accessors return as
soon as a second machine exists. A machine keyed `None` but named after the
primary node would collide with a later `register_lifecycle_services_on(PRIMARY)`.
Both are changes users can see, so this needs:

- a ruling on the key: re-key it, or keep `None` and refuse a second machine
  on the primary node;
- the accessors' behaviour stated and tested for one machine and for several;
- a changelog line if the key moves.

## Next step

Decide the key, then point the executor-level registration at the primary
node when one exists. A zero-node image keeps its configured name until
phase-554 W3 slice C makes that name its one node.

## Resolved, 2026-11-22

The key ruling is the second option above: the machine keeps the
executor-scoped key (`None`), and the primary node cannot get a second one.

- `register_lifecycle_services()` names the machine after the primary node
  when the image declares one (`entity_identity(None)`). A zero-node image
  keeps its configured name.
- Each machine records the node it is served on (`LifecycleEntry::served_on`)
  when it registers. A registration on a node that already serves a machine
  is refused (`LifecycleAlreadyRegistered`), whichever spelling came first.
- The executor-scoped accessors are unchanged: the `None`-keyed machine,
  else the only one. `lifecycle_state_machine_on(PRIMARY)` now also finds
  the executor-level machine, since it is that node's.

Held by the mock-session tests in `nros-node`'s `lifecycle_services`:
`the_executor_level_machine_is_served_on_the_primary_node`,
`an_unnamed_executor_with_a_node_serves_the_node_s_names`,
`a_machine_with_no_node_name_is_refused`,
`a_zero_node_image_keeps_its_configured_name`,
`the_primary_node_cannot_get_a_second_machine` and
`the_executor_level_machine_is_reachable_both_ways`. They assert the five
names `ros2 lifecycle` calls (`/<ns>/<node>/change_state`, `get_state`,
`get_available_states`, `get_available_transitions`,
`get_transition_graph`) and `~/transition_event`. With the executor-identity
naming put back, five of them fail.

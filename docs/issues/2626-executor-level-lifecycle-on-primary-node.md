---
id: 2626
title: "The executor-level lifecycle registration still names the executor's own identity, not the primary node"
status: open
type: tech-debt
area: [node, lifecycle, c-api, cpp-api]
severity: low
found: 2026-11-16
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

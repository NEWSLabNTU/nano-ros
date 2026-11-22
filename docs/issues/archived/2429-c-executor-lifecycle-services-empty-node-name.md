---
id: 2429
title: "The C API's executor-scoped lifecycle services are named `//get_state`: they register on the executor's identity, which is empty on the C road"
status: resolved
type: bug
severity: medium
area: [c-api, lifecycle, runtime]
related: [2428, 2304, phase-488, rfc-0128, phase-554]
found: 2026-11-11
resolved: 2026-11-22
resolved_in: "fix(lifecycle, issues 2429 and 2626): the executor-level machine is served on the primary node"
---

**Home: [phase-537](../../roadmap/phase-537-book-user-guide-organization.md),
beside [issue 2428](../2428-workspace-lifecycle-is-one-machine-per-image.md).** It is
the C road's form of 2428's defect, and its fix waits on 2428's ruling.

## Measured

2026-11-10, while fixing issue 2304 (PR #3493), on a scratch copy of a Linux C
lifecycle program on Cyclone DDS. `rclc_executor_register_lifecycle_services`
returned 0 once 2304's descriptor fix was in, but the five REP-2002 services
came up as `//get_state` (and the matching `//change_state`, ...), and
`ros2 lifecycle get <node>` answered `Node not found`. Not checked on zenoh.

## Why

`rclc_executor_register_lifecycle_services(executor)`
(`packages/api/nros-c/src/lifecycle.rs`) takes no node. It calls
`Executor::register_lifecycle_services()`
(`packages/core/nros-node/src/executor/spin.rs`), which names the services
after the EXECUTOR's own identity (`self.namespace`, `self.node_name`).
`register_lifecycle_services_named` builds the FQN as `"/" + node_name`, and on
the C road the executor's `node_name` is empty, because C creates its nodes
separately. So the FQN is `/` and every service is `//<suffix>`.

The C++ road hit the same thing in phase-488 (a phantom `/node`), and was fixed
by registering on the user's node: `register_lifecycle_services_on(node)`. The
Rust workspace road hits it as issue 2428's `/nros_app`. So three roads,
including this one, name the lifecycle machine after something that is not a
declared node.

## Why this is not fixed here

The name is not a plain join bug. `//get_state` is what the join produces for
an empty name, but the right name is a node's, and which node is 2428's open
question: one state machine per node (REP-2002, `ros2 lifecycle set
/<node>`), or one per image. For C:

- **per node**: the C API needs a node argument (a
  `rclc_..._register_lifecycle_services` that takes the `rcl_node_t`, as rclc's
  own lifecycle API does), or one registration per node it owns;
- **per image**: the C executor needs a declared, non-empty name to give the
  machine, and the services should never be built from an empty one.

Either way, `register_lifecycle_services_named` should refuse an empty
`node_name`, never join it into `//<suffix>`. That refusal alone would turn
today's unusable-but-working registration into a failure for every C lifecycle
program, so it lands together with 2428's choice and not before.

## Fix direction (after 2428's ruling)

- Register C lifecycle services on a declared node, by 2428's rule.
- `register_lifecycle_services_named` refuses an empty node name, naming the
  call.
- Add a Linux C lifecycle cell on Cyclone and on zenoh that checks
  `ros2 lifecycle get /<node>` succeeds. No C lifecycle cell checks the
  service NAMES today.

## Decided by RFC-0128 (2026-11-12)

[RFC-0128](../../design/0128-per-node-lifecycle.md) D8: lifecycle services are
named only after a declared node, `register_lifecycle_services_named` refuses
an empty name, and the executor-scoped C family retires in favour of upstream
`rclc_lifecycle`'s per-node API (D4). The refusal lands in its phase B.

## Resolved, 2026-11-22

`Executor::register_lifecycle_services()`, which
`rclc_executor_register_lifecycle_services` calls, names the machine after
the image's primary node (`entity_identity(None)`, RFC-0128 D8). Since
phase-554 W3.b every C node is bound (`rclc_executor_node_init`), so it is
in the executor's node table, and a C lifecycle program's services are
`/<node>/get_state` rather than `//get_state`.
`register_lifecycle_services_named` refuses an empty name with
`NodeError::LifecycleNodeUnnamed` (C: `RCL_RET_INVALID_ARGUMENT` with the
reason printed; C++: `NROS_CPP_RET_INVALID_ARGUMENT`; boot report class 21).

Not done here: a Linux C lifecycle cell that runs `ros2 lifecycle get
/<node>` on Cyclone and zenoh. The tree has no C lifecycle example to drive.
The zenoh interop cell drives the Rust example, which declares no node and
keeps its configured `/lifecycle_demo`.

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

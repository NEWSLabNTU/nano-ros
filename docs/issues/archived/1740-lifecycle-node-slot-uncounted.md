---
id: 1740
title: "A lifecycle image's node table is one slot short — the executor's own
  name is not counted, so an exactly-sized image dies at boot"
status: resolved
type: bug
area: [codegen, rmw]
severity: medium
found: 2026-10-07
related: [1709, 1676, 1688, phase-480]
---

## What was measured

Found while proving issue 1709 ([1709](1709-zenoh-tl-retain-depth-is-a-constant-one.md)).
Giving `examples/workspaces/features`' `native_rust_qos` a launch contract
(`rust_qos.contract.yaml`) made its pools EXACT instead of budgeted, and the
image died at boot before publishing once:

    nros: application error: Capability { name: "lifecycle",
      reason: "Transport::ConnectionFailed (no session to the router)" }

The router was up. Its sidecar stated `NROS_EXECUTOR_MAX_NODES = "2"` (the
descriptor's `node_count`), and `ros2 node list` on the uncontracted image
shows three names: `/node`, `/qos_listener`, `/reliable_talker`.

## Why

`register_lifecycle_services` creates the five REP-2002 servers and the
`transition_event` publisher on the EXECUTOR's node — the session's own name
(`node`), which in a multi-node image is none of the components'. So it
claims a node-table slot (`claim_node_slot`, `nros-rmw-cffi`) that
`EntityInventory::derive` never counted: `max_nodes` was the components plus
issue 1676's `/diagnostics` reporter, which lands on that same name. The
failure is `ConnectionFailed` from a full table — the shape 1676 measured for
the reporter. Every image WITHOUT a contract boots on the builtin of 4 slots,
which is why it was invisible.

## Resolution

Resolved 2026-10-07 on `fix/1709-tl-retain-depth-derived` (commit "fix(#1740)").
`max_nodes = components + (reporter || lifecycle)`: the two share the one
name, so one slot, never two. The liveliness node-token term is computed from
the named nodes, not from `max_nodes`, so token counts do not move (a node
table slot is not a liveliness token).

Measured: the same contracted image derives `NROS_EXECUTOR_MAX_NODES = "3"`,
boots, and serves `qos_override_e2e` (4/4). Test:
`entity_inventory::a_lifecycle_image_holds_a_node_slot_for_the_executor`
(lifecycle adds exactly one slot; the parameter family adds none).

Sweep: `git grep -n "max_nodes" -- packages/cli/nros-cli-core/src/entity_inventory.rs`

Not measured: a C/C++ lifecycle image sized exactly (same inventory, same
rule; no such image was built), and whether a single-node lifecycle image's
executor name ever equals its component's (then the slot is one over, the
safe direction).

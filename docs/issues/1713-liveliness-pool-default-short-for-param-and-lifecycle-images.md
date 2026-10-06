---
id: 1713
title: "A workspace image with param and lifecycle services overflows the
  undeclared ZPICO_MAX_LIVELINESS (16) — eight of its services are invisible
  to a stock ROS 2 peer"
status: open
type: bug
area: [rmw, codegen]
severity: medium
found: 2026-10-06
related: [1688, 0283, 1270, phase-480]
---

## What was measured

`examples/workspaces/features` image `native_rust_qos` (two nodes,
`features = ["param_services"]` and a `[lifecycle]` block), built from
`fix/1688-lifecycle-block-sizing` (where the image's derived infra token is
already `param+lifecycle`), run against `rmw_zenohd` (Humble):

    [ERROR] nros: liveliness: declare failed (Full) for a 188… keyexpr
    … (8 such lines: the last /reliable_talker and /qos_listener parameter
      services) …

The entities work; their liveliness tokens are not declared, so
`ros2 service list` / `ros2 node info` do not show them. Same count on
origin/main before issue 1688's fix.

`nros-cargo.toml` for that image states no `ZPICO_MAX_LIVELINESS`, so the
crate default applies: 16. The image needs 1 (primary node) + 2 (node names) +
3 application endpoints (`/qos_chatter` pub + sub, `/qos_ok` pub) + 12
parameter servers (6 × 2 nodes) + 5 lifecycle servers + 1 `transition_event`
publisher = 24.

## Why

`leaf_entity_env.rs`'s `NOT_DERIVED_LIVELINESS_NEEDS_INFRA_COUNT` withholds the
knob on purpose and says the crate default "is larger and safe". It is not, for
any image carrying the parameter family on more than one node: two nodes'
parameter servers alone are 12 of the 16. The queryable pool had the same gap
and phase-445 W1 closed it by having `nros-zpico-build` complete the count from
the `NROS_DECLARED_*` facts this road carries; the comment records that
"no consumer completes the liveliness pool from facts".

## Shape of a fix

Do for the liveliness pool what phase-445 W1 did for queryables: complete
`ZPICO_MAX_LIVELINESS` in `nros-zpico-build` from `NROS_DECLARED_NODES`,
`NROS_DECLARED_INFRA_QUERYABLES` and the descriptor's endpoint counts, using the
costs defined beside the code that registers them (one token per node name, per
server, per publisher/subscriber/client).

## Acceptance

`native_rust_qos` boots with no `liveliness: declare failed` line, and
`ros2 service list --no-daemon` lists every parameter and lifecycle service of
both nodes.

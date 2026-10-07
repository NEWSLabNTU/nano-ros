---
id: 1713
title: "A workspace image with param and lifecycle services overflows the
  undeclared ZPICO_MAX_LIVELINESS (16) — eight of its services are invisible
  to a stock ROS 2 peer"
status: resolved
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

## Resolution

Resolved 2026-10-07 on `fix/1713-liveliness-pool-from-facts`, the shape this
issue proposed: the consumer completes the pool from the facts the road
carries, as phase-445 W1 does for queryables.

- `nros-zpico-build::liveliness_default_from` derives `ZPICO_MAX_LIVELINESS`:
  the session's node token, one per node name (the larger of the components
  and `NROS_DECLARED_NODES`, plus the executor's under lifecycle), the
  parameter and lifecycle servers (the queryable table's own
  `infra_queryables` parser), the lifecycle `transition_event` publisher
  (`LIFECYCLE_SERVICE_PUBLISHERS`, a new mirror held by
  `check-infra-queryable-counts`), and the application's entities. With no
  descriptor the application half keeps the old 16, so a fact can only grow a
  pool; with nothing declared at all the pool is 16 exactly.
- The descriptor states the application half as `[image]
  entity_liveliness_tokens` (from `DerivedEntityKnobs::entity_liveliness_tokens`),
  refused on the leaf-declaration road with the other knob fields.
- The inventory's `max_liveliness` stopped counting transient-local cache
  queryables, which declare no token (one over per latched publisher).
- `leaf_entity_env.rs`, `NanoRosEntityFacts.cmake`, the book and
  `check-declared-fact-carriers` no longer call 16 "larger and safe".

**Measured** (`native_rust_qos` against a private `rmw_zenohd`, Humble; same
host, same router, the image rebuilt between runs):

| build | `declare failed (Full)` lines | `ros2 service list --no-daemon` |
| --- | --- | --- |
| origin/main | 8 | 9 services (3 of `/reliable_talker`'s and all 6 of `/qos_listener`'s parameter services missing) |
| this fix (derived pool 38) | 0 | 17 (12 parameter + 5 lifecycle) |

Test: `qos_override_e2e::every_parameter_service_is_visible_to_a_ros2_peer`
passes on the fixed fixture (17/17 listed in 1.99 s) and FAILS on the same
fixture rebuilt with `ZPICO_MAX_LIVELINESS=16` ("8 of the 12 parameter
services never appeared"). Unit tests: `liveliness_default_tests` (6,
including the negative control that the undeclared default is short by
exactly the 8 this issue lost).

Sweep: `git grep -n "ZPICO_MAX_LIVELINESS\|max_liveliness" -- packages cmake zephyr`

**Not measured.** The Zephyr road: its resolver already derives
`NROS_MAX_LIVELINESS` from an inventory composed with the model (infra
included), and the only change there is the transient-local over-count
removed; no Zephyr image was run. An image WITH a descriptor (the exact
count) was measured only through the unit test, because `native_rust_qos`
has no contract on this branch; issue 1709's branch gives it one. The cmake
(C/C++) road shares `nros-zpico-build` and was not run.

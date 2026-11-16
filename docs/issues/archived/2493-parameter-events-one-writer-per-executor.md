---
id: 2493
title: "`/parameter_events` has one writer per executor, so `ros2 topic info` shows only the first node where stock shows every node"
status: resolved
type: limitation
severity: low
area: [params, interop]
found: 2026-11-13
resolved: 2026-11-17
related: [2023, 2482, phase-556]
---

## What

Measured on 2026-11-13 (phase-556 first measurement, Humble,
`rmw_zenoh_cpp`, `--no-daemon`) on the two-node image of fixture
`param-two-node-talker` (`/alpha`, `/beta`, both with parameter services):

```
$ ros2 topic info -v /parameter_events
...
Node name: alpha
...
```

Only `/alpha` is listed as a publisher. In a stock graph every node with
parameters publishes its own `/parameter_events`, so `ros2 topic info -v`
and `ros2 node info /beta` show a writer for each node.

## Why

By design, from issue 2023: the parameter family claims ONE
`/parameter_events` writer per executor, not per node, because every event
names its node in the message's `node` field
(`PARAM_EVENT_PUBLISHERS = 1` in
`packages/core/nros-node/src/parameter_services.rs`). The writer is created by
the first node that enables parameter events and announced under that node's
name; later nodes reuse it. Events for `/beta` are published, with
`node = /beta` in the message, so subscribers that read the content get
everything. What differs is the graph: tools that ask who publishes see one
node.

It is therefore not a failure but a difference from stock that a user can
see, and it costs one publisher slot and one liveliness token per image
instead of per node.

## Options

- Keep one writer per executor and document the difference.
- On zenoh: keep one data key expression and declare one publisher
  liveliness token per node, so the graph shows every node (+1 token per
  node, no extra publisher). phase-554 plans the same pattern for lifecycle
  `transition_event`; whichever lands first makes it one shared helper in the
  zenoh shim.
- On DDS: one writer per node.

## Ruling

User ruling 2026-11-13 (phase-556 W13, relayed by the manager session):
"Take your advice on default services." Parameter services, `/rosout` and
`/parameter_events` are on by default on every board, and `/parameter_events`
takes stock's shape: one publisher per node. On zenoh that is the second
option above (one data key expression, one publisher liveliness token per
node). Work item: phase-556 W14, after the issue 2482 fix.

## Acceptance

`ros2 topic info -v /parameter_events` lists every node of a multi-node image,
as stock does (test in phase-556's parity binary, flipped from
expected-failure).

## Resolution

Resolved 2026-11-17, phase-556 W14. Every node is a `/parameter_events`
publisher, as stock.

- **Runtime (`nros-node`).** `ParamState::event_publishers` holds one writer
  per parameter node (bounded like the service sets, `PARAM_SERVICE_SETS`),
  created through the ordinary node publisher path by
  `create_param_event_pub`, still the one creation site. Each node's events go
  out on its own writer; `disable_parameter_events_on` drops that node's
  writer and leaves the others. The core stays RMW-agnostic.
- **Sizing.** `PARAM_EVENT_PUBLISHERS` (1) is per node: the entity inventory
  multiplies it by `InfraServices::param_nodes`, the zenoh liveliness default
  by the image's nodes, as `/rosout` already was. `check-infra-queryable-counts`
  still ties it to its one creation site.
- **zenoh.** A VOLATILE publisher on a data key the session already publishes
  on, with the same declare-time options (reliability, express), shares that
  zenoh publisher: one data key expression, and a liveliness token, gid and
  sequence number per ROS publisher. The zenoh publisher is undeclared when
  its last holder drops (a per-session, per-handle holder count in
  `zpico.rs`), so dropping the first node's writer leaves the others
  publishing. No C ABI or vtable change; no extra zenoh publisher slot per
  node. `ZenohSession::declare_node_topic_writer` / `NodeTopicWriter` is the
  same thing for a caller holding the session directly (phase-554's
  lifecycle `transition_event`). TRANSIENT_LOCAL is never shared and is
  refused by the writer.
- **DDS / XRCE.** An ordinary writer per node; their pools follow the
  per-node count from the inventory.

Tests: `nros-node` `registering_parameter_services_publishes_parameter_events_for_every_node`
(one publisher per node, events on their own node's writer, opt-out drops
it); `nros-rmw-zenoh` `in_process_graph::node_topic_writers_share_one_data_key_and_each_is_a_publisher`
(live router: three publishers in the graph over one zenoh handle, distinct
gids, the data key outlives the first holder, TRANSIENT_LOCAL and an
anonymous writer refused); the phase-556 parity case is the plain check
`each_node_of_a_two_node_image_publishes_parameter_events_as_stock_does`.

Measured 2026-11-17 in the ros2 distrobox (Humble, `rmw_zenoh_cpp`): the
parity case passes, and `params_per_node_interop`'s four zenoh cases pass
(`ros2_topic_echo_sees_each_nodes_parameter_events` among them).

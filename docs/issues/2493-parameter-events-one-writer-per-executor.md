---
id: 2493
title: "`/parameter_events` has one writer per executor, so `ros2 topic info` shows only the first node where stock shows every node"
status: open
type: limitation
severity: low
area: [params, interop]
found: 2026-11-13
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

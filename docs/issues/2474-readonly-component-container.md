---
id: 2474
title: "An image has no component container, so `ros2 component list` finds nothing where a stock composed process answers"
status: open
type: limitation
severity: low
area: [runtime, interop]
related: [2428, rfc-0128]
found: 2026-11-12
---

**Optional follow-up to [RFC-0128](../design/0128-per-node-lifecycle.md) D8**
(user ruling 2026-11-12: the read-only container is an optional follow-up, not
part of that RFC).

## What a stock process does

A composed ROS 2 process is a component container. It is a node (by default
`/ComponentManager`) serving `~/_container/list_nodes`, `load_node` and
`unload_node` (`composition_interfaces`). `ros2 component list` asks every
container for its nodes.

## What an image does

Since RFC-0128 D8, an image announces only its declared nodes, and its
executor identity is a session label, not a node. Nothing serves
`_container/*`, so `ros2 component list` shows no container for a board, even
though the image is exactly a composed set of components.

## A possible shape

A read-only container, under an opt-in name:

- `list_nodes` answers with the image's declared nodes and their unique ids;
- `load_node` and `unload_node` refuse, because an image's components are
  fixed at build time;
- it costs three services (one queryable on zenoh, if it shares D7's
  dispatcher design).

That is one extra node on the graph, which D8's default hides, so it can only
be opt-in.

## Next

Nothing until someone needs `ros2 component list` against a board. If it is
picked up, it needs a ruling on the container's default name and whether the
opt-in is per image.

---
id: 791
title: "We are visible in the ROS graph and cannot read it — 12 rmw vtable graph
  slots exist, all `None`, while both backends already run the discovery machinery"
status: open
type: bug
area: api, rmw
related: [rfc-0035, rfc-0036, phase-376, phase-379]
---

## Problem

RFC-0036 says nano-ros has "no dynamic discovery — peers static via `nros.toml` /
Kconfig", and phase 379 began by declining the whole graph-query family on that
basis. The `graph` stage did not survive contact with the code: **37 of its 68
rows are gaps, not declines.**

Three facts, each checkable:

**The vtable already carries the family.** `nros/rmw_vtable.h` has held these as
optional slots since phase-376 W4 — `get_node_names`, the four `*_by_node` forms,
`get_topic_names_and_types`, `get_service_names_and_types`,
`get_publishers_info_by_topic`, `get_subscriptions_info_by_topic`,
`count_publishers`, `count_subscribers`, `node_get_graph_guard_condition` — 12
slots for upstream's 15 names, plus `rmw_topic_endpoint_info_t` in
`rmw_entity.h`.

**Every one is `None`.** `packages/rmw/cffi/src/lib.rs` fills none of them, no
runtime wrapper exists above them, and no user-facing entry point exists in C,
C++ or Rust.

**Both real backends already run the machinery.** The zenoh shim *declares and
queries* `@ros2_lv` liveliness tokens for nodes, publishers, subscriptions and
services (`Ros2Liveliness::*_keyexpr`, wildcard GETs in
`packages/rmw/zenoh/nros-rmw-zenoh/src/shim/session.rs`) — the same mechanism
`rmw_zenoh_cpp` builds its graph cache from. `nros-rmw-cyclonedds/src/graph.cpp`
publishes `ros_discovery_info` so `ros2 node list` can see us.

So the position is not "we have no discovery". It is: **we are visible in the
graph, we already speak the protocol that carries it, and the reading half was
never wired up.**

## Why it matters

The asymmetry is the problem. A nano-ros node appears in `ros2 node list` and
`ros2 topic info`, so an operator reasonably expects it to behave like a
participant — but the node itself cannot answer "is anyone subscribed to this
topic", "did the peer I need come up", or "what is on this topic" in any of the
three languages. Code that would branch on the graph has to be written as if
blind, on a system where it is not.

It also makes RFC-0036's blanket sentence misleading in a way that has already
cost work: the first pass of this campaign declined six rows across two stages
citing it, and those had to be re-verdicted once the vtable was read.

## Two smaller findings from the same stage

* **`get_transition_graph`**: `nros-node/src/lifecycle_services.rs` serves our
  full `ALL_TRANSITIONS` table over `~/get_transition_graph`, so a remote peer
  can read the lifecycle state machine over the wire while the node's own code
  cannot read it in-process in any language — only `nros_lifecycle_get_state`
  exists. The table is already `const`.
* **`subscription` vs `subscriber`**: rclrs says
  `get_subscription_names_and_types_by_node`, rcl says `subscriber`. Whichever
  we pick, one lane is not a drop-in. Related to issue 0788.

## Evidence

`scripts/api-parity.py --topic graph`, and
`docs/reference/api-parity-ledger/graph.json` — 37 `gap`, 15 `declined`,
8 `divergence`, 1 `rename`. The declines that survive are the rclcpp
`Event`/`wait_for_graph_change` shape (allocator, listener thread, and a
blocking wait that does not drive the executor) and rclrs's
`notify_on_graph_change` (future + runtime).

## Direction

Not decided here; phase 379 W3 owns the coverage. What the stage established
that a planner should start from:

* The seam exists and is the right shape — this is filling slots, not designing
  an API.
* zenoh can answer the whole family from liveliness tokens it already queries.
  Cyclone would need a reader for `ros_discovery_info` it currently only writes.
  XRCE has no graph at all, so whatever lands must degrade per backend, which is
  what the optional slots are for.
* The result carriers are the real design question, not the queries. rcl returns
  `rcl_names_and_types_t` and `rcl_topic_endpoint_info_array_t`, both allocated;
  `graph.json` records three visitor typedefs as the committed replacement shape.
* **RFC-0036's "no dynamic discovery" line should be narrowed** to say what is
  actually true: no discovery-driven *entity matching* (peers are static), but
  the graph is observable and we do not yet observe it.

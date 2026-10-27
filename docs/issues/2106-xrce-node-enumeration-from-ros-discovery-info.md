---
id: 2106
title: "XRCE answers no graph slot, but get_node_names (and the graph-change edge) is reachable: subscribe to ros_discovery_info through the Agent, as Cyclone does"
status: open
type: enhancement
area: [rmw, xrce]
severity: low
found: 2026-10-27
related: [1292, 1269, 1978]
---

## What is missing

The XRCE backend leaves every graph slot NULL (`check-rmw-slot-producers`:
`xrce graph 0/14`), so `get_node_names`, `node_get_graph_guard_condition` and
the rest answer UNSUPPORTED. Most of that family is a protocol limit — see
"XRCE-DDS: what each NULL slot means" in `book/src/design/rmw-vs-upstream.md`:
micro-XRCE-DDS-Client has no read on the Agent participant's discovery state
(no builtin-topic readers, no participant or endpoint enumeration), so topic
names, types, endpoint info and per-topic counts cannot be learned from a stock
Agent.

Node ENUMERATION is the exception. Node names travel as data, on the ordinary
DDS topic `ros_discovery_info` (`rmw_dds_common::msg::ParticipantEntitiesInfo`),
which an XRCE client can subscribe to like any other topic. This backend already
PUBLISHES that topic (issue 1292, `src/graph.c`), and Cyclone answers
`get_node_names` from the same samples. A datareader on it gives:

- `get_node_names` — every node every DDS-RMW participant on the domain lists;
- `node_get_graph_guard_condition` — "a new sample arrived" is a graph change.

## Bounds to state when doing it

- Only DDS peers publish `ros_discovery_info`. A zenoh peer bridged in does not,
  so the answer is "nodes of DDS participants", and the slot's doc must say so.
- Samples are unbounded (`NodeEntitiesInfo[]`); the reader needs a cap on
  participants/nodes held, refusing past it, not truncating silently.
- Stock `rmw_microxrcedds` does not do this; it answers its graph only under
  `RMW_UXRCE_GRAPH`, from the micro-ROS Agent's own `ros_to_microros_graph`
  topic, which a stock Micro XRCE-DDS Agent (what nano-ros uses) does not
  publish.

## Acceptance

`get_node_names` on an XRCE session lists a Humble `rmw_fastrtps_cpp` peer's
node, live (the issue-1292 rig), and `graph_interop`'s XRCE cell moves from
UNSUPPORTED to an answer for that slot only.

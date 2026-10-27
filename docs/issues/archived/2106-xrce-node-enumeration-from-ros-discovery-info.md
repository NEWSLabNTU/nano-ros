---
id: 2106
title: "XRCE answers no graph slot, but get_node_names (and the graph-change edge) is reachable: subscribe to ros_discovery_info through the Agent, as Cyclone does"
status: resolved
type: enhancement
area: [rmw, xrce]
severity: low
found: 2026-10-27
resolved: 2026-10-28
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

## Resolution (2026-10-28)

`get_node_names` and `node_get_graph_guard_condition` are filled
(`src/graph.c`, the read side beside issue 1292's writer).

### How it works

- **The reader.** A `ros_discovery_info` DataReader is created through the
  Agent on FIRST USE, by a query or an edge registration. A clear (the
  runtime's capability probe) creates nothing. Its QoS is RELIABLE,
  TRANSIENT_LOCAL, KEEP_LAST(16).
  - It is one more endpoint on the participant, so it takes a key from issue
    1292's count. Skipping that would shift every later GID by one.
- **Routing.** Samples reach `xrce_graph_on_topic` from `xrce_topic_callback`.
  They are copied out (fragment-safe) and folded by `xrce_graph_ingest` into a
  per-participant table keyed by GID, holding the latest node list.
- **This session.** Its own echoed sample is skipped. The local node list is
  the source for this session's own nodes.
- **Answer.** `get_node_names` visits this session's nodes, then each remote
  participant's. Enclave is NULL.
- **The edge** fires when:
  - a sample adds a participant, changes one, or (with an empty node list)
    removes one;
  - a local node is created or destroyed.

### Bounds, all documented (graph.c, §9b, `xrce-known-limitations.md`)

- **DDS peers only.** A bridged zenoh peer publishes no
  `ros_discovery_info` sample.
- **A dead participant stays listed.** The client never sees DDS discovery
  loss, so a participant leaves the table only through its last, node-less
  sample, which a clean shutdown publishes.
- **Capped table.** `XRCE_GRAPH_MAX_PEERS` (32) participants and
  `XRCE_GRAPH_MAX_PEER_NODES` (64) nodes each. A sample past a cap is
  refused, and the query then answers ERROR rather than a short list.
- **Warm-up.** The first query reports this session's nodes alone.

### Tests

- **No Agent:** `tests/node_names.c` (`nros_rmw_xrce_node_names`,
  `just check rmw-xrce`). Samples come from graph.c's own serializer over a
  second session state. Checks:
  - this session's nodes first, then the remote ones, with namespaces;
  - its own echoed sample is skipped;
  - a changed sample replaces the entry and fires the edge;
  - an identical sample does not fire it;
  - an empty sample removes the participant;
  - a malformed sample is ignored, and another reader's sample is not consumed;
  - a sample past the node cap yields ERROR;
  - local node create/destroy fires the edge, and a cleared edge does not;
  - an early-stopping visitor works, and a NULL visitor is INVALID_ARGUMENT.
  - Negative control: not skipping its own sample, not answering ERROR on
    overflow, and firing on identical samples failed six checks.
- **Live**, in the new interop cell `native-graph-rust-xrce-r2n`
  (test binary `xrce_ros2_interop`, not `graph_interop`: the nano-ros side is an XRCE client and the Agent is what joins the DDS bus, so the file's `check-dds-isolation-symmetry` allowlist applies; fixture `graph-probe-xrce`; verdict recorded). Both cases
  run against a stock Humble `rmw_fastrtps_cpp` `demo_nodes_cpp` talker
  through the pinned Agent.
  - `xrce_enumerates_a_stock_ros2_node`: the probe lists `/talker`, and the
    topic query answers `Unsupported` (the cell moves from UNSUPPORTED to an
    answer for this slot only).
  - `xrce_a_peer_leaving_fires_the_graph_change_guard`: SIGTERM to the
    talker fires the edge. Measured by hand first: SIGINT and SIGTERM both
    fire it; the SIGKILL that `Ros2DdsProcess`'s drop sends never does,
    hence the new `Ros2DdsProcess::terminate`.
  - Negative controls: skipping the remote participants in the visitor failed
    the enumeration case; dropping the edge on removal gave
    `GRAPH_PROBE_CHANGE_NONE`.

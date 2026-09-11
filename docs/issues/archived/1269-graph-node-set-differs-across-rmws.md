---
id: 1269
title: "The ROS graph shows a different node set for the same image on each
  RMW: zenoh announces every node, Cyclone announces one per session"
status: resolved
type: bug
area: rmw
severity: medium
related: [issue-0105, issue-1268, issue-1292]
---

## Resolution (phase-444 W7)

**Cyclone: fixed.** `graph.cpp` now keeps one record per NODE, fed by the
`create_node` / `destroy_node` vtable slots (both were NULL; the runtime
already called `create_node` once per distinct `(name, namespace)`). Every
endpoint is attributed to the node that created it (`graph_node_of`), and the
latched `ParticipantEntitiesInfo` carries one `NodeEntitiesInfo` per node with
that node's own reader/writer GIDs, republished on every node or endpoint
change. The session's open-time name is no longer published as a node, which
is what produced the phantom `/node`. Two siblings of the same class fixed in
the same change: destroying an endpoint now removes its GID (nothing ever
called `graph_untrack_*`, so a destroyed publisher stayed listed for the
session's life), and ThreadX's `SessionState` is placement-new'd so the graph
state is never read uninitialised.

Evidence: the backend CTest `nros_rmw_cyclonedds_graph_node_set` reads the
sample back from a separate participant and asserts four entries for four
nodes, each publisher's GID under its own node only, an endpoint-less node
present, no session-named entry, and a republish after `destroy_publisher`
and `destroy_node`. Negative control: capping the published sequence at one
entry (the pre-fix shape) makes it fail four assertions.

**Test the issue asked for:** `rust_multi_node_per_node_graph` runs the same
`examples/workspaces/rust` image on zenoh and on Cyclone and asserts one shared
`EXPECTED_NODES` set by equality; cell `native-multinode-rust-cyclone` in
`interop::CELLS`. It needs a live ROS 2 peer and has NOT been run here — it
starts life with no verdict, as every new live-peer cell does.

**XRCE: split out as issue 1292.** The backend writes no
`ros_discovery_info` at all, so the expected reading is zero nodes, not the
one-node collapse this issue guessed — reasoned from `session.c`, not
measured. Recorded as the carve-out `native-multinode-rust-xrce-CARVED`.

## Requirement

The same image must present the same node set to ROS 2 whichever RMW it is
built with. `ros2 node list`, and every tool that addresses a node by name
(`ros2 param`, `ros2 node info`, launch introspection), must see the nodes the
image composes, not an artifact of the transport.

## What each RMW does today

| RMW | nodes shown for a 4-node image | mechanism | evidence |
| --- | --- | --- | --- |
| zenoh | 4 | one liveliness token per node on the shared session | issue 0105, resolved in phase-268 |
| Cyclone | 1 (`/node`) | one `ParticipantEntitiesInfo` per session with one node entry | measured; `graph.cpp` |
| XRCE | not measured | the session is keyed by a single node name | `nros-rmw-xrce/src/session.c` |

**Cyclone.** `nros-rmw-cyclonedds/src/graph.cpp` publishes
`ros_discovery_info` with `node_entities_info_seq._length = 1`, carrying the
one name `session.cpp` passes to `graph_init` for the whole session. A
downstream image with four component nodes on one executor (Autoware Safety
Island) shows `/node` in `ros2 node list`, and `ros2 param list /mrm_handler`
answers "Node not found". Every reader and writer of the four nodes is
attributed to that one node.

Issue 0105 fixed exactly this -- "a multi-node entry now shows one graph node
per launch component in `ros2 node list`" -- but through zenoh's per-node
liveliness tokens, so the Cyclone graph publisher never got it.

**XRCE** keys its session on one node name
(`hash_session_key(node_name)` in `session.c`), which suggests the same
collapse; it has not been run.

## Fix shape

- Cyclone: one `NodeEntitiesInfo` entry per node record, each carrying that
  node's reader and writer GIDs, republished as nodes are added.
- XRCE: measure, then the equivalent.
- One matrix test that builds one multi-node image per RMW and asserts the
  same `ros2 node list`, so the next divergence fails a test rather than a user.

## Acceptance

A 4-node image shows the same four nodes in `ros2 node list` on zenoh,
Cyclone and XRCE, each with its own endpoints.

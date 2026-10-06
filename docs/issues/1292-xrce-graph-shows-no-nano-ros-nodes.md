---
id: 1292
title: "XRCE images publish no `ros_discovery_info`, so `ros2 node list` shows
  none of their nodes"
status: open
type: bug
area: rmw
severity: medium
related: [issue-1269, issue-0105, phase-480]
---

## Requirement

Issue 1269's: the same image presents the same node set to ROS 2 whichever
RMW it is built with. Issue 1269 made Cyclone match zenoh (one graph node per
component). This is the XRCE third of it.

## What XRCE does today (from the source — NOT measured live)

`nros-rmw-xrce` never writes `ros_discovery_info`. `grep -rn
'ros_discovery_info\|ParticipantEntitiesInfo' packages/rmw/xrce` finds
nothing. Its session creates ONE DDS participant on the Agent, named after the
session's open-time node name (`uxr_buffer_create_participant_bin(...,
name_buf, ...)` in `session.c`), and its `create_node` / `destroy_node` vtable
slots are NULL.

A stock `rmw_fastrtps_cpp` / `rmw_cyclonedds_cpp` graph cache
(`rmw_dds_common::GraphCache`) learns node names ONLY from
`ParticipantEntitiesInfo` samples on `ros_discovery_info`; a bare DDS
participant's name is not a node. So the expected reading of `ros2 node list`
against an XRCE image is **zero** of its nodes, and its topics' endpoints are
attributed to no node — reasoned from the code above, not observed. The
Micro-XRCE-DDS Agent does not publish the topic on a client's behalf (micro-ROS
does it in the CLIENT library, `rmw_microxrcedds`'s graph manager, which we do
not use).

The issue 1269 table recorded XRCE as "not measured" and guessed the zenoh-era
collapse to one node; the code says it is worse than that. Measure before
fixing: the cell below is where the measurement goes.

## Fix shape

- Declare a `ros_discovery_info` DataWriter through the Agent (XML or binary
  representation) once per session, with the rmw_dds_common type and the
  stock QoS (RELIABLE, TRANSIENT_LOCAL, KEEP_LAST 1).
- Fill `create_node` / `destroy_node` and keep one record per node, each with
  its readers' and writers' GIDs — the Agent assigns the DDS GUIDs, so this
  needs the entities' GUIDs back from the Agent, which the XRCE protocol does
  not return by default. That is the hard part and why this is not a copy of
  Cyclone's `graph.cpp`.
- Republish on every node/endpoint change, as Cyclone does since 1269.

## Test

`native-multinode-rust-xrce-CARVED` in `interop::CELLS` records the lane as
absent, citing this issue. The fixture it would run already exists
(`workspace-rust-native-xrce`, `[image.native_xrce]` in
`examples/workspaces/rust`); turning the carve-out into a Runtime cell of
`rust_multi_node_per_node_graph` is the acceptance test.

## Acceptance

`rust_multi_node_per_node_graph` gains an XRCE case asserting the same node set
(`/talker`, `/listener`) its zenoh and Cyclone cases assert, and it passes
against a live Agent + ROS 2 peer.

## Measured live — 2026-10-06 (phase-480 W3)

The reasoning above holds. `examples/native/c/talker` built for XRCE
(`build-xrce/c_talker`, node `talker` on `/chatter`), the pinned Agent
`~/.nros/sdk/xrce-agent/2.4.3-nros1` on `udp4 -p 27901`, domain 77, and a stock
Humble `rmw_fastrtps_cpp` peer on the same host:

    ros2 node list --no-daemon                  -> (empty)
    ros2 topic info -v /chatter --no-daemon     -> Publisher count: 1
                                                   Node name: _CREATED_BY_BARE_DDS_APP_
                                                   Node namespace: _CREATED_BY_BARE_DDS_APP_
    ros2 topic echo --once /chatter --no-daemon -> data: 'Hello World: 7'

The data path is fine and the graph is not: the endpoint exists, is attributed
to no node, and `ros2 node list` shows nothing. `_CREATED_BY_BARE_DDS_APP_` is
`rmw_dds_common::GraphCache`'s placeholder for an endpoint that no
`ParticipantEntitiesInfo` sample claims.

## Why this stays open — it is a feature, not a fix

Every piece of the fix shape above is new machinery, and the hard part is the
one the XRCE protocol does not provide:

1. A `ros_discovery_info` DataWriter through the Agent, typed
   `rmw_dds_common::msg::ParticipantEntitiesInfo`, RELIABLE + TRANSIENT_LOCAL +
   KEEP_LAST(1).
2. The DDS GUIDs of every reader and writer the Agent created for this client.
   `ParticipantEntitiesInfo` lists a node's endpoints by GID, and the client
   never learns those: `uxr_buffer_create_*` yields an XRCE object id, not the
   Agent-side DDS GUID. Without them a sample would name the nodes but attach
   no endpoints — `node list` would be right and `topic info` would still read
   `_CREATED_BY_BARE_DDS_APP_`. Getting them needs an Agent-side extension (the
   pinned Agent is our fork, so this is possible) or the micro-ROS approach,
   whose Agent publishes the graph for the client — plain `MicroXRCEAgent` does
   not.
3. `create_node` / `destroy_node` vtable slots and republish-on-change, as
   Cyclone does since issue 1269.

Plan, in order: (a) decide where the GUIDs come from — measure whether the fork
Agent can report them (a reply on entity creation, or a GUID rule the client can
reproduce); (b) declare the `ros_discovery_info` writer once per session and
publish node names with whatever endpoints (a) supplies; (c) turn the
`native-multinode-rust-xrce-CARVED` cell into the acceptance cell above. Step
(b) alone is worth landing first: it makes `ros2 node list` correct before (a)
attaches endpoints.

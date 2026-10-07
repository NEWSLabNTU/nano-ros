---
id: 1292
title: "XRCE images publish no `ros_discovery_info`, so `ros2 node list` shows
  none of their nodes"
status: resolved
type: bug
area: rmw
severity: medium
related: [issue-1269, issue-0105, issue-1732, phase-480]
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

## Why this stayed open — it was a feature, not a fix (2026-10-06)

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

## Measured — the GUID source (2026-10-07)

Plan step (a): where the GIDs come from. The pinned Agent is stock eProsima
v2.4.3 (`nros-sdk-index.toml` `[tool.xrce-agent]`, Fast-DDS 2.14.6 bundled),
not a fork. Its GET_INFO answers for the root only
(`Processor::process_get_info_submessage` -> `root_.get_info`), so no XRCE
message reports an entity's GUID. Three routes were measured with a standalone
micro-XRCE-DDS-Client prototype (built from the pinned client submodule) and a
Humble `rmw_fastrtps_cpp` peer:

| route | no profile | issue-1009 loopback profile on both halves |
| --- | --- | --- |
| XML participant `<prefix>` + XML endpoint `<entityID>` | node listed, publisher attributed, GID = the chosen one | peer sees NOTHING, not even the topic |
| bin participant, bin endpoints (main) | — | `_CREATED_BY_BARE_DDS_APP_`, GID `01.0f.40.dc.ba.40.20.fa.01.00.00.00` + key `000001` |
| bin participant + GUID probe + counted keys | — | node listed, publisher attributed, GID `01.0f.40.dc.ba.40.20.fa.00.00.02.00` + predicted key `000005` |

Why the XML participant loses the profile: the Agent builds it from a
default-constructed `ParticipantAttributes` and copies transports, discovery and
locators from that over the factory default
(`FastDDSParticipant::create_by_xml` -> `set_qos_from_attributes`), so an Agent
operator's `FASTRTPS_DEFAULT_PROFILES_FILE` is discarded for it. Fast-DDS also
refuses an XML `<entityID>` above 255 (`XMLElementParser.cpp`, `i > 255`).

The route taken keeps the bin participant and gets both halves of every GID
from measurements:

- **Prefix.** A replier's request callback carries the request's
  `SampleIdentity`, whose writer GUID is the requester's request DataWriter,
  which is on our own participant. One self-addressed request on a private,
  non-ROS requester/replier pair yields the prefix; the pair is then deleted.
  The request carries a session nonce, because every nano-ros image probes on
  the same names.
- **Entity keys.** Fast-DDS 2.14 numbers user endpoints from one
  per-participant counter (`DomainParticipantImpl::id_counter_`), advanced only
  by the `DataWriterImpl` / `DataReaderImpl` constructors. The probe's own
  writer key says where it stands; each create the backend makes then takes the
  next key(s): writer or reader 1, replier/requester 2 (writer first, as
  `create_by_attributes` builds them). A failed create leaves the count
  unknown, so the next create re-probes.

## Resolution

Resolved 2026-10-07 (phase-480 W3), in `nros-rmw-xrce` alone
(`src/graph.c`, new). Plan steps (a), (b) and (c) all landed, not (b) alone.

- `create_node` / `destroy_node` slots fill a node table, as Cyclone has since
  issue 1269. The session's own name names the participant, not a node.
- At session open: the GUID probe, then a bin `ros_discovery_info` DataWriter
  (RELIABLE, TRANSIENT_LOCAL, KEEP_LAST 1).
- Each endpoint's record is embedded in its entity state and linked into one
  list, so there is no static pool. The sample is republished once per
  `drive_io` tick when anything changed.
- Loud, never silent: a probe with no answer turns attribution off for that
  session with one logged line. A sample too large for one stream slot is
  logged once with `NROS_XRCE_TRANSPORT_MTU`.

**Measured after**, solo, on fixtures rebuilt from this tree
(`workspace-rust-native-xrce`), the pinned Agent through
`XrceAgent::start_unique` (loopback-pinned), a Humble `rmw_fastrtps_cpp` peer:

    ros2 node list            -> /listener /talker
    ros2 node info /talker    -> Publishers:  /chatter: std_msgs/msg/Int32
    ros2 node info /listener  -> Subscribers: /chatter: std_msgs/msg/Int32

By hand, same Agent and peer: the Rust `service-server` and `service-client`
examples (XRCE) show `/add_two_ints` under Service Servers and Service Clients
of their nodes, which checks the two-endpoint key order.

**Test.** `rust_multi_node_per_node_graph` gains
`rust_multi_node_entry_per_node_graph_nodes_xrce`, asserting the node set AND
`node info` attribution. The carve-out `native-multinode-rust-xrce-CARVED`
became the Runtime cell `native-multinode-rust-xrce`. Two negative controls, both
red:

- pre-fix backend (origin/main sources): `left: {}` vs `{"/listener", "/talker"}`;
- keys off by one (`*first_key = counter + 2`): node set right, `node info /talker`
  lists no publisher -> fails on attribution.

Self-contained half: CTest `nros_rmw_xrce_graph_sample` (sample layout,
counting rule, lifetimes, probe nonce), in `just check rmw-xrce`.

Zenoh and Cyclone: no code touched. The Cyclone case passed in the same run.
The zenoh case timed out at 60 s under a concurrent `just check fast`, and
passed solo (37.7 s, `/listener /talker`). All three cases ran on fixtures
rebuilt from this tree.

**Sweep** (every endpoint create the counter must see):
`git grep -n 'uxr_buffer_create_\(datawriter\|datareader\|replier\|requester\)_' packages/rmw/xrce/nros-rmw-xrce/src`

## Not measured

- Embedded XRCE images (Zephyr, FreeRTOS, serial transports). The code path is
  the same C, but no RTOS image was run against a peer.
- An Agent other than the pinned 2.4.3. The counting rule is Fast-DDS 2.14's,
  and a different Agent's Fast-DDS could number differently (e.g. at enable in
  2.6). A failed create re-probes, and a wrong count never shows up as an
  error, only as `_CREATED_BY_BARE_DDS_APP_` again.
- Iron+ peers. The Gid is Humble's 24 bytes, Cyclone's `graph.cpp` assumption.
- Action servers/clients through `node info`.

Filed: issue 1732 (`1732-xrce-entry-exit-leaves-agent-session.md`). A native image exits through `std::process::exit` without
closing its session, so the Agent keeps its participant, and since this fix
also its nodes, until the Agent restarts.

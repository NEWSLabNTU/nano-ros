// Phase 177.36 — per-session ROS 2 graph publisher (`ros_discovery_info`).
//
// Stock `rmw_cyclonedds_cpp` builds its node→endpoint graph from the
// `rmw_dds_common::msg::ParticipantEntitiesInfo` each participant publishes on
// the (un-prefixed) `ros_discovery_info` topic. nano-ros's Cyclone backend
// matches endpoints via raw SEDP, so pub/sub + services interop without this —
// but stock graph introspection (`ros2 node list/info`, `ros2 action info`, and
// crucially an action client's `wait_for_server`) sees the endpoints associated
// with NO node unless we publish this message. This tracks each node's
// reader/writer GIDs and (re)publishes the message on every change.
//
// Issue 1269 — ONE participant, MANY nodes. This used to publish exactly one
// `NodeEntitiesInfo`, named after the SESSION, and attributed every endpoint in
// the image to it: a four-node image showed `/node` in `ros2 node list` and
// `ros2 param list /mrm_handler` answered "Node not found". zenoh had shown one
// graph node per component since phase-268, so the node set a user saw
// depended on the RMW. Now the graph holds one record per NODE (fed by the
// `create_node` slot, which the runtime calls once per distinct
// `(name, namespace)`), every endpoint is attributed to the node that created
// it, and the sample carries one `NodeEntitiesInfo` per node. The session's own
// open-time name is not a node — exactly as on zenoh.
#ifndef NROS_RMW_CYCLONEDDS_GRAPH_HPP
#define NROS_RMW_CYCLONEDDS_GRAPH_HPP

#include <cstdint>

#include "dds/dds.h"

namespace nros_rmw_cyclonedds {

/// One graph node on this participant.
///
/// The strings are BORROWED, not copied: `rmw_node_t` promises its `name` and
/// `namespace_` outlive the node ("Borrowed; outlives the node",
/// `rmw_entity.h`), and the runtime hands them out of its static node table.
/// Copying them would cost 2 x 257 bytes per record for a bound
/// (`string<256>`) nothing in the image comes near, and would make the table
/// too expensive to size for the runtime's largest `NROS_RMW_MAX_NODES`.
struct GraphNode {
    const char* name{nullptr};
    const char* ns{nullptr}; // NULL or "" publishes as "/"
    bool used{false};
};

// Per-session (= per-participant) graph state. Fixed-capacity, no heap
// (matches the backend's alloc-light style + embedded constraints).
struct GraphState {
    /// Reader and writer capacity for the WHOLE participant, shared by every
    /// node on it — the same total the one-node table had.
    static constexpr int kMaxEndpoints = 32;
    /// The upper bound `nros-rmw-cffi`'s `build.rs` accepts for
    /// `NROS_RMW_MAX_NODES` ([1, 64]), so the runtime's own node table always
    /// fills before this one does. A record is two pointers and a flag.
    static constexpr int kMaxNodes = 64;

    dds_entity_t topic{0};
    dds_entity_t writer{0}; // latched ros_discovery_info writer
    /// phase-381 W5 — the READER half. Cyclone published `ros_discovery_info`
    /// and never read it, so a nano-ros node was visible in the graph and
    /// blind to it, which is the asymmetry issue 0791 is about.
    ///
    /// Created lazily on the first graph query: a node that never asks pays
    /// nothing, which matters because most embedded images never ask.
    dds_entity_t graph_reader{0};
    /// phase-444 W3 — the DDS BUILTIN-topic readers the graph QUERIES read
    /// (`DCPSPublication` / `DCPSSubscription`). Lazily created on the first
    /// query for the same reason `graph_reader` is: an image that never asks
    /// pays no reader and no history.
    dds_entity_t builtin_pub_reader{0};
    dds_entity_t builtin_sub_reader{0};
    uint8_t participant_gid[24]{};

    /// Slots are STABLE: a released record is marked unused and never moved,
    /// because `rmw_node_t::backend_data` points at it.
    GraphNode nodes[kMaxNodes]{};

    /// Endpoints are kept GROUPED BY NODE (ascending `*_node`), so each node's
    /// GIDs are one contiguous run and the published sequence can point
    /// straight into this storage — no per-publish copy, no stack array.
    dds_entity_t reader_ent[kMaxEndpoints]{};
    uint8_t reader_gid[kMaxEndpoints][24]{};
    uint8_t reader_node[kMaxEndpoints]{};
    int n_readers{0};

    dds_entity_t writer_ent[kMaxEndpoints]{};
    uint8_t writer_gid[kMaxEndpoints][24]{};
    uint8_t writer_node[kMaxEndpoints]{};
    int n_writers{0};

    bool active{false}; // false if the descriptor/topic/writer wasn't created
};

/// `graph_add_node` results that are not a slot index.
constexpr int kGraphNodeInvalid = -1; ///< no name, or a name longer than `string<256>`
constexpr int kGraphNodeFull = -2;    ///< every slot is taken

// Reset the state, capture the participant GID, register + create the latched
// `ros_discovery_info` writer, and publish the initial sample (no nodes yet).
// If the ParticipantEntitiesInfo descriptor or the writer can't be created the
// graph stays inactive: node records are still kept (they are the node's
// identity), but nothing is published and interop degrades gracefully to the
// pre-177.36 behaviour.
void graph_init(GraphState* g, dds_entity_t participant);
void graph_fini(GraphState* g);

/// Find the record for `(name, ns)`, adding it if absent, and republish when a
/// record is added. Returns the slot index, or `kGraphNodeInvalid` /
/// `kGraphNodeFull`. A name is never truncated: two long names sharing a
/// prefix would MERGE into one node, which is the defect this table exists to
/// remove. `name` and `ns` must outlive the record (see `GraphNode`).
int graph_add_node(GraphState* g, const char* name, const char* ns);

/// The slot index of a record `graph_add_node` handed out as a pointer
/// (`&g->nodes[i]`, the form `rmw_node_t::backend_data` carries), or -1 if
/// `rec` is not one of this graph's live records.
int graph_node_index(const GraphState* g, const void* rec);

/// Release a node record and every endpoint still attributed to it, then
/// republish. The slot becomes reusable; no other slot moves.
void graph_remove_node(GraphState* g, int node);

// Track/untrack an endpoint by its DDS entity (GID derived via dds_get_guid),
// attributed to node slot `node`. A negative `node` is a no-op, so a caller
// that could not resolve its node records nothing rather than guessing an
// owner. Each mutation re-publishes the full ParticipantEntitiesInfo.
void graph_track_writer(GraphState* g, int node, dds_entity_t writer);
void graph_track_reader(GraphState* g, int node, dds_entity_t reader);
void graph_untrack_writer(GraphState* g, dds_entity_t writer);
void graph_untrack_reader(GraphState* g, dds_entity_t reader);

void graph_publish(GraphState* g);

/// phase-381 W5 — visit every node this participant can SEE.
///
/// `visit(ctx, node_name, node_namespace)` per discovered node; return `false`
/// to stop. Strings are BORROWED for the duration of the call.
///
/// Reads the `ros_discovery_info` samples other participants latched, WITHOUT
/// consuming them (`dds_read`, not `dds_take`), so repeated calls see the
/// current view rather than draining it once.
///
/// **No persistent cache.** The topic is keyless, so stock rmw dedups by
/// participant gid in user space and keeps a graph cache; we dedup within the
/// READ BATCH instead and keep nothing between calls. Bounded by the batch
/// size, which is the only storage this adds.
///
/// Returns `false` if the graph is inactive (no descriptor / no reader), which
/// the caller reports as `UNSUPPORTED` — distinct from an empty graph.
bool graph_visit_nodes(GraphState* g, void* ctx,
                       bool (*visit)(void* ctx, const char* node_name, const char* node_namespace));

/// phase-444 W3 — every ENDPOINT the graph attributes to a node, across every
/// participant that has published `ros_discovery_info`.
///
/// `visit(ctx, node_name, node_namespace, gid, is_writer)` per listed GID;
/// return `false` to stop. Strings and `gid` are BORROWED for the call.
///
/// This is the ATTRIBUTION half of the graph queries: the DDS builtin topics
/// say which endpoints exist, on what topic, with what type and QoS, and say
/// nothing about nodes — ROS's node layer exists only in this message. Same
/// read, same dedup and the same "reports what has been DISCOVERED" contract as
/// `graph_visit_nodes`; `false` means the graph is inactive, which the caller
/// reports as UNSUPPORTED.
bool graph_visit_endpoints(GraphState* g, void* ctx,
                           bool (*visit)(void* ctx, const char* node_name,
                                         const char* node_namespace, const uint8_t gid[24],
                                         bool is_writer));

} // namespace nros_rmw_cyclonedds

#endif // NROS_RMW_CYCLONEDDS_GRAPH_HPP

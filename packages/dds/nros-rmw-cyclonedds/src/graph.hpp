// Phase 177.36 — per-session ROS 2 graph publisher (`ros_discovery_info`).
//
// Stock `rmw_cyclonedds_cpp` builds its node→endpoint graph from the
// `rmw_dds_common::msg::ParticipantEntitiesInfo` each participant publishes on
// the (un-prefixed) `ros_discovery_info` topic. nano-ros's Cyclone backend
// matches endpoints via raw SEDP, so pub/sub + services interop without this —
// but stock graph introspection (`ros2 node list/info`, `ros2 action info`, and
// crucially an action client's `wait_for_server`) sees the endpoints associated
// with NO node unless we publish this message. This tracks each node's
// reader/writer GIDs and (re)publishes the message on every endpoint change.
#ifndef NROS_RMW_CYCLONEDDS_GRAPH_HPP
#define NROS_RMW_CYCLONEDDS_GRAPH_HPP

#include <cstdint>

#include "dds/dds.h"

namespace nros_rmw_cyclonedds {

// Per-session (≈ per-participant ≈ per-node) graph state. Fixed-capacity, no
// heap (matches the backend's alloc-light style + embedded constraints).
struct GraphState {
    static constexpr int kMaxEndpoints = 32;

    dds_entity_t topic{0};
    dds_entity_t writer{0}; // latched ros_discovery_info writer
    uint8_t participant_gid[24]{};
    char node_namespace[256]{};
    char node_name[256]{};

    dds_entity_t reader_ent[kMaxEndpoints]{};
    uint8_t reader_gid[kMaxEndpoints][24]{};
    int n_readers{0};

    dds_entity_t writer_ent[kMaxEndpoints]{};
    uint8_t writer_gid[kMaxEndpoints][24]{};
    int n_writers{0};

    bool active{false}; // false if the descriptor/topic/writer wasn't created
};

// Capture the participant GID + node identity, register + create the latched
// `ros_discovery_info` writer, and publish the (empty) initial sample. If the
// ParticipantEntitiesInfo descriptor or the writer can't be created the graph
// stays inactive and every other call is a no-op (interop degrades gracefully
// to the pre-177.36 behaviour).
void graph_init(GraphState* g, dds_entity_t participant, const char* node_name,
                const char* node_namespace);
void graph_fini(GraphState* g);

// Track/untrack an endpoint by its DDS entity (GID derived via dds_get_guid).
// Each mutation re-publishes the full ParticipantEntitiesInfo.
void graph_track_writer(GraphState* g, dds_entity_t writer);
void graph_track_reader(GraphState* g, dds_entity_t reader);
void graph_untrack_writer(GraphState* g, dds_entity_t writer);
void graph_untrack_reader(GraphState* g, dds_entity_t reader);

void graph_publish(GraphState* g);

} // namespace nros_rmw_cyclonedds

#endif // NROS_RMW_CYCLONEDDS_GRAPH_HPP

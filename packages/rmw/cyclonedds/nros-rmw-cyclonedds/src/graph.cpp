// Phase 177.36 — ROS 2 graph publisher (`ros_discovery_info`). See graph.hpp.
#include "graph.hpp"

#include <cstdio>
#include <cstdlib>
#include <cstring>

#include "dds/ddsrt/string.h"
#include "descriptors.hpp"
#include "rmw_dds_common_graph.h" // idlc-generated typed structs + descriptor

// The idlc-generated register TU exports this constructor entry point. Calling
// it explicitly forces the descriptor TU to be linked into consumers of this
// static lib (its `__attribute__((constructor))` registers the descriptor at
// load too, but the explicit call guarantees the object isn't dropped as
// unreferenced). Idempotent — registering the same type name twice is benign.
extern "C" void register_rmw_dds_common_graph_0(void);

namespace nros_rmw_cyclonedds {

namespace {

// `ros_discovery_info` is a special rmw topic: stock sets
// `avoid_ros_namespace_conventions`, so it is NOT `rt/`-prefixed. Use the bare
// name (do NOT route through topic_prefix).
constexpr const char* kGraphTopic = "ros_discovery_info";
constexpr const char* kGraphType = "rmw_dds_common::msg::dds_::ParticipantEntitiesInfo_";

// rmw_dds_common Gid is 24 bytes; a DDS GUID is 16. Stock derives the gid by
// copying the 16-byte GUID into the first 16 bytes (rest zero) and matches the
// same bytes from SEDP, so endpoints associate with the node. (Distinct from
// service.cpp::writer_guid_lo64, which takes the *lower 8* for request-id
// correlation — do not reuse that here.)
void entity_gid_24(dds_entity_t e, uint8_t out[24]) {
    std::memset(out, 0, 24);
    dds_guid_t g;
    if (dds_get_guid(e, &g) == DDS_RETCODE_OK) {
        std::memcpy(out, g.v, sizeof(g.v)); // 16 bytes
    }
}

int find_entity(const dds_entity_t* arr, int n, dds_entity_t e) {
    for (int i = 0; i < n; ++i) {
        if (arr[i] == e) return i;
    }
    return -1;
}

} // namespace

void graph_init(GraphState* g, dds_entity_t participant, const char* node_name,
                const char* node_namespace) {
    if (g == nullptr || participant <= 0) return;

    register_rmw_dds_common_graph_0();

    entity_gid_24(participant, g->participant_gid);
    ddsrt_strlcpy(g->node_name, node_name ? node_name : "", sizeof(g->node_name));
    ddsrt_strlcpy(g->node_namespace, (node_namespace && node_namespace[0]) ? node_namespace : "/",
                  sizeof(g->node_namespace));

    const dds_topic_descriptor_t* desc = find_descriptor(kGraphType);
    if (desc == nullptr) return; // graph stays inactive — interop degrades gracefully

    dds_entity_t topic = dds_create_topic(participant, desc, kGraphTopic, nullptr, nullptr);
    if (topic < 0) return;
    g->topic = topic;

    // RELIABLE + TRANSIENT_LOCAL + KEEP_LAST(1): latched, so late-joining stock
    // tooling always gets the current node→endpoint snapshot. Matches stock.
    dds_qos_t* qos = dds_create_qos();
    dds_qset_reliability(qos, DDS_RELIABILITY_RELIABLE, DDS_SECS(1));
    dds_qset_durability(qos, DDS_DURABILITY_TRANSIENT_LOCAL);
    dds_qset_history(qos, DDS_HISTORY_KEEP_LAST, 1);
    dds_entity_t w = dds_create_writer(participant, topic, qos, nullptr);
    dds_delete_qos(qos);
    if (w < 0) return;

    g->writer = w;
    g->active = true;
    graph_publish(g);
}

void graph_fini(GraphState* g) {
    if (g == nullptr) return;
    // The participant's dds_delete cascades to the writer + topic, so we only
    // reset state here (session_destroy deletes the participant).
    g->writer = 0;
    g->topic = 0;
    // phase-381 W5 — the graph reader cascades from the participant like the
    // writer does; only the handle is reset here.
    g->graph_reader = 0;
    g->active = false;
    g->n_readers = 0;
    g->n_writers = 0;
}

void graph_track_writer(GraphState* g, dds_entity_t writer) {
    if (g == nullptr || !g->active || writer <= 0) return;
    if (find_entity(g->writer_ent, g->n_writers, writer) >= 0) return;
    if (g->n_writers >= GraphState::kMaxEndpoints) return;
    g->writer_ent[g->n_writers] = writer;
    entity_gid_24(writer, g->writer_gid[g->n_writers]);
    g->n_writers++;
    graph_publish(g);
}

void graph_track_reader(GraphState* g, dds_entity_t reader) {
    if (g == nullptr || !g->active || reader <= 0) return;
    if (find_entity(g->reader_ent, g->n_readers, reader) >= 0) return;
    if (g->n_readers >= GraphState::kMaxEndpoints) return;
    g->reader_ent[g->n_readers] = reader;
    entity_gid_24(reader, g->reader_gid[g->n_readers]);
    g->n_readers++;
    graph_publish(g);
}

void graph_untrack_writer(GraphState* g, dds_entity_t writer) {
    if (g == nullptr || !g->active) return;
    int i = find_entity(g->writer_ent, g->n_writers, writer);
    if (i < 0) return;
    for (int j = i; j < g->n_writers - 1; ++j) {
        g->writer_ent[j] = g->writer_ent[j + 1];
        std::memcpy(g->writer_gid[j], g->writer_gid[j + 1], 24);
    }
    g->n_writers--;
    graph_publish(g);
}

void graph_untrack_reader(GraphState* g, dds_entity_t reader) {
    if (g == nullptr || !g->active) return;
    int i = find_entity(g->reader_ent, g->n_readers, reader);
    if (i < 0) return;
    for (int j = i; j < g->n_readers - 1; ++j) {
        g->reader_ent[j] = g->reader_ent[j + 1];
        std::memcpy(g->reader_gid[j], g->reader_gid[j + 1], 24);
    }
    g->n_readers--;
    graph_publish(g);
}

void graph_publish(GraphState* g) {
    if (g == nullptr || !g->active || g->writer <= 0) return;

    // Build the sample entirely on the stack: dds_write serializes synchronously
    // (copies), so pointing the sequence `_buffer`s at stack arrays with
    // `_release = false` needs no heap and no free.
    rmw_dds_common_msg_dds__Gid_ rgids[GraphState::kMaxEndpoints];
    rmw_dds_common_msg_dds__Gid_ wgids[GraphState::kMaxEndpoints];
    for (int i = 0; i < g->n_readers; ++i)
        std::memcpy(rgids[i].data, g->reader_gid[i], 24);
    for (int i = 0; i < g->n_writers; ++i)
        std::memcpy(wgids[i].data, g->writer_gid[i], 24);

    rmw_dds_common_msg_dds__NodeEntitiesInfo_ node;
    std::memset(&node, 0, sizeof(node));
    // Bounded `string<256>` → idlc fixed `char[257]` array, so copy (not assign).
    ddsrt_strlcpy(node.node_namespace, g->node_namespace, sizeof(node.node_namespace));
    ddsrt_strlcpy(node.node_name, g->node_name, sizeof(node.node_name));
    node.reader_gid_seq._length = static_cast<uint32_t>(g->n_readers);
    node.reader_gid_seq._maximum = static_cast<uint32_t>(g->n_readers);
    node.reader_gid_seq._buffer = g->n_readers ? rgids : nullptr;
    node.reader_gid_seq._release = false;
    node.writer_gid_seq._length = static_cast<uint32_t>(g->n_writers);
    node.writer_gid_seq._maximum = static_cast<uint32_t>(g->n_writers);
    node.writer_gid_seq._buffer = g->n_writers ? wgids : nullptr;
    node.writer_gid_seq._release = false;

    rmw_dds_common_msg_dds__NodeEntitiesInfo_ nodes[1] = {node};

    rmw_dds_common_msg_dds__ParticipantEntitiesInfo_ sample;
    std::memset(&sample, 0, sizeof(sample));
    std::memcpy(sample.gid.data, g->participant_gid, 24);
    sample.node_entities_info_seq._length = 1;
    sample.node_entities_info_seq._maximum = 1;
    sample.node_entities_info_seq._buffer = nodes;
    sample.node_entities_info_seq._release = false;

    (void)dds_write(g->writer, &sample);
}

/// phase-381 W5 — the READER half. Contract in graph.hpp.
bool graph_visit_nodes(GraphState* g, void* ctx,
                       bool (*visit)(void* ctx, const char* node_name,
                                     const char* node_namespace)) {
    if (g == nullptr || visit == nullptr || !g->active || g->topic <= 0) {
        return false;
    }

    // How many samples one query looks at, and the only storage this adds.
    // A participant republishes its FULL snapshot on every mutation, so the
    // newest samples are the current view; this bounds how much of it is read
    // at once rather than how much is retained (nothing is).
    constexpr uint32_t kMaxSamples = 16;

    if (g->graph_reader <= 0) {
        // Created on FIRST USE, not at init: a node that never asks pays no
        // reader, no history and no discovery traffic, which is most embedded
        // images.
        //
        // Matches the writer's QoS deliberately — RELIABLE + TRANSIENT_LOCAL is
        // what makes a late reader receive the snapshots participants latched
        // before it existed, which is the whole reason the writer latches.
        // KEEP_LAST(kMaxSamples) rather than (1): the topic is KEYLESS, so one
        // instance carries every participant's samples and a depth of 1 would
        // hold whichever wrote last.
        dds_qos_t* qos = dds_create_qos();
        dds_qset_reliability(qos, DDS_RELIABILITY_RELIABLE, DDS_SECS(1));
        dds_qset_durability(qos, DDS_DURABILITY_TRANSIENT_LOCAL);
        dds_qset_history(qos, DDS_HISTORY_KEEP_LAST, static_cast<int32_t>(kMaxSamples));
        dds_entity_t r = dds_create_reader(dds_get_participant(g->topic), g->topic, qos, nullptr);
        dds_delete_qos(qos);
        if (r < 0) {
            return false;
        }
        g->graph_reader = r;
        // Nothing has been delivered on a reader created this instant, so the
        // first call legitimately reports an empty graph and the next sees the
        // latched snapshots. Same warm-up the zenoh side documents.
    }

    // issue 0927 — the one measurement that separates the two explanations for
    // "we only ever see our own node": a reader that never MATCHED the remote
    // writer, versus one that matched and dropped the sample. Without it both
    // look identical from the outside, and the zenoh side lost days to exactly
    // that ambiguity (issue 0903).
    //
    // Env-gated and permanent, same convention as the zenoh shim's
    // `NROS_GRAPH_DUMP`: the first time this was wanted it was patched in by
    // hand, and the next person had to re-derive it.
    if (std::getenv("NROS_GRAPH_DUMP") != nullptr) {
        dds_instance_handle_t matched[kMaxSamples];
        int32_t m = dds_get_matched_publications(g->graph_reader, matched, kMaxSamples);
        std::fprintf(stderr, "GRAPH_CYCLONE matched_publications=%d\n", static_cast<int>(m));
    }

    void* raw[kMaxSamples] = {nullptr};
    dds_sample_info_t info[kMaxSamples];
    // READ, not TAKE: taking would drain the history, so a second query would
    // see nothing and the graph would appear to vanish after one look.
    int32_t n = dds_read(g->graph_reader, raw, info, kMaxSamples, kMaxSamples);
    if (n <= 0) {
        return true; // active, nothing discovered yet
    }

    // The topic is keyless, so several samples can describe the SAME
    // participant — an older snapshot and a newer one both sit in history.
    // Stock rmw keeps a user-space graph cache keyed by participant gid; we
    // dedup within this batch instead and keep nothing between calls. Newest
    // first, so the first sample seen for a gid is the current one.
    uint8_t seen[kMaxSamples][24];
    int n_seen = 0;

    for (int32_t i = n - 1; i >= 0; --i) {
        if (!info[i].valid_data) {
            continue;
        }
        auto* sample = static_cast<rmw_dds_common_msg_dds__ParticipantEntitiesInfo_*>(raw[i]);
        if (sample == nullptr) {
            continue;
        }
        // Our OWN participant is in the graph too; reporting it is correct —
        // `ros2 node list` lists the asking node as well.
        bool dup = false;
        for (int k = 0; k < n_seen; ++k) {
            if (std::memcmp(seen[k], sample->gid.data, 24) == 0) {
                dup = true;
                break;
            }
        }
        if (dup) {
            continue;
        }
        if (n_seen < static_cast<int>(kMaxSamples)) {
            std::memcpy(seen[n_seen++], sample->gid.data, 24);
        }

        const uint32_t count = sample->node_entities_info_seq._length;
        auto* nodes = sample->node_entities_info_seq._buffer;
        if (nodes == nullptr) {
            continue;
        }
        for (uint32_t j = 0; j < count; ++j) {
            if (!visit(ctx, nodes[j].node_name, nodes[j].node_namespace)) {
                (void)dds_return_loan(g->graph_reader, raw, n);
                return true;
            }
        }
    }

    // The samples are LOANED from the reader; returning them is not optional.
    (void)dds_return_loan(g->graph_reader, raw, n);
    return true;
}

} // namespace nros_rmw_cyclonedds

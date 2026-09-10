// Phase 177.36 — ROS 2 graph publisher (`ros_discovery_info`). See graph.hpp.
#include "graph.hpp"

// issue 0942 — `<stdio.h>` and UNQUALIFIED `fprintf`, matching `descriptors.cpp`,
// the only other library TU here that prints. `<cstdio>` is only required to put
// the C names in namespace `std`; putting them in the GLOBAL namespace too is
// optional, and the freestanding libstdc++ in the riscv64 cross toolchain does
// the opposite — there `std::fprintf` does not exist. So this TU compiled on
// every hosted target and failed only on threadx-riscv64.
#include <stdio.h>

#include <cstddef>
#include <cstdlib>
#include <cstring>

#include "dds/ddsrt/heap.h"
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

// The published sequences point STRAIGHT into GraphState's `uint8_t[24]` GID
// storage, typed as the generated `Gid_`. That is only sound while the two have
// one layout, so it is checked rather than assumed.
static_assert(sizeof(rmw_dds_common_msg_dds__Gid_) == 24, "Gid_ must be 24 bytes");
static_assert(alignof(rmw_dds_common_msg_dds__Gid_) == 1, "Gid_ must be byte-aligned");
static_assert(offsetof(rmw_dds_common_msg_dds__Gid_, data) == 0, "Gid_::data must lead");

// `string<256>` in the IDL: idlc emits a fixed `char[257]`.
constexpr std::size_t kNameCap = sizeof(rmw_dds_common_msg_dds__NodeEntitiesInfo_::node_name);

// rmw_dds_common Gid is 24 bytes; a DDS GUID is 16. Stock derives the gid by
// copying the 16-byte GUID into the first 16 bytes (rest zero) and matches the
// same bytes from SEDP, so endpoints associate with the node. (Distinct from
// service.cpp::writer_request_id64, the per-client request-id correlation value
// — the writer's instance handle, issue 1291 — do not reuse that here.)
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

/// A namespace as it goes on the wire: an absent or empty one is the root.
const char* wire_ns(const char* ns) {
    return (ns != nullptr && ns[0] != '\0') ? ns : "/";
}

bool same_node(const GraphNode& n, const char* name, const char* ns) {
    return n.used && std::strcmp(n.name, name) == 0 && std::strcmp(wire_ns(n.ns), wire_ns(ns)) == 0;
}

/// Insert `e` into one endpoint table, keeping it grouped by node. Returns
/// false if already tracked or full.
bool insert_grouped(dds_entity_t* ents, uint8_t (*gids)[24], uint8_t* owner, int* n, int node,
                    dds_entity_t e) {
    if (find_entity(ents, *n, e) >= 0) return false;
    if (*n >= GraphState::kMaxEndpoints) return false;
    // After the last entry whose node is <= this one: stable within a node.
    int pos = *n;
    while (pos > 0 && owner[pos - 1] > static_cast<uint8_t>(node))
        --pos;
    for (int j = *n; j > pos; --j) {
        ents[j] = ents[j - 1];
        std::memcpy(gids[j], gids[j - 1], 24);
        owner[j] = owner[j - 1];
    }
    ents[pos] = e;
    entity_gid_24(e, gids[pos]);
    owner[pos] = static_cast<uint8_t>(node);
    ++*n;
    return true;
}

void erase_at(dds_entity_t* ents, uint8_t (*gids)[24], uint8_t* owner, int* n, int i) {
    for (int j = i; j < *n - 1; ++j) {
        ents[j] = ents[j + 1];
        std::memcpy(gids[j], gids[j + 1], 24);
        owner[j] = owner[j + 1];
    }
    --*n;
}

void erase_node(dds_entity_t* ents, uint8_t (*gids)[24], uint8_t* owner, int* n, int node) {
    for (int i = *n - 1; i >= 0; --i) {
        if (owner[i] == static_cast<uint8_t>(node)) erase_at(ents, gids, owner, n, i);
    }
}

/// The contiguous run of `node`'s endpoints in one grouped table.
void node_run(const uint8_t* owner, int n, int node, int* start, int* len) {
    int s = 0;
    while (s < n && owner[s] < static_cast<uint8_t>(node))
        ++s;
    int e = s;
    while (e < n && owner[e] == static_cast<uint8_t>(node))
        ++e;
    *start = s;
    *len = e - s;
}

rmw_dds_common_msg_dds__Gid_* as_gids(uint8_t (*gids)[24], int start) {
    return reinterpret_cast<rmw_dds_common_msg_dds__Gid_*>(gids[start]);
}

} // namespace

void graph_init(GraphState* g, dds_entity_t participant) {
    if (g == nullptr) return;
    // A clean slate regardless of how the enclosing state was allocated —
    // node records and endpoint counts are read on every later call. memset,
    // not `*g = GraphState{}`: that builds a ~5 KiB temporary on the caller's
    // stack, which is an RTOS app task. All-zero IS the default state (null
    // pointers, false flags, zero counts), and the type is trivially copyable.
    std::memset(static_cast<void*>(g), 0, sizeof(*g));
    if (participant <= 0) return;

    register_rmw_dds_common_graph_0();

    entity_gid_24(participant, g->participant_gid);

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
    for (GraphNode& n : g->nodes)
        n = GraphNode{};
}

int graph_add_node(GraphState* g, const char* name, const char* ns) {
    if (g == nullptr || name == nullptr || name[0] == '\0') return kGraphNodeInvalid;
    // Refuse rather than truncate — see graph.hpp.
    if (std::strlen(name) >= kNameCap || std::strlen(wire_ns(ns)) >= kNameCap) {
        return kGraphNodeInvalid;
    }
    int free_slot = -1;
    for (int i = 0; i < GraphState::kMaxNodes; ++i) {
        if (same_node(g->nodes[i], name, ns)) return i;
        if (!g->nodes[i].used && free_slot < 0) free_slot = i;
    }
    if (free_slot < 0) return kGraphNodeFull;
    g->nodes[free_slot].name = name;
    g->nodes[free_slot].ns = ns;
    g->nodes[free_slot].used = true;
    // A node with no endpoints is still a node: `ros2 node list` shows it the
    // moment it exists, which is also when zenoh declares its token.
    graph_publish(g);
    return free_slot;
}

int graph_node_index(const GraphState* g, const void* rec) {
    if (g == nullptr || rec == nullptr) return -1;
    for (int i = 0; i < GraphState::kMaxNodes; ++i) {
        if (rec == static_cast<const void*>(&g->nodes[i])) return g->nodes[i].used ? i : -1;
    }
    return -1;
}

void graph_remove_node(GraphState* g, int node) {
    if (g == nullptr || node < 0 || node >= GraphState::kMaxNodes || !g->nodes[node].used) return;
    erase_node(g->reader_ent, g->reader_gid, g->reader_node, &g->n_readers, node);
    erase_node(g->writer_ent, g->writer_gid, g->writer_node, &g->n_writers, node);
    g->nodes[node] = GraphNode{};
    graph_publish(g);
}

void graph_track_writer(GraphState* g, int node, dds_entity_t writer) {
    if (g == nullptr || writer <= 0 || node < 0 || node >= GraphState::kMaxNodes ||
        !g->nodes[node].used) {
        return;
    }
    if (insert_grouped(g->writer_ent, g->writer_gid, g->writer_node, &g->n_writers, node, writer)) {
        graph_publish(g);
    }
}

void graph_track_reader(GraphState* g, int node, dds_entity_t reader) {
    if (g == nullptr || reader <= 0 || node < 0 || node >= GraphState::kMaxNodes ||
        !g->nodes[node].used) {
        return;
    }
    if (insert_grouped(g->reader_ent, g->reader_gid, g->reader_node, &g->n_readers, node, reader)) {
        graph_publish(g);
    }
}

void graph_untrack_writer(GraphState* g, dds_entity_t writer) {
    if (g == nullptr) return;
    int i = find_entity(g->writer_ent, g->n_writers, writer);
    if (i < 0) return;
    erase_at(g->writer_ent, g->writer_gid, g->writer_node, &g->n_writers, i);
    graph_publish(g);
}

void graph_untrack_reader(GraphState* g, dds_entity_t reader) {
    if (g == nullptr) return;
    int i = find_entity(g->reader_ent, g->n_readers, reader);
    if (i < 0) return;
    erase_at(g->reader_ent, g->reader_gid, g->reader_node, &g->n_readers, i);
    graph_publish(g);
}

void graph_publish(GraphState* g) {
    if (g == nullptr || !g->active || g->writer <= 0) return;

    int n_nodes = 0;
    for (const GraphNode& n : g->nodes)
        n_nodes += n.used ? 1 : 0;

    // One `NodeEntitiesInfo_` per node. Each carries two `char[257]` arrays, so
    // a stack array sized for the table would be ~35 KiB — beyond what an RTOS
    // app task can give. It is a TRANSIENT sample, so it comes from
    // `ddsrt_malloc` (never libc: the RTOS heap is separate, see
    // cyclonedds-known-limitations.md), sized to the nodes that exist, and is
    // freed once `dds_write` has serialised it. The GID sequences need no
    // allocation at all: they point into the grouped tables.
    rmw_dds_common_msg_dds__NodeEntitiesInfo_* infos = nullptr;
    if (n_nodes > 0) {
        infos = static_cast<rmw_dds_common_msg_dds__NodeEntitiesInfo_*>(
            ddsrt_malloc(static_cast<std::size_t>(n_nodes) * sizeof(*infos)));
        if (infos == nullptr) {
            // Keep the last published snapshot rather than publish one that
            // drops every node. Say so: a stale graph is otherwise silent.
            fprintf(stderr,
                    "nros-rmw-cyclonedds: ros_discovery_info not republished "
                    "(out of memory for %d node entries)\n",
                    n_nodes);
            return;
        }
        std::memset(infos, 0, static_cast<std::size_t>(n_nodes) * sizeof(*infos));
    }

    int k = 0;
    for (int i = 0; i < GraphState::kMaxNodes; ++i) {
        const GraphNode& n = g->nodes[i];
        if (!n.used) continue;
        rmw_dds_common_msg_dds__NodeEntitiesInfo_& info = infos[k++];
        // Lengths were checked in graph_add_node, so these never truncate.
        ddsrt_strlcpy(info.node_namespace, wire_ns(n.ns), sizeof(info.node_namespace));
        ddsrt_strlcpy(info.node_name, n.name, sizeof(info.node_name));

        int rs = 0, rl = 0, ws = 0, wl = 0;
        node_run(g->reader_node, g->n_readers, i, &rs, &rl);
        node_run(g->writer_node, g->n_writers, i, &ws, &wl);
        info.reader_gid_seq._length = static_cast<uint32_t>(rl);
        info.reader_gid_seq._maximum = static_cast<uint32_t>(rl);
        info.reader_gid_seq._buffer = rl ? as_gids(g->reader_gid, rs) : nullptr;
        info.reader_gid_seq._release = false;
        info.writer_gid_seq._length = static_cast<uint32_t>(wl);
        info.writer_gid_seq._maximum = static_cast<uint32_t>(wl);
        info.writer_gid_seq._buffer = wl ? as_gids(g->writer_gid, ws) : nullptr;
        info.writer_gid_seq._release = false;
    }

    rmw_dds_common_msg_dds__ParticipantEntitiesInfo_ sample;
    std::memset(&sample, 0, sizeof(sample));
    std::memcpy(sample.gid.data, g->participant_gid, 24);
    sample.node_entities_info_seq._length = static_cast<uint32_t>(n_nodes);
    sample.node_entities_info_seq._maximum = static_cast<uint32_t>(n_nodes);
    sample.node_entities_info_seq._buffer = infos;
    sample.node_entities_info_seq._release = false;

    (void)dds_write(g->writer, &sample);
    ddsrt_free(infos);
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
    // `fprintf`, NOT `std::fprintf` — the crate's other TU (descriptors.cpp)
    // already spells it this way and compiles everywhere. `<cstdio>` is only
    // REQUIRED to declare the name in namespace `std`; putting it in the global
    // namespace too is permitted, not guaranteed, and the minimal C++ library on
    // `threadx-riscv64` does the opposite of what a hosted libstdc++ does. The
    // `std::` spelling built fine on every host lane and failed only there:
    //
    //   graph.cpp:234: error: 'fprintf' is not a member of 'std'
    //
    // Same family as issue 0112 (`<string>` gated on `__STDC_HOSTED__` rather
    // than on the C++ library actually in use): "the compiler is hosted" does
    // not imply "the C++ standard library is complete".
    if (std::getenv("NROS_GRAPH_DUMP") != nullptr) {
        dds_instance_handle_t matched[kMaxSamples];
        int32_t m = dds_get_matched_publications(g->graph_reader, matched, kMaxSamples);
        fprintf(stderr, "GRAPH_CYCLONE matched_publications=%d\n", static_cast<int>(m));
        // WHY a writer was refused, which `matched_publications` cannot say.
        // A remote writer whose offered QoS is weaker than what this reader
        // REQUESTS is never matched, and the reader looks identical to one on a
        // topic nobody publishes. `last_policy_id` names the exact policy, so
        // this distinguishes "incompatible" from "not there" in one number.
        dds_requested_incompatible_qos_status_t iq = {};
        if (dds_get_requested_incompatible_qos_status(g->graph_reader, &iq) == DDS_RETCODE_OK) {
            fprintf(stderr, "GRAPH_CYCLONE incompatible_qos total=%u last_policy_id=%u\n",
                    iq.total_count, iq.last_policy_id);
        }
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

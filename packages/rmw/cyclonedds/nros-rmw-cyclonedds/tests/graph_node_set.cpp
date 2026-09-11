// Issue 1269 — what Cyclone PUBLISHES on `ros_discovery_info` for a
// multi-node session.
//
// The requirement is that one image presents the same node set to ROS 2
// whichever RMW it is built with. zenoh declared one liveliness token per node
// (phase-268); Cyclone published ONE `NodeEntitiesInfo`, named after the
// session, carrying every endpoint in the image — so a four-node image showed a
// single `/node` in `ros2 node list`.
//
// This test reads the published sample back from a SEPARATE participant, i.e.
// after it has crossed the wire as CDR, and asserts its content entry by entry:
//
//   * one entry per declared node, and only those — the session's own name,
//     which names no node, must not appear (the phantom `/node`);
//   * each node carries its OWN endpoints: a publisher's gid appears under the
//     node that created it, byte-identical to `get_gid_for_publisher`, and
//     under no other node;
//   * a node with no endpoints is still a node;
//   * the snapshot is REPUBLISHED on change: a destroyed publisher leaves its
//     node's writer list, and a destroyed node leaves the set;
//   * a node handed to `create_publisher` without `create_node` (a direct
//     vtable caller) is still recorded under its own name.
//
// The live half — `ros2 node list` against the same image on each RMW — is
// `native-multinode-rust-cyclone` in `interop::CELLS`; this is what can be
// asserted with no ROS 2 on the host.

#include <chrono>
#include <cstdio>
#include <cstring>
#include <string>
#include <thread>
#include <vector>

#include <dds/dds.h>

#include "nros/rmw_entity.h"
#include "nros/rmw_ret.h"
#include "nros/rmw_vtable.h"
#include "nros_rmw_cyclonedds.h"
#include "nros_test_domain.h"
#include "rmw_dds_common_graph.h" // the generated wire type the backend publishes

namespace {
const nros_rmw_vtable_t* g_vt = nullptr;
int g_bad = 0;

void expect(bool ok, const char* what) {
    if (!ok) {
        std::fprintf(stderr, "FAIL: %s\n", what);
        g_bad = 1;
    }
}

/// One decoded `NodeEntitiesInfo`.
struct NodeView {
    std::string ns;
    std::string name;
    std::vector<std::string> readers; // gids as 24-byte strings
    std::vector<std::string> writers;
};

std::string gid_str(const uint8_t* p) {
    return std::string(reinterpret_cast<const char*>(p), 24);
}

/// The newest `ros_discovery_info` snapshot the session's participant latched.
bool read_snapshot(dds_entity_t reader, std::vector<NodeView>* out) {
    void* raw[1] = {nullptr};
    dds_sample_info_t info[1];
    const int32_t n = dds_read(reader, raw, info, 1, 1);
    if (n <= 0) return false;
    bool ok = false;
    if (info[0].valid_data) {
        auto* s = static_cast<rmw_dds_common_msg_dds__ParticipantEntitiesInfo_*>(raw[0]);
        out->clear();
        for (uint32_t i = 0; i < s->node_entities_info_seq._length; ++i) {
            const auto& e = s->node_entities_info_seq._buffer[i];
            NodeView v;
            v.ns = e.node_namespace;
            v.name = e.node_name;
            for (uint32_t r = 0; r < e.reader_gid_seq._length; ++r)
                v.readers.push_back(gid_str(e.reader_gid_seq._buffer[r].data));
            for (uint32_t w = 0; w < e.writer_gid_seq._length; ++w)
                v.writers.push_back(gid_str(e.writer_gid_seq._buffer[w].data));
            out->push_back(v);
        }
        ok = true;
    }
    (void)dds_return_loan(reader, raw, n);
    return ok;
}

const NodeView* find(const std::vector<NodeView>& v, const char* ns, const char* name) {
    for (const auto& n : v) {
        if (n.ns == ns && n.name == name) return &n;
    }
    return nullptr;
}

/// Poll until `pred` holds for the latest snapshot. The graph writer is
/// TRANSIENT_LOCAL + KEEP_LAST(1), so the newest sample is the current state,
/// but it still has to cross discovery to reach our participant.
template <typename Pred>
bool await_snapshot(dds_entity_t reader, std::vector<NodeView>* snap, Pred pred) {
    for (int i = 0; i < 100; ++i) {
        if (read_snapshot(reader, snap) && pred(*snap)) return true;
        std::this_thread::sleep_for(std::chrono::milliseconds(50));
    }
    return false;
}

void dump(const char* when, const std::vector<NodeView>& snap) {
    std::fprintf(stderr, "  snapshot %s: %zu node(s)\n", when, snap.size());
    for (const auto& n : snap) {
        std::fprintf(stderr, "    ns=%s name=%s readers=%zu writers=%zu\n", n.ns.c_str(),
                     n.name.c_str(), n.readers.size(), n.writers.size());
    }
}

bool create_pub(rmw_node_t* node, const char* topic, uint32_t domain, rmw_publisher_t* out) {
    static const rmw_qos_profile_t qos = [] {
        rmw_qos_profile_t q{};
        q.reliability = NROS_RMW_RELIABILITY_RELIABLE;
        q.durability = NROS_RMW_DURABILITY_VOLATILE;
        q.history = NROS_RMW_HISTORY_KEEP_LAST;
        q.depth = 5;
        return q;
    }();
    out->topic_name = topic;
    out->type_name = "nros_test::msg::TestString";
    const rmw_message_type_support_t ts{out->type_name, ""};
    return g_vt->create_publisher(node, &ts, topic, domain, &qos, nullptr, out) == NROS_RMW_RET_OK;
}

bool create_sub(rmw_node_t* node, const char* topic, uint32_t domain, rmw_subscription_t* out) {
    rmw_qos_profile_t qos{};
    qos.reliability = NROS_RMW_RELIABILITY_RELIABLE;
    qos.durability = NROS_RMW_DURABILITY_VOLATILE;
    qos.history = NROS_RMW_HISTORY_KEEP_LAST;
    qos.depth = 5;
    out->topic_name = topic;
    out->type_name = "nros_test::msg::TestString";
    const rmw_message_type_support_t ts{out->type_name, ""};
    return g_vt->create_subscription(node, &ts, topic, domain, &qos, nullptr, out) ==
           NROS_RMW_RET_OK;
}

std::string pub_gid(rmw_publisher_t* p) {
    rmw_gid_t g{};
    if (g_vt->get_gid_for_publisher(p, &g) != NROS_RMW_RET_OK) return std::string();
    return gid_str(g.data);
}

bool contains(const std::vector<std::string>& v, const std::string& x) {
    for (const auto& e : v)
        if (e == x) return true;
    return false;
}
} // namespace

extern "C" rmw_ret_t nros_rmw_cffi_register_named(const char* /*name*/,
                                                  const nros_rmw_vtable_t* vt) {
    g_vt = vt;
    return NROS_RMW_RET_OK;
}

int main() {
    if (nros_rmw_cyclonedds_register() != NROS_RMW_RET_OK || g_vt == nullptr) {
        return 1;
    }
    // Precondition, not a skip: without the node slots there is nothing to
    // attribute an endpoint to, and every assertion below would be about the
    // one session-named record this test exists to rule out.
    if (g_vt->create_node == nullptr || g_vt->destroy_node == nullptr ||
        g_vt->get_gid_for_publisher == nullptr) {
        std::fprintf(stderr, "FAIL: create_node / destroy_node / get_gid slots are NULL\n");
        return 2;
    }

    const uint32_t domain = nros_test_domain(96);
    constexpr const char* kSessionName = "graph_node_set_session";

    rmw_session_t s{};
    s.node_name = kSessionName;
    s.namespace_ = "/";
    if (g_vt->create_session(nullptr, 0, domain, s.node_name, nullptr, &s) != NROS_RMW_RET_OK) {
        return 3;
    }

    // Four nodes, the way the runtime declares them: `create_node` once each,
    // on the one session. One is namespaced, one never gets an endpoint.
    struct Decl {
        const char* name;
        const char* ns;
    };
    const Decl decls[4] = {{"alpha", "/"}, {"bravo", "/"}, {"charlie", "/robot"}, {"delta", "/"}};
    rmw_node_t nodes[4]{};
    for (int i = 0; i < 4; ++i) {
        nodes[i].name = decls[i].name;
        nodes[i].namespace_ = decls[i].ns;
        nodes[i].session = &s;
        const rmw_ret_t r = g_vt->create_node(&s, decls[i].name, decls[i].ns, &nodes[i]);
        expect(r == NROS_RMW_RET_OK, "create_node must accept a fresh node");
        expect(nodes[i].backend_data != nullptr, "create_node must record the node");
    }
    // The runtime deduplicates, but the backend must not MERGE or duplicate
    // either: re-declaring a node is the same record.
    {
        rmw_node_t again{};
        again.name = "alpha";
        again.namespace_ = "/";
        again.session = &s;
        expect(g_vt->create_node(&s, "alpha", "/", &again) == NROS_RMW_RET_OK &&
                   again.backend_data == nodes[0].backend_data,
               "re-declaring (alpha, /) must return the same record, not a second node");
    }
    // A name longer than `string<256>` would have to be truncated, and a
    // truncated name can collide with another node's — refused instead.
    {
        std::string long_name(300, 'x');
        rmw_node_t too_long{};
        expect(g_vt->create_node(&s, long_name.c_str(), "/", &too_long) ==
                   NROS_RMW_RET_INVALID_ARGUMENT,
               "a node name that would not fit string<256> must be refused, not truncated");
    }

    rmw_publisher_t pa{}, pb{};
    rmw_subscription_t sb{}, sc{};
    const bool made = create_pub(&nodes[0], "rt/gns_a", domain, &pa) &&
                      create_pub(&nodes[1], "rt/gns_b", domain, &pb) &&
                      create_sub(&nodes[1], "rt/gns_a", domain, &sb) &&
                      create_sub(&nodes[2], "rt/gns_b", domain, &sc);
    if (!made) {
        std::fprintf(stderr, "FAIL: could not create the endpoints\n");
        (void)g_vt->destroy_session(&s);
        return 4;
    }
    const std::string gid_a = pub_gid(&pa);
    const std::string gid_b = pub_gid(&pb);
    expect(gid_a.size() == 24 && gid_b.size() == 24 && gid_a != gid_b,
           "the two publishers must have distinct gids");

    // The observer: its own participant, so what it reads is the CDR sample
    // a stock `rmw_dds_common` graph cache would receive, not our structs.
    const dds_entity_t pp = dds_create_participant(domain, nullptr, nullptr);
    const dds_entity_t topic =
        dds_create_topic(pp, &rmw_dds_common_msg_dds__ParticipantEntitiesInfo__desc,
                         "ros_discovery_info", nullptr, nullptr);
    dds_qos_t* q = dds_create_qos();
    dds_qset_reliability(q, DDS_RELIABILITY_RELIABLE, DDS_SECS(1));
    dds_qset_durability(q, DDS_DURABILITY_TRANSIENT_LOCAL);
    dds_qset_history(q, DDS_HISTORY_KEEP_LAST, 1);
    const dds_entity_t reader = dds_create_reader(pp, topic, q, nullptr);
    dds_delete_qos(q);
    if (pp < 0 || topic < 0 || reader < 0) {
        std::fprintf(stderr, "FAIL: could not create the observer\n");
        (void)g_vt->destroy_session(&s);
        return 5;
    }

    std::vector<NodeView> snap;

    // ---- 1. one entry per node, each with its own endpoints --------------
    const bool settled = await_snapshot(reader, &snap, [&](const std::vector<NodeView>& v) {
        const NodeView* a = find(v, "/", "alpha");
        const NodeView* b = find(v, "/", "bravo");
        const NodeView* c = find(v, "/robot", "charlie");
        return v.size() == 4 && a && b && c && find(v, "/", "delta") && a->writers.size() == 1 &&
               b->writers.size() == 1 && b->readers.size() == 1 && c->readers.size() == 1;
    });
    if (!settled) dump("after setup", snap);
    expect(settled, "the snapshot must list exactly the four declared nodes, each with its "
                    "own endpoints (issue 1269: was one session-named entry)");
    expect(find(snap, "/", kSessionName) == nullptr,
           "the session's open-time name is not a node and must not be published");
    if (settled) {
        const NodeView* a = find(snap, "/", "alpha");
        const NodeView* b = find(snap, "/", "bravo");
        const NodeView* c = find(snap, "/robot", "charlie");
        const NodeView* d = find(snap, "/", "delta");
        expect(a->writers[0] == gid_a, "alpha's writer gid must be its publisher's gid");
        expect(b->writers[0] == gid_b, "bravo's writer gid must be its publisher's gid");
        expect(!contains(b->writers, gid_a) && !contains(c->writers, gid_a) &&
                   !contains(d->writers, gid_a),
               "a publisher must be listed under the node that created it and no other");
        expect(a->readers.empty() && c->writers.empty(),
               "a node must not inherit another node's endpoints");
        expect(d->readers.empty() && d->writers.empty(),
               "a node with no endpoints is still a node, with empty lists");
        expect(b->readers[0] != c->readers[0], "two readers must have two gids");
    }

    // ---- 2. destroying a publisher republishes without its gid ------------
    expect(g_vt->destroy_publisher(&pb) == NROS_RMW_RET_OK, "destroy_publisher(pb)");
    const bool untracked = await_snapshot(reader, &snap, [&](const std::vector<NodeView>& v) {
        const NodeView* b = find(v, "/", "bravo");
        return v.size() == 4 && b && b->writers.empty() && b->readers.size() == 1;
    });
    if (!untracked) dump("after destroy_publisher", snap);
    expect(untracked, "a destroyed publisher must leave its node's writer list");

    // ---- 3. destroying a node republishes without it ----------------------
    expect(g_vt->destroy_node(&nodes[3]) == NROS_RMW_RET_OK, "destroy_node(delta)");
    expect(nodes[3].backend_data == nullptr, "destroy_node must clear backend_data");
    const bool dropped = await_snapshot(reader, &snap, [&](const std::vector<NodeView>& v) {
        return v.size() == 3 && find(v, "/", "delta") == nullptr;
    });
    if (!dropped) dump("after destroy_node", snap);
    expect(dropped, "a destroyed node must leave the published set");

    // ---- 4. a node never declared through create_node --------------------
    rmw_node_t undeclared{};
    undeclared.name = "echo";
    undeclared.namespace_ = "/";
    undeclared.session = &s; // backend_data stays NULL: a pure identity carrier
    rmw_publisher_t pe{};
    expect(create_pub(&undeclared, "rt/gns_e", domain, &pe), "create_publisher on undeclared");
    const std::string gid_e = pub_gid(&pe);
    const bool fallback = await_snapshot(reader, &snap, [&](const std::vector<NodeView>& v) {
        const NodeView* e = find(v, "/", "echo");
        return v.size() == 4 && e && e->writers.size() == 1 && e->writers[0] == gid_e;
    });
    if (!fallback) dump("after the undeclared node's publisher", snap);
    expect(fallback, "an endpoint on an undeclared node must be listed under that node's name");

    (void)g_vt->destroy_publisher(&pe);
    (void)g_vt->destroy_subscription(&sc);
    (void)g_vt->destroy_subscription(&sb);
    (void)g_vt->destroy_publisher(&pa);
    for (int i = 0; i < 3; ++i)
        (void)g_vt->destroy_node(&nodes[i]);
    (void)g_vt->destroy_session(&s);
    (void)dds_delete(pp);

    if (g_bad == 0) {
        std::printf("graph_node_set: OK (one NodeEntitiesInfo per node, own gids, "
                    "republished on change)\n");
    }
    return g_bad;
}

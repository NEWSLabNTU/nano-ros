/* Issue 2106 — `get_node_names` and the graph-change edge from
 * `ros_discovery_info`.
 *
 * No agent. A remote participant's sample is produced by graph.c's own writer
 * side (`xrce_graph_serialize` over a second session state — the layout a
 * stock rmw_fastrtps_cpp peer publishes, issue 1292) and fed to the read side
 * through `xrce_graph_on_topic`, the path `xrce_topic_callback` takes. The
 * reader is marked created, so no transport is touched. Pinned:
 *
 *   - the answer is this session's nodes, then each remote participant's;
 *   - this session's own sample, echoed back by DDS, is not listed twice;
 *   - a changed sample replaces the participant's entry and fires the edge, an
 *     identical one does not, an empty one (a graceful exit) removes it;
 *   - a sample past the node cap is REFUSED and the query answers ERROR, never
 *     a silently short list; a malformed sample is ignored;
 *   - a local node created or destroyed fires the edge; a cleared edge does
 *     not fire.
 * The live half is `xrce_ros2_interop::xrce_enumerates_a_stock_ros2_node`. */

#include "internal.h"
#include "nros/rmw_ret.h"
#include "nros/rmw_vtable.h"
#include "nros_rmw_xrce.h"

#include <stdio.h>
#include <string.h>

static int failures = 0;

#define CHECK(cond, what)                                                                          \
    do {                                                                                           \
        if (!(cond)) {                                                                             \
            printf("  FAIL: %s\n", (what));                                                        \
            failures++;                                                                            \
        } else {                                                                                   \
            printf("  ok    %s\n", (what));                                                        \
        }                                                                                          \
    } while (0)

static const nros_rmw_vtable_t* g_vtable = NULL;
rmw_ret_t nros_rmw_cffi_register_named(const char* name, const nros_rmw_vtable_t* vtable) {
    (void)name;
    g_vtable = vtable;
    return NROS_RMW_RET_OK;
}

/* ---- the visitor: collect "ns/name" lines ---- */
typedef struct {
    char out[2048];
    int count;
    int stop_after;
} collect_t;

static bool collect(void* ctx, const char* name, const char* ns, const char* enclave) {
    collect_t* c = (collect_t*)ctx;
    (void)enclave;
    char line[600];
    snprintf(line, sizeof(line), "%s%s%s;", ns, (ns[strlen(ns) - 1] == '/') ? "" : "/", name);
    strncat(c->out, line, sizeof(c->out) - strlen(c->out) - 1);
    c->count++;
    return c->stop_after == 0 || c->count < c->stop_after;
}

static rmw_ret_t names(rmw_session_t* s, collect_t* c) {
    memset(c->out, 0, sizeof(c->out));
    c->count = 0;
    rmw_node_visitor_t v = {collect, c};
    return g_vtable->get_node_names(s, v);
}

static int g_edges = 0;
static const void* g_edge_user = NULL;
static void on_edge(const void* user, size_t n) {
    (void)n;
    g_edges++;
    g_edge_user = user;
}

static xrce_session_state_t* new_state(uint8_t prefix_byte) {
    xrce_session_state_t* st =
        (xrce_session_state_t*)nros_xrce_calloc(1, sizeof(xrce_session_state_t));
    if (st != NULL) {
        memset(st->graph.prefix, prefix_byte, sizeof(st->graph.prefix));
        st->graph.counter = 4;
        st->graph.counter_known = true;
    }
    return st;
}

/* Feed `peer`'s current sample to `st`'s reader, as `xrce_topic_callback` does. */
static void deliver(xrce_session_state_t* st, xrce_session_state_t* peer) {
    uint8_t buf[8192];
    size_t len = xrce_graph_serialize(&peer->graph, buf, sizeof(buf));
    ucdrBuffer ub;
    ucdr_init_buffer(&ub, buf, len);
    (void)xrce_graph_on_topic(st, uxr_object_id(st->graph.reader_id, UXR_DATAREADER_ID), &ub,
                              len);
}

int main(void) {
    printf("get_node_names / graph-change edge from ros_discovery_info\n");
    CHECK(nros_rmw_xrce_register() == NROS_RMW_RET_OK && g_vtable != NULL, "vtable registered");
    if (g_vtable == NULL || g_vtable->get_node_names == NULL ||
        g_vtable->node_get_graph_guard_condition == NULL) {
        printf("  FAIL: get_node_names / node_get_graph_guard_condition are NULL\n");
        return 1;
    }

    xrce_session_state_t* st = new_state(0x11);
    xrce_session_state_t* peer = new_state(0x22);
    if (st == NULL || peer == NULL) {
        return 1;
    }
    st->graph.reader_active = true; /* "created": no transport in this test */
    st->graph.reader_id = 77;
    rmw_session_t session, peer_session;
    memset(&session, 0, sizeof(session));
    memset(&peer_session, 0, sizeof(peer_session));
    session.backend_data = st;
    peer_session.backend_data = peer;

    /* Edge registered before anything happens. */
    int user = 5;
    CHECK(g_vtable->node_get_graph_guard_condition(&session, on_edge, &user) == NROS_RMW_RET_OK,
          "graph-change edge registered");

    rmw_node_t local;
    memset(&local, 0, sizeof(local));
    local.session = &session;
    CHECK(xrce_node_create(&session, "probe", "", &local) == NROS_RMW_RET_OK && g_edges == 1 &&
              g_edge_user == &user,
          "a local node fires the edge with the registering user data");

    collect_t c;
    memset(&c, 0, sizeof(c));
    CHECK(names(&session, &c) == NROS_RMW_RET_OK && c.count == 1 && strcmp(c.out, "/probe;") == 0,
          "before any sample: this session's node alone");

    /* A remote participant with two nodes and some endpoints. */
    rmw_node_t talker, listener;
    memset(&talker, 0, sizeof(talker));
    memset(&listener, 0, sizeof(listener));
    talker.session = &peer_session;
    listener.session = &peer_session;
    (void)xrce_node_create(&peer_session, "talker", "", &talker);
    (void)xrce_node_create(&peer_session, "listener", "/ns", &listener);
    xrce_graph_endpoint w;
    memset(&w, 0, sizeof(w));
    xrce_graph_track(peer, &w, &talker, 9, XRCE_GRAPH_WRITER);
    int before = g_edges;
    deliver(st, peer);
    CHECK(g_edges == before + 1, "a new participant fires the edge");
    CHECK(names(&session, &c) == NROS_RMW_RET_OK && c.count == 3 &&
              strcmp(c.out, "/probe;/talker;/ns/listener;") == 0,
          "local node, then the remote participant's two, namespaces intact");

    before = g_edges;
    deliver(st, peer);
    CHECK(g_edges == before, "an identical sample does not fire the edge");

    /* Our own sample echoed back is not listed twice. */
    deliver(st, st);
    CHECK(names(&session, &c) == NROS_RMW_RET_OK && c.count == 3,
          "this session's own sample is skipped (the local list is the source)");

    /* A changed sample replaces the entry. */
    (void)xrce_node_destroy(&listener);
    before = g_edges;
    deliver(st, peer);
    CHECK(g_edges == before + 1 && names(&session, &c) == NROS_RMW_RET_OK && c.count == 2 &&
              strcmp(c.out, "/probe;/talker;") == 0,
          "a changed sample replaces the participant's nodes and fires the edge");

    /* A visitor that stops early is honoured. */
    c.stop_after = 1;
    CHECK(names(&session, &c) == NROS_RMW_RET_OK && c.count == 1, "a visitor can stop early");
    c.stop_after = 0;

    /* A malformed sample is ignored. */
    uint8_t junk[10] = {1, 2, 3};
    ucdrBuffer jb;
    ucdr_init_buffer(&jb, junk, sizeof(junk));
    (void)xrce_graph_on_topic(st, uxr_object_id(77, UXR_DATAREADER_ID), &jb, sizeof(junk));
    CHECK(names(&session, &c) == NROS_RMW_RET_OK && c.count == 2, "a malformed sample is ignored");

    /* Another reader's sample is not the graph's. */
    ucdr_init_buffer(&jb, junk, sizeof(junk));
    CHECK(!xrce_graph_on_topic(st, uxr_object_id(78, UXR_DATAREADER_ID), &jb, sizeof(junk)),
          "a sample for another data reader is not consumed");

    /* A graceful exit: the participant publishes no nodes, and leaves. */
    xrce_graph_untrack(peer, &w);
    (void)xrce_node_destroy(&talker);
    before = g_edges;
    deliver(st, peer);
    CHECK(g_edges == before + 1 && names(&session, &c) == NROS_RMW_RET_OK && c.count == 1 &&
              st->graph.n_peers == 0,
          "an empty sample removes the participant and fires the edge");

    /* Past the node cap: refused, and the query says so. */
    static char many[XRCE_GRAPH_MAX_PEER_NODES + 1][16];
    static rmw_node_t many_nodes[XRCE_GRAPH_MAX_PEER_NODES + 1];
    for (unsigned i = 0; i <= XRCE_GRAPH_MAX_PEER_NODES; ++i) {
        snprintf(many[i], sizeof(many[i]), "n%u", i);
        memset(&many_nodes[i], 0, sizeof(many_nodes[i]));
        many_nodes[i].session = &peer_session;
        (void)xrce_node_create(&peer_session, many[i], "", &many_nodes[i]);
    }
    deliver(st, peer);
    CHECK(st->graph.peers_overflowed && names(&session, &c) == NROS_RMW_RET_ERROR,
          "a participant past XRCE_GRAPH_MAX_PEER_NODES is refused; the query answers ERROR");

    /* Clearing the edge stops it. */
    CHECK(g_vtable->node_get_graph_guard_condition(&session, NULL, NULL) == NROS_RMW_RET_OK,
          "edge cleared (also the capability probe)");
    before = g_edges;
    (void)xrce_node_destroy(&local);
    CHECK(g_edges == before, "a cleared edge does not fire");

    rmw_node_visitor_t none = {NULL, NULL};
    CHECK(g_vtable->get_node_names(&session, none) == NROS_RMW_RET_INVALID_ARGUMENT,
          "a NULL visitor is INVALID_ARGUMENT");

    xrce_graph_release(st);
    xrce_graph_release(peer);
    nros_xrce_free(st);
    nros_xrce_free(peer);
    if (failures != 0) {
        printf("node_names: %d failure(s)\n", failures);
        return 1;
    }
    printf("node_names: OK\n");
    return 0;
}

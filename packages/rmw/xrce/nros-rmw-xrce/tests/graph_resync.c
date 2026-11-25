/* Issue 2970 — the graph re-sync: a participant whose last `ros_discovery_info`
 * sample never arrived is dropped once a re-created reader fails to re-deliver
 * it, and one that is still there is kept.
 *
 * No agent. Samples come from graph.c's own writer side over other session
 * states (as in node_names.c) and are fed through `xrce_graph_on_topic`; a
 * re-sync's generation is advanced by hand, and its confirmation goes through
 * `xrce_graph_on_status`, the path the session's status callback takes.
 * Pinned:
 *
 *   - a peer the current generation re-delivered is kept, one it did not is
 *     dropped, and the graph-change edge fires once for the sweep;
 *   - a sweep with nothing stale fires nothing;
 *   - the re-created reader's confirmation claims one endpoint key and
 *     confirms the generation; a refused one claims none, leaves the count
 *     unknown and confirms nothing; a status for another request is ignored;
 *   - the re-sync does nothing before its period is due, and nothing while no
 *     reader exists (an image that never reads the graph sends nothing).
 * The live half is `xrce_ros2_interop::xrce_a_peer_leaving_fires_the_graph_change_guard`. */

#include "internal.h"
#include "nros/rmw_ret.h"
#include "nros/rmw_vtable.h"
#include "nros_rmw_xrce.h"

#include <stdio.h>
#include <string.h>
#include <uxr/client/util/time.h>

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

rmw_ret_t nros_rmw_cffi_register_named(const char* name, const nros_rmw_vtable_t* vtable) {
    (void)name;
    (void)vtable;
    return NROS_RMW_RET_OK;
}

static int g_edges = 0;
static void on_edge(const void* user, size_t n) {
    (void)user;
    (void)n;
    g_edges++;
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
    uint8_t buf[4096];
    size_t len = xrce_graph_serialize(&peer->graph, buf, sizeof(buf));
    ucdrBuffer ub;
    ucdr_init_buffer(&ub, buf, len);
    (void)xrce_graph_on_topic(st, uxr_object_id(st->graph.reader_id, UXR_DATAREADER_ID), &ub, len);
}

static bool has_peer(const xrce_graph* g, uint8_t prefix_byte) {
    for (const xrce_graph_peer* p = g->peers; p != NULL; p = p->next) {
        if (p->gid[0] == prefix_byte) {
            return true;
        }
    }
    return false;
}

int main(void) {
    printf("graph re-sync: stale participants dropped, present ones kept\n");

    xrce_session_state_t* st = new_state(0x11);
    xrce_session_state_t* a = new_state(0x22);
    xrce_session_state_t* b = new_state(0x33);
    if (st == NULL || a == NULL || b == NULL) {
        return 1;
    }
    /* The reader as `graph_ensure_reader` leaves it: created and confirmed,
     * generation 1. No transport in this test. */
    st->graph.reader_active = true;
    st->graph.reader_id = 77;
    st->graph.sync_gen = 1;
    st->graph.confirmed_gen = 1;
    st->graph.resync_req = UXR_INVALID_REQUEST_ID;
    st->graph.on_change = on_edge;

    rmw_session_t sa, sb;
    memset(&sa, 0, sizeof(sa));
    memset(&sb, 0, sizeof(sb));
    sa.backend_data = a;
    sb.backend_data = b;
    rmw_node_t na, nb;
    memset(&na, 0, sizeof(na));
    memset(&nb, 0, sizeof(nb));
    na.session = &sa;
    nb.session = &sb;
    (void)xrce_node_create(&sa, "stays", "", &na);
    (void)xrce_node_create(&sb, "leaves", "", &nb);

    deliver(st, a);
    deliver(st, b);
    CHECK(st->graph.n_peers == 2 && has_peer(&st->graph, 0x22) && has_peer(&st->graph, 0x33),
          "generation 1 delivered both participants");

    /* A re-sync: the re-created reader re-delivers A only. B left and its last
     * sample never arrived. */
    st->graph.sync_gen = 2;
    st->graph.confirmed_gen = 2;
    deliver(st, a);
    int before = g_edges;
    uint32_t dropped = xrce_graph_sweep(&st->graph);
    CHECK(dropped == 1 && st->graph.n_peers == 1 && has_peer(&st->graph, 0x22) &&
              !has_peer(&st->graph, 0x33),
          "a participant the new generation did not re-deliver is dropped; the other is kept");
    CHECK(g_edges == before + 1, "the sweep fires the graph-change edge once");

    before = g_edges;
    CHECK(xrce_graph_sweep(&st->graph) == 0 && g_edges == before && st->graph.n_peers == 1,
          "a sweep with nothing stale drops nothing and fires nothing");

    /* The re-create's status. */
    st->graph.sync_gen = 3;
    st->graph.resync_req = 9;
    uint32_t counter = st->graph.counter;
    xrce_graph_on_status(st, 8, UXR_STATUS_OK);
    CHECK(st->graph.resync_req == 9 && st->graph.confirmed_gen == 2,
          "a status for another request is not the re-sync's");
    xrce_graph_on_status(st, 9, UXR_STATUS_OK);
    CHECK(st->graph.resync_req == UXR_INVALID_REQUEST_ID && st->graph.confirmed_gen == 3 &&
              st->graph.counter == counter + 1 && st->graph.counter_known,
          "a confirmed re-create confirms the generation and claims one endpoint key");

    st->graph.sync_gen = 4;
    st->graph.resync_req = 10;
    xrce_graph_on_status(st, 10, UXR_STATUS_ERR_DDS_ERROR);
    CHECK(st->graph.confirmed_gen == 3 && !st->graph.counter_known,
          "a refused re-create confirms nothing and leaves the key count unknown");

    /* Not due yet: nothing changes. */
    st->graph.counter_known = true;
    st->graph.resync_req = UXR_INVALID_REQUEST_ID;
    st->graph.resync_at_ms = uxr_millis();
    xrce_graph_resync(st);
    CHECK(st->graph.sync_gen == 4 && st->graph.resync_req == UXR_INVALID_REQUEST_ID,
          "a re-sync before its period is due does nothing");

    /* No reader: nothing at all, however long it has been. */
    xrce_session_state_t* idle = new_state(0x44);
    if (idle == NULL) {
        return 1;
    }
    idle->graph.resync_at_ms = 0;
    xrce_graph_resync(idle);
    CHECK(idle->graph.sync_gen == 0 && idle->graph.resync_req == UXR_INVALID_REQUEST_ID,
          "with no graph reader the re-sync sends nothing");

    xrce_graph_release(st);
    xrce_graph_release(a);
    xrce_graph_release(b);
    xrce_graph_release(idle);
    nros_xrce_free(st);
    nros_xrce_free(a);
    nros_xrce_free(b);
    nros_xrce_free(idle);
    if (failures != 0) {
        printf("graph_resync: %d failure(s)\n", failures);
        return 1;
    }
    printf("graph_resync: OK\n");
    return 0;
}

/* Issue 2110 — `get_gid_for_publisher` / `get_gid_for_client`.
 *
 * No agent. The slots answer the DDS GID graph.c derives for a TRACKED endpoint
 * (probed prefix + Fast-DDS key + kind byte, zero-extended to 24), the same
 * bytes `ros_discovery_info` lists for it, and UNSUPPORTED for an endpoint that
 * was never tracked — never a guessed prefix. A client's identity is its
 * request WRITER, not its reply reader.
 *
 * The live half — a stock `ros2 topic info --verbose` printing the GID our
 * `get_gid()` reports — is `xrce_ros2_interop::test_xrce_publisher_gid_is_the_one_a_stock_graph_reports`.
 */

#include "internal.h"
#include "nros/rmw_ret.h"
#include "nros/rmw_vtable.h"
#include "nros_rmw_xrce.h"

#include <stdio.h>
#include <stdlib.h>
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

/* The vtable is static in `vtable.c`; the registry hand-off is how it leaves,
 * so the stub records it (as `smoke.c` does). */
static const nros_rmw_vtable_t* g_vtable = NULL;
rmw_ret_t nros_rmw_cffi_register_named(const char* name, const nros_rmw_vtable_t* vtable) {
    (void)name;
    g_vtable = vtable;
    return NROS_RMW_RET_OK;
}

static const uint8_t kPrefix[12] = {0x01, 0x0f, 0x40, 0xdc, 0xba, 0x40,
                                    0x20, 0xfa, 0x00, 0x00, 0x02, 0x00};

static void want_gid(uint32_t key, uint8_t kind, uint8_t out[RMW_GID_STORAGE_SIZE]) {
    memset(out, 0, RMW_GID_STORAGE_SIZE);
    memcpy(out, kPrefix, sizeof(kPrefix));
    out[12] = (uint8_t)(key >> 16);
    out[13] = (uint8_t)(key >> 8);
    out[14] = (uint8_t)key;
    out[15] = kind;
}

/* A gid pre-filled with garbage, so a slot that writes nothing is caught. */
static void poison(rmw_gid_t* gid) {
    memset(gid->data, 0xEE, sizeof(gid->data));
    gid->implementation_identifier = NULL;
}

int main(void) {
    printf("get_gid_for_publisher / get_gid_for_client\n");
    CHECK(nros_rmw_xrce_register() == NROS_RMW_RET_OK && g_vtable != NULL, "vtable registered");
    if (g_vtable == NULL) {
        return 1;
    }
    CHECK(g_vtable->get_gid_for_publisher != NULL, "get_gid_for_publisher is filled");
    CHECK(g_vtable->get_gid_for_client != NULL, "get_gid_for_client is filled");
    if (g_vtable->get_gid_for_publisher == NULL || g_vtable->get_gid_for_client == NULL) {
        return 1;
    }
    const char* ident = g_vtable->get_implementation_identifier();

    xrce_session_state_t* st =
        (xrce_session_state_t*)nros_xrce_calloc(1, sizeof(xrce_session_state_t));
    if (st == NULL) {
        return 1;
    }
    memcpy(st->graph.prefix, kPrefix, sizeof(kPrefix));
    st->graph.counter = 4;
    st->graph.counter_known = true;

    rmw_session_t session;
    memset(&session, 0, sizeof(session));
    session.backend_data = st;
    rmw_node_t node;
    memset(&node, 0, sizeof(node));
    node.name = "talker";
    node.namespace_ = "";
    node.session = &session;
    CHECK(xrce_node_create(&session, "talker", "", &node) == NROS_RMW_RET_OK, "create_node");

    /* A tracked publisher (key 5) and a tracked client (writer 6, reader 7),
     * claimed and tracked exactly as publisher.c / service.c do. */
    xrce_publisher_state ps;
    memset(&ps, 0, sizeof(ps));
    ps.session_state = st;
    uint32_t key = 0;
    CHECK(xrce_graph_claim(st, 1, true, &key) && key == 5, "publisher claims key 5");
    xrce_graph_track(st, &ps.graph_writer, &node, key, XRCE_GRAPH_WRITER);
    rmw_publisher_t pub;
    memset(&pub, 0, sizeof(pub));
    pub.backend_data = &ps;

    xrce_service_client_state cs;
    memset(&cs, 0, sizeof(cs));
    cs.session_state = st;
    CHECK(xrce_graph_claim(st, 2, true, &key) && key == 6, "client claims keys 6, 7");
    xrce_graph_track(st, &cs.graph_writer, &node, key, XRCE_GRAPH_WRITER);
    xrce_graph_track(st, &cs.graph_reader, &node, key + 1u, XRCE_GRAPH_READER);
    rmw_client_t cli;
    memset(&cli, 0, sizeof(cli));
    cli.backend_data = &cs;

    rmw_gid_t gid;
    uint8_t want[RMW_GID_STORAGE_SIZE];

    poison(&gid);
    want_gid(5, XRCE_GRAPH_WRITER, want);
    CHECK(g_vtable->get_gid_for_publisher(&pub, &gid) == NROS_RMW_RET_OK &&
              memcmp(gid.data, want, sizeof(want)) == 0,
          "publisher gid = prefix + key 5 + writer kind 0x03, zero-extended to 24");
    CHECK(gid.implementation_identifier != NULL && ident != NULL &&
              strcmp(gid.implementation_identifier, ident) == 0,
          "...stamped with the backend's own identifier");

    poison(&gid);
    want_gid(6, XRCE_GRAPH_WRITER, want);
    CHECK(g_vtable->get_gid_for_client(&cli, &gid) == NROS_RMW_RET_OK &&
              memcmp(gid.data, want, sizeof(want)) == 0,
          "client gid = its REQUEST writer (key 6), not its reply reader (7)");

    /* The bytes `ros_discovery_info` lists for the publisher are the same: the
     * sample's first node-level writer GID. */
    uint8_t buf[512];
    size_t len = xrce_graph_serialize(&st->graph, buf, sizeof(buf));
    poison(&gid);
    (void)g_vtable->get_gid_for_publisher(&pub, &gid);
    bool listed = false;
    for (size_t i = 0; len >= 16 && i + 16 <= len; ++i) {
        if (memcmp(buf + i, gid.data, 16) == 0 && i != 0) {
            listed = true;
            break;
        }
    }
    CHECK(listed, "the publisher gid is one ros_discovery_info lists");

    /* Untracked: the probe went unanswered, or a create failed and withheld
     * the key. No GID is known, so none is invented. */
    xrce_publisher_state untracked;
    memset(&untracked, 0, sizeof(untracked));
    untracked.session_state = st;
    rmw_publisher_t upub;
    memset(&upub, 0, sizeof(upub));
    upub.backend_data = &untracked;
    poison(&gid);
    CHECK(g_vtable->get_gid_for_publisher(&upub, &gid) == NROS_RMW_RET_UNSUPPORTED,
          "an untracked publisher answers UNSUPPORTED");
    xrce_service_client_state ucs;
    memset(&ucs, 0, sizeof(ucs));
    ucs.session_state = st;
    rmw_client_t ucli;
    memset(&ucli, 0, sizeof(ucli));
    ucli.backend_data = &ucs;
    CHECK(g_vtable->get_gid_for_client(&ucli, &gid) == NROS_RMW_RET_UNSUPPORTED,
          "an untracked client answers UNSUPPORTED");

    CHECK(g_vtable->get_gid_for_publisher(&pub, NULL) == NROS_RMW_RET_INVALID_ARGUMENT &&
              g_vtable->get_gid_for_client(NULL, &gid) == NROS_RMW_RET_INVALID_ARGUMENT,
          "NULL arguments are INVALID_ARGUMENT");

    xrce_graph_untrack(st, &ps.graph_writer);
    xrce_graph_untrack(st, &cs.graph_writer);
    xrce_graph_untrack(st, &cs.graph_reader);
    poison(&gid);
    CHECK(g_vtable->get_gid_for_publisher(&pub, &gid) == NROS_RMW_RET_UNSUPPORTED,
          "after untrack (destroy) the publisher is no longer known");
    (void)xrce_node_destroy(&node);
    xrce_graph_release(st);
    nros_xrce_free(st);

    if (failures != 0) {
        printf("gid: %d failure(s)\n", failures);
        return 1;
    }
    printf("gid: OK\n");
    return 0;
}

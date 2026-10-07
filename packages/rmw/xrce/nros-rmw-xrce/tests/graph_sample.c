/* Issue 1292 — the `ros_discovery_info` sample an XRCE session publishes.
 *
 * No agent. The live half (a stock `ros2 node list` / `ros2 node info` against
 * an image through the pinned Agent) is `rust_multi_node_per_node_graph`'s XRCE
 * case; this file pins the parts of graph.c that need no wire:
 *
 *   - the sample's layout: participant GID, one NodeEntitiesInfo per node in
 *     creation order, each node's reader GIDs then writer GIDs, every GID the
 *     probed prefix + a 24-bit key + the Fast-DDS kind byte, Humble's 24 bytes;
 *   - the counting rule: a successful create hands out consecutive keys, a
 *     FAILED one makes the count unknown and hands out nothing until a probe
 *     re-reads it — attributing a guessed GID is the defect this replaces;
 *   - lifetimes: a destroyed node takes its entry out of the sample and leaves
 *     its endpoints attributed to nobody, not to freed memory;
 *   - the probe answer: only a request carrying this session's nonce is
 *     believed, because every nano-ros XRCE image probes on the same names. */

#include "internal.h"
#include "nros/rmw_ret.h"

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

static const uint8_t kPrefix[12] = {0x01, 0x0f, 0x40, 0xdc, 0xba, 0x40,
                                    0x20, 0xfa, 0x00, 0x00, 0x02, 0x00};

static xrce_session_state_t* make_session(void) {
    xrce_session_state_t* st =
        (xrce_session_state_t*)nros_xrce_calloc(1, sizeof(xrce_session_state_t));
    if (st == NULL) {
        printf("  FAIL: could not allocate a session state\n");
        exit(1);
    }
    memcpy(st->graph.prefix, kPrefix, sizeof(kPrefix));
    st->graph.counter = 4; /* where the probe left it in the 2026-10-07 run */
    st->graph.counter_known = true;
    return st;
}

static bool read_gid(ucdrBuffer* ub, uint32_t key, uint8_t kind) {
    uint8_t gid[24];
    if (!ucdr_deserialize_array_uint8_t(ub, gid, sizeof(gid))) {
        return false;
    }
    uint8_t want[24] = {0};
    memcpy(want, kPrefix, sizeof(kPrefix));
    want[12] = (uint8_t)(key >> 16);
    want[13] = (uint8_t)(key >> 8);
    want[14] = (uint8_t)key;
    want[15] = kind;
    return memcmp(gid, want, sizeof(gid)) == 0;
}

static bool read_str(ucdrBuffer* ub, const char* want) {
    char got[300];
    return ucdr_deserialize_string(ub, got, sizeof(got)) && strcmp(got, want) == 0;
}

static uint32_t read_u32(ucdrBuffer* ub) {
    uint32_t v = 0xFFFFFFFFu;
    (void)ucdr_deserialize_uint32_t(ub, &v);
    return v;
}

int main(void) {
    printf("ros_discovery_info sample, counting rule and lifetimes\n");
    xrce_session_state_t* st = make_session();
    rmw_session_t session;
    memset(&session, 0, sizeof(session));
    session.backend_data = st;

    rmw_node_t talker, listener;
    memset(&talker, 0, sizeof(talker));
    memset(&listener, 0, sizeof(listener));
    talker.name = "talker";
    talker.namespace_ = "";
    talker.session = &session;
    listener.name = "listener";
    listener.namespace_ = "/ns";
    listener.session = &session;
    CHECK(xrce_node_create(&session, "talker", "", &talker) == NROS_RMW_RET_OK,
          "create_node talker");
    CHECK(xrce_node_create(&session, "listener", "/ns", &listener) == NROS_RMW_RET_OK,
          "create_node listener");

    char long_name[300];
    memset(long_name, 'n', sizeof(long_name) - 1);
    long_name[sizeof(long_name) - 1] = '\0';
    rmw_node_t too_long;
    memset(&too_long, 0, sizeof(too_long));
    CHECK(xrce_node_create(&session, long_name, "", &too_long) == NROS_RMW_RET_INVALID_ARGUMENT,
          "a name string<256> cannot carry is refused, not truncated");

    /* A publisher on talker (1 key), a service on listener (2 keys). */
    xrce_graph_endpoint pub_w, srv_w, srv_r;
    memset(&pub_w, 0, sizeof(pub_w));
    memset(&srv_w, 0, sizeof(srv_w));
    memset(&srv_r, 0, sizeof(srv_r));
    uint32_t key = 0;
    CHECK(xrce_graph_claim(st, 1, true, &key) && key == 5, "first endpoint takes counter + 1");
    xrce_graph_track(st, &pub_w, &talker, key, XRCE_GRAPH_WRITER);
    CHECK(xrce_graph_claim(st, 2, true, &key) && key == 6, "a replier takes the next two");
    xrce_graph_track(st, &srv_w, &listener, key, XRCE_GRAPH_WRITER);
    xrce_graph_track(st, &srv_r, &listener, key + 1u, XRCE_GRAPH_READER);
    CHECK(st->graph.counter == 7, "counter advanced by every endpoint");

    uint8_t buf[1024];
    size_t len = xrce_graph_serialize(&st->graph, buf, sizeof(buf));
    CHECK(len > 0 && len <= xrce_graph_sample_bound(&st->graph),
          "sample serializes within its computed bound");
    ucdrBuffer ub;
    ucdr_init_buffer(&ub, buf, len);
    CHECK(read_gid(&ub, 0x000001u, 0xC1u), "participant GID = prefix + 000001c1");
    CHECK(read_u32(&ub) == 2, "two NodeEntitiesInfo");
    CHECK(read_str(&ub, "/") && read_str(&ub, "talker"), "node 1 = / talker (empty ns -> /)");
    CHECK(read_u32(&ub) == 0, "talker: no readers");
    CHECK(read_u32(&ub) == 1 && read_gid(&ub, 5, XRCE_GRAPH_WRITER), "talker: writer key 5");
    CHECK(read_str(&ub, "/ns") && read_str(&ub, "listener"), "node 2 = /ns listener");
    CHECK(read_u32(&ub) == 1 && read_gid(&ub, 7, XRCE_GRAPH_READER),
          "listener: request reader key 7");
    CHECK(read_u32(&ub) == 1 && read_gid(&ub, 6, XRCE_GRAPH_WRITER),
          "listener: reply writer key 6");
    CHECK(!ub.error && ucdr_buffer_length(&ub) == len, "nothing left over");

    /* The counting rule. */
    CHECK(!xrce_graph_claim(st, 1, false, &key), "a failed create hands out no key");
    CHECK(!st->graph.counter_known, "...and leaves the count unknown");
    CHECK(!xrce_graph_claim(st, 1, true, &key), "no key after that until a probe re-reads it");

    /* The probe answer: wrong nonce ignored, right nonce believed. */
    memcpy(st->graph.probe_nonce, "nonce-01", 8);
    st->graph.probe_replier_id = 42;
    SampleIdentity sid;
    memset(&sid, 0, sizeof(sid));
    memset(sid.writer_guid.guidPrefix.data, 0xAB, 12);
    sid.writer_guid.entityId.entityKey[2] = 9;
    uint8_t other[8];
    memcpy(other, "nonce-02", 8);
    ucdrBuffer req;
    ucdr_init_buffer(&req, other, sizeof(other));
    CHECK(xrce_graph_on_request(st, uxr_object_id(42, UXR_REPLIER_ID), &sid, &req, 8) &&
              !st->graph.probe_seen,
          "another image's probe request is consumed and NOT believed");
    uint8_t mine[8];
    memcpy(mine, "nonce-01", 8);
    ucdr_init_buffer(&req, mine, sizeof(mine));
    CHECK(xrce_graph_on_request(st, uxr_object_id(42, UXR_REPLIER_ID), &sid, &req, 8) &&
              st->graph.probe_seen && st->graph.probe_key == 9 && st->graph.prefix[0] == 0xAB,
          "this session's probe request yields the prefix and the writer key");
    ucdr_init_buffer(&req, mine, sizeof(mine));
    CHECK(!xrce_graph_on_request(st, uxr_object_id(43, UXR_REPLIER_ID), &sid, &req, 8),
          "a real service's request is not the probe's");
    memcpy(st->graph.prefix, kPrefix, sizeof(kPrefix));

    /* Lifetimes. */
    CHECK(xrce_node_destroy(&talker) == NROS_RMW_RET_OK && talker.backend_data == NULL,
          "destroy_node talker");
    CHECK(pub_w.node == NULL, "its publisher is attributed to nobody, not to freed memory");
    len = xrce_graph_serialize(&st->graph, buf, sizeof(buf));
    ucdr_init_buffer(&ub, buf, len);
    (void)read_gid(&ub, 1, 0xC1u);
    CHECK(read_u32(&ub) == 1 && read_str(&ub, "/ns") && read_str(&ub, "listener"),
          "the sample now lists listener alone");
    xrce_graph_untrack(st, &pub_w);
    xrce_graph_untrack(st, &srv_w);
    xrce_graph_untrack(st, &srv_r);
    CHECK(st->graph.endpoints == NULL, "every endpoint untracked");
    CHECK(xrce_node_destroy(&listener) == NROS_RMW_RET_OK, "destroy_node listener");
    CHECK(st->graph.nodes == NULL, "no node left");

    xrce_graph_release(st);
    nros_xrce_free(st);

    if (failures != 0) {
        printf("graph_sample: %d failure(s)\n", failures);
        return 1;
    }
    printf("graph_sample: OK\n");
    return 0;
}

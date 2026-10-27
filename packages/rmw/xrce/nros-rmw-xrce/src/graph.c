/* Issue 1292 — the ROS 2 graph an XRCE image presents to a stock peer.
 *
 * A stock `rmw_fastrtps_cpp` / `rmw_cyclonedds_cpp` learns node names ONLY from
 * `rmw_dds_common::msg::ParticipantEntitiesInfo` samples on `ros_discovery_info`,
 * and attributes an endpoint to a node only when that sample lists the
 * endpoint's DDS GID under the node. Zenoh and Cyclone images already publish
 * this (issue 1269); an XRCE image published nothing, so `ros2 node list`
 * showed none of its nodes and every endpoint read `_CREATED_BY_BARE_DDS_APP_`
 * (measured 2026-10-06, recorded in the issue).
 *
 * WHERE THE GIDs COME FROM — the part the XRCE protocol does not provide.
 *
 * The Agent creates every DDS entity and assigns its GUID; a CREATE status
 * carries none, and GET_INFO answers for the Agent's root only
 * (`Processor::process_get_info_submessage` -> `root_.get_info`). Three routes
 * were MEASURED against the pinned Agent (2.4.3-nros1, Fast-DDS 2.14.6) with a
 * Humble `rmw_fastrtps_cpp` peer, issue 1292 "Measured — the GUID source":
 *
 *   1. An XML participant with an explicit `<prefix>` plus XML endpoints with
 *      `<entityID>`. Fast-DDS honours both, and the peer attributes the
 *      endpoint correctly. REJECTED: the Agent builds an XML participant from
 *      a default-constructed `ParticipantAttributes`
 *      (`FastDDSParticipant::create_by_xml` -> `set_qos_from_attributes`), so
 *      the Agent operator's FASTRTPS_DEFAULT_PROFILES_FILE is discarded for
 *      it: with the issue-1009 loopback profile on both halves the peer saw
 *      NOTHING, not even the topic. Fast-DDS also refuses an XML `<entityID>`
 *      above 255 (`XMLElementParser.cpp`, `i > 255`).
 *   2. A BIN participant (what this backend always created, so the operator's
 *      profile still applies) whose endpoints the Agent numbers itself. This is
 *      the route taken, in two halves:
 *
 *      a. THE PREFIX, by asking DDS. A replier's request callback hands the
 *         client the request's `SampleIdentity`, and its `writer_guid` is the
 *         GUID of the requester's request DataWriter — on OUR participant, so
 *         its prefix is our participant's prefix. The probe is a private
 *         requester + replier pair, one self-addressed request carrying a
 *         nonce, then both deleted (`graph_probe`).
 *      b. THE ENTITY KEYS, by counting. Fast-DDS numbers user endpoints from
 *         ONE per-participant counter (`DomainParticipantImpl::id_counter_`),
 *         advanced by exactly the DataWriterImpl / DataReaderImpl constructors
 *         and nothing else (grep `id_counter()` in Fast-DDS 2.14). Every
 *         endpoint on this participant is one this backend asked for, in an
 *         order it chose, so the key of each is the previous one plus one. The
 *         probe also tells us where the counter stands (the requester's writer
 *         key), so the count starts from a measurement, not an assumption.
 *
 *      Measured with both halves loopback-pinned: `ros2 node list` showed the
 *      node, `ros2 topic info -v` attributed the publisher to it, and the GID
 *      it printed was the probed prefix plus the predicted key.
 *
 * WHEN THE COUNT CAN BE WRONG, and what happens then. A create whose confirm
 * fails may or may not have constructed its DDS endpoints Agent-side (a
 * DataWriterImpl is constructed — and numbered — before `enable` can fail), so
 * after any failed endpoint create the count is UNKNOWN, and the next endpoint
 * create re-probes first (`xrce_graph_ensure_counter`). A probe that gets no
 * answer turns attribution off for the session with one logged line, rather
 * than publishing GIDs that might name someone else's endpoints.
 *
 * WHAT IS NOT COVERED. The Gid is the Humble layout (24 bytes), as Cyclone's
 * `graph.cpp` assumes; an Iron+ peer reads a 16-byte Gid. Same limit, same
 * place, not widened here. */

#include "internal.h"

#include "nros/rmw_ret.h"

#include <uxr/client/client.h>
#include <uxr/client/core/session/object_id.h>
#include <ucdr/microcdr.h>

#include <stdio.h>
#include <string.h>

#define XRCE_GRAPH_TOPIC "ros_discovery_info"
#define XRCE_GRAPH_TYPE "rmw_dds_common::msg::dds_::ParticipantEntitiesInfo_"

/* The probe's DDS names. Deliberately NOT ROS-mangled (no `rq/` / `rr/`), so a
 * ROS graph cache never lists them as a service, and short-lived: both entities
 * are deleted as soon as the answer arrives. */
#define XRCE_GRAPH_PROBE_SERVICE "nros_xrce_guid_probe"
#define XRCE_GRAPH_PROBE_TYPE "nros_xrce::GuidProbe_"
#define XRCE_GRAPH_PROBE_REQ_TOPIC "nros_xrce_guid_probe_q"
#define XRCE_GRAPH_PROBE_REP_TOPIC "nros_xrce_guid_probe_r"

/* `string<256>` in rmw_dds_common's NodeEntitiesInfo. */
#define XRCE_GRAPH_NAME_CAP 256u

#define XRCE_GRAPH_GID_LEN 24u
#define XRCE_GRAPH_PARTICIPANT_KEY 0x000001u
#define XRCE_GRAPH_PARTICIPANT_KIND 0xC1u

static void graph_log(int severity, const char* msg) {
    static const char kLogger[] = "nros_rmw_xrce";
    nros_platform_log_write((uint8_t)severity, (const uint8_t*)kLogger, sizeof(kLogger) - 1,
                            (const uint8_t*)msg, (uintptr_t)strlen(msg));
}

static const char* wire_ns(const char* ns) {
    return (ns != NULL && ns[0] != '\0') ? ns : "/";
}

static void gid_of(const xrce_graph* g, uint32_t key, uint8_t kind,
                   uint8_t out[XRCE_GRAPH_GID_LEN]) {
    memset(out, 0, XRCE_GRAPH_GID_LEN);
    memcpy(out, g->prefix, sizeof(g->prefix));
    out[12] = (uint8_t)(key >> 16);
    out[13] = (uint8_t)(key >> 8);
    out[14] = (uint8_t)key;
    out[15] = kind;
}

/* Issue 2106 — the graph-change edge: a local node came or went, or a remote
 * participant's `ros_discovery_info` sample changed the node table. */
static void graph_changed(const xrce_graph* g) {
    if (g->on_change != NULL) {
        g->on_change(g->on_change_user, 1);
    }
}

/* ---- The probe -------------------------------------------------------- */

bool xrce_graph_on_request(xrce_session_state_t* st, uxrObjectId object_id,
                           const SampleIdentity* sample_id, struct ucdrBuffer* ub, size_t len) {
    if (st == NULL || st->graph.probe_replier_id == 0 ||
        object_id.id != st->graph.probe_replier_id) {
        return false;
    }
    /* Ours by object id — consumed whatever it holds. Only a request carrying
     * THIS session's nonce is believed: the probe names are shared by every
     * nano-ros XRCE image on the domain, so another image's probe request can
     * reach this replier, and its writer GUID is that image's prefix. */
    uint8_t nonce[sizeof(st->graph.probe_nonce)];
    if (sample_id == NULL || ub == NULL || len != sizeof(nonce) ||
        !ucdr_deserialize_array_uint8_t(ub, nonce, sizeof(nonce)) ||
        memcmp(nonce, st->graph.probe_nonce, sizeof(nonce)) != 0) {
        return true;
    }
    memcpy(st->graph.prefix, sample_id->writer_guid.guidPrefix.data, sizeof(st->graph.prefix));
    st->graph.probe_key = ((uint32_t)sample_id->writer_guid.entityId.entityKey[0] << 16) |
                          ((uint32_t)sample_id->writer_guid.entityId.entityKey[1] << 8) |
                          (uint32_t)sample_id->writer_guid.entityId.entityKey[2];
    st->graph.probe_seen = true;
    return true;
}

static void graph_delete(xrce_session_state_t* st, uxrObjectId oid) {
    uint16_t req = uxr_buffer_delete_entity(&st->session, st->output_reliable, oid);
    uint8_t status = 0;
    (void)uxr_run_session_until_all_status(&st->session, XRCE_ENTITY_CREATION_TIMEOUT_MS, &req,
                                           &status, 1);
}

/* Learn the participant's GUID prefix and where the Agent's endpoint counter
 * stands. Returns true and sets `counter_known` on an answer. */
static bool graph_probe(xrce_session_state_t* st) {
    xrce_graph* g = &st->graph;
    g->counter_known = false;

    uxrQoS_t qos = {UXR_DURABILITY_VOLATILE, UXR_RELIABILITY_RELIABLE, UXR_HISTORY_KEEP_LAST, 1};
    uxrObjectId rp = xrce_alloc_entity_id(st, UXR_REPLIER_ID);
    uxrObjectId rq = xrce_alloc_entity_id(st, UXR_REQUESTER_ID);

    /* The replier FIRST and confirmed before the requester is created: the
     * counter after the probe is the requester's reader key, which is its
     * writer key + 1 only if the requester's two endpoints were numbered last. */
    uint16_t req = uxr_buffer_create_replier_bin(
        &st->session, st->output_reliable, rp, st->participant_oid, XRCE_GRAPH_PROBE_SERVICE,
        XRCE_GRAPH_PROBE_TYPE, XRCE_GRAPH_PROBE_TYPE, XRCE_GRAPH_PROBE_REQ_TOPIC,
        XRCE_GRAPH_PROBE_REP_TOPIC, qos, UXR_REPLACE);
    uint8_t status = 0;
    if (xrce_confirm_entities(st, &req, &status, 1) != NROS_RMW_RET_OK) {
        return false;
    }
    req = uxr_buffer_create_requester_bin(
        &st->session, st->output_reliable, rq, st->participant_oid, XRCE_GRAPH_PROBE_SERVICE,
        XRCE_GRAPH_PROBE_TYPE, XRCE_GRAPH_PROBE_TYPE, XRCE_GRAPH_PROBE_REQ_TOPIC,
        XRCE_GRAPH_PROBE_REP_TOPIC, qos, UXR_REPLACE);
    if (xrce_confirm_entities(st, &req, &status, 1) != NROS_RMW_RET_OK) {
        graph_delete(st, rp);
        return false;
    }

    /* A nonce no other session can produce: the session key plus the clock. */
    uint64_t now = nros_platform_clock_ns();
    uint32_t key = st->session.info.key[0] | ((uint32_t)st->session.info.key[1] << 8) |
                   ((uint32_t)st->session.info.key[2] << 16) |
                   ((uint32_t)st->session.info.key[3] << 24);
    for (size_t i = 0; i < 4; ++i) {
        g->probe_nonce[i] = (uint8_t)(key >> (8 * i));
        g->probe_nonce[4 + i] = (uint8_t)(now >> (8 * i));
    }
    g->probe_seen = false;
    g->probe_replier_id = rp.id;

    uxrDeliveryControl delivery = {
        .max_samples = UXR_MAX_SAMPLES_UNLIMITED,
        .max_elapsed_time = UXR_MAX_ELAPSED_TIME_UNLIMITED,
        .max_bytes_per_second = UXR_MAX_BYTES_PER_SECOND_UNLIMITED,
        .min_pace_period = 0,
    };
    (void)uxr_buffer_request_data(&st->session, st->output_reliable, rp, st->input_reliable,
                                  &delivery);

    /* Resend until answered: a request written before the replier's reader has
     * matched the requester's writer is lost (VOLATILE), and matching inside
     * one participant is fast but not instantaneous. */
    uint64_t start_ms = nros_platform_clock_ns() / 1000000u;
    while (!g->probe_seen) {
        uint64_t now_ms = nros_platform_clock_ns() / 1000000u;
        if (now_ms - start_ms >= XRCE_GRAPH_PROBE_TIMEOUT_MS) {
            break;
        }
        (void)uxr_buffer_request(&st->session, st->output_reliable, rq, g->probe_nonce,
                                 sizeof(g->probe_nonce));
        (void)uxr_run_session_time(&st->session, XRCE_GRAPH_PROBE_RETRY_MS);
    }
    g->probe_replier_id = 0;

    graph_delete(st, rq);
    graph_delete(st, rp);

    if (!g->probe_seen) {
        return false;
    }
    /* The requester's writer carries `probe_key`; its reader, created right
     * after it, took the next one. */
    g->counter = g->probe_key + 1u;
    g->counter_known = true;
    return true;
}

/* ---- Session lifecycle ------------------------------------------------ */

void xrce_graph_open(xrce_session_state_t* st) {
    if (st == NULL) {
        return;
    }
    xrce_graph* g = &st->graph;
    if (!graph_probe(st)) {
        graph_log(XRCE_LOG_ERROR,
                  "ros_discovery_info is OFF for this session: the GUID probe got no answer "
                  "from the Agent, so this image's endpoint GIDs are unknown. Data still "
                  "flows; `ros2 node list` will not show this image's nodes (issue 1292).");
        return;
    }

    uxrObjectId topic_oid = xrce_alloc_entity_id(st, UXR_TOPIC_ID);
    uxrObjectId pub_oid = xrce_alloc_entity_id(st, UXR_PUBLISHER_ID);
    uxrObjectId dw_oid = xrce_alloc_entity_id(st, UXR_DATAWRITER_ID);
    /* rmw_dds_common's own profile for the topic: RELIABLE, TRANSIENT_LOCAL,
     * KEEP_LAST(1), so a peer that joins later still reads the current sample. */
    uxrQoS_t qos = {UXR_DURABILITY_TRANSIENT_LOCAL, UXR_RELIABILITY_RELIABLE, UXR_HISTORY_KEEP_LAST,
                    1};
    uint16_t reqs[3];
    reqs[0] = uxr_buffer_create_topic_bin(&st->session, st->output_reliable, topic_oid,
                                          st->participant_oid, XRCE_GRAPH_TOPIC, XRCE_GRAPH_TYPE,
                                          UXR_REPLACE);
    reqs[1] = uxr_buffer_create_publisher_bin(&st->session, st->output_reliable, pub_oid,
                                              st->participant_oid, UXR_REPLACE);
    reqs[2] = uxr_buffer_create_datawriter_bin(&st->session, st->output_reliable, dw_oid, pub_oid,
                                               topic_oid, qos, UXR_REPLACE);
    uint8_t statuses[3] = {0, 0, 0};
    bool ok = xrce_confirm_entities(st, reqs, statuses, 3) == NROS_RMW_RET_OK;
    /* The graph writer is an endpoint like any other: it took a key, and is
     * listed under no node, exactly as rmw_fastrtps lists its own. */
    uint32_t unused_key = 0;
    (void)xrce_graph_claim(st, 1, ok, &unused_key);
    if (!ok) {
        graph_log(XRCE_LOG_ERROR,
                  "ros_discovery_info is OFF for this session: the Agent refused its "
                  "DataWriter. Data still flows; `ros2 node list` will not show this "
                  "image's nodes (issue 1292).");
        return;
    }
    g->writer_oid = dw_oid;
    g->active = true;
    g->dirty = true;
    xrce_graph_flush(st);
}

void xrce_graph_release(xrce_session_state_t* st) {
    if (st == NULL) {
        return;
    }
    xrce_graph_node* n = st->graph.nodes;
    while (n != NULL) {
        xrce_graph_node* next = n->next;
        nros_xrce_free(n);
        n = next;
    }
    st->graph.nodes = NULL;
    st->graph.endpoints = NULL;
    st->graph.active = false;
    xrce_graph_peer* p = st->graph.peers;
    while (p != NULL) {
        xrce_graph_peer* next = p->next;
        nros_xrce_free(p->names);
        nros_xrce_free(p);
        p = next;
    }
    st->graph.peers = NULL;
    st->graph.n_peers = 0;
    st->graph.reader_active = false;
    st->graph.on_change = NULL;
}

/* ---- Endpoint keys ---------------------------------------------------- */

void xrce_graph_ensure_counter(xrce_session_state_t* st) {
    if (st == NULL || !st->graph.active || st->graph.counter_known) {
        return;
    }
    if (!graph_probe(st)) {
        st->graph.active = false;
        graph_log(XRCE_LOG_ERROR,
                  "ros_discovery_info stopped updating: an endpoint create failed, so the "
                  "Agent's endpoint numbering had to be re-read, and the GUID probe got no "
                  "answer. Endpoints created from here on are not attributed to a node "
                  "(issue 1292).");
    }
}

bool xrce_graph_claim(xrce_session_state_t* st, unsigned count, bool created, uint32_t* first_key) {
    if (st == NULL || first_key == NULL) {
        return false;
    }
    xrce_graph* g = &st->graph;
    if (!created) {
        /* The Agent may have constructed — and numbered — some of these before
         * failing, so the count is no longer known. */
        g->counter_known = false;
        return false;
    }
    if (!g->counter_known) {
        return false;
    }
    *first_key = g->counter + 1u;
    g->counter += count;
    return true;
}

/* ---- Nodes ------------------------------------------------------------ */

static bool graph_has_node(const xrce_graph* g, const xrce_graph_node* rec) {
    for (const xrce_graph_node* n = g->nodes; n != NULL; n = n->next) {
        if (n == rec) {
            return true;
        }
    }
    return false;
}

static bool name_fits(const char* name, const char* ns) {
    return name != NULL && name[0] != '\0' && strlen(name) < XRCE_GRAPH_NAME_CAP &&
           strlen(wire_ns(ns)) < XRCE_GRAPH_NAME_CAP;
}

static xrce_graph_node* graph_add_node(xrce_graph* g, const char* name, const char* ns,
                                       bool implicit) {
    xrce_graph_node* n = (xrce_graph_node*)nros_xrce_calloc(1, sizeof(xrce_graph_node));
    if (n == NULL) {
        return NULL;
    }
    n->name = name;
    n->ns = ns;
    n->implicit = implicit;
    /* Appended, so the published order is creation order. */
    xrce_graph_node** tail = &g->nodes;
    while (*tail != NULL) {
        tail = &(*tail)->next;
    }
    *tail = n;
    g->dirty = true;
    return n;
}

/* The record an endpoint created on `node` belongs to. The runtime's path:
 * `create_node` ran and `backend_data` is our record. A caller that drives the
 * vtable directly may hand an endpoint a node it never declared (NULL
 * `backend_data` is a pure identity carrier in the ABI); its name and namespace
 * are still its identity, so it is recorded by them — Cyclone's
 * `graph_node_of` rule. */
static xrce_graph_node* graph_node_of(xrce_graph* g, const rmw_node_t* node) {
    if (node == NULL) {
        return NULL;
    }
    xrce_graph_node* rec = (xrce_graph_node*)node->backend_data;
    if (rec != NULL) {
        return graph_has_node(g, rec) ? rec : NULL;
    }
    if (!name_fits(node->name, node->namespace_)) {
        return NULL;
    }
    for (xrce_graph_node* n = g->nodes; n != NULL; n = n->next) {
        if (strcmp(n->name, node->name) == 0 &&
            strcmp(wire_ns(n->ns), wire_ns(node->namespace_)) == 0) {
            return n;
        }
    }
    return graph_add_node(g, node->name, node->namespace_, true);
}

static rmw_ret_t xrce_node_create_locked(rmw_session_t* session, const char* name,
                                         const char* namespace_, rmw_node_t* out) {
    if (session == NULL || out == NULL || name == NULL) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    xrce_session_state_t* st = (xrce_session_state_t*)session->backend_data;
    if (st == NULL) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    /* A name the wire type cannot carry is refused, not truncated: a truncated
     * name is a different node. */
    if (!name_fits(name, namespace_)) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    xrce_graph_node* rec = graph_add_node(&st->graph, name, namespace_, false);
    if (rec == NULL) {
        return NROS_RMW_RET_BAD_ALLOC;
    }
    out->backend_data = rec;
    graph_changed(&st->graph); /* issue 2106 */
    return NROS_RMW_RET_OK;
}

static rmw_ret_t xrce_node_destroy_locked(rmw_node_t* node) {
    if (node == NULL || node->backend_data == NULL || node->session == NULL) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    xrce_session_state_t* st = (xrce_session_state_t*)node->session->backend_data;
    if (st == NULL) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    xrce_graph* g = &st->graph;
    xrce_graph_node* rec = (xrce_graph_node*)node->backend_data;
    xrce_graph_node** link = &g->nodes;
    while (*link != NULL && *link != rec) {
        link = &(*link)->next;
    }
    if (*link == NULL) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    *link = rec->next;
    /* An endpoint that outlives its node stays alive and attributed to nobody,
     * rather than to freed memory. */
    for (xrce_graph_endpoint* ep = g->endpoints; ep != NULL; ep = ep->next) {
        if (ep->node == rec) {
            ep->node = NULL;
        }
    }
    nros_xrce_free(rec);
    node->backend_data = NULL;
    g->dirty = true;
    graph_changed(g); /* issue 2106 */
    return NROS_RMW_RET_OK;
}

/* ---- Endpoints -------------------------------------------------------- */

void xrce_graph_track(xrce_session_state_t* st, xrce_graph_endpoint* ep, const rmw_node_t* node,
                      uint32_t key, uint8_t kind) {
    if (st == NULL || ep == NULL || ep->linked) {
        return;
    }
    ep->node = graph_node_of(&st->graph, node);
    ep->key = key;
    ep->kind = kind;
    ep->next = st->graph.endpoints;
    st->graph.endpoints = ep;
    ep->linked = true;
    st->graph.dirty = true;
}

void xrce_graph_untrack(xrce_session_state_t* st, xrce_graph_endpoint* ep) {
    if (st == NULL || ep == NULL || !ep->linked) {
        return;
    }
    xrce_graph_endpoint** link = &st->graph.endpoints;
    while (*link != NULL && *link != ep) {
        link = &(*link)->next;
    }
    if (*link == ep) {
        *link = ep->next;
    }
    ep->next = NULL;
    ep->linked = false;
    st->graph.dirty = true;
}

/* Issue 2110 — the GID a stock peer knows this endpoint by: the bytes
 * `ros_discovery_info` lists for it, so the gid a caller is told and the gid a
 * peer sees cannot disagree (the issue-1495 rule). 16 bytes (prefix, key,
 * kind), zero-extended to the 24 `rmw_gid_t` carries — the shape Cyclone's
 * DDS GUID has, and the bytes `take_request_with_info` reports for a request
 * this endpoint wrote. An endpoint that was never TRACKED has no known GID —
 * attribution off after an unanswered probe, or a key withheld after a failed
 * create — and answers false rather than a fabricated prefix. */
bool xrce_graph_endpoint_gid(const xrce_session_state_t* st, const xrce_graph_endpoint* ep,
                             uint8_t out[XRCE_GRAPH_GID_LEN]) {
    if (st == NULL || ep == NULL || out == NULL || !ep->linked) {
        return false;
    }
    gid_of(&st->graph, ep->key, ep->kind, out);
    return true;
}

/* ---- The sample ------------------------------------------------------- */

static size_t graph_sample_bound(const xrce_graph* g) {
    /* Each CDR field may pad to 4; counting 3 bytes of padding per field
     * over-sizes the buffer slightly and never under-sizes it. */
    size_t size = XRCE_GRAPH_GID_LEN + 3u + 4u;
    for (const xrce_graph_node* n = g->nodes; n != NULL; n = n->next) {
        size += 3u + 4u + strlen(wire_ns(n->ns)) + 1u;
        size += 3u + 4u + strlen(n->name) + 1u;
        size += 2u * (3u + 4u);
    }
    for (const xrce_graph_endpoint* ep = g->endpoints; ep != NULL; ep = ep->next) {
        size += XRCE_GRAPH_GID_LEN;
    }
    return size;
}

static uint32_t graph_count(const xrce_graph* g, const xrce_graph_node* n, uint8_t kind) {
    uint32_t count = 0;
    for (const xrce_graph_endpoint* ep = g->endpoints; ep != NULL; ep = ep->next) {
        count += (ep->node == n && ep->kind == kind) ? 1u : 0u;
    }
    return count;
}

static void graph_serialize_gids(const xrce_graph* g, ucdrBuffer* ub, const xrce_graph_node* n,
                                 uint8_t kind) {
    (void)ucdr_serialize_uint32_t(ub, graph_count(g, n, kind));
    for (const xrce_graph_endpoint* ep = g->endpoints; ep != NULL; ep = ep->next) {
        if (ep->node == n && ep->kind == kind) {
            uint8_t gid[XRCE_GRAPH_GID_LEN];
            gid_of(g, ep->key, ep->kind, gid);
            (void)ucdr_serialize_array_uint8_t(ub, gid, sizeof(gid));
        }
    }
}

size_t xrce_graph_serialize(const xrce_graph* g, uint8_t* buf, size_t cap) {
    if (g == NULL || buf == NULL) {
        return 0;
    }
    ucdrBuffer ub;
    ucdr_init_buffer(&ub, buf, cap);
    uint8_t gid[XRCE_GRAPH_GID_LEN];
    gid_of(g, XRCE_GRAPH_PARTICIPANT_KEY, XRCE_GRAPH_PARTICIPANT_KIND, gid);
    (void)ucdr_serialize_array_uint8_t(&ub, gid, sizeof(gid));
    uint32_t n_nodes = 0;
    for (const xrce_graph_node* n = g->nodes; n != NULL; n = n->next) {
        ++n_nodes;
    }
    (void)ucdr_serialize_uint32_t(&ub, n_nodes);
    for (const xrce_graph_node* n = g->nodes; n != NULL; n = n->next) {
        (void)ucdr_serialize_string(&ub, wire_ns(n->ns));
        (void)ucdr_serialize_string(&ub, n->name);
        graph_serialize_gids(g, &ub, n, XRCE_GRAPH_READER);
        graph_serialize_gids(g, &ub, n, XRCE_GRAPH_WRITER);
    }
    return ub.error ? 0 : ucdr_buffer_length(&ub);
}

size_t xrce_graph_sample_bound(const xrce_graph* g) {
    return g == NULL ? 0 : graph_sample_bound(g);
}

void xrce_graph_flush(xrce_session_state_t* st) {
    if (st == NULL || !st->graph.active || !st->graph.dirty || xrce_session_is_closed(st)) {
        return;
    }
    xrce_graph* g = &st->graph;
    size_t cap = graph_sample_bound(g);
    uint8_t* buf = (uint8_t*)nros_xrce_calloc(1, cap);
    if (buf == NULL) {
        /* Stays dirty: the next drive_io tries again. */
        return;
    }
    size_t len = xrce_graph_serialize(g, buf, cap);
    uint16_t req = UXR_INVALID_REQUEST_ID;
    if (len > 0) {
        req = uxr_buffer_topic(&st->session, st->output_reliable, g->writer_oid, buf, len);
    }
    nros_xrce_free(buf);
    if (req != UXR_INVALID_REQUEST_ID) {
        g->dirty = false;
        (void)uxr_run_session_time(&st->session, 0);
        return;
    }
    /* `uxr_buffer_topic` refuses a sample larger than one reliable-stream slot
     * (one transport MTU). Say so once, with the number, instead of retrying a
     * sample that can never fit. */
    if (!g->warned_too_large) {
        char msg[256];
        (void)snprintf(msg, sizeof(msg),
                       "ros_discovery_info sample (%lu bytes) did not fit one XRCE stream "
                       "slot; raise NROS_XRCE_TRANSPORT_MTU (or NROS_XRCE_CUSTOM_TRANSPORT_MTU). "
                       "Nodes created since are not listed by `ros2 node list` (issue 1292).",
                       (unsigned long)len);
        graph_log(XRCE_LOG_ERROR, msg);
        g->warned_too_large = true;
    }
    g->dirty = false;
}

/* ---- Issue 2106: the read side -------------------------------------------
 *
 * Node names travel as DATA on `ros_discovery_info`, an ordinary topic an XRCE
 * client can subscribe to through the Agent like any other — which is how this
 * backend answers `get_node_names` although the protocol gives it no read on
 * the Agent participant's discovery state. Cyclone answers the same slot from
 * the same samples.
 *
 * BOUNDS, each one a property of the source rather than of this code:
 *
 *  - Only DDS-RMW participants publish `ros_discovery_info`. A zenoh peer
 *    bridged onto the domain does not, so the answer is "nodes of DDS
 *    participants" plus this session's own.
 *  - A participant that LEAVES without destroying its nodes stays listed: the
 *    client never sees DDS discovery loss (no dispose / unregister reaches an
 *    XRCE reader). A graceful exit publishes an empty node list first, and
 *    that removes it here.
 *  - The table is capped (`XRCE_GRAPH_MAX_PEERS` participants,
 *    `XRCE_GRAPH_MAX_PEER_NODES` nodes each). A sample past a cap is refused
 *    and `get_node_names` then answers ERROR rather than a silently short
 *    list.
 *  - The reader is created on FIRST USE (a query or a graph-change
 *    registration), and transient-local samples arrive on later `drive_io`s:
 *    the first call legitimately reports this session's nodes alone. Same
 *    warm-up as every other backend's graph. */

#define XRCE_GRAPH_READER_DEPTH 16u

/* Our own sample comes back to our own reader (same participant). The prefix
 * is known once a probe has answered; an all-zero prefix means it never did. */
static bool peer_is_self(const xrce_graph* g, const uint8_t gid[XRCE_GRAPH_GID_LEN]) {
    static const uint8_t kZero[sizeof(g->prefix)] = {0};
    return memcmp(g->prefix, kZero, sizeof(g->prefix)) != 0 &&
           memcmp(gid, g->prefix, sizeof(g->prefix)) == 0;
}

static xrce_graph_peer** peer_link(xrce_graph* g, const uint8_t gid[XRCE_GRAPH_GID_LEN]) {
    xrce_graph_peer** link = &g->peers;
    while (*link != NULL && memcmp((*link)->gid, gid, XRCE_GRAPH_GID_LEN) != 0) {
        link = &(*link)->next;
    }
    return link;
}

bool xrce_graph_ingest(xrce_graph* g, const uint8_t* buf, size_t len) {
    if (g == NULL || buf == NULL) {
        return false;
    }
    ucdrBuffer ub;
    ucdr_init_buffer(&ub, (uint8_t*)(uintptr_t)buf, len);
    uint8_t gid[XRCE_GRAPH_GID_LEN];
    uint32_t n_nodes = 0;
    if (!ucdr_deserialize_array_uint8_t(&ub, gid, sizeof(gid)) ||
        !ucdr_deserialize_uint32_t(&ub, &n_nodes)) {
        return false;
    }
    if (peer_is_self(g, gid)) {
        return true; /* our own sample: the local list is the source */
    }
    if (n_nodes > XRCE_GRAPH_MAX_PEER_NODES) {
        g->peers_overflowed = true;
        return false;
    }
    /* Names total at most the sample's size; pack them as ns\0name\0 pairs. */
    char* names = NULL;
    size_t used = 0;
    if (n_nodes > 0) {
        names = (char*)nros_xrce_calloc(1, len);
        if (names == NULL) {
            g->peers_overflowed = true;
            return false;
        }
    }
    for (uint32_t i = 0; i < n_nodes; ++i) {
        char tmp[XRCE_GRAPH_NAME_CAP];
        for (int part = 0; part < 2; ++part) {
            if (!ucdr_deserialize_string(&ub, tmp, sizeof(tmp))) {
                nros_xrce_free(names);
                return false;
            }
            size_t l = strlen(tmp) + 1u;
            if (used + l > len) {
                nros_xrce_free(names);
                return false;
            }
            memcpy(names + used, tmp, l);
            used += l;
        }
        for (int seq = 0; seq < 2; ++seq) {
            uint32_t n_gids = 0;
            if (!ucdr_deserialize_uint32_t(&ub, &n_gids) ||
                (size_t)n_gids > len / XRCE_GRAPH_GID_LEN) {
                nros_xrce_free(names);
                return false;
            }
            for (uint32_t k = 0; k < n_gids; ++k) {
                uint8_t skip[XRCE_GRAPH_GID_LEN];
                if (!ucdr_deserialize_array_uint8_t(&ub, skip, sizeof(skip))) {
                    nros_xrce_free(names);
                    return false;
                }
            }
        }
    }

    xrce_graph_peer** link = peer_link(g, gid);
    xrce_graph_peer* cur = *link;
    if (n_nodes == 0) {
        /* A participant with no nodes — a graceful exit's last word. */
        if (cur != NULL) {
            *link = cur->next;
            nros_xrce_free(cur->names);
            nros_xrce_free(cur);
            g->n_peers--;
            graph_changed(g);
        }
        return true;
    }
    if (cur != NULL) {
        bool same = cur->n_nodes == n_nodes && cur->names_len == used &&
                    memcmp(cur->names, names, used) == 0;
        nros_xrce_free(cur->names);
        cur->names = names;
        cur->names_len = used;
        cur->n_nodes = n_nodes;
        if (!same) {
            graph_changed(g);
        }
        return true;
    }
    if (g->n_peers >= XRCE_GRAPH_MAX_PEERS) {
        nros_xrce_free(names);
        g->peers_overflowed = true;
        return false;
    }
    xrce_graph_peer* p = (xrce_graph_peer*)nros_xrce_calloc(1, sizeof(xrce_graph_peer));
    if (p == NULL) {
        nros_xrce_free(names);
        g->peers_overflowed = true;
        return false;
    }
    memcpy(p->gid, gid, sizeof(gid));
    p->names = names;
    p->names_len = used;
    p->n_nodes = n_nodes;
    *link = p; /* appended: discovery order */
    g->n_peers++;
    graph_changed(g);
    return true;
}

bool xrce_graph_on_topic(xrce_session_state_t* st, uxrObjectId object_id, struct ucdrBuffer* ub,
                         size_t len) {
    if (st == NULL || !st->graph.reader_active || object_id.id != st->graph.reader_id ||
        object_id.type != UXR_DATAREADER_ID) {
        return false;
    }
    /* Copied out first: a fragmented sample's iterator addresses only the first
     * fragment, and the array read is what walks the chain (`xrce_stage_inbound`). */
    uint8_t* buf = (uint8_t*)nros_xrce_calloc(1, len);
    if (buf != NULL && (len == 0 || ucdr_deserialize_array_uint8_t(ub, buf, len))) {
        (void)xrce_graph_ingest(&st->graph, buf, len);
    }
    nros_xrce_free(buf);
    if (st->graph.peers_overflowed && !st->graph.warned_overflow) {
        graph_log(XRCE_LOG_ERROR,
                  "ros_discovery_info: a participant's node list did not fit "
                  "(XRCE_GRAPH_MAX_PEERS / XRCE_GRAPH_MAX_PEER_NODES); get_node_names answers "
                  "ERROR rather than a short list (issue 2106).");
        st->graph.warned_overflow = true;
    }
    return true;
}

/* Create the reader on first use. Endpoint keys: the DataReader is one more
 * endpoint on this participant, so it takes a key from the count like every
 * other (issue 1292) — skipping that would shift every later GID by one. */
static rmw_ret_t graph_ensure_reader(xrce_session_state_t* st) {
    xrce_graph* g = &st->graph;
    if (g->reader_active) {
        return NROS_RMW_RET_OK;
    }
    if (xrce_session_is_closed(st)) {
        return NROS_RMW_RET_ERROR;
    }
    xrce_graph_ensure_counter(st);
    uxrObjectId topic_oid = xrce_alloc_entity_id(st, UXR_TOPIC_ID);
    uxrObjectId sub_oid = xrce_alloc_entity_id(st, UXR_SUBSCRIBER_ID);
    uxrObjectId dr_oid = xrce_alloc_entity_id(st, UXR_DATAREADER_ID);
    /* rmw_dds_common's profile, with room for several participants' latched
     * samples arriving together. */
    uxrQoS_t qos = {UXR_DURABILITY_TRANSIENT_LOCAL, UXR_RELIABILITY_RELIABLE, UXR_HISTORY_KEEP_LAST,
                    XRCE_GRAPH_READER_DEPTH};
    uint16_t reqs[3];
    reqs[0] = uxr_buffer_create_topic_bin(&st->session, st->output_reliable, topic_oid,
                                          st->participant_oid, XRCE_GRAPH_TOPIC, XRCE_GRAPH_TYPE,
                                          UXR_REPLACE);
    reqs[1] = uxr_buffer_create_subscriber_bin(&st->session, st->output_reliable, sub_oid,
                                               st->participant_oid, UXR_REPLACE);
    reqs[2] = uxr_buffer_create_datareader_bin(&st->session, st->output_reliable, dr_oid, sub_oid,
                                               topic_oid, qos, UXR_REPLACE);
    uint8_t statuses[3] = {0, 0, 0};
    bool ok = xrce_confirm_entities(st, reqs, statuses, 3) == NROS_RMW_RET_OK;
    uint32_t unused_key = 0;
    (void)xrce_graph_claim(st, 1, ok, &unused_key);
    if (!ok) {
        return NROS_RMW_RET_ERROR;
    }
    g->reader_id = dr_oid.id;
    g->reader_active = true;
    uxrDeliveryControl delivery = {
        .max_samples = UXR_MAX_SAMPLES_UNLIMITED,
        .max_elapsed_time = UXR_MAX_ELAPSED_TIME_UNLIMITED,
        .max_bytes_per_second = UXR_MAX_BYTES_PER_SECOND_UNLIMITED,
        .min_pace_period = 0,
    };
    (void)uxr_buffer_request_data(&st->session, st->output_reliable, dr_oid, st->input_reliable,
                                  &delivery);
    (void)uxr_run_session_time(&st->session, 0);
    return NROS_RMW_RET_OK;
}

static rmw_ret_t xrce_get_node_names_locked(xrce_session_state_t* st,
                                            rmw_node_visitor_t visitor) {
    rmw_ret_t rc = graph_ensure_reader(st);
    if (rc != NROS_RMW_RET_OK) {
        return rc;
    }
    const xrce_graph* g = &st->graph;
    if (g->peers_overflowed) {
        return NROS_RMW_RET_ERROR;
    }
    for (const xrce_graph_node* n = g->nodes; n != NULL; n = n->next) {
        if (!visitor.visit(visitor.ctx, n->name, wire_ns(n->ns), NULL)) {
            return NROS_RMW_RET_OK;
        }
    }
    for (const xrce_graph_peer* p = g->peers; p != NULL; p = p->next) {
        const char* s = p->names;
        for (uint32_t i = 0; i < p->n_nodes; ++i) {
            const char* ns = s;
            const char* name = ns + strlen(ns) + 1u;
            s = name + strlen(name) + 1u;
            if (!visitor.visit(visitor.ctx, name, ns, NULL)) {
                return NROS_RMW_RET_OK;
            }
        }
    }
    return NROS_RMW_RET_OK;
}

/* ---- Issue 1734: locked entry points ---------------------------------
 *
 * Each public entry point takes its session's API lock around the body (see
 * `api_lock` in `internal.h`). `st` is read BEFORE the body, which may free
 * the entity that leads to it; a destroy that leaves a closed session empty
 * has its free deferred to the unlock below. */

rmw_ret_t xrce_node_create(rmw_session_t* session, const char* name, const char* namespace_,
                           rmw_node_t* out) {
    xrce_session_state_t* st = XRCE_ST_OF_SESSION(session);
    xrce_session_lock(st);
    rmw_ret_t ret = xrce_node_create_locked(session, name, namespace_, out);
    xrce_session_unlock(st);
    return ret;
}

/* Issue 2106 — `get_node_names`: this session's nodes, then every remote DDS
 * participant's from `ros_discovery_info`. Enclave is not tracked (NULL). */
rmw_ret_t xrce_get_node_names(const rmw_session_t* session, rmw_node_visitor_t visitor) {
    if (session == NULL || session->backend_data == NULL || visitor.visit == NULL) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    xrce_session_state_t* st = (xrce_session_state_t*)session->backend_data;
    xrce_session_lock(st);
    rmw_ret_t ret = xrce_get_node_names_locked(st, visitor);
    xrce_session_unlock(st);
    return ret;
}

/* Issue 2106 — the graph-change edge. A clear (NULL) is also the runtime's
 * capability probe, so it answers OK without creating the reader. */
rmw_ret_t xrce_node_get_graph_guard_condition(rmw_session_t* session,
                                              rmw_event_callback_t callback,
                                              const void* user_data) {
    if (session == NULL || session->backend_data == NULL) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    xrce_session_state_t* st = (xrce_session_state_t*)session->backend_data;
    xrce_session_lock(st);
    rmw_ret_t ret = NROS_RMW_RET_OK;
    if (callback != NULL) {
        ret = graph_ensure_reader(st);
    }
    if (ret == NROS_RMW_RET_OK) {
        st->graph.on_change = callback;
        st->graph.on_change_user = callback != NULL ? user_data : NULL;
    }
    xrce_session_unlock(st);
    return ret;
}

rmw_ret_t xrce_node_destroy(rmw_node_t* node) {
    xrce_session_state_t* st = XRCE_ST_OF_NODE(node);
    xrce_session_lock(st);
    rmw_ret_t ret = xrce_node_destroy_locked(node);
    xrce_session_unlock(st);
    return ret;
}

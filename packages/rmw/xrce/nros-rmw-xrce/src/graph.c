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

rmw_ret_t xrce_node_create(rmw_session_t* session, const char* name, const char* namespace_,
                           rmw_node_t* out) {
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
    return NROS_RMW_RET_OK;
}

rmw_ret_t xrce_node_destroy(rmw_node_t* node) {
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

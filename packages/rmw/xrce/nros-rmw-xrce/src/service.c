/* Phase 115.K.2.3 — service server / client paths.
 *
 * Mirrors the Rust impl's `XrceSession::create_service_server` /
 * `XrceServiceServer::send_response` / `XrceServiceClient::send_request_raw`
 * shape. Bin profile only — no QoS XML; service requests/replies
 * use the services-default QoS (reliable / volatile / keep-last(10)).
 *
 * Single-slot inbox per server / client (overflow flags + drops).
 * Request/reply correlation goes through micro-XRCE-DDS-Client's
 * `SampleIdentity` (24 bytes); the runtime's int64_t `seq` is
 * unused by this backend (XRCE doesn't carry a sequence number on
 * the wire — see lib.rs:2305).
 */

#include "internal.h"

#include "nros/rmw_ret.h"

#include <stdlib.h>
#include <string.h>

#include <uxr/client/client.h>
#include <uxr/client/core/session/object_id.h>

/* Single-session callbacks — registered once at session_open via
 * `uxr_set_request_callback` / `uxr_set_reply_callback`. Dispatch
 * by object_id to the matching slot in the per-session pool. */

void xrce_request_callback(uxrSession* session, uxrObjectId object_id, uint16_t request_id,
                           SampleIdentity* sample_id, struct ucdrBuffer* ub, uint16_t length,
                           void* args) {
    (void)session;
    (void)request_id;
    if (args == NULL || ub == NULL || sample_id == NULL) {
        return;
    }
    xrce_session_state_t* st = (xrce_session_state_t*)args;
    size_t len = (size_t)length;
    for (size_t i = 0; i < XRCE_MAX_SERVICE_SERVERS; ++i) {
        xrce_service_server_slot* slot = &st->service_server_slots[i];
        if (!slot->active || slot->replier_id != object_id.id) {
            continue;
        }
        /* Phase 237 follow-up — enqueue into the request ring. Drop the newest
         * when full (preserves in-order delivery of already-buffered requests),
         * so a burst of concurrent arrivals doesn't clobber an unread request. */
        if (slot->req_count >= XRCE_SERVICE_REQUEST_RING_DEPTH) {
            return;
        }
        xrce_service_request_entry* e = &slot->req_ring[slot->req_write_idx];
        /* Issue 0819 — one staging path for every inbound payload, fragments
         * included; see `xrce_stage_inbound`. */
        e->overflow = !xrce_stage_inbound(e->data, XRCE_BUFFER_SIZE, ub, len, &e->len);
        e->sample_id = *sample_id;
        slot->req_write_idx =
            (uint16_t)((slot->req_write_idx + 1) % XRCE_SERVICE_REQUEST_RING_DEPTH);
        slot->req_count++;
        return;
    }
}

void xrce_reply_callback(uxrSession* session, uxrObjectId object_id, uint16_t request_id,
                         uint16_t reply_id, struct ucdrBuffer* ub, uint16_t length, void* args) {
    (void)session;
    (void)reply_id;
    if (args == NULL || ub == NULL) {
        return;
    }
    xrce_session_state_t* st = (xrce_session_state_t*)args;
    size_t len = (size_t)length;
    for (size_t i = 0; i < XRCE_MAX_SERVICE_CLIENTS; ++i) {
        xrce_service_client_slot* slot = &st->service_client_slots[i];
        if (!slot->active || slot->requester_id != object_id.id) {
            continue;
        }
        /* Issue 0778 — remember WHICH request this answers. */
        slot->reply_request_id = request_id;
        /* Issue 0819 — one staging path for every inbound payload, fragments
         * included; see `xrce_stage_inbound`. */
        slot->overflow = !xrce_stage_inbound(slot->data, XRCE_BUFFER_SIZE, ub, len, &slot->len);
        slot->has_reply = true;
        return;
    }
}

/* ---- Service server -------------------------------------------------- */

rmw_ret_t xrce_service_create(const rmw_node_t* node, const char* service_name,
                                          const char* type_name, const char* type_hash,
                                          uint32_t domain_id, const rmw_qos_profile_t* qos,
                                          rmw_service_t* out) {
    /* Phase 376 W5/B1 — the entity is created ON ITS NODE, as upstream does.
     * The node carries the route to its session (our `context`). */
    if (node == NULL) return NROS_RMW_RET_INVALID_ARGUMENT;
    rmw_session_t* session = node->session;
    (void)type_hash;
    (void)domain_id;

    if (session == NULL || out == NULL || service_name == NULL || type_name == NULL) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    xrce_session_state_t* st = (xrce_session_state_t*)session->backend_data;
    if (st == NULL) {
        return NROS_RMW_RET_ERROR;
    }

    /* Find a free slot. */
    xrce_service_server_slot* slot = NULL;
    for (size_t i = 0; i < XRCE_MAX_SERVICE_SERVERS; ++i) {
        if (!st->service_server_slots[i].active) {
            slot = &st->service_server_slots[i];
            break;
        }
    }
    if (slot == NULL) {
        return NROS_RMW_RET_ERROR;
    }

    xrce_service_server_state* ss =
        (xrce_service_server_state*)nros_xrce_calloc(1, sizeof(xrce_service_server_state));
    if (ss == NULL) {
        return NROS_RMW_RET_BAD_ALLOC;
    }
    ss->session_state = st;
    ss->slot = slot;

    uxrObjectId replier_oid = xrce_alloc_entity_id(st, UXR_REPLIER_ID);

    char service_buf[XRCE_DDS_NAME_BUF_SIZE];
    char req_type_buf[XRCE_DDS_NAME_BUF_SIZE];
    char reply_type_buf[XRCE_DDS_NAME_BUF_SIZE];
    char req_topic_buf[XRCE_DDS_NAME_BUF_SIZE];
    char reply_topic_buf[XRCE_DDS_NAME_BUF_SIZE];

    /* Service name: pass through as-is (the Rust impl does likewise
     * — `service_name` itself is the FastDDS service name; the rq/
     * + rr/ topic-name dance handles wire formatting). */
    size_t sn_len = strlen(service_name);
    if (sn_len + 1 > sizeof(service_buf)) sn_len = sizeof(service_buf) - 1;
    memcpy(service_buf, service_name, sn_len);
    service_buf[sn_len] = '\0';

    xrce_dds_request_type(type_name, req_type_buf, sizeof(req_type_buf));
    xrce_dds_reply_type(type_name, reply_type_buf, sizeof(reply_type_buf));
    xrce_dds_request_topic(service_name, req_topic_buf, sizeof(req_topic_buf));
    xrce_dds_reply_topic(service_name, reply_topic_buf, sizeof(reply_topic_buf));

    /* Honor the caller's QoS; fall back to the default reliable /
     * volatile / keep-last(10) profile (matches the Rust impl's
     * `QoSProfile::services_default`) when none is supplied. */
    rmw_qos_profile_t default_qos = NROS_RMW_QOS_PROFILE_SERVICES_DEFAULT;
    const rmw_qos_profile_t* eff_qos = (qos != NULL) ? qos : &default_qos;
    uxrQoS_t xrce_qos = xrce_map_qos(eff_qos);

    uint16_t req = uxr_buffer_create_replier_bin(
        &st->session, st->output_reliable, replier_oid, st->participant_oid, service_buf,
        req_type_buf, reply_type_buf, req_topic_buf, reply_topic_buf, xrce_qos, UXR_REPLACE);

    uint16_t requests[1] = {req};
    uint8_t statuses[1] = {0};
    rmw_ret_t cret = xrce_confirm_entities(st, requests, statuses, 1);
    if (cret != NROS_RMW_RET_OK) {
        nros_xrce_free(ss);
        return cret;
    }

    /* Activate the slot — empty request ring. */
    slot->replier_id = replier_oid.id;
    slot->req_write_idx = 0;
    slot->req_read_idx = 0;
    slot->req_count = 0;
    for (int i = 0; i < XRCE_MAX_PENDING_REPLIES; ++i) {
        slot->reply_tokens[i].in_use = false;
    }
    slot->active = true;
    ss->replier_oid = replier_oid;

    /* Continuous delivery for inbound requests. */
    uxrDeliveryControl delivery = {
        .max_samples = UXR_MAX_SAMPLES_UNLIMITED,
        .max_elapsed_time = UXR_MAX_ELAPSED_TIME_UNLIMITED,
        .max_bytes_per_second = UXR_MAX_BYTES_PER_SECOND_UNLIMITED,
        .min_pace_period = 0,
    };
    (void)uxr_buffer_request_data(&st->session, st->output_reliable, replier_oid,
                                  st->input_reliable, &delivery);
    (void)uxr_run_session_time(&st->session, XRCE_SESSION_FLUSH_TIMEOUT_MS);

    out->backend_data = ss;
    return NROS_RMW_RET_OK;
}

rmw_ret_t xrce_service_destroy(rmw_service_t* server) {
    if (server == NULL || server->backend_data == NULL) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    xrce_service_server_state* ss = (xrce_service_server_state*)server->backend_data;
    xrce_session_state_t* st = ss->session_state;

    if (ss->slot != NULL) {
        ss->slot->active = false;
        ss->slot->req_count = 0;
    }
    /* Phase 376 W5 — see xrce_publisher_destroy: fire-and-forget by design,
     * so a buffering failure is the only verdict this frame can have. */
    uint16_t req = uxr_buffer_delete_entity(&st->session, st->output_reliable, ss->replier_oid);
    (void)uxr_run_session_time(&st->session, 0);

    nros_xrce_free(ss);
    server->backend_data = NULL;
    return req == UXR_INVALID_REQUEST_ID ? NROS_RMW_RET_ERROR : NROS_RMW_RET_OK;
}

/* Issue 0773 — the length-or-status shape is retired here too.
 *
 * The W3.d step A adapters below were written as THIN translations over
 * length-returning helpers, separating a byte count from an error with
 * `if (n < 0)`. Step B then gave the error codes upstream's POSITIVE numbering
 * and nothing failed to compile: `NROS_RMW_RET_NO_DATA` (1003) and
 * `NROS_RMW_RET_BUFFER_TOO_SMALL` (1005) stopped being negative and started
 * being lengths. The cyclonedds copy of this same shape is what crashed a
 * user's executor with `range end index 1005 out of range for slice of
 * length 256`; xrce had not been exercised on the failing path, which is the
 * only reason it did not crash too.
 *
 * Status is returned, length goes to an out-parameter. */
static rmw_ret_t xrce_service_try_recv_request_len(const rmw_service_t* server, uint8_t* buf,
                                                   size_t buf_len, int64_t* seq_out,
                                                   size_t* out_len) {
    if (out_len == NULL) return NROS_RMW_RET_INVALID_ARGUMENT;
    *out_len = 0;
    if (server == NULL || server->backend_data == NULL) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    xrce_service_server_state* ss = (xrce_service_server_state*)server->backend_data;
    xrce_service_server_slot* slot = ss->slot;
    if (slot == NULL || slot->req_count == 0) {
        return NROS_RMW_RET_NO_DATA;
    }
    xrce_service_request_entry* e = &slot->req_ring[slot->req_read_idx];

    /* Advance the read cursor + drop the head entry. */
#define XRCE_REQ_RING_POP()                                                                        \
    do {                                                                                           \
        slot->req_read_idx =                                                                       \
            (uint16_t)((slot->req_read_idx + 1) % XRCE_SERVICE_REQUEST_RING_DEPTH);                \
        slot->req_count--;                                                                         \
    } while (0)

    if (e->overflow) {
        XRCE_REQ_RING_POP();
        return NROS_RMW_RET_MESSAGE_TOO_LARGE;
    }
    size_t len = e->len;
    if (len > buf_len) {
        XRCE_REQ_RING_POP();
        return NROS_RMW_RET_BUFFER_TOO_SMALL;
    }
    /* Phase 237 — move the request's `SampleIdentity` into a seq-keyed reply
     * token so the reply can be sent after later requests (deferred action
     * `get_result`). Return the token index as the runtime `sequence_number`;
     * `send_response(seq)` reads it back. WOULD_BLOCK (leaving the request in the
     * ring) if the token table is full so the runtime retries on a later spin
     * rather than losing the correlation. */
    int reply_idx = -1;
    for (int i = 0; i < XRCE_MAX_PENDING_REPLIES; ++i) {
        if (!slot->reply_tokens[i].in_use) {
            reply_idx = i;
            break;
        }
    }
    if (reply_idx < 0) {
        return NROS_RMW_RET_WOULD_BLOCK;
    }
    if (buf != NULL && len > 0) {
        memcpy(buf, e->data, len);
    }
    slot->reply_tokens[reply_idx].sample_id = e->sample_id;
    slot->reply_tokens[reply_idx].in_use = true;
    if (seq_out != NULL) {
        *seq_out = (int64_t)reply_idx;
    }
    XRCE_REQ_RING_POP();
#undef XRCE_REQ_RING_POP
    *out_len = len;
    return NROS_RMW_RET_OK;
}

/* Phase 376 W3.b/W3.d step A — upstream's shape over the unchanged body above.
 * A THIN adapter rather than a rewrite: the length-returning logic below has
 * error paths that are easy to get subtly wrong (WOULD_BLOCK is an error here,
 * not "nothing to take"), so it is preserved verbatim and only the reporting
 * convention is translated. NO_DATA is the one code that becomes
 * `taken = false` with OK. */
rmw_ret_t xrce_service_take_request(const rmw_service_t* server, uint8_t* buf,
                                         size_t buf_len, int64_t* seq_out, size_t* out_len,
                                         bool* taken) {
    if (out_len == NULL || taken == NULL) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    *out_len = 0;
    *taken = false;
    size_t n = 0;
    rmw_ret_t rc = xrce_service_try_recv_request_len(server, buf, buf_len, seq_out, &n);
    if (rc == NROS_RMW_RET_NO_DATA) {
        return NROS_RMW_RET_OK;
    }
    if (rc != NROS_RMW_RET_OK) {
        return rc;
    }
    *out_len = n;
    *taken = true;
    return NROS_RMW_RET_OK;
}

rmw_ret_t xrce_service_has_request(rmw_service_t* server, bool* out_has_request) {
    /* Phase 376 W3.d step A — flag out, status returned. */
    if (out_has_request == NULL) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    if (server == NULL || server->backend_data == NULL) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    xrce_service_server_state* ss = (xrce_service_server_state*)server->backend_data;
    if (ss->slot == NULL) {
        *out_has_request = false;
        return NROS_RMW_RET_OK;
    }
    *out_has_request = ss->slot->req_count > 0;
    return NROS_RMW_RET_OK;
}

rmw_ret_t xrce_service_send_response(const rmw_service_t* server, int64_t seq,
                                       const uint8_t* data, size_t len) {
    if (server == NULL || server->backend_data == NULL) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    if (data == NULL && len > 0) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    xrce_service_server_state* ss = (xrce_service_server_state*)server->backend_data;
    xrce_session_state_t* st = ss->session_state;

    /* Phase 237 — resolve the reply token captured by `try_recv_request`. `seq`
     * is its index; the token holds the request's `SampleIdentity` and survives
     * later requests on the same server, so a deferred `get_result` reply still
     * reaches the original requester. */
    if (seq < 0 || seq >= XRCE_MAX_PENDING_REPLIES || !ss->slot->reply_tokens[seq].in_use) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    SampleIdentity* reply_sample_id = &ss->slot->reply_tokens[seq].sample_id;

    /* XRCE-DDS interop: strip the executor's 4-byte CDR encapsulation header —
     * the XRCE reply payload is the bare serialized sample (the agent owns the
     * DDS representation header). Symmetric with the reply-inbox re-prepend and
     * the topic publish path. */
    const uint8_t* body = data;
    size_t body_len = len;
    if (body_len >= XRCE_CDR_HEADER_LEN) {
        body += XRCE_CDR_HEADER_LEN;
        body_len -= XRCE_CDR_HEADER_LEN;
    }

    /* `uxr_buffer_reply` takes a mutable pointer; cast away const on data. */
    uint16_t req = uxr_buffer_reply(&st->session, st->output_reliable, ss->replier_oid,
                                    reply_sample_id, (uint8_t*)(uintptr_t)body, body_len);
    if (req == UXR_INVALID_REQUEST_ID) {
        return NROS_RMW_RET_ERROR;
    }
    /* Release the token whether or not the flush below fully drains — the
     * request has been answered at the XRCE layer. */
    ss->slot->reply_tokens[seq].in_use = false;
    (void)uxr_run_session_time(&st->session, XRCE_SESSION_FLUSH_TIMEOUT_MS);
    return NROS_RMW_RET_OK;
}

/* Phase 130.4 — non-blocking send/recv split (paired vtable
 * slots). Avoids the removed blocking-call shape that conflated
 * "send pending request" + "block for reply"; lets the
 * executor's spin loop poll for a late-arriving reply without
 * re-sending the request or sleeping in a never-signaled
 * wake-primitive wait (Phase 127.C.4 root cause for the C++
 * action send_goal trampoline). */
rmw_ret_t xrce_service_send_request_raw(const rmw_client_t* client,
                                             const uint8_t* request, size_t req_len,
                                             int64_t* sequence_id) {
    if (client == NULL || client->backend_data == NULL) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    if (request == NULL && req_len > 0) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    xrce_service_client_state* cs = (xrce_service_client_state*)client->backend_data;
    xrce_session_state_t* st = cs->session_state;
    xrce_service_client_slot* slot = cs->slot;
    if (slot == NULL) {
        return NROS_RMW_RET_ERROR;
    }
    /* Clear any stale reply so try_recv_reply_raw doesn't surface
     * an earlier request's response. */
    slot->has_reply = false;
    slot->overflow = false;
    /* XRCE-DDS interop: strip the 4-byte CDR encapsulation header (see
     * send_response / publish_raw). */
    const uint8_t* body = request;
    size_t body_len = req_len;
    if (body_len >= XRCE_CDR_HEADER_LEN) {
        body += XRCE_CDR_HEADER_LEN;
        body_len -= XRCE_CDR_HEADER_LEN;
    }
    uint16_t req = uxr_buffer_request(&st->session, st->output_reliable, cs->requester_oid,
                                      (uint8_t*)(uintptr_t)body, body_len);
    if (req == UXR_INVALID_REQUEST_ID) {
        return NROS_RMW_RET_ERROR;
    }
    /* Issue 0778 — XRCE's own request id IS the sequence. It was read here
     * only to test it for validity and then discarded, which is what left a
     * client unable to tell two replies apart. */
    if (sequence_id != NULL) {
        *sequence_id = (int64_t)req;
    }
    /* Flush the reliable output stream so the request actually
     * leaves the session — matches the publisher / send_response
     * paths' explicit flush. Subsequent `drive_io` calls drive
     * reliable retransmission. */
    (void)uxr_run_session_time(&st->session, XRCE_SESSION_FLUSH_TIMEOUT_MS);
    return NROS_RMW_RET_OK;
}

static rmw_ret_t xrce_service_try_recv_reply_raw_len(const rmw_client_t* client, uint8_t* reply_buf,
                                                     size_t reply_buf_len, int64_t* seq_out,
                                                     size_t* out_len) {
    if (out_len == NULL) return NROS_RMW_RET_INVALID_ARGUMENT;
    *out_len = 0;
    if (client == NULL || client->backend_data == NULL) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    xrce_service_client_state* cs = (xrce_service_client_state*)client->backend_data;
    xrce_service_client_slot* slot = cs->slot;
    if (slot == NULL) {
        return NROS_RMW_RET_ERROR;
    }
    if (!slot->has_reply) {
        return NROS_RMW_RET_NO_DATA;
    }
    if (slot->overflow) {
        slot->overflow = false;
        slot->has_reply = false;
        return NROS_RMW_RET_MESSAGE_TOO_LARGE;
    }
    size_t len = slot->len;
    if (len > reply_buf_len) {
        slot->has_reply = false;
        return NROS_RMW_RET_BUFFER_TOO_SMALL;
    }
    if (reply_buf != NULL && len > 0) {
        memcpy(reply_buf, slot->data, len);
    }
    slot->has_reply = false;
    /* Issue 0778 — the slot holds one reply for one outstanding request, so
     * the id it answers is the one the slot recorded at send time. */
    if (seq_out != NULL) {
        *seq_out = (int64_t)slot->reply_request_id;
    }
    *out_len = len;
    return NROS_RMW_RET_OK;
}

/* Phase 376 W3.b/W3.d step A — upstream's shape over the unchanged body above.
 * A THIN adapter rather than a rewrite: the length-returning logic below has
 * error paths that are easy to get subtly wrong (WOULD_BLOCK is an error here,
 * not "nothing to take"), so it is preserved verbatim and only the reporting
 * convention is translated. NO_DATA is the one code that becomes
 * `taken = false` with OK. */
rmw_ret_t xrce_service_take_response(const rmw_client_t* client, uint8_t* reply_buf,
                                          size_t reply_buf_len, int64_t* seq_out,
                                          size_t* out_len, bool* taken) {
    if (out_len == NULL || taken == NULL) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    *out_len = 0;
    *taken = false;
    size_t n = 0;
    rmw_ret_t rc =
        xrce_service_try_recv_reply_raw_len(client, reply_buf, reply_buf_len, seq_out, &n);
    if (rc == NROS_RMW_RET_NO_DATA) {
        return NROS_RMW_RET_OK;
    }
    if (rc != NROS_RMW_RET_OK) {
        return rc;
    }
    *out_len = n;
    *taken = true;
    return NROS_RMW_RET_OK;
}

/* ---- Service client -------------------------------------------------- */

rmw_ret_t xrce_client_create(const rmw_node_t* node, const char* service_name,
                                          const char* type_name, const char* type_hash,
                                          uint32_t domain_id, const rmw_qos_profile_t* qos,
                                          rmw_client_t* out) {
    /* Phase 376 W5/B1 — the entity is created ON ITS NODE, as upstream does.
     * The node carries the route to its session (our `context`). */
    if (node == NULL) return NROS_RMW_RET_INVALID_ARGUMENT;
    rmw_session_t* session = node->session;
    (void)type_hash;
    (void)domain_id;

    if (session == NULL || out == NULL || service_name == NULL || type_name == NULL) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    xrce_session_state_t* st = (xrce_session_state_t*)session->backend_data;
    if (st == NULL) {
        return NROS_RMW_RET_ERROR;
    }

    xrce_service_client_slot* slot = NULL;
    for (size_t i = 0; i < XRCE_MAX_SERVICE_CLIENTS; ++i) {
        if (!st->service_client_slots[i].active) {
            slot = &st->service_client_slots[i];
            break;
        }
    }
    if (slot == NULL) {
        return NROS_RMW_RET_ERROR;
    }

    xrce_service_client_state* cs =
        (xrce_service_client_state*)nros_xrce_calloc(1, sizeof(xrce_service_client_state));
    if (cs == NULL) {
        return NROS_RMW_RET_BAD_ALLOC;
    }
    cs->session_state = st;
    cs->slot = slot;

    uxrObjectId requester_oid = xrce_alloc_entity_id(st, UXR_REQUESTER_ID);

    char service_buf[XRCE_DDS_NAME_BUF_SIZE];
    char req_type_buf[XRCE_DDS_NAME_BUF_SIZE];
    char reply_type_buf[XRCE_DDS_NAME_BUF_SIZE];
    char req_topic_buf[XRCE_DDS_NAME_BUF_SIZE];
    char reply_topic_buf[XRCE_DDS_NAME_BUF_SIZE];

    size_t sn_len = strlen(service_name);
    if (sn_len + 1 > sizeof(service_buf)) sn_len = sizeof(service_buf) - 1;
    memcpy(service_buf, service_name, sn_len);
    service_buf[sn_len] = '\0';

    xrce_dds_request_type(type_name, req_type_buf, sizeof(req_type_buf));
    xrce_dds_reply_type(type_name, reply_type_buf, sizeof(reply_type_buf));
    xrce_dds_request_topic(service_name, req_topic_buf, sizeof(req_topic_buf));
    xrce_dds_reply_topic(service_name, reply_topic_buf, sizeof(reply_topic_buf));

    rmw_qos_profile_t default_qos = NROS_RMW_QOS_PROFILE_SERVICES_DEFAULT;
    const rmw_qos_profile_t* eff_qos = (qos != NULL) ? qos : &default_qos;
    uxrQoS_t xrce_qos = xrce_map_qos(eff_qos);

    uint16_t req = uxr_buffer_create_requester_bin(
        &st->session, st->output_reliable, requester_oid, st->participant_oid, service_buf,
        req_type_buf, reply_type_buf, req_topic_buf, reply_topic_buf, xrce_qos, UXR_REPLACE);

    uint16_t requests[1] = {req};
    uint8_t statuses[1] = {0};
    rmw_ret_t cret = xrce_confirm_entities(st, requests, statuses, 1);
    if (cret != NROS_RMW_RET_OK) {
        nros_xrce_free(cs);
        return cret;
    }

    slot->requester_id = requester_oid.id;
    slot->has_reply = false;
    slot->overflow = false;
    slot->len = 0;
    slot->active = true;
    cs->requester_oid = requester_oid;

    uxrDeliveryControl delivery = {
        .max_samples = UXR_MAX_SAMPLES_UNLIMITED,
        .max_elapsed_time = UXR_MAX_ELAPSED_TIME_UNLIMITED,
        .max_bytes_per_second = UXR_MAX_BYTES_PER_SECOND_UNLIMITED,
        .min_pace_period = 0,
    };
    (void)uxr_buffer_request_data(&st->session, st->output_reliable, requester_oid,
                                  st->input_reliable, &delivery);
    (void)uxr_run_session_time(&st->session, XRCE_SESSION_FLUSH_TIMEOUT_MS);

    out->backend_data = cs;
    return NROS_RMW_RET_OK;
}

rmw_ret_t xrce_client_destroy(rmw_client_t* client) {
    if (client == NULL || client->backend_data == NULL) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    xrce_service_client_state* cs = (xrce_service_client_state*)client->backend_data;
    xrce_session_state_t* st = cs->session_state;

    if (cs->slot != NULL) {
        cs->slot->active = false;
        cs->slot->has_reply = false;
    }
    /* Phase 376 W5 — see xrce_publisher_destroy: fire-and-forget by design,
     * so a buffering failure is the only verdict this frame can have. */
    uint16_t req = uxr_buffer_delete_entity(&st->session, st->output_reliable, cs->requester_oid);
    (void)uxr_run_session_time(&st->session, 0);

    nros_xrce_free(cs);
    client->backend_data = NULL;
    return req == UXR_INVALID_REQUEST_ID ? NROS_RMW_RET_ERROR : NROS_RMW_RET_OK;
}


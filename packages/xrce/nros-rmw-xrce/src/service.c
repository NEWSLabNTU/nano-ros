/* Phase 115.K.2.3 — service server / client paths.
 *
 * Mirrors the Rust impl's `XrceSession::create_service_server` /
 * `XrceServiceServer::send_reply` / `XrceServiceClient::call_raw`
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

void xrce_request_callback(uxrSession *session,
                           uxrObjectId object_id,
                           uint16_t request_id,
                           SampleIdentity *sample_id,
                           struct ucdrBuffer *ub,
                           uint16_t length,
                           void *args) {
    (void)session; (void)request_id;
    if (args == NULL || ub == NULL || sample_id == NULL) {
        return;
    }
    xrce_session_state_t *st = (xrce_session_state_t *)args;
    size_t len = (size_t)length;
    for (size_t i = 0; i < XRCE_MAX_SERVICE_SERVERS; ++i) {
        xrce_service_server_slot *slot = &st->service_server_slots[i];
        if (!slot->active || slot->replier_id != object_id.id) {
            continue;
        }
        /* XRCE-DDS interop: the agent delivers the bare CDR-serialized request
         * WITHOUT the 4-byte CDR encapsulation header (it owns the DDS-side
         * representation header). nano-ros's deserializers expect the header,
         * so prepend it — mirrors `xrce_topic_callback` and is symmetric with
         * the strip on `uxr_buffer_request` / `uxr_buffer_reply`. */
        if (len + XRCE_CDR_HEADER_LEN > XRCE_BUFFER_SIZE) {
            slot->overflow = true;
            slot->has_request = true;
            return;
        }
        slot->overflow = false;
        slot->data[0] = 0x00; /* CDR_LE representation id */
        slot->data[1] = 0x01;
        slot->data[2] = 0x00; /* options */
        slot->data[3] = 0x00;
        memcpy(slot->data + XRCE_CDR_HEADER_LEN, ub->iterator, len);
        slot->len = len + XRCE_CDR_HEADER_LEN;
        slot->sample_id = *sample_id;
        slot->has_request = true;
        return;
    }
}

void xrce_reply_callback(uxrSession *session,
                         uxrObjectId object_id,
                         uint16_t request_id,
                         uint16_t reply_id,
                         struct ucdrBuffer *ub,
                         uint16_t length,
                         void *args) {
    (void)session; (void)request_id; (void)reply_id;
    if (args == NULL || ub == NULL) {
        return;
    }
    xrce_session_state_t *st = (xrce_session_state_t *)args;
    size_t len = (size_t)length;
    for (size_t i = 0; i < XRCE_MAX_SERVICE_CLIENTS; ++i) {
        xrce_service_client_slot *slot = &st->service_client_slots[i];
        if (!slot->active || slot->requester_id != object_id.id) {
            continue;
        }
        /* XRCE-DDS interop: re-prepend the CDR encapsulation header stripped on
         * the wire (mirrors the request inbox + `xrce_topic_callback`). */
        if (len + XRCE_CDR_HEADER_LEN > XRCE_BUFFER_SIZE) {
            slot->overflow = true;
            slot->has_reply = true;
            return;
        }
        slot->overflow = false;
        slot->data[0] = 0x00;
        slot->data[1] = 0x01;
        slot->data[2] = 0x00;
        slot->data[3] = 0x00;
        memcpy(slot->data + XRCE_CDR_HEADER_LEN, ub->iterator, len);
        slot->len = len + XRCE_CDR_HEADER_LEN;
        slot->has_reply = true;
        return;
    }
}

/* ---- Service server -------------------------------------------------- */

nros_rmw_ret_t xrce_service_server_create(nros_rmw_session_t *session,
                                          const char *service_name,
                                          const char *type_name,
                                          const char *type_hash,
                                          uint32_t domain_id,
                                          const nros_rmw_qos_t *qos,
                                          nros_rmw_service_server_t *out) {
    (void)type_hash;
    (void)domain_id;

    if (session == NULL || out == NULL || service_name == NULL || type_name == NULL) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    xrce_session_state_t *st = (xrce_session_state_t *)session->backend_data;
    if (st == NULL) {
        return NROS_RMW_RET_ERROR;
    }

    /* Find a free slot. */
    xrce_service_server_slot *slot = NULL;
    for (size_t i = 0; i < XRCE_MAX_SERVICE_SERVERS; ++i) {
        if (!st->service_server_slots[i].active) {
            slot = &st->service_server_slots[i];
            break;
        }
    }
    if (slot == NULL) {
        return NROS_RMW_RET_ERROR;
    }

    xrce_service_server_state *ss = (xrce_service_server_state *)
        calloc(1, sizeof(xrce_service_server_state));
    if (ss == NULL) {
        return NROS_RMW_RET_BAD_ALLOC;
    }
    ss->session_state = st;
    ss->slot          = slot;

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
    xrce_dds_reply_type  (type_name, reply_type_buf, sizeof(reply_type_buf));
    xrce_dds_request_topic(service_name, req_topic_buf, sizeof(req_topic_buf));
    xrce_dds_reply_topic  (service_name, reply_topic_buf, sizeof(reply_topic_buf));

    /* Honor the caller's QoS; fall back to the default reliable /
     * volatile / keep-last(10) profile (matches the Rust impl's
     * `QosSettings::services_default`) when none is supplied. */
    nros_rmw_qos_t default_qos = NROS_RMW_QOS_PROFILE_SERVICES_DEFAULT;
    const nros_rmw_qos_t *eff_qos = (qos != NULL) ? qos : &default_qos;
    uxrQoS_t xrce_qos = xrce_map_qos(eff_qos);

    uint16_t req = uxr_buffer_create_replier_bin(
        &st->session, st->output_reliable, replier_oid, st->participant_oid,
        service_buf, req_type_buf, reply_type_buf,
        req_topic_buf, reply_topic_buf, xrce_qos, UXR_REPLACE);

    uint16_t requests[1] = { req };
    uint8_t  statuses[1] = { 0 };
    nros_rmw_ret_t cret = xrce_confirm_entities(st, requests, statuses, 1);
    if (cret != NROS_RMW_RET_OK) {
        free(ss);
        return cret;
    }

    /* Activate the slot. */
    slot->replier_id  = replier_oid.id;
    slot->has_request = false;
    slot->overflow    = false;
    slot->len         = 0;
    slot->active      = true;
    ss->replier_oid   = replier_oid;

    /* Continuous delivery for inbound requests. */
    uxrDeliveryControl delivery = {
        .max_samples           = UXR_MAX_SAMPLES_UNLIMITED,
        .max_elapsed_time      = UXR_MAX_ELAPSED_TIME_UNLIMITED,
        .max_bytes_per_second  = UXR_MAX_BYTES_PER_SECOND_UNLIMITED,
        .min_pace_period       = 0,
    };
    (void)uxr_buffer_request_data(&st->session, st->output_reliable,
                                  replier_oid, st->input_reliable, &delivery);
    (void)uxr_run_session_time(&st->session, XRCE_SESSION_FLUSH_TIMEOUT_MS);

    out->backend_data = ss;
    return NROS_RMW_RET_OK;
}

void xrce_service_server_destroy(nros_rmw_service_server_t *server) {
    if (server == NULL || server->backend_data == NULL) {
        return;
    }
    xrce_service_server_state *ss = (xrce_service_server_state *)server->backend_data;
    xrce_session_state_t *st = ss->session_state;

    if (ss->slot != NULL) {
        ss->slot->active = false;
        ss->slot->has_request = false;
    }
    (void)uxr_buffer_delete_entity(&st->session, st->output_reliable,
                                   ss->replier_oid);
    (void)uxr_run_session_time(&st->session, 0);

    free(ss);
    server->backend_data = NULL;
}

int32_t xrce_service_try_recv_request(nros_rmw_service_server_t *server,
                                      uint8_t *buf, size_t buf_len,
                                      int64_t *seq_out) {
    if (server == NULL || server->backend_data == NULL) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    xrce_service_server_state *ss = (xrce_service_server_state *)server->backend_data;
    xrce_service_server_slot *slot = ss->slot;
    if (slot == NULL || !slot->has_request) {
        return NROS_RMW_RET_NO_DATA;
    }
    if (slot->overflow) {
        slot->overflow = false;
        slot->has_request = false;
        return NROS_RMW_RET_MESSAGE_TOO_LARGE;
    }
    size_t len = slot->len;
    if (len > buf_len) {
        slot->has_request = false;
        return NROS_RMW_RET_BUFFER_TOO_SMALL;
    }
    if (buf != NULL && len > 0) {
        memcpy(buf, slot->data, len);
    }
    /* XRCE correlates request/reply via SampleIdentity, not seq. The
     * runtime's int64_t `seq` slot is unused — see lib.rs:2305. We
     * write 0 so callers don't read uninitialised memory. The
     * `sample_id` stays inside the slot for `send_reply` to read. */
    if (seq_out != NULL) {
        *seq_out = 0;
    }
    slot->has_request = false;
    return (int32_t)len;
}

int32_t xrce_service_has_request(nros_rmw_service_server_t *server) {
    if (server == NULL || server->backend_data == NULL) {
        return 0;
    }
    xrce_service_server_state *ss = (xrce_service_server_state *)server->backend_data;
    if (ss->slot == NULL) {
        return 0;
    }
    return ss->slot->has_request ? 1 : 0;
}

nros_rmw_ret_t xrce_service_send_reply(nros_rmw_service_server_t *server,
                                       int64_t seq,
                                       const uint8_t *data, size_t len) {
    (void)seq;
    if (server == NULL || server->backend_data == NULL) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    if (data == NULL && len > 0) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    xrce_service_server_state *ss = (xrce_service_server_state *)server->backend_data;
    xrce_session_state_t *st = ss->session_state;

    /* XRCE-DDS interop: strip the executor's 4-byte CDR encapsulation header —
     * the XRCE reply payload is the bare serialized sample (the agent owns the
     * DDS representation header). Symmetric with the reply-inbox re-prepend and
     * the topic publish path. */
    const uint8_t *body = data;
    size_t body_len = len;
    if (body_len >= XRCE_CDR_HEADER_LEN) {
        body += XRCE_CDR_HEADER_LEN;
        body_len -= XRCE_CDR_HEADER_LEN;
    }

    /* The slot's `sample_id` was captured by `request_callback` and
     * read out by `try_recv_request`. `uxr_buffer_reply` takes a
     * mutable pointer; cast away const-ness on the data side. */
    uint16_t req = uxr_buffer_reply(
        &st->session, st->output_reliable, ss->replier_oid,
        &ss->slot->sample_id,
        (uint8_t *)(uintptr_t)body, body_len);
    if (req == UXR_INVALID_REQUEST_ID) {
        return NROS_RMW_RET_ERROR;
    }
    (void)uxr_run_session_time(&st->session, XRCE_SESSION_FLUSH_TIMEOUT_MS);
    return NROS_RMW_RET_OK;
}

/* Phase 130.4 — non-blocking send/recv split (paired vtable
 * slots). Avoids the blocking call_raw burst that conflated
 * "send pending request" + "block for reply"; lets the
 * executor's spin loop poll for a late-arriving reply without
 * re-sending the request or sleeping in a never-signaled
 * wake-primitive wait (Phase 127.C.4 root cause for the C++
 * action send_goal trampoline). */
nros_rmw_ret_t xrce_service_send_request_raw(nros_rmw_service_client_t *client,
                                              const uint8_t *request,
                                              size_t req_len) {
    if (client == NULL || client->backend_data == NULL) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    if (request == NULL && req_len > 0) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    xrce_service_client_state *cs = (xrce_service_client_state *)client->backend_data;
    xrce_session_state_t *st = cs->session_state;
    xrce_service_client_slot *slot = cs->slot;
    if (slot == NULL) {
        return NROS_RMW_RET_ERROR;
    }
    /* Clear any stale reply so try_recv_reply_raw doesn't surface
     * an earlier request's response. */
    slot->has_reply = false;
    slot->overflow = false;
    /* XRCE-DDS interop: strip the 4-byte CDR encapsulation header (see
     * send_reply / publish_raw). */
    const uint8_t *body = request;
    size_t body_len = req_len;
    if (body_len >= XRCE_CDR_HEADER_LEN) {
        body += XRCE_CDR_HEADER_LEN;
        body_len -= XRCE_CDR_HEADER_LEN;
    }
    uint16_t req = uxr_buffer_request(
        &st->session, st->output_reliable, cs->requester_oid,
        (uint8_t *)(uintptr_t)body, body_len);
    if (req == UXR_INVALID_REQUEST_ID) {
        return NROS_RMW_RET_ERROR;
    }
    /* Flush the reliable output stream so the request actually
     * leaves the session — matches the publisher / send_reply
     * paths' explicit flush. Subsequent `drive_io` calls drive
     * reliable retransmission. */
    (void)uxr_run_session_time(&st->session, XRCE_SESSION_FLUSH_TIMEOUT_MS);
    return NROS_RMW_RET_OK;
}

int32_t xrce_service_try_recv_reply_raw(nros_rmw_service_client_t *client,
                                         uint8_t *reply_buf,
                                         size_t reply_buf_len) {
    if (client == NULL || client->backend_data == NULL) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    xrce_service_client_state *cs = (xrce_service_client_state *)client->backend_data;
    xrce_service_client_slot *slot = cs->slot;
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
    return (int32_t)len;
}

/* ---- Service client -------------------------------------------------- */

nros_rmw_ret_t xrce_service_client_create(nros_rmw_session_t *session,
                                          const char *service_name,
                                          const char *type_name,
                                          const char *type_hash,
                                          uint32_t domain_id,
                                          const nros_rmw_qos_t *qos,
                                          nros_rmw_service_client_t *out) {
    (void)type_hash;
    (void)domain_id;

    if (session == NULL || out == NULL || service_name == NULL || type_name == NULL) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    xrce_session_state_t *st = (xrce_session_state_t *)session->backend_data;
    if (st == NULL) {
        return NROS_RMW_RET_ERROR;
    }

    xrce_service_client_slot *slot = NULL;
    for (size_t i = 0; i < XRCE_MAX_SERVICE_CLIENTS; ++i) {
        if (!st->service_client_slots[i].active) {
            slot = &st->service_client_slots[i];
            break;
        }
    }
    if (slot == NULL) {
        return NROS_RMW_RET_ERROR;
    }

    xrce_service_client_state *cs = (xrce_service_client_state *)
        calloc(1, sizeof(xrce_service_client_state));
    if (cs == NULL) {
        return NROS_RMW_RET_BAD_ALLOC;
    }
    cs->session_state = st;
    cs->slot          = slot;

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
    xrce_dds_reply_type  (type_name, reply_type_buf, sizeof(reply_type_buf));
    xrce_dds_request_topic(service_name, req_topic_buf, sizeof(req_topic_buf));
    xrce_dds_reply_topic  (service_name, reply_topic_buf, sizeof(reply_topic_buf));

    nros_rmw_qos_t default_qos = NROS_RMW_QOS_PROFILE_SERVICES_DEFAULT;
    const nros_rmw_qos_t *eff_qos = (qos != NULL) ? qos : &default_qos;
    uxrQoS_t xrce_qos = xrce_map_qos(eff_qos);

    uint16_t req = uxr_buffer_create_requester_bin(
        &st->session, st->output_reliable, requester_oid, st->participant_oid,
        service_buf, req_type_buf, reply_type_buf,
        req_topic_buf, reply_topic_buf, xrce_qos, UXR_REPLACE);

    uint16_t requests[1] = { req };
    uint8_t  statuses[1] = { 0 };
    nros_rmw_ret_t cret = xrce_confirm_entities(st, requests, statuses, 1);
    if (cret != NROS_RMW_RET_OK) {
        free(cs);
        return cret;
    }

    slot->requester_id = requester_oid.id;
    slot->has_reply    = false;
    slot->overflow     = false;
    slot->len          = 0;
    slot->active       = true;
    cs->requester_oid  = requester_oid;

    uxrDeliveryControl delivery = {
        .max_samples           = UXR_MAX_SAMPLES_UNLIMITED,
        .max_elapsed_time      = UXR_MAX_ELAPSED_TIME_UNLIMITED,
        .max_bytes_per_second  = UXR_MAX_BYTES_PER_SECOND_UNLIMITED,
        .min_pace_period       = 0,
    };
    (void)uxr_buffer_request_data(&st->session, st->output_reliable,
                                  requester_oid, st->input_reliable, &delivery);
    (void)uxr_run_session_time(&st->session, XRCE_SESSION_FLUSH_TIMEOUT_MS);

    out->backend_data = cs;
    return NROS_RMW_RET_OK;
}

void xrce_service_client_destroy(nros_rmw_service_client_t *client) {
    if (client == NULL || client->backend_data == NULL) {
        return;
    }
    xrce_service_client_state *cs = (xrce_service_client_state *)client->backend_data;
    xrce_session_state_t *st = cs->session_state;

    if (cs->slot != NULL) {
        cs->slot->active = false;
        cs->slot->has_reply = false;
    }
    (void)uxr_buffer_delete_entity(&st->session, st->output_reliable,
                                   cs->requester_oid);
    (void)uxr_run_session_time(&st->session, 0);

    free(cs);
    client->backend_data = NULL;
}

int32_t xrce_service_call_raw(nros_rmw_service_client_t *client,
                              const uint8_t *request, size_t req_len,
                              uint8_t *reply_buf, size_t reply_buf_len) {
    if (client == NULL || client->backend_data == NULL) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    if (request == NULL && req_len > 0) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    xrce_service_client_state *cs = (xrce_service_client_state *)client->backend_data;
    xrce_session_state_t *st = cs->session_state;
    xrce_service_client_slot *slot = cs->slot;
    if (slot == NULL) {
        return NROS_RMW_RET_ERROR;
    }

    /* Drop any stale reply. */
    slot->has_reply = false;
    slot->overflow = false;

    /* XRCE-DDS interop: strip the 4-byte CDR encapsulation header. */
    const uint8_t *body = request;
    size_t body_len = req_len;
    if (body_len >= XRCE_CDR_HEADER_LEN) {
        body += XRCE_CDR_HEADER_LEN;
        body_len -= XRCE_CDR_HEADER_LEN;
    }
    uint16_t req = uxr_buffer_request(
        &st->session, st->output_reliable, cs->requester_oid,
        (uint8_t *)(uintptr_t)body, body_len);
    if (req == UXR_INVALID_REQUEST_ID) {
        return NROS_RMW_RET_ERROR;
    }

    /* Bounded busy-wait. Mirrors the Rust impl's
     * SERVICE_REPLY_RETRIES / SERVICE_REPLY_TIMEOUT_MS but spelled
     * inline since there's no executor at this layer. */
    int32_t total_ms = 0;
    while (total_ms < XRCE_SERVICE_REPLY_TOTAL_MS) {
        (void)uxr_run_session_time(&st->session, XRCE_SERVICE_REPLY_TIMEOUT_MS);
        total_ms += XRCE_SERVICE_REPLY_TIMEOUT_MS;
        if (slot->has_reply) {
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
            return (int32_t)len;
        }
    }
    return NROS_RMW_RET_TIMEOUT;
}

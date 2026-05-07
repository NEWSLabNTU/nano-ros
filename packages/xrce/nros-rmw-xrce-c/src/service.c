/* Phase 115.K.2 — service server / client paths.
 *
 * Phase 115.K.2.2 lands the request/reply callback stubs (registered
 * once at session_open) so the link satisfies. Phase 115.K.2.3 fills
 * in `xrce_service_*_create` / `xrce_service_call_raw` /
 * `xrce_service_send_reply` against `uxr_buffer_create_replier_bin`
 * and friends.
 */

#include "internal.h"

#include "nros/rmw_ret.h"

#include <string.h>

/* Single-session callbacks — registered once at session_open via
 * `uxr_set_request_callback` / `uxr_set_reply_callback`. Phase
 * 115.K.2.3 dispatches by object_id to the matching slot in the
 * per-session pool. The K.2.2 stubs below are link-only. */

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
        if (len > XRCE_BUFFER_SIZE) {
            slot->overflow = true;
            slot->has_request = true;
            return;
        }
        slot->overflow = false;
        memcpy(slot->data, ub->iterator, len);
        slot->len = len;
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
        if (len > XRCE_BUFFER_SIZE) {
            slot->overflow = true;
            slot->has_reply = true;
            return;
        }
        slot->overflow = false;
        memcpy(slot->data, ub->iterator, len);
        slot->len = len;
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
                                          nros_rmw_service_server_t *out) {
    (void)session;
    (void)service_name;
    (void)type_name;
    (void)type_hash;
    (void)domain_id;
    (void)out;
    return NROS_RMW_RET_UNSUPPORTED;
}

void xrce_service_server_destroy(nros_rmw_service_server_t *server) {
    (void)server;
}

int32_t xrce_service_try_recv_request(nros_rmw_service_server_t *server,
                                      uint8_t *buf, size_t buf_len,
                                      int64_t *seq_out) {
    (void)server;
    (void)buf;
    (void)buf_len;
    (void)seq_out;
    return NROS_RMW_RET_UNSUPPORTED;
}

int32_t xrce_service_has_request(nros_rmw_service_server_t *server) {
    (void)server;
    return 0;
}

nros_rmw_ret_t xrce_service_send_reply(nros_rmw_service_server_t *server,
                                       int64_t seq,
                                       const uint8_t *data, size_t len) {
    (void)server;
    (void)seq;
    (void)data;
    (void)len;
    return NROS_RMW_RET_UNSUPPORTED;
}

/* ---- Service client -------------------------------------------------- */

nros_rmw_ret_t xrce_service_client_create(nros_rmw_session_t *session,
                                          const char *service_name,
                                          const char *type_name,
                                          const char *type_hash,
                                          uint32_t domain_id,
                                          nros_rmw_service_client_t *out) {
    (void)session;
    (void)service_name;
    (void)type_name;
    (void)type_hash;
    (void)domain_id;
    (void)out;
    return NROS_RMW_RET_UNSUPPORTED;
}

void xrce_service_client_destroy(nros_rmw_service_client_t *client) {
    (void)client;
}

int32_t xrce_service_call_raw(nros_rmw_service_client_t *client,
                              const uint8_t *request, size_t req_len,
                              uint8_t *reply_buf, size_t reply_buf_len) {
    (void)client;
    (void)request;
    (void)req_len;
    (void)reply_buf;
    (void)reply_buf_len;
    return NROS_RMW_RET_UNSUPPORTED;
}

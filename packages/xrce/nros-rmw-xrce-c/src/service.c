/* Service server + client stubs — see session.c for the Phase 115.K.2
 * scaffold rationale.
 */

#include "internal.h"

#include "nros/rmw_ret.h"

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

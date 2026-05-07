/* Subscriber stubs — see session.c for the Phase 115.K.2 scaffold rationale. */

#include "internal.h"

#include "nros/rmw_ret.h"

nros_rmw_ret_t xrce_subscriber_create(nros_rmw_session_t *session,
                                      const char *topic_name,
                                      const char *type_name,
                                      const char *type_hash,
                                      uint32_t domain_id,
                                      const nros_rmw_qos_t *qos,
                                      nros_rmw_subscriber_t *out) {
    (void)session;
    (void)topic_name;
    (void)type_name;
    (void)type_hash;
    (void)domain_id;
    (void)qos;
    (void)out;
    return NROS_RMW_RET_UNSUPPORTED;
}

void xrce_subscriber_destroy(nros_rmw_subscriber_t *subscriber) {
    (void)subscriber;
}

int32_t xrce_subscriber_try_recv_raw(nros_rmw_subscriber_t *subscriber,
                                     uint8_t *buf, size_t buf_len) {
    (void)subscriber;
    (void)buf;
    (void)buf_len;
    return NROS_RMW_RET_UNSUPPORTED;
}

int32_t xrce_subscriber_has_data(nros_rmw_subscriber_t *subscriber) {
    (void)subscriber;
    return 0;
}

/* Publisher stubs — see session.c for the Phase 115.K.2 scaffold rationale. */

#include "internal.h"

#include "nros/rmw_ret.h"

nros_rmw_ret_t xrce_publisher_create(nros_rmw_session_t *session,
                                     const char *topic_name,
                                     const char *type_name,
                                     const char *type_hash,
                                     uint32_t domain_id,
                                     const nros_rmw_qos_t *qos,
                                     nros_rmw_publisher_t *out) {
    (void)session;
    (void)topic_name;
    (void)type_name;
    (void)type_hash;
    (void)domain_id;
    (void)qos;
    (void)out;
    return NROS_RMW_RET_UNSUPPORTED;
}

void xrce_publisher_destroy(nros_rmw_publisher_t *publisher) {
    (void)publisher;
}

nros_rmw_ret_t xrce_publisher_publish_raw(nros_rmw_publisher_t *publisher,
                                          const uint8_t *data, size_t len) {
    (void)publisher;
    (void)data;
    (void)len;
    return NROS_RMW_RET_UNSUPPORTED;
}

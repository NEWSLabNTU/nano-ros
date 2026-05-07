#ifndef NROS_RMW_XRCE_C_INTERNAL_H
#define NROS_RMW_XRCE_C_INTERNAL_H

/* Shared declarations across vtable.c / session.c / publisher.c /
 * subscriber.c / service.c. Phase 115.K.2 ships only the stub bodies;
 * later sub-phases flesh out the actual `uxr_*` calls.
 */

#include "nros/rmw_entity.h"
#include "nros/rmw_event.h"
#include "nros/rmw_ret.h"

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* ---- session.c ---- */
nros_rmw_ret_t xrce_session_open(const char *locator, uint8_t mode,
                                 uint32_t domain_id, const char *node_name,
                                 nros_rmw_session_t *out);
nros_rmw_ret_t xrce_session_close(nros_rmw_session_t *session);
nros_rmw_ret_t xrce_session_drive_io(nros_rmw_session_t *session,
                                     int32_t timeout_ms);

/* ---- publisher.c ---- */
nros_rmw_ret_t xrce_publisher_create(nros_rmw_session_t *session,
                                     const char *topic_name,
                                     const char *type_name,
                                     const char *type_hash,
                                     uint32_t domain_id,
                                     const nros_rmw_qos_t *qos,
                                     nros_rmw_publisher_t *out);
void           xrce_publisher_destroy(nros_rmw_publisher_t *publisher);
nros_rmw_ret_t xrce_publisher_publish_raw(nros_rmw_publisher_t *publisher,
                                          const uint8_t *data, size_t len);

/* ---- subscriber.c ---- */
nros_rmw_ret_t xrce_subscriber_create(nros_rmw_session_t *session,
                                      const char *topic_name,
                                      const char *type_name,
                                      const char *type_hash,
                                      uint32_t domain_id,
                                      const nros_rmw_qos_t *qos,
                                      nros_rmw_subscriber_t *out);
void           xrce_subscriber_destroy(nros_rmw_subscriber_t *subscriber);
int32_t        xrce_subscriber_try_recv_raw(nros_rmw_subscriber_t *subscriber,
                                            uint8_t *buf, size_t buf_len);
int32_t        xrce_subscriber_has_data(nros_rmw_subscriber_t *subscriber);

/* ---- service.c ---- */
nros_rmw_ret_t xrce_service_server_create(nros_rmw_session_t *session,
                                          const char *service_name,
                                          const char *type_name,
                                          const char *type_hash,
                                          uint32_t domain_id,
                                          nros_rmw_service_server_t *out);
void           xrce_service_server_destroy(nros_rmw_service_server_t *server);
int32_t        xrce_service_try_recv_request(nros_rmw_service_server_t *server,
                                             uint8_t *buf, size_t buf_len,
                                             int64_t *seq_out);
int32_t        xrce_service_has_request(nros_rmw_service_server_t *server);
nros_rmw_ret_t xrce_service_send_reply(nros_rmw_service_server_t *server,
                                       int64_t seq,
                                       const uint8_t *data, size_t len);

nros_rmw_ret_t xrce_service_client_create(nros_rmw_session_t *session,
                                          const char *service_name,
                                          const char *type_name,
                                          const char *type_hash,
                                          uint32_t domain_id,
                                          nros_rmw_service_client_t *out);
void           xrce_service_client_destroy(nros_rmw_service_client_t *client);
int32_t        xrce_service_call_raw(nros_rmw_service_client_t *client,
                                     const uint8_t *request, size_t req_len,
                                     uint8_t *reply_buf, size_t reply_buf_len);

#ifdef __cplusplus
}
#endif

#endif /* NROS_RMW_XRCE_C_INTERNAL_H */

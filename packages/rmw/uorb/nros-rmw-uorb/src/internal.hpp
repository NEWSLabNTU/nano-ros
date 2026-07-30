#ifndef NROS_RMW_UORB_INTERNAL_HPP
#define NROS_RMW_UORB_INTERNAL_HPP

// Shared declarations across vtable.cpp / session.cpp /
// publisher.cpp / subscriber.cpp / service.cpp. Phase 115.K.4.0
// ships stub bodies; K.4.1–K.4.3 flesh out the actual uORB calls.

#include "nros/rmw_entity.h"
#include "nros/rmw_event.h"
#include "nros/rmw_ret.h"

namespace nros_rmw_uorb {

/* ---- session.cpp ---- */
nros_rmw_ret_t session_create(const char* locator, uint8_t mode, uint32_t domain_id,
                            const char* node_name, nros_rmw_session_t* out);
nros_rmw_ret_t session_destroy(nros_rmw_session_t* session);
nros_rmw_ret_t session_drive_io(nros_rmw_session_t* session, int32_t timeout_ms);

/* ---- publisher.cpp ---- */
nros_rmw_ret_t publisher_create(nros_rmw_session_t* session, const char* topic_name,
                                const char* type_name, const char* type_hash, uint32_t domain_id,
                                const nros_rmw_qos_t* qos,
                                const nros_rmw_publisher_options_t* options,
                                nros_rmw_publisher_t* out);
void publisher_destroy(nros_rmw_publisher_t* publisher);
nros_rmw_ret_t publisher_publish_raw(nros_rmw_publisher_t* publisher, const uint8_t* data,
                                     size_t len);

/* ---- subscriber.cpp ---- */
nros_rmw_ret_t subscription_create(nros_rmw_session_t* session, const char* topic_name,
                                 const char* type_name, const char* type_hash, uint32_t domain_id,
                                 const nros_rmw_qos_t* qos,
                                 const nros_rmw_subscription_options_t* options,
                                 nros_rmw_subscription_t* out);
void subscription_destroy(nros_rmw_subscription_t* subscriber);
int32_t subscription_try_recv_raw(nros_rmw_subscription_t* subscriber, uint8_t* buf, size_t buf_len);
int32_t subscription_has_data(nros_rmw_subscription_t* subscriber);

/* ---- service.cpp ---- */
nros_rmw_ret_t service_create(nros_rmw_session_t* session, const char* service_name,
                                     const char* type_name, const char* type_hash,
                                     uint32_t domain_id, const nros_rmw_qos_t* qos,
                                     nros_rmw_service_t* out);
void service_destroy(nros_rmw_service_t* server);
int32_t service_try_recv_request(nros_rmw_service_t* server, uint8_t* buf, size_t buf_len,
                                 int64_t* seq_out);
int32_t service_has_request(nros_rmw_service_t* server);
nros_rmw_ret_t service_send_reply(nros_rmw_service_t* server, int64_t seq,
                                  const uint8_t* data, size_t len);

nros_rmw_ret_t client_create(nros_rmw_session_t* session, const char* service_name,
                                     const char* type_name, const char* type_hash,
                                     uint32_t domain_id, const nros_rmw_qos_t* qos,
                                     nros_rmw_client_t* out);
void client_destroy(nros_rmw_client_t* client);
/* Phase-301: the deprecated blocking `call_raw` slot was deleted from the
 * vtable; the non-blocking `send_request_raw` / `try_recv_reply_raw` pair
 * stays NULL on this backend (services unsupported). */

} // namespace nros_rmw_uorb

#endif // NROS_RMW_UORB_INTERNAL_HPP

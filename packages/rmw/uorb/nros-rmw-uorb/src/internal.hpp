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
rmw_ret_t session_create(const char* locator, uint8_t mode, uint32_t domain_id,
                            const char* node_name,
                            const rmw_session_options_t* options, rmw_session_t* out);
rmw_ret_t session_destroy(rmw_session_t* session);
rmw_ret_t session_drive_io(rmw_session_t* session, int32_t timeout_ms);

/* ---- publisher.cpp ---- */
rmw_ret_t publisher_create(const rmw_node_t* node, const char* topic_name,
                                const char* type_name, const char* type_hash, uint32_t domain_id,
                                const rmw_qos_profile_t* qos,
                                const rmw_publisher_options_t* options,
                                rmw_publisher_t* out);
rmw_ret_t publisher_destroy(rmw_publisher_t* publisher);
rmw_ret_t publisher_publish_raw(const rmw_publisher_t* publisher, const uint8_t* data,
                                     size_t len);

/* ---- subscriber.cpp ---- */
rmw_ret_t subscription_create(const rmw_node_t* node, const char* topic_name,
                                 const char* type_name, const char* type_hash, uint32_t domain_id,
                                 const rmw_qos_profile_t* qos,
                                 const rmw_subscription_options_t* options,
                                 rmw_subscription_t* out);
rmw_ret_t subscription_destroy(rmw_subscription_t* subscriber);
rmw_ret_t subscription_take(const rmw_subscription_t* subscriber, uint8_t* buf,
                                 size_t buf_len, size_t* out_len, bool* taken);
rmw_ret_t subscription_has_data(rmw_subscription_t* subscriber, bool* out_has_data);

/* ---- service.cpp ---- */
rmw_ret_t service_create(const rmw_node_t* node, const char* service_name,
                                     const char* type_name, const char* type_hash,
                                     uint32_t domain_id, const rmw_qos_profile_t* qos,
                                     rmw_service_t* out);
rmw_ret_t service_destroy(rmw_service_t* server);
rmw_ret_t service_take_request(const rmw_service_t* server, uint8_t* buf,
                                    size_t buf_len, int64_t* seq_out, size_t* out_len,
                                    bool* taken);
rmw_ret_t service_has_request(rmw_service_t* server, bool* out_has_request);
rmw_ret_t service_send_response(const rmw_service_t* server, int64_t seq,
                                  const uint8_t* data, size_t len);

rmw_ret_t client_create(const rmw_node_t* node, const char* service_name,
                                     const char* type_name, const char* type_hash,
                                     uint32_t domain_id, const rmw_qos_profile_t* qos,
                                     rmw_client_t* out);
rmw_ret_t client_destroy(rmw_client_t* client);
/* Phase-301: the deprecated blocking `call_raw` slot was deleted from the
 * vtable; the non-blocking `send_request_raw` / `try_recv_reply_raw` pair
 * stays NULL on this backend (services unsupported). */

} // namespace nros_rmw_uorb

#endif // NROS_RMW_UORB_INTERNAL_HPP

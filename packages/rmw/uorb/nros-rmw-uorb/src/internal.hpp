#ifndef NROS_RMW_UORB_INTERNAL_HPP
#define NROS_RMW_UORB_INTERNAL_HPP

// Shared declarations across vtable.cpp / session.cpp /
// publisher.cpp / subscriber.cpp / service.cpp. Phase 115.K.4.0
// ships stub bodies; K.4.1–K.4.3 flesh out the actual uORB calls.

#include "nros/rmw_entity.h"
#include "nros/rmw_event.h"
#include "nros/rmw_ret.h"

struct orb_metadata;

namespace nros_rmw_uorb {

/* ---- qos.cpp (issue 1329) ---- */

/** Refuse a profile this backend cannot serve, at create time.
 *
 *  `NROS_RMW_RET_INCOMPATIBLE_QOS` for KEEP_ALL history or TRANSIENT_LOCAL
 *  durability; OK otherwise, including for a NULL profile. See `qos.cpp` for
 *  what each of the four CORE policies means on a shared-memory ring. */
rmw_ret_t qos_admit(const rmw_qos_profile_t* qos);

/** Overwrite `in_out` — which arrives carrying the REQUEST — with what this
 *  backend actually gave, leaving every unreportable field as it came in.
 *  The `*_get_actual_qos` slots below are one-liners over this. */
rmw_ret_t qos_granted(const struct orb_metadata* meta, rmw_qos_profile_t* in_out);

/** The `NROS_RMW_QOS_POLICY_*` bits this backend honours. */
rmw_ret_t supported_qos_policies(const rmw_session_t* session, uint32_t* out_mask);

/* ---- session.cpp ---- */
rmw_ret_t session_create(const char* locator, uint8_t mode, uint32_t domain_id,
                         const char* node_name, const rmw_session_options_t* options,
                         rmw_session_t* out);
rmw_ret_t session_destroy(rmw_session_t* session);
rmw_ret_t session_drive_io(rmw_session_t* session, int32_t timeout_ms);

/* ---- publisher.cpp ---- */
rmw_ret_t publisher_create(const rmw_node_t* node, const rmw_message_type_support_t* type_support,
                           const char* topic_name, uint32_t domain_id, const rmw_qos_profile_t* qos,
                           const rmw_publisher_options_t* options, rmw_publisher_t* out);
rmw_ret_t publisher_destroy(rmw_publisher_t* publisher);
rmw_ret_t publisher_publish_raw(const rmw_publisher_t* publisher, rmw_byte_span_t payload);
rmw_ret_t publisher_get_actual_qos(const rmw_publisher_t* publisher, rmw_qos_profile_t* qos);

/* ---- subscriber.cpp ---- */
rmw_ret_t subscription_create(const rmw_node_t* node,
                              const rmw_message_type_support_t* type_support,
                              const char* topic_name, uint32_t domain_id,
                              const rmw_qos_profile_t* qos,
                              const rmw_subscription_options_t* options, rmw_subscription_t* out);
rmw_ret_t subscription_destroy(rmw_subscription_t* subscriber);
rmw_ret_t subscription_take(const rmw_subscription_t* subscriber, rmw_mut_byte_span_t* message,
                            bool* taken);
rmw_ret_t subscription_has_data(rmw_subscription_t* subscriber, bool* out_has_data);
rmw_ret_t subscription_get_actual_qos(const rmw_subscription_t* subscriber,
                                      rmw_qos_profile_t* qos);

/* ---- service.cpp ---- */
rmw_ret_t service_create(const rmw_node_t* node, const rmw_service_type_support_t* type_support,
                         const char* service_name, uint32_t domain_id, const rmw_qos_profile_t* qos,
                         rmw_service_t* out);
rmw_ret_t service_destroy(rmw_service_t* server);
rmw_ret_t service_take_request(const rmw_service_t* server, rmw_mut_byte_span_t* request,
                               int64_t* seq_out, bool* taken);
rmw_ret_t service_has_request(rmw_service_t* server, bool* out_has_request);
rmw_ret_t service_send_response(const rmw_service_t* server, int64_t seq, rmw_byte_span_t response);

rmw_ret_t client_create(const rmw_node_t* node, const rmw_service_type_support_t* type_support,
                        const char* service_name, uint32_t domain_id, const rmw_qos_profile_t* qos,
                        rmw_client_t* out);
rmw_ret_t client_destroy(rmw_client_t* client);
/* Phase-301: the deprecated blocking `call_raw` slot was deleted from the
 * vtable; the non-blocking `send_request_raw` / `take_response_raw` pair
 * stays NULL on this backend (services unsupported). */

} // namespace nros_rmw_uorb

#endif // NROS_RMW_UORB_INTERNAL_HPP

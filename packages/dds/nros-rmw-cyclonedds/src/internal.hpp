#ifndef NROS_RMW_CYCLONEDDS_INTERNAL_HPP
#define NROS_RMW_CYCLONEDDS_INTERNAL_HPP

// Shared declarations across vtable.cpp / session.cpp / publisher.cpp /
// subscriber.cpp / service.cpp / qos.cpp / descriptors.cpp. Phase
// 117.3 ships only the stub bodies; later sub-phases flesh out the
// actual Cyclone calls.

#include <dds/dds.h>

#include "nros/rmw_entity.h"
#include "nros/rmw_event.h"
#include "nros/rmw_ret.h"

namespace nros_rmw_cyclonedds {

/* ---- session.cpp helpers ---- */
/** Return the Cyclone participant handle for an open session, or 0
 *  if the session is uninitialised / closed. */
dds_entity_t session_participant(const nros_rmw_session_t *session);

/* ---- publisher.cpp / subscriber.cpp helpers ---- */
/** Return the Cyclone writer handle for a publisher created by
 *  this backend, or 0 if the publisher is uninitialised. Used by
 *  Phase 117.6.B's data-plane wiring once the raw-CDR path lands. */
dds_entity_t publisher_writer(const nros_rmw_publisher_t *publisher);
/** Return the Cyclone reader handle for a subscriber, or 0 if
 *  uninitialised. */
dds_entity_t subscriber_reader(const nros_rmw_subscriber_t *subscriber);


/* ---- session.cpp ---- */
nros_rmw_ret_t session_open(const char *locator, uint8_t mode,
                            uint32_t domain_id, const char *node_name,
                            nros_rmw_session_t *out);
nros_rmw_ret_t session_close(nros_rmw_session_t *session);
nros_rmw_ret_t session_drive_io(nros_rmw_session_t *session, int32_t timeout_ms);

/* ---- publisher.cpp ---- */
nros_rmw_ret_t publisher_create(nros_rmw_session_t *session,
                                const char *topic_name, const char *type_name,
                                const char *type_hash, uint32_t domain_id,
                                const nros_rmw_qos_t *qos,
                                nros_rmw_publisher_t *out);
void           publisher_destroy(nros_rmw_publisher_t *publisher);
nros_rmw_ret_t publisher_publish_raw(nros_rmw_publisher_t *publisher,
                                     const uint8_t *data, size_t len);

/* ---- subscriber.cpp ---- */
nros_rmw_ret_t subscriber_create(nros_rmw_session_t *session,
                                 const char *topic_name, const char *type_name,
                                 const char *type_hash, uint32_t domain_id,
                                 const nros_rmw_qos_t *qos,
                                 nros_rmw_subscriber_t *out);
void           subscriber_destroy(nros_rmw_subscriber_t *subscriber);
int32_t        subscriber_try_recv_raw(nros_rmw_subscriber_t *subscriber,
                                       uint8_t *buf, size_t buf_len);
int32_t        subscriber_has_data(nros_rmw_subscriber_t *subscriber);

/* ---- service.cpp ---- */
nros_rmw_ret_t service_server_create(nros_rmw_session_t *session,
                                     const char *service_name,
                                     const char *type_name,
                                     const char *type_hash,
                                     uint32_t domain_id,
                                     nros_rmw_service_server_t *out);
void           service_server_destroy(nros_rmw_service_server_t *server);
int32_t        service_try_recv_request(nros_rmw_service_server_t *server,
                                        uint8_t *buf, size_t buf_len,
                                        int64_t *seq_out);
int32_t        service_has_request(nros_rmw_service_server_t *server);
nros_rmw_ret_t service_send_reply(nros_rmw_service_server_t *server, int64_t seq,
                                  const uint8_t *data, size_t len);

nros_rmw_ret_t service_client_create(nros_rmw_session_t *session,
                                     const char *service_name,
                                     const char *type_name,
                                     const char *type_hash,
                                     uint32_t domain_id,
                                     nros_rmw_service_client_t *out);
void           service_client_destroy(nros_rmw_service_client_t *client);
int32_t        service_call_raw(nros_rmw_service_client_t *client,
                                const uint8_t *request, size_t req_len,
                                uint8_t *reply_buf, size_t reply_buf_len);

} // namespace nros_rmw_cyclonedds

#endif // NROS_RMW_CYCLONEDDS_INTERNAL_HPP

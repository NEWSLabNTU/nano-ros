#ifndef NROS_RMW_CYCLONEDDS_INTERNAL_HPP
#define NROS_RMW_CYCLONEDDS_INTERNAL_HPP

// Shared declarations across vtable.cpp / session.cpp / publisher.cpp /
// subscriber.cpp / service.cpp / qos.cpp / descriptors.cpp. Phase
// 117.3 ships only the stub bodies; later sub-phases flesh out the
// actual Cyclone calls.

#include <dds/dds.h>

#include "graph.hpp"  // Phase 177.36 — GraphState + ros_discovery_info API
#include "nros/rmw_entity.h"
#include "nros/rmw_event.h"
#include "nros/rmw_ret.h"

#include <cstddef>
#include <cstdint>

#if defined(NROS_PLATFORM_FREERTOS)
#include <FreeRTOS.h>
#include <task.h>
extern "C" {
uint64_t nros_platform_random_u64(void);
}
#elif defined(NROS_PLATFORM_ZEPHYR) || defined(__ZEPHYR__)
extern "C" {
uint64_t nros_platform_clock_ms(void);
void nros_platform_sleep_ms(size_t ms);
uint64_t nros_platform_random_u64(void);
}
#elif defined(NROS_PLATFORM_THREADX)
extern "C" {
uint64_t nros_platform_clock_ms(void);
void nros_platform_sleep_ms(size_t ms);
uint64_t nros_platform_random_u64(void);
}
#else
#include <chrono>
#include <thread>
#endif

namespace nros_rmw_cyclonedds {

inline void platform_sleep_ms(uint32_t timeout_ms) {
    if (timeout_ms == 0) {
        return;
    }
#if defined(NROS_PLATFORM_FREERTOS)
    vTaskDelay(pdMS_TO_TICKS(timeout_ms));
#elif defined(NROS_PLATFORM_ZEPHYR) || defined(__ZEPHYR__)
    nros_platform_sleep_ms(static_cast<size_t>(timeout_ms));
#elif defined(NROS_PLATFORM_THREADX)
    nros_platform_sleep_ms(static_cast<size_t>(timeout_ms));
#else
    std::this_thread::sleep_for(std::chrono::milliseconds(timeout_ms));
#endif
}

inline uint64_t platform_now_ms() {
#if defined(NROS_PLATFORM_FREERTOS)
    return static_cast<uint64_t>(xTaskGetTickCount()) * portTICK_PERIOD_MS;
#elif defined(NROS_PLATFORM_ZEPHYR) || defined(__ZEPHYR__)
    return nros_platform_clock_ms();
#elif defined(NROS_PLATFORM_THREADX)
    return nros_platform_clock_ms();
#else
    const auto now = std::chrono::steady_clock::now().time_since_epoch();
    return static_cast<uint64_t>(
        std::chrono::duration_cast<std::chrono::milliseconds>(now).count());
#endif
}

inline uint64_t platform_random_u64() {
#if defined(NROS_PLATFORM_FREERTOS) || defined(NROS_PLATFORM_ZEPHYR) || \
    defined(__ZEPHYR__) || \
    defined(NROS_PLATFORM_THREADX)
    return nros_platform_random_u64();
#else
    return 0;
#endif
}

/* ---- session.cpp helpers ---- */
/** Return the Cyclone participant handle for an open session, or 0
 *  if the session is uninitialised / closed. */
dds_entity_t session_participant(const nros_rmw_session_t *session);

/** Phase 177.36 — the per-session ros_discovery_info graph state, or nullptr
 *  for an unopened session. Endpoint-create paths register their reader/writer
 *  GIDs via graph_track_*. */
GraphState *session_graph(nros_rmw_session_t *session);

/* ---- publisher.cpp / subscriber.cpp helpers ---- */
/** Return the Cyclone writer handle for a publisher created by
 *  this backend, or 0 if the publisher is uninitialised. Used by
 *  Phase 117.6.B's data-plane wiring once the raw-CDR path lands. */
dds_entity_t publisher_writer(const nros_rmw_publisher_t *publisher);
/** Return the Cyclone reader handle for a subscriber, or 0 if
 *  uninitialised. */
dds_entity_t subscription_reader(const nros_rmw_subscription_t *subscriber);


/* ---- session.cpp ---- */
nros_rmw_ret_t session_create(const char *locator, uint8_t mode,
                            uint32_t domain_id, const char *node_name,
                            nros_rmw_session_t *out);
nros_rmw_ret_t session_destroy(nros_rmw_session_t *session);
nros_rmw_ret_t session_drive_io(nros_rmw_session_t *session, int32_t timeout_ms);

/* ---- publisher.cpp ---- */
nros_rmw_ret_t publisher_create(nros_rmw_session_t *session,
                                const char *topic_name, const char *type_name,
                                const char *type_hash, uint32_t domain_id,
                                const nros_rmw_qos_t *qos,
                                const nros_rmw_publisher_options_t *options,
                                nros_rmw_publisher_t *out);
void           publisher_destroy(nros_rmw_publisher_t *publisher);
nros_rmw_ret_t publisher_publish_raw(nros_rmw_publisher_t *publisher,
                                     const uint8_t *data, size_t len);

/* ---- subscriber.cpp ---- */
nros_rmw_ret_t subscription_create(nros_rmw_session_t *session,
                                 const char *topic_name, const char *type_name,
                                 const char *type_hash, uint32_t domain_id,
                                 const nros_rmw_qos_t *qos,
                                 const nros_rmw_subscription_options_t *options,
                                 nros_rmw_subscription_t *out);
void           subscription_destroy(nros_rmw_subscription_t *subscriber);
int32_t        subscription_try_recv_raw(nros_rmw_subscription_t *subscriber,
                                       uint8_t *buf, size_t buf_len);
int32_t        subscription_try_recv_sequence(nros_rmw_subscription_t *subscriber,
                                            uint8_t *buf,
                                            size_t   per_msg_cap,
                                            size_t   max_msgs,
                                            size_t  *out_lens);
int32_t        subscription_has_data(nros_rmw_subscription_t *subscriber);

/* ---- service.cpp ---- */
nros_rmw_ret_t service_create(nros_rmw_session_t *session,
                                     const char *service_name,
                                     const char *type_name,
                                     const char *type_hash,
                                     uint32_t domain_id,
                                     const nros_rmw_qos_t *qos,
                                     nros_rmw_service_t *out);
void           service_destroy(nros_rmw_service_t *server);
int32_t        service_try_recv_request(nros_rmw_service_t *server,
                                        uint8_t *buf, size_t buf_len,
                                        int64_t *seq_out);
int32_t        service_has_request(nros_rmw_service_t *server);
nros_rmw_ret_t service_send_reply(nros_rmw_service_t *server, int64_t seq,
                                  const uint8_t *data, size_t len);

nros_rmw_ret_t client_create(nros_rmw_session_t *session,
                                     const char *service_name,
                                     const char *type_name,
                                     const char *type_hash,
                                     uint32_t domain_id,
                                     const nros_rmw_qos_t *qos,
                                     nros_rmw_client_t *out);
void           client_destroy(nros_rmw_client_t *client);
// Phase 130.8 — non-blocking send/recv split (phase-301: the deprecated
// blocking `call_raw` slot was deleted from the vtable; this pair is the
// one request/reply path).
nros_rmw_ret_t service_send_request_raw(nros_rmw_client_t *client,
                                        const uint8_t *request,
                                        size_t req_len);
int32_t        service_try_recv_reply_raw(nros_rmw_client_t *client,
                                          uint8_t *reply_buf,
                                          size_t reply_buf_len);

} // namespace nros_rmw_cyclonedds

#endif // NROS_RMW_CYCLONEDDS_INTERNAL_HPP

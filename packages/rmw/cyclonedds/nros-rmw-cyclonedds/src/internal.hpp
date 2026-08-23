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

// issue 0547 — the platform ABI comes from its OWN header, never re-declared.
//
// This block used to hand-declare `nros_platform_{clock_ms,sleep_ms,random_u64}`
// in three per-platform `extern "C"` blocks. RFC-0073 (phase-352) then replaced
// the `clock_ms`/`clock_us` pair with `clock_ns` and made `clock_ms` a
// `static inline` shim in `nros/platform.h` — at which point a local
// re-declaration saying `extern` still COMPILED, and the linker was left to
// discover there was no such symbol:
//
//     internal.hpp:63: undefined reference to `nros_platform_clock_ms'
//
// (W6 has since retired the shim outright, so the name is gone entirely and
// the call here divides `clock_ns` itself.)
//
// All three symbols are declared in `nros/platform.h`, so none of the hand
// copies were load-bearing; they only made the file able to disagree with the
// header. RFC-0054's rule is that the C header IS the SSoT for this ABI, and
// CLAUDE.md names hand-mirrored FFI declarations as a recurring defect class —
// this is that class in FUNCTION form, which fails at link rather than at
// compile and so reads as a missing implementation.
//
// The `#if` guards stay, and the include sits INSIDE them, because the hosted
// build genuinely cannot see this header: `check-rmw-cyclonedds` compiles the
// backend without `nros-platform-api/include` on its path (hosted uses
// `<chrono>`/`<thread>` and never touches the platform ABI), so an unguarded
// include fails with `nros/platform.h: No such file or directory`. Measured —
// the first cut of this fix hoisted it and broke that lane.
//
// So the guards select the IMPLEMENTATION and gate the header that backs it.
// What was never justified is DECLARING the ABI by hand inside them.

// phase-370 W4 — `env_lookup`, kept in its own dependency-free header so the
// light TUs that need it do not acquire this file's CycloneDDS includes.
//
// Included OUTSIDE the platform switch: it sat in the FREERTOS arm only, while
// `session.cpp` calls `env_lookup` unconditionally, so every non-FreeRTOS
// platform failed to compile it. ThreadX is where that surfaced (tier-2
// fixture build); Zephyr and the host arm had the same hole. The header pulls
// in nothing, which is the whole reason it exists, so there is no arm that
// cannot afford it.
#include "env_compat.hpp"
#if defined(NROS_PLATFORM_FREERTOS)
#include <FreeRTOS.h>
#include <task.h>
#include "nros/platform.h"

#elif defined(NROS_PLATFORM_ZEPHYR) || defined(__ZEPHYR__)
#include "nros/platform.h"
#elif defined(NROS_PLATFORM_THREADX)
#include "nros/platform.h"
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
    return (nros_platform_clock_ns() / 1000000ULL);
#elif defined(NROS_PLATFORM_THREADX)
    return (nros_platform_clock_ns() / 1000000ULL);
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
nros_rmw_ret_t subscription_take(nros_rmw_subscription_t *subscriber, uint8_t *buf,
                                 size_t buf_len, size_t *out_len, bool *taken);
int32_t        subscription_try_recv_sequence(nros_rmw_subscription_t *subscriber,
                                            uint8_t *buf,
                                            size_t   per_msg_cap,
                                            size_t   max_msgs,
                                            size_t  *out_lens);
nros_rmw_ret_t subscription_has_data(nros_rmw_subscription_t *subscriber, bool *out_has_data);

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
nros_rmw_ret_t service_has_request(nros_rmw_service_t *server, bool *out_has_request);
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

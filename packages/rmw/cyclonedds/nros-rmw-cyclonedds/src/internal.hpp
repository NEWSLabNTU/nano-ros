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
// phase-444 W3 — the graph slots' visitor types (`rmw_names_and_types_visitor_t`
// and friends) are declared with the vtable, not with the entities.
#include "nros/rmw_vtable.h"

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
dds_entity_t session_participant(const rmw_session_t *session);

/** Phase 177.36 — the per-session ros_discovery_info graph state, or nullptr
 *  for an unopened session. Endpoint-create paths register their reader/writer
 *  GIDs via graph_track_*. */
GraphState *session_graph(rmw_session_t *session);

/** Issue 1269 — the graph slot an endpoint created on `node` belongs to, or -1.
 *  The record `create_node` stored in `backend_data` when there is one;
 *  otherwise the node is recorded by its name + namespace (a direct vtable
 *  caller that never declared it). */
int graph_node_of(const rmw_node_t *node);

/** Issue 1269 — the `create_node` / `destroy_node` slots: one graph record per
 *  node, so each shows in `ros2 node list` with its own endpoints. */
rmw_ret_t node_create(rmw_session_t *session, const char *name, const char *namespace_,
                      rmw_node_t *out);
rmw_ret_t node_destroy(rmw_node_t *node);

/* ---- graph_query.cpp — phase-444 W3, the graph READ slots ----------------
 *
 * The DDS builtin topics (`DCPSPublication` / `DCPSSubscription`) supply topic,
 * type, QoS and endpoint GUID; `ros_discovery_info` supplies the node the
 * endpoint belongs to. Each reports what has been DISCOVERED and never blocks;
 * an inactive graph answers UNSUPPORTED, which stays distinct from empty. */
rmw_ret_t graph_get_topic_names_and_types(const rmw_session_t *session, bool no_demangle,
                                          rmw_names_and_types_visitor_t visitor);
rmw_ret_t graph_get_service_names_and_types(const rmw_session_t *session,
                                            rmw_names_and_types_visitor_t visitor);
rmw_ret_t graph_get_publisher_names_and_types_by_node(const rmw_session_t *session,
                                                      const char *node_name,
                                                      const char *node_namespace,
                                                      bool no_demangle,
                                                      rmw_names_and_types_visitor_t visitor);
rmw_ret_t graph_get_subscriber_names_and_types_by_node(const rmw_session_t *session,
                                                       const char *node_name,
                                                       const char *node_namespace,
                                                       bool no_demangle,
                                                       rmw_names_and_types_visitor_t visitor);
rmw_ret_t graph_get_service_names_and_types_by_node(const rmw_session_t *session,
                                                    const char *node_name,
                                                    const char *node_namespace,
                                                    rmw_names_and_types_visitor_t visitor);
rmw_ret_t graph_get_client_names_and_types_by_node(const rmw_session_t *session,
                                                   const char *node_name,
                                                   const char *node_namespace,
                                                   rmw_names_and_types_visitor_t visitor);
rmw_ret_t graph_get_publishers_info_by_topic(const rmw_session_t *session, const char *topic_name,
                                             bool no_mangle,
                                             rmw_topic_endpoint_info_visitor_t visitor);
rmw_ret_t graph_get_subscriptions_info_by_topic(const rmw_session_t *session,
                                                const char *topic_name, bool no_mangle,
                                                rmw_topic_endpoint_info_visitor_t visitor);
rmw_ret_t graph_count_publishers(const rmw_session_t *session, const char *topic_name,
                                 std::size_t *count);
rmw_ret_t graph_count_subscribers(const rmw_session_t *session, const char *topic_name,
                                  std::size_t *count);

/* ---- publisher.cpp / subscriber.cpp helpers ---- */
/** Return the Cyclone writer handle for a publisher created by
 *  this backend, or 0 if the publisher is uninitialised. Used by
 *  Phase 117.6.B's data-plane wiring once the raw-CDR path lands. */
dds_entity_t publisher_writer(const rmw_publisher_t *publisher);
/** Return the Cyclone reader handle for a subscriber, or 0 if
 *  uninitialised. */
dds_entity_t subscription_reader(const rmw_subscription_t *subscriber);

/** issue 0823 — the QoS the participant ACTUALLY holds for `entity`.
 *
 * Inverse of `make_dds_qos`. `out` must arrive carrying the REQUESTED profile:
 * fields Cyclone does not report are left as they came in, so an unreported
 * field reads as "unchanged" instead of as a zero that looks like an answer. */
rmw_ret_t read_entity_qos(dds_entity_t entity, rmw_qos_profile_t *out);

/** phase-393 W1 — the DDS entities behind a client / service, for the QoS
 *  read-back. The mapping inverts between the two sides: a CLIENT's request
 *  publisher is its writer and its response subscription is its reader; a
 *  SERVICE's request subscription is its reader and its response publisher is
 *  its writer. */
dds_entity_t client_request_writer(const rmw_client_t *client);
dds_entity_t client_response_reader(const rmw_client_t *client);
dds_entity_t service_request_reader(const rmw_service_t *service);
dds_entity_t service_response_writer(const rmw_service_t *service);

/** phase-428 W13.c — the `service_server_is_available` slot: does a server
 *  hold BOTH halves of this client's pair (a matched reader on the request
 *  writer and a matched writer on the reply reader), paired by `serviceid`
 *  user data when the server advertises one. Read from Cyclone's matched
 *  sets, no query issued. `*out_available` is written only on OK. */
rmw_ret_t client_server_is_available(const rmw_client_t *client, bool *out_available);


/* ---- session.cpp ---- */
rmw_ret_t session_create(const char *locator, uint8_t mode,
                            uint32_t domain_id, const char *node_name,
                            const rmw_session_options_t *options,
                            rmw_session_t *out);
rmw_ret_t session_destroy(rmw_session_t *session);

/* Phase 124.B.1 / issue 0889 — install the executor's wake callback. Cyclone
 * fires it from its delivery thread via a participant-level data_available
 * listener, which is what lets `spin_once` block on its wake primitive instead
 * of polling on a timer. `cb == nullptr` clears. */
rmw_ret_t session_set_wake_callback(rmw_session_t *session, void (*cb)(void *),
                                    void *ctx);
rmw_ret_t session_drive_io(rmw_session_t *session, int32_t timeout_ms);

/* ---- publisher.cpp ---- */
rmw_ret_t publisher_create(const rmw_node_t* node,
                                const rmw_message_type_support_t *type_support,
                                const char *topic_name, uint32_t domain_id,
                                const rmw_qos_profile_t *qos,
                                const rmw_publisher_options_t *options,
                                rmw_publisher_t *out);
rmw_ret_t           publisher_destroy(rmw_publisher_t *publisher);
rmw_ret_t publisher_publish_raw(const rmw_publisher_t *publisher,
                                     rmw_byte_span_t payload);
/** Issue 1231 — manual liveliness assertion. `dds_assert_liveliness` on the
 *  writer for a MANUAL_BY_TOPIC / MANUAL_BY_NODE publisher; the ABI's
 *  documented no-op (`NROS_RMW_RET_OK`) for every other kind. The rationale,
 *  and what was measured in the pinned 0.10.5, is at the definition. */
rmw_ret_t publisher_assert_liveliness(const rmw_publisher_t *publisher);

/* ---- subscriber.cpp ---- */
rmw_ret_t subscription_create(const rmw_node_t* node,
                                 const rmw_message_type_support_t *type_support,
                                 const char *topic_name, uint32_t domain_id,
                                 const rmw_qos_profile_t *qos,
                                 const rmw_subscription_options_t *options,
                                 rmw_subscription_t *out);
rmw_ret_t           subscription_destroy(rmw_subscription_t *subscriber);
rmw_ret_t subscription_take(const rmw_subscription_t *subscriber,
                                 rmw_mut_byte_span_t *out, bool *taken);
rmw_ret_t subscription_take_sequence(const rmw_subscription_t *subscriber, uint8_t *buf,
                                          size_t per_msg_cap, size_t max_msgs, size_t *out_lens,
                                          size_t *taken);
rmw_ret_t subscription_has_data(rmw_subscription_t *subscriber, bool *out_has_data);

/* ---- service.cpp ---- */
rmw_ret_t service_create(const rmw_node_t* node,
                                     const rmw_service_type_support_t *type_support,
                                     const char *service_name,
                                     uint32_t domain_id,
                                     const rmw_qos_profile_t *qos,
                                     rmw_service_t *out);
rmw_ret_t           service_destroy(rmw_service_t *server);
rmw_ret_t service_take_request(const rmw_service_t *server, rmw_mut_byte_span_t *request,
                                    int64_t *seq_out, bool *taken);
rmw_ret_t service_has_request(rmw_service_t *server, bool *out_has_request);
rmw_ret_t service_send_response(const rmw_service_t *server, int64_t seq,
                                  rmw_byte_span_t response);

rmw_ret_t client_create(const rmw_node_t* node,
                                     const rmw_service_type_support_t *type_support,
                                     const char *service_name,
                                     uint32_t domain_id,
                                     const rmw_qos_profile_t *qos,
                                     rmw_client_t *out);
rmw_ret_t           client_destroy(rmw_client_t *client);
// Phase 130.8 — non-blocking send/recv split (phase-301: the deprecated
// blocking `call_raw` slot was deleted from the vtable; this pair is the
// one request/reply path).
/* Issue 0780 — polled status events. */
rmw_ret_t subscription_take_event(const rmw_subscription_t *subscription,
                                       rmw_event_type_t kind, rmw_event_payload_t *out,
                                       bool *taken);
rmw_ret_t publisher_take_event(const rmw_publisher_t *publisher, rmw_event_type_t kind,
                                    rmw_event_payload_t *out, bool *taken);

rmw_ret_t service_send_request_raw(const rmw_client_t *client,
                                        rmw_byte_span_t request_span,
                                        int64_t *sequence_id);
rmw_ret_t service_take_response(const rmw_client_t *client, rmw_mut_byte_span_t *reply,
                                     int64_t *seq_out, bool *taken);

} // namespace nros_rmw_cyclonedds

#endif // NROS_RMW_CYCLONEDDS_INTERNAL_HPP

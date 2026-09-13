/**
 * @file service.h
 * @ingroup grp_service
 * @brief Service server API.
 *
 * Create service servers with nros_service_init().  For executor-driven
 * dispatch — the usual shape — register a `nros_service_callback_t` at init
 * time and let the executor deliver requests and send responses for you.
 *
 * For manual polling, create the server with nros_service_init_polling(),
 * take requests with nros_service_take_request_raw(), and send responses
 * with nros_service_send_response_raw().
 *
 * For a TYPED handler — a deserialised request in, a typed response out,
 * with no hand-written CDR — the generated per-service header emits
 * `<Srv>_service_handler_t`, `<Srv>_service_handler_init()` and
 * `<Srv>_service_init()`; see `packs/c/service.h.jinja` (phase-417 W5.e).
 * Those are `static inline` glue over the same entry points documented
 * here, so the two paths cannot diverge.
 *
 * (The `c:take_request` question in the phase-379 parity ledger is
 * ANSWERED: `nros_service_take_request()` is now a deprecated forwarder
 * onto `nros_service_take_request_raw()` — see below.)
 */

#ifndef NROS_SERVICE_H
#define NROS_SERVICE_H

/* Type and function definitions live in <nros/nros_generated.h>.
 * This per-module header is kept as a thin shim so existing code that
 * does `#include <nros/service.h>` continues to compile. */
#include "nros/types.h"

/**
 * @brief The QoS rclc's service/client `_best_effort` presets actually use.
 *
 * phase-417 W5.d. Read from rclc's own source (`rclc/src/rclc/service.c` and
 * `client.c` @ `10eadcc`): rclc does NOT reach for a named upstream profile
 * here the way the pub/sub presets reach for `rmw_qos_profile_sensor_data`. It
 * COPIES `rmw_qos_profile_services_default` and flips one field —
 *
 *     rmw_qos_profile_t p = rmw_qos_profile_services_default;
 *     p.reliability = RMW_QOS_POLICY_RELIABILITY_BEST_EFFORT;
 *
 * — so the depth stays 10 rather than dropping to the sensor profile's 5. Two
 * call sites need that value, so it is computed in ONE place rather than
 * written out twice (issue 0160's hand-mirror class, at header scale).
 *
 * ::NROS_QOS_SERVICES is our mirror of `rmw_qos_profile_services_default`, so
 * the copy starts there. The reliable sibling `rclc_service_init_default`
 * reaches its profile by passing a NULL `qos`, which resolves to
 * ::NROS_QOS_DEFAULT — field-identical to ::NROS_QOS_SERVICES today (both
 * RELIABLE / VOLATILE / KEEP_LAST(10)), so the pair differs only in
 * reliability, exactly as rclc's pair does.
 *
 * Returns by value: a `nros_qos_t` is a plain scalar struct, and the caller
 * needs a mutable copy to take the address of. Nothing here is static, so the
 * header adds no writable data.
 */
static inline struct nros_qos_t nros_qos_services_best_effort(void) {
    struct nros_qos_t qos = NROS_QOS_SERVICES;
    qos.reliability = NROS_QOS_RELIABILITY_BEST_EFFORT;
    return qos;
}

/**
 * @brief rclc's best-effort preset constructor for a service server.
 *
 * phase-417 W5.d; the service-side sibling of
 * rclc_publisher_init_best_effort(), and the same argument. Ledger row
 * `c:service_init_best_effort` filed a `divergence` on "ours takes a QoS
 * value" — true, and not a reason for the name rclc ships to be missing.
 *
 * Takes no callback, at rclc's arity: the handler is supplied at registration
 * by nros_executor_add_service_raw(), where rclc supplies it
 * (`rclc_executor_add_service`). Same split `rclc_service_init_default`
 * already makes.
 *
 * `static inline`: no symbol, no writable data.
 *
 * @param[out] service      Zero-initialised service to fill in.
 * @param[in]  node         An initialised node.
 * @param[in]  type_info    Generated service type descriptor.
 * @param[in]  service_name Service name, null-terminated.
 * @return Whatever nros_service_init_with_qos() returns.
 */
static inline nros_ret_t rclc_service_init_best_effort(struct nros_service_t* service,
                                                       const struct nros_node_t* node,
                                                       const struct nros_service_type_t* type_info,
                                                       const char* service_name) {
    struct nros_qos_t qos = nros_qos_services_best_effort();
    return nros_service_init_with_qos(service, node, type_info, service_name, NULL, NULL, &qos);
}

#endif /* NROS_SERVICE_H */

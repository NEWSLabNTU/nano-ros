/**
 * @file service.h
 * @ingroup grp_service
 * @brief Service server API.
 *
 * Create a service server with rclc_service_init_default() (or
 * rclc_service_init_with_qos()) and add it to an executor with
 * rclc_executor_add_service_raw(), which takes the `rclc_service_raw_callback_t`.
 * The executor arena owns the server and the callback, delivers requests and
 * sends responses for you.
 *
 * For manual polling with no executor, use the separate type
 * ::rclc_polling_service_t: create it with rclc_polling_service_init(), take
 * requests with rclc_polling_service_take_request_raw(), and send responses
 * with rclc_polling_service_send_response_raw(). The two types are separate
 * since phase-487: a dispatched server's storage is in the executor arena, a
 * polled one's is in the struct.
 *
 * For a TYPED handler — a deserialised request in, a typed response out,
 * with no hand-written CDR — the generated per-service header emits
 * `<Srv>_service_handler_t`, `<Srv>_service_handler_init()`,
 * `<Srv>_service_init()` and `<Srv>_executor_add_service()`; see
 * `packs/c/service.h.jinja` (phase-417 W5.e). Those are `static inline` glue
 * over the same entry points documented here, so the two paths cannot
 * diverge.
 */

#ifndef NROS_SERVICE_H
#define NROS_SERVICE_H

/* Type and function definitions live in <nros/nros_generated.h>.
 * This per-module header is kept as a thin shim so existing code that
 * does `#include <rclc/service.h>` continues to compile. */
#include "rclc/types.h"

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
 * ::RCLC_QOS_SERVICES is our mirror of `rmw_qos_profile_services_default`, so
 * the copy starts there. The reliable sibling `rclc_service_init_default`
 * reaches its profile by passing a NULL `qos`, which resolves to
 * ::RCLC_QOS_DEFAULT — field-identical to ::RCLC_QOS_SERVICES today (both
 * RELIABLE / VOLATILE / KEEP_LAST(10)), so the pair differs only in
 * reliability, exactly as rclc's pair does.
 *
 * Returns by value: a `rclc_qos_t` is a plain scalar struct, and the caller
 * needs a mutable copy to take the address of. Nothing here is static, so the
 * header adds no writable data.
 */
static inline struct rclc_qos_t rclc_qos_services_best_effort(void) {
    struct rclc_qos_t qos = RCLC_QOS_SERVICES;
    qos.reliability = RCLC_QOS_RELIABILITY_BEST_EFFORT;
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
 * by rclc_executor_add_service_raw(), where rclc supplies it
 * (`rclc_executor_add_service`). Same split `rclc_service_init_default`
 * already makes.
 *
 * `static inline`: no symbol, no writable data.
 *
 * @param[out] service      Zero-initialised service to fill in.
 * @param[in]  node         An initialised node.
 * @param[in]  type_info    Generated service type descriptor.
 * @param[in]  service_name Service name, null-terminated.
 * @return Whatever rclc_service_init_with_qos() returns.
 */
static inline rcl_ret_t rclc_service_init_best_effort(struct rcl_service_t* service,
                                                      const struct rcl_node_t* node,
                                                      const struct rclc_service_type_t* type_info,
                                                      const char* service_name) {
    struct rclc_qos_t qos = rclc_qos_services_best_effort();
    return rclc_service_init_with_qos(service, node, type_info, service_name, &qos);
}

#endif /* NROS_SERVICE_H */

/* Every upstream-path header carries the whole C API, as `<rclc/rclc.h>` does
 * (phase-492): a ported file that includes only `<rclc/service.h>` still finds
 * `RCLC_CHECK_RET`, `RCLC_LOG_*` and the rcl spellings. Outside the guard, so
 * this header's own declarations are complete before the umbrella reads them. */
#include "rclc/rclc.h"

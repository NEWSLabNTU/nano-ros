/**
 * @file subscription.h
 * @ingroup grp_pubsub
 * @brief Topic subscription API.
 *
 * Create subscriptions with nros_subscription_init() and receive
 * deserialised messages via a user-provided callback.
 *
 * For manual polling, create the subscription with
 * nros_subscription_init_polling() and drain it with
 * nros_subscription_take_serialized() — or
 * nros_subscription_take_sequence() for a batch and
 * nros_subscription_take_validated() for the E2E-safety variant.
 */

#ifndef NROS_SUBSCRIPTION_H
#define NROS_SUBSCRIPTION_H

/* Type and function definitions live in <nros/nros_generated.h>.
 * This per-module header is kept as a thin shim so existing code that
 * does `#include <nros/subscription.h>` continues to compile. */
#include "nros/types.h"
/* phase-454 W10 — the DECLARED QoS depth of this component's subscriptions,
 * and `NROS_ASSERT_DECLARED_DEPTH` over it. Included HERE, from the subscribe
 * surface, so a C call site that spells a depth reaches the check without
 * naming a second header — and so `<nros/nros.h>` carries it, which is what
 * `just check c`'s umbrella syntax check compiles. Costs nothing when no
 * contract declared anything: the table is then absent and every assertion is
 * a comparison of a number with itself. */
#include "nros/declared_qos.h"

/**
 * @brief rclc's best-effort preset constructor for a subscription.
 *
 * phase-417 W5.d; the sibling of rclc_publisher_init_best_effort() and the
 * same argument. rclc's body (`rclc/src/rclc/subscription.c` @ `10eadcc`) is
 * `rclc_subscription_init(..., &rmw_qos_profile_sensor_data)` — best-effort,
 * volatile, KEEP_LAST(5) — which ::NROS_QOS_SENSOR_DATA mirrors.
 *
 * Takes no callback, at rclc's arity: the callback is supplied at registration
 * by nros_executor_add_subscription_raw(), exactly where rclc supplies it
 * (`rclc_executor_add_subscription`). That is the same split
 * `rclc_subscription_init_default` already makes, so the two presets differ
 * only in the profile they pass.
 *
 * `static inline`: no symbol, no writable data.
 *
 * @param[out] subscription Zero-initialised subscription to fill in.
 * @param[in]  node         An initialised node.
 * @param[in]  type_info    Generated message type descriptor.
 * @param[in]  topic_name   Topic name, null-terminated.
 * @return Whatever nros_subscription_init_with_qos() returns.
 */
static inline nros_ret_t rclc_subscription_init_best_effort(
    struct nros_subscription_t* subscription, const struct nros_node_t* node,
    const struct nros_message_type_t* type_info, const char* topic_name) {
    return nros_subscription_init_with_qos(subscription, node, type_info, topic_name, NULL, NULL,
                                           &NROS_QOS_SENSOR_DATA);
}

#endif /* NROS_SUBSCRIPTION_H */

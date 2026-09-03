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

#ifdef __cplusplus
extern "C" {
#endif

/* ===================================================================
 * DEPRECATED spellings — phase-379 W6 decision 1 (2026-09-03)
 *
 * `try_recv` -> `take`. rcl (`rcl_take`), rclcpp
 * (`Subscription::take`) and our OWN RMW vtable (`take`, `take_request`,
 * `take_response`, `take_sequence`) all spell the non-blocking consuming
 * receive that way; only the user-facing layer said `try_recv`, which is
 * Rust-channel vocabulary that reads as a different contract to a ROS 2
 * user. Both forms are non-blocking and both report emptiness without
 * failing, so no platform constraint asked for the other word.
 *
 * `_raw` -> `_serialized` for the pre-CDR byte form, because that is
 * ROS 2's word for it (`rcl_take_serialized_message`).
 *
 * The forwarders are `NROS_DEPRECATED_MSG` `static inline`, the shape the
 * `nros_param_*` family established in `nros/parameter.h`: an inline
 * definition in a header has no external linkage, so every translation
 * unit may define it and none of them export it. The `take_*` name stays
 * the ONLY exported symbol — this is a SOURCE compatibility promise, not
 * a binary one, and an object file built against the pre-rename library
 * must be recompiled.
 *
 * Define NROS_NO_DEPRECATED_SUBSCRIPTION_ALIASES to compile without any
 * of it — for a consumer whose build is `-Werror` and who wants the old
 * names to be a hard error rather than a warning.
 *
 * These are scheduled for removal; migrate.
 * =================================================================== */

#ifndef NROS_NO_DEPRECATED_SUBSCRIPTION_ALIASES

NROS_DEPRECATED_MSG("nros_subscription_try_recv_raw() is deprecated; use "
                    "nros_subscription_take_serialized()")
static inline int32_t nros_subscription_try_recv_raw(struct nros_subscription_t* subscription,
                                                     uint8_t* buf, size_t buf_len) {
    return nros_subscription_take_serialized(subscription, buf, buf_len);
}

NROS_DEPRECATED_MSG("nros_subscription_try_recv_sequence() is deprecated; use "
                    "nros_subscription_take_sequence()")
static inline int32_t nros_subscription_try_recv_sequence(struct nros_subscription_t* subscription,
                                                          uint8_t* buf, size_t per_msg_cap,
                                                          size_t max_msgs, size_t* out_lens) {
    return nros_subscription_take_sequence(subscription, buf, per_msg_cap, max_msgs, out_lens);
}

NROS_DEPRECATED_MSG("nros_subscription_try_recv_validated() is deprecated; use "
                    "nros_subscription_take_validated()")
static inline int32_t
nros_subscription_try_recv_validated(struct nros_subscription_t* subscription, uint8_t* buf,
                                     size_t buf_len, struct nros_integrity_status_t* out_status) {
    return nros_subscription_take_validated(subscription, buf, buf_len, out_status);
}

#endif /* NROS_NO_DEPRECATED_SUBSCRIPTION_ALIASES */

#ifdef __cplusplus
}
#endif

#endif /* NROS_SUBSCRIPTION_H */

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

#endif /* NROS_SUBSCRIPTION_H */

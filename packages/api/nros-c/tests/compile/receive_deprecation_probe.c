/*
 * Phase 379 W6 decision 1 — the NEGATIVE half of `receive_verb_aliases.c`.
 *
 * That file proves the old `try_recv*` spellings still COMPILE. This one proves
 * they still WARN. A deprecation nobody is told about is just an alias, and the
 * reason this family used NROS_DEPRECATED_MSG forwarders was to get the
 * diagnostic — so "the attribute reaches callers" is the thing worth pinning,
 * not an implementation detail.
 *
 * `just check c` compiles this with `-Werror=deprecated-declarations` and
 * requires it to FAIL. It is a normal, valid TU otherwise; only that flag turns
 * the warning into an error. Written as an expected failure because a clean
 * compile is exactly what a silently-dropped attribute looks like.
 */

#include "nros/subscription.h"

int32_t nros_receive_deprecation_probe(struct nros_subscription_t* subscription, uint8_t* buf,
                                       size_t buf_len);
int32_t nros_receive_deprecation_probe(struct nros_subscription_t* subscription, uint8_t* buf,
                                       size_t buf_len) {
    return nros_subscription_try_recv_raw(subscription, buf, buf_len);
}

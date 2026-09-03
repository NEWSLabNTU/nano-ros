/*
 * Phase 379 W6 decision 1 — the non-blocking receive verb was renamed
 * `try_recv` -> `take` across the C surface (ledger rows `c:take`,
 * `c:take_serialized_message`, `c:take_request`, `c:take_response`), with
 * `_raw` -> `_serialized` for the pre-CDR byte form because that is ROS 2's
 * word for it. This TU pins BOTH halves, the way `param_name_aliases.c` pins
 * the parameter family's:
 *
 *   1. every entry point exists under the NEW spelling, with the signature the
 *      header documents, and
 *   2. every OLD spelling still compiles, with that same signature, so a
 *      consumer gets a release to migrate.
 *
 * Compile-only (no main): the assertion is that these names resolve with these
 * types. Taking a function POINTER forces a real lookup and a real signature
 * match — a forwarder whose argument list drifted from the function it forwards
 * to fails HERE rather than at some consumer's call site.
 *
 * The old half is `static inline` forwarders carrying NROS_DEPRECATED_MSG, so
 * naming them is supposed to warn. That warning is asserted separately by
 * `receive_deprecation_probe.c`, an expected-failure compile under
 * `-Werror=deprecated-declarations`. Here it would just be noise on a passing
 * gate, so it is suppressed for that section only.
 */

#include "nros/client.h"
#include "nros/service.h"
#include "nros/subscription.h"

/* 1. The renamed entry points exist, with the documented signatures. */
static int32_t (*const k_new_sub_take_serialized)(struct nros_subscription_t*, uint8_t*,
                                                  size_t) = nros_subscription_take_serialized;
static int32_t (*const k_new_sub_take_sequence)(struct nros_subscription_t*, uint8_t*, size_t,
                                                size_t, size_t*) = nros_subscription_take_sequence;
static int32_t (*const k_new_sub_take_validated)(struct nros_subscription_t*, uint8_t*, size_t,
                                                 struct nros_integrity_status_t*) =
    nros_subscription_take_validated;
static int32_t (*const k_new_srv_take_request_raw)(struct nros_service_t*, uint8_t*, size_t,
                                                   int64_t*) = nros_service_take_request_raw;
static int32_t (*const k_new_cli_take_response_raw)(struct nros_client_t*, uint8_t*,
                                                    size_t) = nros_client_take_response_raw;
static nros_ret_t (*const k_new_cli_take_response)(struct nros_client_t*, uint8_t*, size_t,
                                                   size_t*) = nros_client_take_response;

/* 2. The deprecated spellings still resolve, with the same signatures. */
#if defined(__GNUC__) || defined(__clang__)
#pragma GCC diagnostic push
#pragma GCC diagnostic ignored "-Wdeprecated-declarations"
#endif
static int32_t (*const k_old_sub_try_recv_raw)(struct nros_subscription_t*, uint8_t*,
                                               size_t) = nros_subscription_try_recv_raw;
static int32_t (*const k_old_sub_try_recv_sequence)(struct nros_subscription_t*, uint8_t*, size_t,
                                                    size_t,
                                                    size_t*) = nros_subscription_try_recv_sequence;
static int32_t (*const k_old_sub_try_recv_validated)(struct nros_subscription_t*, uint8_t*, size_t,
                                                     struct nros_integrity_status_t*) =
    nros_subscription_try_recv_validated;
static int32_t (*const k_old_srv_try_recv_request_raw)(
    struct nros_service_t*, uint8_t*, size_t, int64_t*) = nros_service_try_recv_request_raw;
static int32_t (*const k_old_cli_try_recv_reply_raw)(struct nros_client_t*, uint8_t*,
                                                     size_t) = nros_client_try_recv_reply_raw;
static nros_ret_t (*const k_old_cli_try_recv_response)(struct nros_client_t*, uint8_t*, size_t,
                                                       size_t*) = nros_client_try_recv_response;
#if defined(__GNUC__) || defined(__clang__)
#pragma GCC diagnostic pop
#endif

/* Silence "defined but not used" without needing a main(). */
const void* nros_receive_verb_alias_probe(void);
const void* nros_receive_verb_alias_probe(void) {
    (void)k_new_sub_take_serialized;
    (void)k_old_sub_try_recv_raw;
    (void)k_new_sub_take_sequence;
    (void)k_old_sub_try_recv_sequence;
    (void)k_new_sub_take_validated;
    (void)k_old_sub_try_recv_validated;
    (void)k_new_srv_take_request_raw;
    (void)k_old_srv_try_recv_request_raw;
    (void)k_new_cli_take_response_raw;
    (void)k_old_cli_try_recv_reply_raw;
    (void)k_new_cli_take_response;
    (void)k_old_cli_try_recv_response;
    return (const void*)k_new_sub_take_serialized;
}

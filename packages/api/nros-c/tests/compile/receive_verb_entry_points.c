/*
 * The non-blocking receive family's entry points, pinned by SIGNATURE.
 *
 * Phase 379 W6 decision 1 renamed the verb `try_recv` -> `take` across the C
 * surface (ledger rows `c:take`, `c:take_serialized_message`,
 * `c:take_request`, `c:take_response`), with `_raw` -> `_serialized` for the
 * pre-CDR byte form because that is ROS 2's word for it, and kept the old
 * spellings one release as `NROS_DEPRECATED_MSG` `static inline` forwarders.
 * Phase-417 stage 6 step B retired those, so what this TU pins is the live
 * half alone.
 *
 * Note which direction the rename moved: the RMW vtable one layer down has
 * said `take` / `take_request` / `take_response` / `take_sequence` since
 * phase-376 W3.b, so the USER API was the only layer still spelling it the
 * Rust-channel way.
 *
 * Compile-only (no main): taking a function POINTER forces a real lookup and a
 * real signature match, so a declaration whose argument list drifts from the
 * Rust definition cbindgen emitted fails HERE rather than at some consumer's
 * call site.
 */

#include "rclc/client.h"
#include "rclc/service.h"
#include "rclc/subscription.h"

/* The live entry points, with the documented signatures. The subscription
 * drains and the `_raw` service/client drains take the POLLED types: since
 * phase-487 a polled entity is `nros_polling_<kind>_t`, and the dispatched
 * struct has no entity of its own to take from. */
static int32_t (*const k_new_sub_take_serialized)(struct rclc_polling_subscription_t*, uint8_t*,
                                                  size_t) =
    rclc_polling_subscription_take_serialized;
static int32_t (*const k_new_sub_take_sequence)(struct rclc_polling_subscription_t*, uint8_t*,
                                                size_t, size_t,
                                                size_t*) = rclc_polling_subscription_take_sequence;
static int32_t (*const k_new_sub_take_validated)(struct rclc_polling_subscription_t*, uint8_t*,
                                                 size_t, struct rclc_integrity_status_t*) =
    rclc_polling_subscription_take_validated;
static int32_t (*const k_new_srv_take_request_raw)(struct rclc_polling_service_t*, uint8_t*, size_t,
                                                   int64_t*) =
    rclc_polling_service_take_request_raw;
static int32_t (*const k_new_cli_take_response_raw)(struct rclc_polling_client_t*, uint8_t*,
                                                    size_t) = rclc_polling_client_take_response_raw;
static rcl_ret_t (*const k_new_cli_take_response)(struct rcl_client_t*, uint8_t*, size_t,
                                                  size_t*) = rclc_client_take_response;

/* Silence "defined but not used" without needing a main(). */
const void* nros_receive_verb_entry_point_probe(void);
const void* nros_receive_verb_entry_point_probe(void) {
    (void)k_new_sub_take_serialized;
    (void)k_new_sub_take_sequence;
    (void)k_new_sub_take_validated;
    (void)k_new_srv_take_request_raw;
    (void)k_new_cli_take_response_raw;
    (void)k_new_cli_take_response;
    return (const void*)k_new_sub_take_serialized;
}

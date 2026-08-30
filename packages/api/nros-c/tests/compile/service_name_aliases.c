/*
 * Phase 379 W5 — the service reply verb was renamed `send_reply` ->
 * `send_response` (rcl, rclcpp and rclrs all say `send_response`). This TU pins
 * both halves of that change, the way `param_name_aliases.c` pins the
 * `nros_param_*` -> `nros_parameter_*` rename:
 *
 *   1. the live entry point exists under the NEW spelling with the signature
 *      the header documents, and
 *   2. both OLD spellings still compile, with that same signature, so a
 *      consumer gets a release to migrate.
 *
 * Compile-only (no main): the assertion is that these names resolve with these
 * types. Taking a function POINTER forces a real lookup and a real signature
 * match — a forwarder whose argument list drifted from the function it forwards
 * to fails HERE rather than at some consumer's call site.
 *
 * The old half is `static inline` forwarders carrying NROS_DEPRECATED_MSG, so
 * naming them is supposed to warn. That warning is the point and is asserted
 * separately, by `service_deprecation_probe.c` under
 * `-Werror=deprecated-declarations`. Here it would be noise on a passing gate,
 * so it is suppressed for that section only.
 *
 * Reconstructed 2026-08-27: commit 23dcdafdc added the `just check c` lane that
 * compiles this file and the probe beside it, but neither file was committed —
 * so the lane referenced a path that did not exist and `check-c` failed with
 * "No such file or directory" for everyone. Content follows `nros/service.h`'s
 * own documented contract.
 */

#include "nros/service.h"

/* 1. The live entry point, under the name the ROS client libraries use. */
static nros_ret_t (*const k_send_response_raw)(struct nros_service_t*, int64_t, const uint8_t*,
                                               size_t) = nros_service_send_response_raw;

/* 2. Both deprecated spellings still resolve, with the same signature.
 *
 * `nros_service_send_response` is the odd one: it was never an alias but a
 * permanent NROS_RET_NOT_INIT stub from before the polling API, kept as a
 * forwarder so the un-suffixed name cannot read like a second, working entry
 * point. Pinned here for exactly that reason. */
#ifndef NROS_NO_DEPRECATED_SERVICE_ALIASES
#pragma GCC diagnostic push
#pragma GCC diagnostic ignored "-Wdeprecated-declarations"

static nros_ret_t (*const k_send_reply_raw)(struct nros_service_t*, int64_t, const uint8_t*,
                                            size_t) = nros_service_send_reply_raw;
static nros_ret_t (*const k_send_response)(struct nros_service_t*, int64_t, const uint8_t*,
                                           size_t) = nros_service_send_response;

#pragma GCC diagnostic pop
#endif /* NROS_NO_DEPRECATED_SERVICE_ALIASES */

/* Reference them so no compiler prunes the lookups this file exists to force. */
const void* nros_service_name_alias_anchors[] = {
    (const void*) &k_send_response_raw,
#ifndef NROS_NO_DEPRECATED_SERVICE_ALIASES
    (const void*) &k_send_reply_raw,
    (const void*) &k_send_response,
#endif
};

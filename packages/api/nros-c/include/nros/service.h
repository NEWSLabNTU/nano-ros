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
 * (nros_service_take_request() is the unimplemented twin of
 * nros_service_take_request_raw() and returns `NROS_RET_NOT_INIT`; which
 * of the two spellings survives is the `c:take_request` question in the
 * phase-379 parity ledger.)
 */

#ifndef NROS_SERVICE_H
#define NROS_SERVICE_H

/* Type and function definitions live in <nros/nros_generated.h>.
 * This per-module header is kept as a thin shim so existing code that
 * does `#include <nros/service.h>` continues to compile. */
#include "nros/types.h"

#ifdef __cplusplus
extern "C" {
#endif

/* ===================================================================
 * DEPRECATED spellings — phase-379 W5 (2026-08-27)
 *
 * `send_reply` -> `send_response`. rcl, rclcpp and rclrs all say
 * `send_response`; our C said BOTH, with `nros_service_send_response` sitting
 * beside `nros_service_send_reply_raw`. The live spelling is
 * `nros_service_send_response_raw()` — `_raw` because that is the family
 * convention for the byte-buffer entry points (`nros_publish_raw`,
 * `nros_client_send_request_raw`), not because a non-raw twin exists.
 *
 * Both old spellings survive here as `NROS_DEPRECATED_MSG` `static inline`
 * forwarders, the shape the `nros_param_*` family established in
 * `nros/parameter.h`: an inline definition in a header has no external
 * linkage, so every translation unit may define it and none of them export
 * it. That makes `nros_service_send_response_raw` the only exported symbol
 * and these a SOURCE compatibility promise, not a binary one — an object file
 * built against the pre-rename library must be recompiled.
 *
 * `nros_service_send_response()` is the odd one: it was never an alias, it
 * was a permanent `NROS_RET_NOT_INIT` stub from before the polling API
 * existed, so no caller can ever have had it succeed. Forwarding it is
 * therefore strictly an improvement on what it did — and it keeps the
 * un-suffixed name from reading like a second, working entry point.
 *
 * Define NROS_NO_DEPRECATED_SERVICE_ALIASES to compile without any of it —
 * for a consumer whose build is `-Werror` and who wants the old names to be a
 * hard error rather than a warning.
 *
 * These are scheduled for removal; migrate.
 * =================================================================== */

#ifndef NROS_NO_DEPRECATED_SERVICE_ALIASES

NROS_DEPRECATED_MSG("nros_service_send_reply_raw() is deprecated; use "
                    "nros_service_send_response_raw()")
static inline nros_ret_t nros_service_send_reply_raw(struct nros_service_t* service,
                                                     int64_t sequence_number, const uint8_t* data,
                                                     size_t len) {
    return nros_service_send_response_raw(service, sequence_number, data, len);
}

/* phase-379 W6 decision 1 (2026-09-03): `try_recv` -> `take`. rcl
 * (`rcl_take_request`), rclcpp (`Service::take_request`) and our own RMW
 * vtable (`take_request`) all spell the non-blocking consuming receive that
 * way; only the user-facing layer said `try_recv`. Same `static inline`
 * shape, same source-not-binary promise -- see `nros/subscription.h`. */

NROS_DEPRECATED_MSG("nros_service_try_recv_request_raw() is deprecated; use "
                    "nros_service_take_request_raw()")
static inline int32_t nros_service_try_recv_request_raw(struct nros_service_t* service,
                                                        uint8_t* buf, size_t buf_len,
                                                        int64_t* sequence_number) {
    return nros_service_take_request_raw(service, buf, buf_len, sequence_number);
}

NROS_DEPRECATED_MSG("nros_service_send_response() is deprecated; use "
                    "nros_service_send_response_raw()")
static inline nros_ret_t nros_service_send_response(struct nros_service_t* service,
                                                    int64_t sequence_number,
                                                    const uint8_t* response_data,
                                                    size_t response_len) {
    return nros_service_send_response_raw(service, sequence_number, response_data, response_len);
}

#endif /* NROS_NO_DEPRECATED_SERVICE_ALIASES */

#ifdef __cplusplus
}
#endif

#endif /* NROS_SERVICE_H */

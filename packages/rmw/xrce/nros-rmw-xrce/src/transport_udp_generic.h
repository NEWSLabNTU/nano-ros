/* transport_udp_generic.h — shared XRCE custom-UDP trampolines.
 *
 * Phase 214.I.1 — both `transport_posix_udp.c` (host POSIX) and
 * `transport_nros_udp.c` (nros-platform-net shim) drove
 * `uxrCustomTransport` via the same four-trampoline shape
 * (open / close / write / read) bracketed by a
 * `uxr_set_custom_transport_callbacks` + `uxr_init_custom_transport`
 * pair. The trampoline bodies necessarily differ (each platform's
 * socket primitive is different — POSIX hits `send` / `poll` / `recv`,
 * NROS hits `nros_platform_udp_*`), but two pieces ARE shared:
 *
 *   1. The `_open()` trampoline is a no-op on both platforms — the
 *      fd / nros-platform handle was opened *before* the trampoline
 *      registration window in each `*_init()` body. Both copies were
 *      4 lines of `(void)t; return true;` boilerplate.
 *
 *   2. The init-time bracketing pair
 *      `uxr_set_custom_transport_callbacks(...) +
 *       uxr_init_custom_transport(...)` is structurally identical;
 *      the platform-specific `*_init()` only varies the open / close /
 *      write / read function pointers passed in + the bridge `args`
 *      pointer.
 *
 * This header hosts the shared `xrce_udp_open_noop` trampoline +
 * the `XRCE_UDP_BIND_AND_INIT` macro. Per-platform .c files keep
 * the close / write / read trampolines (where the socket primitive
 * sits) + the platform-specific endpoint resolution / binding /
 * fd creation. Net shrink: ≥30 LoC across the two consumers
 * combined.
 *
 * No file-scope storage; multi-session safe.
 */

#ifndef NROS_RMW_XRCE_TRANSPORT_UDP_GENERIC_H
#define NROS_RMW_XRCE_TRANSPORT_UDP_GENERIC_H

#include "internal.h"

#include <stdbool.h>

#include <uxr/client/profile/transport/custom/custom_transport.h>

/* No-op open trampoline. Both POSIX + NROS perform the actual socket
 * creation in their `*_init()` body BEFORE registering the custom
 * transport, so there's nothing for `_open` to do — the trampoline
 * exists only because `uxr_set_custom_transport_callbacks` requires
 * a non-NULL `open` pointer. */
static inline bool xrce_udp_open_noop(struct uxrCustomTransport *t) {
    (void)t;
    return true;
}

/* Bracketing helper. After `*_init()` has opened the per-platform
 * socket + stashed bridge state in `st->udp_bridge`, this macro
 * registers the trampoline quartet + initialises the custom
 * transport.
 *
 * `st`         — `xrce_session_state_t *`
 * `close_cb`   — `bool (*)(struct uxrCustomTransport *)`
 * `write_cb`   — `size_t (*)(struct uxrCustomTransport *, const uint8_t *, size_t, uint8_t *)`
 * `read_cb`    — `size_t (*)(struct uxrCustomTransport *, uint8_t *, size_t, int, uint8_t *)`
 * `bridge_arg` — `void *` pointer stored in `transport->args`
 * `on_failure` — statement block executed if `uxr_init_custom_transport`
 *                returns false (per-platform cleanup of the socket /
 *                bridge state opened above); the macro returns
 *                `NROS_RMW_RET_ERROR` afterwards.
 *
 * Framing is hard-coded to `false` — UDP is packet-preserving, so
 * the HDLC-style framing layer isn't needed (matches every existing
 * caller). */
#define XRCE_UDP_BIND_AND_INIT(st, close_cb, write_cb, read_cb,        \
                               bridge_arg, on_failure)                 \
    do {                                                               \
        uxr_set_custom_transport_callbacks(                            \
            &(st)->custom, /*framing=*/false,                          \
            xrce_udp_open_noop,                                        \
            (close_cb),                                                \
            (write_cb),                                                \
            (read_cb));                                                \
        if (!uxr_init_custom_transport(&(st)->custom, (bridge_arg))) { \
            on_failure;                                                \
            return NROS_RMW_RET_ERROR;                                 \
        }                                                              \
    } while (0)

#endif /* NROS_RMW_XRCE_TRANSPORT_UDP_GENERIC_H */

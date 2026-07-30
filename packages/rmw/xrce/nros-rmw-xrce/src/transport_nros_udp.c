/* Phase 129.NET.3 — platform-agnostic XRCE UDP transport.
 *
 * Routes the micro-XRCE custom-transport callbacks through the
 * canonical `nros_platform_udp_*` ABI in <nros/platform_net.h>.
 * Replaces both `transport_posix_udp.c` (libc BSD sockets) and
 * `transport_zephyr_udp.c` (already on the platform ABI but
 * gated by `UCLIENT_PLATFORM_ZEPHYR`). The XRCE C build no
 * longer needs to know what platform it is on; whichever
 * `nros-platform-<rtos>` library satisfies the symbols at link
 * time wins.
 *
 * Bridge state piggy-backs on the existing
 * `xrce_session_state_t.udp_bridge` struct (`fd` + `sock` +
 * `endpoint`). This TU uses only `sock` and `endpoint` — the
 * `fd` slot stays untouched, keeping wire compatibility with
 * any consumer that still pulls in `transport_posix_udp.c`
 * during the transition.
 */

#include "internal.h"

#include "nros/platform_net.h"
#include "nros/rmw_ret.h"
#include "transport_udp_generic.h"

#include <stddef.h>
#include <stdint.h>
#include <stdlib.h>

typedef struct xrce_nros_udp_bridge {
    int    fd_unused;  /* keeps layout 1:1 with `udp_bridge` struct */
    void  *sock;
    void  *endpoint;
} xrce_nros_udp_bridge;

/* `_open` is shared via `xrce_udp_open_noop` in transport_udp_generic.h. */

static bool nros_udp_close(struct uxrCustomTransport *t) {
    if (t == NULL) return true;
    xrce_nros_udp_bridge *b = (xrce_nros_udp_bridge *)t->args;
    if (b == NULL) return true;
    if (b->sock != NULL) {
        nros_platform_udp_close(b->sock);
        free(b->sock);
        b->sock = NULL;
    }
    if (b->endpoint != NULL) {
        nros_platform_udp_free_endpoint(b->endpoint);
        free(b->endpoint);
        b->endpoint = NULL;
    }
    return true;
}

static size_t nros_udp_write(struct uxrCustomTransport *t,
                             const uint8_t *buf, size_t len,
                             uint8_t *err) {
    (void)err;
    if (t == NULL) return 0;
    xrce_nros_udp_bridge *b = (xrce_nros_udp_bridge *)t->args;
    if (b == NULL || b->sock == NULL || b->endpoint == NULL) return 0;
    size_t n = nros_platform_udp_send(b->sock, buf, len, b->endpoint);
    return n == NROS_PLATFORM_NET_SOCKET_ERROR ? 0u : n;
}

static size_t nros_udp_read(struct uxrCustomTransport *t,
                            uint8_t *buf, size_t len,
                            int timeout, uint8_t *err) {
    (void)err;
    if (t == NULL) return 0;
    xrce_nros_udp_bridge *b = (xrce_nros_udp_bridge *)t->args;
    if (b == NULL || b->sock == NULL) return 0;
    nros_platform_udp_set_recv_timeout(b->sock,
                                       timeout < 0 ? 0u : (uint32_t)timeout);
    size_t n = nros_platform_udp_read(b->sock, buf, len);
    return n == NROS_PLATFORM_NET_SOCKET_ERROR ? 0u : n;
}

/* Per-platform socket / endpoint opaque storage sizes. The XRCE
 * TU cannot include the platform-internal header, so it sizes the
 * boxed allocation generously enough to cover every supported
 * provider (POSIX `int _fd` ≤ 16, smoltcp handle + extras ≤ 32,
 * lwIP / NetX similar). 64 B both is a safe upper bound. */
#define XRCE_NROS_SOCK_STORAGE_BYTES     64
#define XRCE_NROS_ENDPOINT_STORAGE_BYTES 64

nros_rmw_ret_t xrce_nros_udp_init(xrce_session_state_t *st,
                                  const char *host, const char *port) {
    if (st == NULL || host == NULL || port == NULL) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }

    xrce_nros_udp_bridge *bridge = (xrce_nros_udp_bridge *)&st->udp_bridge;
    bridge->sock = calloc(1, XRCE_NROS_SOCK_STORAGE_BYTES);
    bridge->endpoint = calloc(1, XRCE_NROS_ENDPOINT_STORAGE_BYTES);
    if (bridge->sock == NULL || bridge->endpoint == NULL) {
        free(bridge->sock);
        free(bridge->endpoint);
        bridge->sock = NULL;
        bridge->endpoint = NULL;
        return NROS_RMW_RET_ERROR;
    }

    if (nros_platform_udp_create_endpoint(bridge->endpoint,
                                          (const uint8_t *)host,
                                          (const uint8_t *)port) != 0) {
        free(bridge->sock);
        free(bridge->endpoint);
        bridge->sock = NULL;
        bridge->endpoint = NULL;
        return NROS_RMW_RET_ERROR;
    }
    if (nros_platform_udp_open(bridge->sock, bridge->endpoint, 100) != 0) {
        nros_platform_udp_free_endpoint(bridge->endpoint);
        free(bridge->sock);
        free(bridge->endpoint);
        bridge->sock = NULL;
        bridge->endpoint = NULL;
        return NROS_RMW_RET_ERROR;
    }

    /* Register trampoline quartet + init custom transport via the
     * Phase 214.I.1 shared bracketing macro. UDP is packet-
     * preserving; framing stays off. */
    XRCE_UDP_BIND_AND_INIT(
        st, nros_udp_close, nros_udp_write, nros_udp_read,
        bridge, {
            nros_platform_udp_close(bridge->sock);
            nros_platform_udp_free_endpoint(bridge->endpoint);
            free(bridge->sock);
            free(bridge->endpoint);
            bridge->sock = NULL;
            bridge->endpoint = NULL;
        });
    return NROS_RMW_RET_OK;
}

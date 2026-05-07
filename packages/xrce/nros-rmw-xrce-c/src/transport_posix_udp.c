/* Phase 115.K.2.5.1.2.a-fix-transport — POSIX UDP via custom-transport.
 *
 * Replaces the `uxr_init_udp_transport` path that K.2.1 used. The
 * upstream UDP transport's poll-based recv has a different timing
 * profile than the agent expects; routing UDP through
 * `uxr_set_custom_transport_callbacks` + `uxr_init_custom_transport`
 * (matching `xrce-sys`'s legacy shape) makes the participant-create
 * confirmation reliable.
 *
 * Per-session state lives in `xrce_session_state_t` — an `int fd`
 * + the four trampolines. No file-scope storage; multi-session
 * safe. Trampolines reach the per-session fd via the
 * `uxrCustomTransport.args` pointer that
 * `uxr_set_custom_transport_callbacks` carries through.
 *
 * NOTE: as of this commit `uxrCustomTransport` does not expose an
 * `args` field that we control. The trampolines fall back on the
 * `t->args` pointer set by upstream; until that lands as a public
 * field we keep the fd in a per-session bridge struct that stores
 * a back-pointer to the custom transport. The current upstream
 * shape uses `uxr_set_custom_transport_callbacks(transport, framing,
 * ...)` which leaves `transport->args` to the caller.
 */

#include "internal.h"

#include "nros/rmw_ret.h"

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include <string.h>
#include <stdlib.h>

#include <sys/socket.h>
#include <sys/types.h>
#include <netinet/in.h>
#include <arpa/inet.h>
#include <netdb.h>
#include <unistd.h>
#include <poll.h>
#include <fcntl.h>
#include <errno.h>

#include <uxr/client/profile/transport/custom/custom_transport.h>

/* Bridge struct hung off `uxrCustomTransport.args`. Carries the
 * connected UDP fd. */
typedef struct xrce_posix_udp_bridge {
    int fd;
} xrce_posix_udp_bridge;

/* ---- Trampolines (registered with uxr) -------------------------- */

static bool posix_udp_open(struct uxrCustomTransport *t) {
    /* Open is a no-op — the fd was created in
     * `xrce_posix_udp_init` before
     * `uxr_set_custom_transport_callbacks`. */
    (void)t;
    return true;
}

static bool posix_udp_close(struct uxrCustomTransport *t) {
    if (t == NULL) return true;
    xrce_posix_udp_bridge *b = (xrce_posix_udp_bridge *)t->args;
    if (b == NULL) return true;
    if (b->fd >= 0) {
        close(b->fd);
        b->fd = -1;
    }
    return true;
}

static size_t posix_udp_write(struct uxrCustomTransport *t,
                              const uint8_t *buf, size_t len,
                              uint8_t *err) {
    (void)err;
    if (t == NULL) return 0;
    xrce_posix_udp_bridge *b = (xrce_posix_udp_bridge *)t->args;
    if (b == NULL || b->fd < 0) return 0;
    ssize_t n = send(b->fd, buf, len, 0);
    return n < 0 ? 0u : (size_t)n;
}

static size_t posix_udp_read(struct uxrCustomTransport *t,
                             uint8_t *buf, size_t len,
                             int timeout, uint8_t *err) {
    (void)err;
    if (t == NULL) return 0;
    xrce_posix_udp_bridge *b = (xrce_posix_udp_bridge *)t->args;
    if (b == NULL || b->fd < 0) return 0;

    struct pollfd pfd;
    pfd.fd = b->fd;
    pfd.events = POLLIN;
    int rc = poll(&pfd, 1, timeout);
    if (rc <= 0) {
        return 0; /* timeout or error */
    }
    ssize_t n = recv(b->fd, buf, len, 0);
    return n < 0 ? 0u : (size_t)n;
}

/* ---- Init ------------------------------------------------------- */

nros_rmw_ret_t xrce_posix_udp_init(xrce_session_state_t *st,
                                   const char *host, const char *port) {
    if (st == NULL || host == NULL || port == NULL) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }

    /* Resolve + connect a UDP socket — same shape as the upstream
     * `uxr_init_udp_platform` does, but we own the fd here so we
     * can drive recv via `poll()` from our own trampoline. */
    struct addrinfo hints;
    memset(&hints, 0, sizeof(hints));
    hints.ai_family = AF_INET;
    hints.ai_socktype = SOCK_DGRAM;
    struct addrinfo *result = NULL;
    if (getaddrinfo(host, port, &hints, &result) != 0 || result == NULL) {
        return NROS_RMW_RET_ERROR;
    }
    int fd = socket(AF_INET, SOCK_DGRAM, 0);
    if (fd < 0) {
        freeaddrinfo(result);
        return NROS_RMW_RET_ERROR;
    }
    bool connected = false;
    for (struct addrinfo *p = result; p != NULL; p = p->ai_next) {
        if (connect(fd, p->ai_addr, p->ai_addrlen) == 0) {
            connected = true;
            break;
        }
    }
    freeaddrinfo(result);
    if (!connected) {
        close(fd);
        return NROS_RMW_RET_ERROR;
    }

    /* Stash bridge state in the session-state. The per-session
     * `udp_bridge` field hosts the fd + back-pointer the
     * trampolines use via `uxrCustomTransport.args`. */
    st->udp_bridge.fd = fd;

    /* Wire the custom transport with framing=false (UDP is packet-
     * preserving; no HDLC framing needed). `uxr_init_custom_transport`
     * stores its `args` into `transport->args`, so the bridge
     * pointer rides along through every trampoline call. */
    uxr_set_custom_transport_callbacks(
        &st->custom, /*framing=*/false,
        posix_udp_open,
        posix_udp_close,
        posix_udp_write,
        posix_udp_read);

    if (!uxr_init_custom_transport(&st->custom, &st->udp_bridge)) {
        close(fd);
        st->udp_bridge.fd = -1;
        return NROS_RMW_RET_ERROR;
    }
    return NROS_RMW_RET_OK;
}

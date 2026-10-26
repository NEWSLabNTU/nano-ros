/*
 * The socket half of zenoh-pico's link ABI, for the platforms whose sockets
 * come from the nano-ros platform net ABI (`<nros/platform_net.h>`): ThreadX
 * (NetX Duo) and bare metal (smoltcp). phase-508.
 *
 * zenoh-pico 1.9 split each network stack's links into two halves: the TCP /
 * UDP links themselves (`src/link/transport/{tcp,udp}/*_<stack>.c`) and a
 * socket half every link shares — close, blocking mode and a readiness wait
 * over a set of sockets (`src/system/unix/network.c`, `system/socket/lwip.c`).
 * Upstream ships neither half for these two platforms. The links are forwarded
 * by `platform_aliases.c`; the socket half lives here because it needs
 * zenoh-pico's own `_z_socket_wait_iter_t`, which the alias TU cannot include.
 *
 * Compiled only where a port row names it (`extra_sources` in the zenoh
 * `nros-rmw.toml`), so it is never a second definition beside upstream's.
 */

#include <stdbool.h>
#include <stdint.h>

#include "zenoh-pico/link/transport/socket.h"
#include "zenoh-pico/system/platform.h"
#include "zenoh-pico/utils/result.h"

#include <nros/platform_net.h>

z_result_t _z_socket_set_blocking(const _z_sys_net_socket_t *sock, bool blocking) {
    /* The platform ABI can make a socket non-blocking and nothing else. Only a
     * listener's accept asks for blocking (`unicast/accept.c`), and neither
     * platform listens, so that request is refused rather than dropped. */
    if (blocking) {
        return _Z_ERR_GENERIC;
    }
    return nros_platform_socket_set_non_blocking(sock) == 0 ? _Z_RES_OK : _Z_ERR_GENERIC;
}

void _z_socket_close(_z_sys_net_socket_t *sock) { nros_platform_socket_close(sock); }

z_result_t _z_socket_wait_readable(_z_socket_wait_iter_t *iter, uint32_t timeout_ms) {
    /* The platform ABI has no readiness wait over several sockets. Report every
     * socket readable: each caller then reads it once, and that read returns
     * whatever its own receive timeout yields — what these platforms did before
     * 1.9, when nothing asked first. It is also what upstream's scout does when
     * this call fails (`session/scout.c`). Peer mode, the other caller, is not
     * used on either platform. */
    (void)timeout_ms;
    _z_socket_wait_iter_reset(iter);
    while (_z_socket_wait_iter_next(iter)) {
        _z_socket_wait_iter_set_ready(iter, true);
    }
    return _Z_RES_OK;
}

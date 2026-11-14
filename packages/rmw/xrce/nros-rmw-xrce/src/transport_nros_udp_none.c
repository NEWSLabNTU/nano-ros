/* Issue 2579 — the UDP transport of a serial-only image (`NROS_LINK_IP=0`).
 *
 * `transport_nros_udp.c` routes XRCE over the `nros_platform_udp_*` ABI, and
 * it used to be compiled on every target. A board with no IP stack, talking to
 * its Agent over a UART through the custom transport, therefore still had to
 * link nine `nros_platform_udp_*` symbols, so the porter wrote nine stubs that
 * nothing would ever call. `xrce-sources.txt` now compiles exactly one of the
 * two TUs: that one under the `ip` condition, this one under `no_ip`.
 *
 * This TU references no network symbol. A `udp://` (or bare `host:port`)
 * locator in such an image is refused with NROS_RMW_RET_UNSUPPORTED, and
 * `session.c` logs which knob produced the image. The other two entry points
 * answer -1 ("this transport has no such hook"), which is never reached,
 * because no session in this image can have opened UDP.
 */

#include "internal.h"

#include "nros/rmw_ret.h"

rmw_ret_t xrce_nros_udp_init(xrce_session_state_t* st, const char* host, const char* port) {
    (void)st;
    (void)host;
    (void)port;
    return NROS_RMW_RET_UNSUPPORTED;
}

int xrce_nros_udp_readiness_handle(xrce_session_state_t* st) {
    (void)st;
    return -1;
}

int xrce_nros_udp_set_rx_notify(xrce_session_state_t* st, void (*cb)(void* ctx), void* ctx) {
    (void)st;
    (void)cb;
    (void)ctx;
    return -1;
}

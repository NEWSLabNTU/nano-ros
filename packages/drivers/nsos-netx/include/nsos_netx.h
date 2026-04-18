/**
 * nsos_netx.h — NetX Duo BSD socket compatibility shim over host POSIX
 *
 * Provides the `nx_bsd_*` symbols by forwarding to the host kernel's BSD
 * socket API. Lets a ThreadX-Linux simulation skip the entire NetX Duo
 * TCP/IP stack (no IP instance, no packet pool, no ARP, no
 * /dev/net/tun, no veth bridge).
 *
 * Modeled after Zephyr's NSOS (Native Sim Offloaded Sockets): the
 * application code sees the NetX BSD API unchanged; under the hood
 * the calls become standard POSIX syscalls handled by the host kernel.
 *
 * Usage:
 *   - Link against `libnsos_netx.a` instead of NetX Duo's `nxd_bsd.c`
 *   - Skip `nx_packet_pool_create`, `nx_ip_create`, `nx_*_enable`,
 *     `nx_bsd_initialize` in your `tx_application_define()`
 *   - Application code keeps calling `nx_bsd_socket()`, `nx_bsd_bind()`,
 *     etc. — they just become `socket()`, `bind()`, etc.
 *
 * Limitations:
 *   - Linux only (uses POSIX `<sys/socket.h>`)
 *   - No IP-level features (no nx_ip_status_check, no NX_IP* APIs)
 *   - Sockets are real kernel fds; no NetX packet pool involvement
 */

#ifndef NSOS_NETX_H
#define NSOS_NETX_H

#ifdef __cplusplus
extern "C" {
#endif

/**
 * Optional initialization hook. Currently a no-op — provided for symmetry
 * with `nx_bsd_initialize` so callers can keep the same lifecycle.
 *
 * Returns 0 on success.
 */
int nsos_netx_init(void);

#ifdef __cplusplus
}
#endif

#endif /* NSOS_NETX_H */

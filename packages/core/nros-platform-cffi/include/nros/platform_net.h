#ifndef NROS_PLATFORM_NET_H
#define NROS_PLATFORM_NET_H

#include <stdint.h>
#include <stddef.h>

#include "nros/platform.h"

#ifdef __cplusplus
extern "C" {
#endif

/**
 * @file platform_net.h
 * @brief Canonical C ABI for the nros platform networking surface.
 *
 * Companion to `<nros/platform.h>`; sits beside the core 39-symbol
 * platform ABI as the network interface zenoh-pico and the XRCE-DDS
 * transports use. Carved into a separate header because bare-metal
 * targets without a network stack can omit it.
 *
 * # Sockets + endpoints
 *
 *  Sockets and endpoints are opaque storage pointers. The application
 *  allocates the bytes (size known to the implementor; matches
 *  zenoh-pico's `_z_sys_net_socket_t` / `_z_sys_net_endpoint_t`).
 *  The implementation writes its handle into the supplied storage.
 *  `read_*` and `send` return `(size_t) -1` on error, otherwise the
 *  number of bytes transferred (zero is a valid short-read result).
 *
 * # Timeout semantics
 *
 *  Read functions inherit the socket's recv timeout (set via
 *  `set_recv_timeout`). `0` ms means block indefinitely; non-zero
 *  is an upper bound on per-call duration.
 *
 * # Optional capabilities
 *
 *  `udp_listen` and `udp_multicast_*` are optional. A platform that
 *  doesn't implement them supplies a stub that returns -1 (or
 *  `(size_t) -1` on the read/send variants); higher layers detect
 *  the failure and fall back. `network_poll` exists for platforms
 *  with a non-OS network stack (smoltcp on bare-metal) — POSIX /
 *  Zephyr / NuttX / FreeRTOS+lwIP / ThreadX+NetXDuo provide a no-op
 *  stub.
 */

#define NROS_PLATFORM_NET_SOCKET_ERROR ((size_t) -1)

/* ---- TCP ---- */

/** Resolve `(address, port)` strings into the caller-allocated endpoint
 *  storage at `ep`. Both strings are NUL-terminated. */
int8_t nros_platform_tcp_create_endpoint(void *ep,
                                         const uint8_t *address,
                                         const uint8_t *port);

/** Release any resources held by the endpoint at `ep`. */
void nros_platform_tcp_free_endpoint(void *ep);

/** Open a TCP client connection to `endpoint`. */
int8_t nros_platform_tcp_open(void *sock, const void *endpoint, uint32_t timeout_ms);

/** Open a listening TCP socket bound to `endpoint`. */
int8_t nros_platform_tcp_listen(void *sock, const void *endpoint);

/** Close a TCP socket. */
void nros_platform_tcp_close(void *sock);

/** Read up to `len` bytes. Returns bytes received, or
 *  `NROS_PLATFORM_NET_SOCKET_ERROR` on error. */
size_t nros_platform_tcp_read(const void *sock, uint8_t *buf, size_t len);

/** Read exactly `len` bytes. Returns `len` on success, or
 *  `NROS_PLATFORM_NET_SOCKET_ERROR` on error. */
size_t nros_platform_tcp_read_exact(const void *sock, uint8_t *buf, size_t len);

/** Send `len` bytes. Returns bytes sent, or
 *  `NROS_PLATFORM_NET_SOCKET_ERROR` on error. */
size_t nros_platform_tcp_send(const void *sock, const uint8_t *buf, size_t len);

/* ---- UDP unicast ---- */

int8_t nros_platform_udp_create_endpoint(void *ep,
                                         const uint8_t *address,
                                         const uint8_t *port);
void   nros_platform_udp_free_endpoint(void *ep);
int8_t nros_platform_udp_open(void *sock, const void *endpoint, uint32_t timeout_ms);

/** Bind a UDP socket in listen / server mode. Optional — return -1
 *  on platforms that don't expose a UDP-server primitive. */
int8_t nros_platform_udp_listen(void *sock, const void *endpoint, uint32_t timeout_ms);

void   nros_platform_udp_close(void *sock);
size_t nros_platform_udp_read(const void *sock, uint8_t *buf, size_t len);
size_t nros_platform_udp_read_exact(const void *sock, uint8_t *buf, size_t len);

/** Send `len` bytes to `endpoint`. */
size_t nros_platform_udp_send(const void *sock,
                              const uint8_t *buf, size_t len,
                              const void *endpoint);

/** Set the recv timeout in milliseconds; `0` means block indefinitely. */
void nros_platform_udp_set_recv_timeout(const void *sock, uint32_t timeout_ms);

/* ---- UDP multicast (optional) ----
 *
 * Signatures mirror the Rust `PlatformUdpMulticast` trait. `lep`
 * is the local-endpoint storage (caller-allocated); `iface` /
 * `join` are NUL-terminated strings naming the network interface
 * and the multicast group to join.
 */

int8_t nros_platform_udp_mcast_open(void *sock,
                                    const void *endpoint,
                                    void *lep,
                                    uint32_t timeout_ms,
                                    const uint8_t *iface);

int8_t nros_platform_udp_mcast_listen(void *sock,
                                      const void *endpoint,
                                      uint32_t timeout_ms,
                                      const uint8_t *iface,
                                      const uint8_t *join);

void nros_platform_udp_mcast_close(void *sockrecv,
                                   void *socksend,
                                   const void *rep,
                                   const void *lep);

size_t nros_platform_udp_mcast_read(const void *sock,
                                    uint8_t *buf, size_t len,
                                    const void *lep,
                                    void *addr);

size_t nros_platform_udp_mcast_read_exact(const void *sock,
                                          uint8_t *buf, size_t len,
                                          const void *lep,
                                          void *addr);

size_t nros_platform_udp_mcast_send(const void *sock,
                                    const uint8_t *buf, size_t len,
                                    const void *endpoint);

/* ---- Socket helpers ---- */

/** Switch a socket to non-blocking mode. */
int8_t nros_platform_socket_set_non_blocking(const void *sock);

/** Accept a pending connection from `sock_in` into `sock_out`. */
int8_t nros_platform_socket_accept(const void *sock_in, void *sock_out);

/** Socket-layer shutdown + close. Distinct from
 *  `nros_platform_tcp_close` because zenoh-pico's helper layer
 *  exposes both. */
void nros_platform_socket_close(void *sock);

/** Wait for socket events on a multi-peer set. Optional on platforms
 *  without a poll/select primitive. */
int8_t nros_platform_socket_wait_event(void *peers, void *mutex);

/* ---- Network poll (bare-metal / non-OS stacks) ---- */

/** Pump the underlying network stack to process pending I/O.
 *  No-op on platforms with a kernel-driven socket layer (POSIX,
 *  Zephyr, lwIP, NetX Duo). Bare-metal smoltcp targets advance
 *  the stack from this entry point. */
void nros_platform_network_poll(void);

/* ---- Socket accessor (Phase 154) ---- *
 *
 * Every `nros_platform_*` backend stores its socket struct with
 * `int fd` (or equivalent) at offset 0:
 *
 *   - POSIX:        `typedef struct { int fd; } nros_posix_socket_t;`
 *   - ThreadX:      `typedef struct { INT fd; } nros_threadx_socket_t;`
 *   - FreeRTOS+lwIP same shape.
 *
 * This accessor lets callers (e.g. zpico-sys's `get_session_fd`
 * helper used by the read-task wakeup path on ThreadX-Linux) read
 * the fd without depending on the per-RTOS struct typedef being
 * visible at compile time. Static-inline so it has zero call
 * overhead and ships with the header — no extra TU to link.
 *
 * Returns -1 if `sock` is NULL. Otherwise returns the leading
 * `int` (which platforms treat as -1 for an unbound socket).
 */
static inline int nros_platform_socket_get_fd(const void *sock) {
    if (sock == NULL) { return -1; }
    /* SAFETY: every backend's socket struct is `{ int fd; ... }`,
     * so the first sizeof(int) bytes of the opaque storage are
     * the BSD-style fd. Cast-to-int read is the canonical way to
     * extract it. */
    return *(const int *)sock;
}

#ifdef __cplusplus
}  /* extern "C" */
#endif

#endif /* NROS_PLATFORM_NET_H */

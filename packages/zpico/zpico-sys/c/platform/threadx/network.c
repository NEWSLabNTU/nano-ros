/**
 * network.c — zenoh-pico network transport for ThreadX + NetX Duo
 *
 * Implements zenoh-pico's network interface using NetX Duo's BSD socket API
 * (nxd_bsd.h). Provides TCP and UDP unicast transport.
 *
 * Uses nx_bsd_* prefixed names throughout so that the code compiles both
 * on embedded targets (where the macros remap nx_bsd_* → standard names)
 * and on the Linux sim (where NX_BSD_ENABLE_NATIVE_API keeps the prefix
 * to avoid conflicts with system headers).
 *
 * Gated on ZENOH_THREADX.
 */

#if defined(ZENOH_THREADX)

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "tx_api.h"
#include "nx_api.h"
#include "nxd_bsd.h"

#include "zenoh-pico/config.h"
#include "zenoh-pico/system/platform.h"
#include "zenoh-pico/utils/logging.h"
#include "zenoh-pico/utils/result.h"

/* ── Helper: parse "a.b.c.d" → network-order uint32 ─────────────────────── */

static uint32_t _z_parse_ipv4(const char *s) {
    unsigned int a, b, c, d;
    if (sscanf(s, "%u.%u.%u.%u", &a, &b, &c, &d) != 4) {
        return 0;
    }
    /* sin_addr.s_addr must be in network byte order (big-endian).
     * The manual construction gives host byte order, so apply htonl(). */
    return htonl((uint32_t)((a << 24) | (b << 16) | (c << 8) | d));
}

static uint16_t _z_parse_port(const char *s) {
    return htons((uint16_t)atoi(s));
}

/* Convert endpoint to sockaddr_in */
static void _z_ep_to_sockaddr(const _z_sys_net_endpoint_t *ep, struct nx_bsd_sockaddr_in *addr) {
    memset(addr, 0, sizeof(*addr));
    addr->sin_family = AF_INET;
    addr->sin_addr.s_addr = ep->_addr;
    addr->sin_port = ep->_port;
}

/* ── Socket utilities ────────────────────────────────────────────────────── */

z_result_t _z_socket_set_non_blocking(const _z_sys_net_socket_t *sock) {
    /* NetX Duo BSD does not support fcntl/O_NONBLOCK.
     * Non-blocking I/O is not used in the ThreadX port. */
    (void)sock;
    return _Z_RES_OK;
}

z_result_t _z_socket_accept(const _z_sys_net_socket_t *sock_in, _z_sys_net_socket_t *sock_out) {
    struct nx_bsd_sockaddr_in addr;
    INT addr_len = sizeof(addr);
    sock_out->_fd = nx_bsd_accept(sock_in->_fd, (struct nx_bsd_sockaddr *)&addr, &addr_len);
    if (sock_out->_fd < 0) {
        return _Z_ERR_GENERIC;
    }
    return _Z_RES_OK;
}

void _z_socket_close(_z_sys_net_socket_t *sock) {
    if (sock->_fd >= 0) {
        nx_bsd_soc_close(sock->_fd);
        sock->_fd = -1;
    }
}

z_result_t _z_socket_wait_event(void *v_peers, _z_mutex_rec_t *mutex) {
    /*
     * NetX Duo BSD select() only supports readfds (no writefds/exceptfds).
     * We use a simple polling approach with a timeout.
     */
    (void)v_peers;
    (void)mutex;

    /* Sleep briefly to yield — actual event waiting is handled by
     * the blocking recv/select in read functions. */
    tx_thread_sleep(1);
    return _Z_RES_OK;
}

/* ── TCP endpoint ────────────────────────────────────────────────────────── */

#if Z_FEATURE_LINK_TCP == 1

z_result_t _z_create_endpoint_tcp(_z_sys_net_endpoint_t *ep, const char *s_address, const char *s_port) {
    ep->_addr = _z_parse_ipv4(s_address);
    ep->_port = _z_parse_port(s_port);

    if (ep->_addr == 0) {
        _Z_ERROR("Failed to parse TCP address: %s", s_address);
        return _Z_ERR_GENERIC;
    }
    return _Z_RES_OK;
}

void _z_free_endpoint_tcp(_z_sys_net_endpoint_t *ep) {
    /* Static storage — nothing to free */
    (void)ep;
}

/* ── TCP sockets ─────────────────────────────────────────────────────────── */

z_result_t _z_open_tcp(_z_sys_net_socket_t *sock, const _z_sys_net_endpoint_t rep, uint32_t tout) {
    z_result_t ret = _Z_RES_OK;

    sock->_fd = nx_bsd_socket(AF_INET, SOCK_STREAM, IPPROTO_TCP);
    if (sock->_fd < 0) {
        _Z_ERROR("Failed to create TCP socket");
        return _Z_ERR_GENERIC;
    }

    /* Set receive timeout */
    if (tout > 0) {
        INT tv_ms = (INT)tout;
        if (nx_bsd_setsockopt(sock->_fd, SOL_SOCKET, SO_RCVTIMEO, &tv_ms, sizeof(tv_ms)) < 0) {
            _Z_DEBUG("Warning: SO_RCVTIMEO not supported, continuing without timeout");
        }
    }

    /* Connect to remote endpoint */
    struct nx_bsd_sockaddr_in addr;
    _z_ep_to_sockaddr(&rep, &addr);

    if (nx_bsd_connect(sock->_fd, (struct nx_bsd_sockaddr *)&addr, sizeof(addr)) < 0) {
        _Z_ERROR("TCP connect failed");
        nx_bsd_soc_close(sock->_fd);
        sock->_fd = -1;
        ret = _Z_ERR_GENERIC;
    }

    return ret;
}

z_result_t _z_listen_tcp(_z_sys_net_socket_t *sock, const _z_sys_net_endpoint_t lep) {
    z_result_t ret = _Z_RES_OK;

    sock->_fd = nx_bsd_socket(AF_INET, SOCK_STREAM, IPPROTO_TCP);
    if (sock->_fd < 0) {
        return _Z_ERR_GENERIC;
    }

    /* SO_REUSEADDR */
    int value = 1;
    nx_bsd_setsockopt(sock->_fd, SOL_SOCKET, SO_REUSEADDR, &value, sizeof(value));

    struct nx_bsd_sockaddr_in addr;
    _z_ep_to_sockaddr(&lep, &addr);

    if (nx_bsd_bind(sock->_fd, (struct nx_bsd_sockaddr *)&addr, sizeof(addr)) < 0) {
        nx_bsd_soc_close(sock->_fd);
        sock->_fd = -1;
        return _Z_ERR_GENERIC;
    }

    if (nx_bsd_listen(sock->_fd, 1) < 0) {
        nx_bsd_soc_close(sock->_fd);
        sock->_fd = -1;
        return _Z_ERR_GENERIC;
    }

    return ret;
}

void _z_close_tcp(_z_sys_net_socket_t *sock) {
    _z_socket_close(sock);
}

size_t _z_read_tcp(const _z_sys_net_socket_t sock, uint8_t *ptr, size_t len) {
    INT rb = nx_bsd_recv(sock._fd, (CHAR *)ptr, (INT)len, 0);
    if (rb <= 0) {
        return SIZE_MAX;
    }
    return (size_t)rb;
}

size_t _z_read_exact_tcp(const _z_sys_net_socket_t sock, uint8_t *ptr, size_t len) {
    size_t n = 0;
    uint8_t *pos = ptr;

    while (n < len) {
        size_t rb = _z_read_tcp(sock, pos, len - n);
        if (rb == SIZE_MAX) {
            return SIZE_MAX;
        }
        n += rb;
        pos += rb;
    }
    return n;
}

size_t _z_send_tcp(const _z_sys_net_socket_t sock, const uint8_t *ptr, size_t len) {
    INT sb = nx_bsd_send(sock._fd, (CHAR *)ptr, (INT)len, 0);
    if (sb <= 0) {
        return SIZE_MAX;
    }
    return (size_t)sb;
}

#endif  /* Z_FEATURE_LINK_TCP */

/* ── UDP unicast endpoint ────────────────────────────────────────────────── */

#if Z_FEATURE_LINK_UDP_UNICAST == 1

z_result_t _z_create_endpoint_udp(_z_sys_net_endpoint_t *ep, const char *s_address, const char *s_port) {
    ep->_addr = _z_parse_ipv4(s_address);
    ep->_port = _z_parse_port(s_port);

    if (ep->_addr == 0) {
        _Z_ERROR("Failed to parse UDP address: %s", s_address);
        return _Z_ERR_GENERIC;
    }
    return _Z_RES_OK;
}

void _z_free_endpoint_udp(_z_sys_net_endpoint_t *ep) {
    (void)ep;
}

/* ── UDP unicast sockets ─────────────────────────────────────────────────── */

z_result_t _z_open_udp_unicast(_z_sys_net_socket_t *sock, const _z_sys_net_endpoint_t rep, uint32_t tout) {
    sock->_fd = nx_bsd_socket(AF_INET, SOCK_DGRAM, IPPROTO_UDP);
    if (sock->_fd < 0) {
        return _Z_ERR_GENERIC;
    }

    /* Set receive timeout */
    if (tout > 0) {
        INT tv_ms = (INT)tout;
        nx_bsd_setsockopt(sock->_fd, SOL_SOCKET, SO_RCVTIMEO, &tv_ms, sizeof(tv_ms));
    }

    /* Connect to remote endpoint (allows using send/recv instead of sendto/recvfrom) */
    struct nx_bsd_sockaddr_in addr;
    _z_ep_to_sockaddr(&rep, &addr);

    if (nx_bsd_connect(sock->_fd, (struct nx_bsd_sockaddr *)&addr, sizeof(addr)) < 0) {
        nx_bsd_soc_close(sock->_fd);
        sock->_fd = -1;
        return _Z_ERR_GENERIC;
    }

    return _Z_RES_OK;
}

z_result_t _z_listen_udp_unicast(_z_sys_net_socket_t *sock, const _z_sys_net_endpoint_t lep, uint32_t tout) {
    sock->_fd = nx_bsd_socket(AF_INET, SOCK_DGRAM, IPPROTO_UDP);
    if (sock->_fd < 0) {
        return _Z_ERR_GENERIC;
    }

    struct nx_bsd_sockaddr_in addr;
    _z_ep_to_sockaddr(&lep, &addr);

    if (nx_bsd_bind(sock->_fd, (struct nx_bsd_sockaddr *)&addr, sizeof(addr)) < 0) {
        nx_bsd_soc_close(sock->_fd);
        sock->_fd = -1;
        return _Z_ERR_GENERIC;
    }

    return _Z_RES_OK;
}

void _z_close_udp_unicast(_z_sys_net_socket_t *sock) {
    _z_socket_close(sock);
}

size_t _z_read_udp_unicast(const _z_sys_net_socket_t sock, uint8_t *ptr, size_t len) {
    INT rb = nx_bsd_recv(sock._fd, (CHAR *)ptr, (INT)len, 0);
    if (rb <= 0) {
        return SIZE_MAX;
    }
    return (size_t)rb;
}

size_t _z_read_exact_udp_unicast(const _z_sys_net_socket_t sock, uint8_t *ptr, size_t len) {
    size_t n = 0;
    uint8_t *pos = ptr;

    while (n < len) {
        size_t rb = _z_read_udp_unicast(sock, pos, len - n);
        if (rb == SIZE_MAX) {
            return SIZE_MAX;
        }
        n += rb;
        pos += rb;
    }
    return n;
}

size_t _z_send_udp_unicast(const _z_sys_net_socket_t sock, const uint8_t *ptr, size_t len,
                            const _z_sys_net_endpoint_t rep) {
    struct nx_bsd_sockaddr_in addr;
    _z_ep_to_sockaddr(&rep, &addr);

    INT sb = nx_bsd_sendto(sock._fd, (CHAR *)ptr, (INT)len, 0,
                    (struct nx_bsd_sockaddr *)&addr, sizeof(addr));
    if (sb <= 0) {
        return SIZE_MAX;
    }
    return (size_t)sb;
}

#endif  /* Z_FEATURE_LINK_UDP_UNICAST */

/* ── UDP multicast (not supported on ThreadX — NetX Duo BSD has limited multicast) */

#if Z_FEATURE_LINK_UDP_MULTICAST == 1

z_result_t _z_open_udp_multicast(_z_sys_net_socket_t *sock, const _z_sys_net_endpoint_t rep,
                                  _z_sys_net_endpoint_t *lep, uint32_t tout, const char *iface) {
    (void)sock; (void)rep; (void)lep; (void)tout; (void)iface;
    _Z_ERROR("UDP multicast not supported on ThreadX");
    return _Z_ERR_GENERIC;
}

z_result_t _z_listen_udp_multicast(_z_sys_net_socket_t *sock, const _z_sys_net_endpoint_t rep,
                                    uint32_t tout, const char *iface, const char *join) {
    (void)sock; (void)rep; (void)tout; (void)iface; (void)join;
    return _Z_ERR_GENERIC;
}

void _z_close_udp_multicast(_z_sys_net_socket_t *sockrecv, _z_sys_net_socket_t *socksend,
                             _z_sys_net_endpoint_t rep, _z_sys_net_endpoint_t lep) {
    (void)rep; (void)lep;
    _z_socket_close(sockrecv);
    _z_socket_close(socksend);
}

size_t _z_read_udp_multicast(const _z_sys_net_socket_t sock, uint8_t *ptr, size_t len,
                              const _z_sys_net_endpoint_t lep, _z_slice_t *addr) {
    (void)sock; (void)ptr; (void)len; (void)lep; (void)addr;
    return SIZE_MAX;
}

size_t _z_read_exact_udp_multicast(const _z_sys_net_socket_t sock, uint8_t *ptr, size_t len,
                                    const _z_sys_net_endpoint_t lep, _z_slice_t *addr) {
    (void)sock; (void)ptr; (void)len; (void)lep; (void)addr;
    return SIZE_MAX;
}

size_t _z_send_udp_multicast(const _z_sys_net_socket_t sock, const uint8_t *ptr, size_t len,
                              const _z_sys_net_endpoint_t rep) {
    (void)sock; (void)ptr; (void)len; (void)rep;
    return SIZE_MAX;
}

#endif  /* Z_FEATURE_LINK_UDP_MULTICAST */

/* ── Serial (not supported — use TCP/UDP over NetX Duo) ──────────────────── */

#if Z_FEATURE_LINK_SERIAL == 1

z_result_t _z_open_serial_from_dev(_z_sys_net_socket_t *sock, char *dev, uint32_t baudrate) {
    (void)sock; (void)dev; (void)baudrate;
    return _Z_ERR_GENERIC;
}

z_result_t _z_open_serial_from_pins(_z_sys_net_socket_t *sock, uint32_t txpin, uint32_t rxpin, uint32_t baudrate) {
    (void)sock; (void)txpin; (void)rxpin; (void)baudrate;
    return _Z_ERR_GENERIC;
}

z_result_t _z_listen_serial_from_dev(_z_sys_net_socket_t *sock, char *dev, uint32_t baudrate) {
    (void)sock; (void)dev; (void)baudrate;
    return _Z_ERR_GENERIC;
}

z_result_t _z_listen_serial_from_pins(_z_sys_net_socket_t *sock, uint32_t txpin, uint32_t rxpin, uint32_t baudrate) {
    (void)sock; (void)txpin; (void)rxpin; (void)baudrate;
    return _Z_ERR_GENERIC;
}

void _z_close_serial(_z_sys_net_socket_t *sock) { (void)sock; }

size_t _z_read_serial_internal(const _z_sys_net_socket_t sock, uint8_t *header, uint8_t *ptr, size_t len) {
    (void)sock; (void)header; (void)ptr; (void)len;
    return SIZE_MAX;
}

size_t _z_send_serial_internal(const _z_sys_net_socket_t sock, uint8_t header, const uint8_t *ptr, size_t len) {
    (void)sock; (void)header; (void)ptr; (void)len;
    return SIZE_MAX;
}

#endif  /* Z_FEATURE_LINK_SERIAL */

#endif  /* ZENOH_THREADX */

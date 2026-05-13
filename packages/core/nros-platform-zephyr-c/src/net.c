/*
 * Phase 121.6.zephyr-c — Zephyr implementation of the canonical
 * networking ABI declared in `<nros/platform_net.h>`.
 *
 * Backed by Zephyr's BSD socket layer (`<zephyr/net/socket.h>`):
 * zsock_socket / connect / bind / listen / accept / send / recv /
 * sendto / recvfrom / setsockopt / fcntl / close +
 * zsock_getaddrinfo / zsock_freeaddrinfo.
 *
 * Storage layouts:
 *   endpoint = { struct zsock_addrinfo *iptcp; }
 *   socket   = { int fd; }
 *
 * Multicast stubbed (returns -1 / (size_t) -1) — full Zephyr
 * mcast wiring (zsock_setsockopt(IP_ADD_MEMBERSHIP) per iface)
 * lands as a follow-up.
 */

#include <nros/platform_net.h>

#include <zephyr/kernel.h>
#include <zephyr/net/socket.h>
#include <zephyr/net/net_ip.h>

#include <stddef.h>
#include <string.h>
#include <errno.h>

typedef struct {
    struct zsock_addrinfo *iptcp;
} nros_zephyr_endpoint_t;

typedef struct {
    int fd;
} nros_zephyr_socket_t;

#define TRANSPORT_LEASE_MS 10000u

static void set_rcv_timeout(int fd, uint32_t timeout_ms) {
    struct zsock_timeval tv = {
        .tv_sec  = (long) (timeout_ms / 1000u),
        .tv_usec = (long) ((timeout_ms % 1000u) * 1000u),
    };
    (void) zsock_setsockopt(fd, SOL_SOCKET, SO_RCVTIMEO, &tv, sizeof(tv));
}

static void set_int_opt(int fd, int level, int optname, int value) {
    (void) zsock_setsockopt(fd, level, optname, &value, sizeof(int));
}

static void apply_tcp_common_options(int fd, uint32_t recv_timeout_ms) {
    set_rcv_timeout(fd, recv_timeout_ms);
    set_int_opt(fd, SOL_SOCKET, SO_KEEPALIVE, 1);
    set_int_opt(fd, IPPROTO_TCP, TCP_NODELAY, 1);
    /* Zephyr socket SO_LINGER may not be universally supported; best-effort. */
}

/* ---- TCP ---- */

int8_t nros_platform_tcp_create_endpoint(void *ep_raw,
                                         const uint8_t *address,
                                         const uint8_t *port) {
    if (ep_raw == NULL) return -1;
    nros_zephyr_endpoint_t *ep = (nros_zephyr_endpoint_t *) ep_raw;
    struct zsock_addrinfo hints;
    memset(&hints, 0, sizeof(hints));
    hints.ai_family   = AF_UNSPEC;
    hints.ai_socktype = SOCK_STREAM;
    hints.ai_protocol = IPPROTO_TCP;
    return zsock_getaddrinfo((const char *) address, (const char *) port,
                             &hints, &ep->iptcp) == 0 ? 0 : -1;
}

void nros_platform_tcp_free_endpoint(void *ep_raw) {
    if (ep_raw == NULL) return;
    nros_zephyr_endpoint_t *ep = (nros_zephyr_endpoint_t *) ep_raw;
    if (ep->iptcp != NULL) {
        zsock_freeaddrinfo(ep->iptcp);
        ep->iptcp = NULL;
    }
}

int8_t nros_platform_tcp_open(void *sock_raw, const void *endpoint, uint32_t timeout_ms) {
    if (sock_raw == NULL || endpoint == NULL) return -1;
    nros_zephyr_socket_t *sock = (nros_zephyr_socket_t *) sock_raw;
    const nros_zephyr_endpoint_t *ep = (const nros_zephyr_endpoint_t *) endpoint;
    if (ep->iptcp == NULL) return -1;

    struct zsock_addrinfo *first = ep->iptcp;
    int fd = zsock_socket(first->ai_family, first->ai_socktype, first->ai_protocol);
    if (fd < 0) return -1;
    sock->fd = fd;
    apply_tcp_common_options(fd, timeout_ms);

    for (struct zsock_addrinfo *it = ep->iptcp; it != NULL; it = it->ai_next) {
        if (zsock_connect(fd, it->ai_addr, it->ai_addrlen) == 0) {
            return 0;
        }
    }
    zsock_close(fd);
    sock->fd = -1;
    return -1;
}

int8_t nros_platform_tcp_listen(void *sock_raw, const void *endpoint) {
    if (sock_raw == NULL || endpoint == NULL) return -1;
    nros_zephyr_socket_t *sock = (nros_zephyr_socket_t *) sock_raw;
    const nros_zephyr_endpoint_t *ep = (const nros_zephyr_endpoint_t *) endpoint;
    if (ep->iptcp == NULL) return -1;

    struct zsock_addrinfo *first = ep->iptcp;
    int fd = zsock_socket(first->ai_family, first->ai_socktype, first->ai_protocol);
    if (fd < 0) return -1;
    sock->fd = fd;
    set_int_opt(fd, SOL_SOCKET, SO_REUSEADDR, 1);
    apply_tcp_common_options(fd, 0);

    for (struct zsock_addrinfo *it = ep->iptcp; it != NULL; it = it->ai_next) {
        if (zsock_bind(fd, it->ai_addr, it->ai_addrlen) == 0
            && zsock_listen(fd, 16) == 0) {
            return 0;
        }
    }
    zsock_close(fd);
    sock->fd = -1;
    return -1;
}

void nros_platform_tcp_close(void *sock_raw) {
    if (sock_raw == NULL) return;
    nros_zephyr_socket_t *sock = (nros_zephyr_socket_t *) sock_raw;
    if (sock->fd >= 0) {
        (void) zsock_shutdown(sock->fd, ZSOCK_SHUT_RDWR);
        (void) zsock_close(sock->fd);
        sock->fd = -1;
    }
}

size_t nros_platform_tcp_read(const void *sock_raw, uint8_t *buf, size_t len) {
    if (sock_raw == NULL) return NROS_PLATFORM_NET_SOCKET_ERROR;
    const nros_zephyr_socket_t *sock = (const nros_zephyr_socket_t *) sock_raw;
    ssize_t r = zsock_recv(sock->fd, buf, len, 0);
    return r < 0 ? NROS_PLATFORM_NET_SOCKET_ERROR : (size_t) r;
}

size_t nros_platform_tcp_read_exact(const void *sock_raw, uint8_t *buf, size_t len) {
    size_t n = 0;
    while (n < len) {
        size_t r = nros_platform_tcp_read(sock_raw, buf + n, len - n);
        if (r == NROS_PLATFORM_NET_SOCKET_ERROR) return r;
        if (r == 0) return 0;
        n += r;
    }
    return n;
}

size_t nros_platform_tcp_send(const void *sock_raw, const uint8_t *buf, size_t len) {
    if (sock_raw == NULL) return NROS_PLATFORM_NET_SOCKET_ERROR;
    const nros_zephyr_socket_t *sock = (const nros_zephyr_socket_t *) sock_raw;
    ssize_t r = zsock_send(sock->fd, buf, len, 0);
    return r < 0 ? NROS_PLATFORM_NET_SOCKET_ERROR : (size_t) r;
}

/* ---- UDP unicast ---- */

int8_t nros_platform_udp_create_endpoint(void *ep_raw,
                                         const uint8_t *address,
                                         const uint8_t *port) {
    if (ep_raw == NULL) return -1;
    nros_zephyr_endpoint_t *ep = (nros_zephyr_endpoint_t *) ep_raw;
    struct zsock_addrinfo hints;
    memset(&hints, 0, sizeof(hints));
    hints.ai_family   = AF_UNSPEC;
    hints.ai_socktype = SOCK_DGRAM;
    hints.ai_protocol = IPPROTO_UDP;
    return zsock_getaddrinfo((const char *) address, (const char *) port,
                             &hints, &ep->iptcp) == 0 ? 0 : -1;
}

void nros_platform_udp_free_endpoint(void *ep_raw) {
    nros_platform_tcp_free_endpoint(ep_raw);
}

int8_t nros_platform_udp_open(void *sock_raw, const void *endpoint, uint32_t timeout_ms) {
    if (sock_raw == NULL || endpoint == NULL) return -1;
    nros_zephyr_socket_t *sock = (nros_zephyr_socket_t *) sock_raw;
    const nros_zephyr_endpoint_t *ep = (const nros_zephyr_endpoint_t *) endpoint;
    if (ep->iptcp == NULL) return -1;
    struct zsock_addrinfo *ai = ep->iptcp;
    int fd = zsock_socket(ai->ai_family, ai->ai_socktype, ai->ai_protocol);
    if (fd < 0) return -1;
    sock->fd = fd;
    set_rcv_timeout(fd, timeout_ms);
    return 0;
}

int8_t nros_platform_udp_listen(void *sock_raw, const void *endpoint, uint32_t timeout_ms) {
    if (sock_raw == NULL || endpoint == NULL) return -1;
    nros_zephyr_socket_t *sock = (nros_zephyr_socket_t *) sock_raw;
    const nros_zephyr_endpoint_t *ep = (const nros_zephyr_endpoint_t *) endpoint;
    if (ep->iptcp == NULL) return -1;

    struct zsock_addrinfo *first = ep->iptcp;
    int fd = zsock_socket(first->ai_family, first->ai_socktype, first->ai_protocol);
    if (fd < 0) return -1;
    sock->fd = fd;
    set_int_opt(fd, SOL_SOCKET, SO_REUSEADDR, 1);
    set_rcv_timeout(fd, timeout_ms);

    for (struct zsock_addrinfo *it = ep->iptcp; it != NULL; it = it->ai_next) {
        if (zsock_bind(fd, it->ai_addr, it->ai_addrlen) == 0) {
            return 0;
        }
    }
    zsock_close(fd);
    sock->fd = -1;
    return -1;
}

void nros_platform_udp_close(void *sock_raw) {
    if (sock_raw == NULL) return;
    nros_zephyr_socket_t *sock = (nros_zephyr_socket_t *) sock_raw;
    if (sock->fd >= 0) {
        (void) zsock_close(sock->fd);
        sock->fd = -1;
    }
}

size_t nros_platform_udp_read(const void *sock_raw, uint8_t *buf, size_t len) {
    if (sock_raw == NULL) return NROS_PLATFORM_NET_SOCKET_ERROR;
    const nros_zephyr_socket_t *sock = (const nros_zephyr_socket_t *) sock_raw;
    struct sockaddr_storage raddr;
    socklen_t addrlen = (socklen_t) sizeof(raddr);
    ssize_t r = zsock_recvfrom(sock->fd, buf, len, 0,
                               (struct sockaddr *) &raddr, &addrlen);
    return r < 0 ? NROS_PLATFORM_NET_SOCKET_ERROR : (size_t) r;
}

size_t nros_platform_udp_read_exact(const void *sock_raw, uint8_t *buf, size_t len) {
    size_t n = 0;
    while (n < len) {
        size_t r = nros_platform_udp_read(sock_raw, buf + n, len - n);
        if (r == NROS_PLATFORM_NET_SOCKET_ERROR) return r;
        if (r == 0) return 0;
        n += r;
    }
    return n;
}

size_t nros_platform_udp_send(const void *sock_raw,
                              const uint8_t *buf, size_t len,
                              const void *endpoint) {
    if (sock_raw == NULL || endpoint == NULL) return NROS_PLATFORM_NET_SOCKET_ERROR;
    const nros_zephyr_socket_t *sock = (const nros_zephyr_socket_t *) sock_raw;
    const nros_zephyr_endpoint_t *ep = (const nros_zephyr_endpoint_t *) endpoint;
    if (ep->iptcp == NULL) return NROS_PLATFORM_NET_SOCKET_ERROR;
    struct zsock_addrinfo *ai = ep->iptcp;
    ssize_t r = zsock_sendto(sock->fd, buf, len, 0, ai->ai_addr, ai->ai_addrlen);
    return r < 0 ? NROS_PLATFORM_NET_SOCKET_ERROR : (size_t) r;
}

void nros_platform_udp_set_recv_timeout(const void *sock_raw, uint32_t timeout_ms) {
    if (sock_raw == NULL) return;
    const nros_zephyr_socket_t *sock = (const nros_zephyr_socket_t *) sock_raw;
    if (timeout_ms == 0) {
        int flags = zsock_fcntl(sock->fd, F_GETFL, 0);
        if (flags >= 0) {
            (void) zsock_fcntl(sock->fd, F_SETFL, flags | O_NONBLOCK);
        }
        return;
    }
    set_rcv_timeout(sock->fd, timeout_ms);
}

/* ---- UDP multicast (stubs) ---- */

int8_t nros_platform_udp_mcast_open(void *s, const void *e, void *l, uint32_t t, const uint8_t *i) {
    (void) s; (void) e; (void) l; (void) t; (void) i;
    return -1;
}
int8_t nros_platform_udp_mcast_listen(void *s, const void *e, uint32_t t,
                                      const uint8_t *i, const uint8_t *j) {
    (void) s; (void) e; (void) t; (void) i; (void) j;
    return -1;
}
void nros_platform_udp_mcast_close(void *sr, void *ss, const void *r, const void *l) {
    (void) sr; (void) ss; (void) r; (void) l;
}
size_t nros_platform_udp_mcast_read(const void *s, uint8_t *b, size_t n, const void *l, void *a) {
    (void) s; (void) b; (void) n; (void) l; (void) a;
    return NROS_PLATFORM_NET_SOCKET_ERROR;
}
size_t nros_platform_udp_mcast_read_exact(const void *s, uint8_t *b, size_t n, const void *l, void *a) {
    (void) s; (void) b; (void) n; (void) l; (void) a;
    return NROS_PLATFORM_NET_SOCKET_ERROR;
}
size_t nros_platform_udp_mcast_send(const void *s, const uint8_t *b, size_t n, const void *e) {
    (void) s; (void) b; (void) n; (void) e;
    return NROS_PLATFORM_NET_SOCKET_ERROR;
}

/* ---- Socket helpers ---- */

int8_t nros_platform_socket_set_non_blocking(const void *sock_raw) {
    if (sock_raw == NULL) return -1;
    const nros_zephyr_socket_t *sock = (const nros_zephyr_socket_t *) sock_raw;
    int flags = zsock_fcntl(sock->fd, F_GETFL, 0);
    if (flags < 0) return -1;
    if (zsock_fcntl(sock->fd, F_SETFL, flags | O_NONBLOCK) < 0) return -1;
    return 0;
}

int8_t nros_platform_socket_accept(const void *in_raw, void *out_raw) {
    if (in_raw == NULL || out_raw == NULL) return -1;
    const nros_zephyr_socket_t *in = (const nros_zephyr_socket_t *) in_raw;
    nros_zephyr_socket_t *out = (nros_zephyr_socket_t *) out_raw;
    struct sockaddr_storage naddr;
    socklen_t nlen = (socklen_t) sizeof(naddr);
    int con = zsock_accept(in->fd, (struct sockaddr *) &naddr, &nlen);
    if (con < 0) return -1;
    out->fd = con;
    return 0;
}

void nros_platform_socket_close(void *sock_raw) {
    nros_platform_tcp_close(sock_raw);
}

int8_t nros_platform_socket_wait_event(void *peers, void *mutex) {
    (void) peers; (void) mutex;
    k_yield();
    return 0;
}

/* ---- Network poll ---- */

void nros_platform_network_poll(void) {
    /* Zephyr drives net I/O from its own threads — no-op. */
}

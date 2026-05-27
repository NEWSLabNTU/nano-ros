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
#include <zephyr/posix/fcntl.h>

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

static void set_udp_recv_timeout(int fd, uint32_t timeout_ms) {
    if (timeout_ms == 0) {
        int flags = zsock_fcntl(fd, F_GETFL, 0);
        if (flags >= 0) {
            (void) zsock_fcntl(fd, F_SETFL, flags | O_NONBLOCK);
        }
        return;
    }
    set_rcv_timeout(fd, timeout_ms);
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
    set_udp_recv_timeout(fd, timeout_ms);
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
    set_udp_recv_timeout(fd, timeout_ms);

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
    set_udp_recv_timeout(sock->fd, timeout_ms);
}

/* ---- UDP multicast ----
 *
 * Zephyr's zsock layer exposes IP_ADD_MEMBERSHIP + a membership-request
 * struct. Requires CONFIG_NET_IPV4_IGMP=y. iface resolution skipped —
 * Zephyr applications that need iface-pinned multicast can
 * `net_if_ipv4_select_src_iface` and post-set IP_MULTICAST_IF.
 */

#include <zephyr/posix/sys/socket.h>

/* Older Zephyr (≤3.5) doesn't expose IP_ADD_MEMBERSHIP via the BSD
 * socket layer. Fall back to net_ipv4_igmp_join() / _leave() from
 * <zephyr/net/igmp.h>, which the same `CONFIG_NET_IPV4_IGMP=y`
 * enables. Detect by the macro absence — newer Zephyr defines it
 * via <zephyr/net/socket.h>. */
#ifndef IP_ADD_MEMBERSHIP
#  define NROS_NET_USE_IGMP_HELPER 1
#  include <zephyr/net/igmp.h>
#  include <zephyr/net/net_if.h>
#endif

#ifndef NROS_NET_USE_IGMP_HELPER
/* Phase 184.B — the IP_ADD_MEMBERSHIP membership-request struct differs by
 * the active libc. newlib's <netinet/in.h> ships the BSD `struct ip_mreq`
 * (imr_multiaddr + imr_interface) and NOT the Linux `ip_mreqn` extension —
 * so on CONFIG_NEWLIB_LIBC targets (e.g. fvp_baser_aemv8r) `struct ip_mreqn`
 * is an incomplete type. Zephyr's own minimal/picolibc BSD layer ships
 * `ip_mreqn` (imr_multiaddr + imr_address + imr_ifindex). Zephyr's
 * IP_ADD_MEMBERSHIP accepts both the 8-byte and 12-byte forms (see the NSOS
 * multicast patch in zephyr/patches), so use whichever the active libc
 * actually defines. INADDR_ANY in the interface field = default iface,
 * identical semantics across both structs. */
#  if defined(CONFIG_NEWLIB_LIBC)
typedef struct ip_mreq  nros_mcast_membership_t;
#    define NROS_MCAST_IFACE(m) ((m).imr_interface)
#  else
typedef struct ip_mreqn nros_mcast_membership_t;
#    define NROS_MCAST_IFACE(m) ((m).imr_address)
#  endif
#endif

typedef struct {
    size_t  len;
    const uint8_t *start;
    void   *_deleter;
    void   *_context;
} nros_z_slice_t;

int8_t nros_platform_udp_mcast_open(void *sock_raw, const void *endpoint,
                                    void *lep_raw, uint32_t timeout_ms,
                                    const uint8_t *iface) {
    (void) iface;
    if (sock_raw == NULL || endpoint == NULL || lep_raw == NULL) return -1;
    nros_zephyr_socket_t *sock = (nros_zephyr_socket_t *) sock_raw;
    nros_zephyr_endpoint_t *lep = (nros_zephyr_endpoint_t *) lep_raw;
    const nros_zephyr_endpoint_t *ep = (const nros_zephyr_endpoint_t *) endpoint;
    if (ep->iptcp == NULL) return -1;
    struct zsock_addrinfo *ai = ep->iptcp;

    int fd = zsock_socket(ai->ai_family, ai->ai_socktype, ai->ai_protocol);
    if (fd < 0) return -1;
    sock->fd = fd;
    set_udp_recv_timeout(fd, timeout_ms);

    struct sockaddr_in addr;
    memset(&addr, 0, sizeof(addr));
    addr.sin_family      = AF_INET;
    addr.sin_port        = ((const struct sockaddr_in *) ai->ai_addr)->sin_port;
    addr.sin_addr.s_addr = htonl(INADDR_ANY);
    if (zsock_bind(fd, (struct sockaddr *) &addr, sizeof(addr)) < 0) {
        zsock_close(fd); sock->fd = -1; return -1;
    }

    socklen_t bound_len = (socklen_t) sizeof(addr);
    (void) zsock_getsockname(fd, (struct sockaddr *) &addr, &bound_len);

    struct sockaddr *lsockaddr = (struct sockaddr *) k_malloc(sizeof(addr));
    if (lsockaddr == NULL) {
        zsock_close(fd); sock->fd = -1; return -1;
    }
    memcpy(lsockaddr, &addr, sizeof(addr));

    struct zsock_addrinfo *laddr = (struct zsock_addrinfo *)
        k_malloc(sizeof(struct zsock_addrinfo));
    if (laddr == NULL) {
        k_free(lsockaddr); zsock_close(fd); sock->fd = -1; return -1;
    }
    memset(laddr, 0, sizeof(*laddr));
    laddr->ai_family   = ai->ai_family;
    laddr->ai_socktype = ai->ai_socktype;
    laddr->ai_protocol = ai->ai_protocol;
    laddr->ai_addrlen  = (socklen_t) sizeof(addr);
    laddr->ai_addr     = lsockaddr;
    lep->iptcp = laddr;
    return 0;
}

int8_t nros_platform_udp_mcast_listen(void *sock_raw, const void *endpoint,
                                      uint32_t timeout_ms,
                                      const uint8_t *iface,
                                      const uint8_t *join) {
    (void) iface;
    if (sock_raw == NULL || endpoint == NULL) return -1;
    nros_zephyr_socket_t *sock = (nros_zephyr_socket_t *) sock_raw;
    const nros_zephyr_endpoint_t *ep = (const nros_zephyr_endpoint_t *) endpoint;
    if (ep->iptcp == NULL) return -1;
    struct zsock_addrinfo *ai = ep->iptcp;

    int fd = zsock_socket(ai->ai_family, ai->ai_socktype, ai->ai_protocol);
    if (fd < 0) return -1;
    sock->fd = fd;
    set_udp_recv_timeout(fd, timeout_ms);
    set_int_opt(fd, SOL_SOCKET, SO_REUSEADDR, 1);

    struct sockaddr_in addr;
    memset(&addr, 0, sizeof(addr));
    addr.sin_family      = AF_INET;
    addr.sin_port        = ((const struct sockaddr_in *) ai->ai_addr)->sin_port;
    addr.sin_addr.s_addr = htonl(INADDR_ANY);
    if (zsock_bind(fd, (struct sockaddr *) &addr, sizeof(addr)) < 0) {
        zsock_close(fd); sock->fd = -1; return -1;
    }

#ifdef NROS_NET_USE_IGMP_HELPER
    struct in_addr mcast;
    if (join != NULL) {
        if (zsock_inet_pton(AF_INET, (const char *) join, &mcast) != 1) {
            zsock_close(fd); sock->fd = -1; return -1;
        }
    } else {
        mcast = ((const struct sockaddr_in *) ai->ai_addr)->sin_addr;
    }
    {
        struct net_if *nif = net_if_get_default();
        if (nif == NULL || net_ipv4_igmp_join(nif, &mcast) < 0) {
            zsock_close(fd); sock->fd = -1; return -1;
        }
    }
#else
    nros_mcast_membership_t mreq;
    memset(&mreq, 0, sizeof(mreq));
    if (join != NULL
        && zsock_inet_pton(AF_INET, (const char *) join, &mreq.imr_multiaddr) != 1) {
        zsock_close(fd); sock->fd = -1; return -1;
    }
    if (join == NULL) {
        mreq.imr_multiaddr = ((const struct sockaddr_in *) ai->ai_addr)->sin_addr;
    }
    NROS_MCAST_IFACE(mreq).s_addr = htonl(INADDR_ANY);
    if (zsock_setsockopt(fd, IPPROTO_IP, IP_ADD_MEMBERSHIP,
                         &mreq, sizeof(mreq)) < 0) {
        zsock_close(fd); sock->fd = -1; return -1;
    }
#endif
    return 0;
}

void nros_platform_udp_mcast_close(void *sockrecv_raw, void *socksend_raw,
                                   const void *rep_raw, const void *lep_raw) {
    nros_zephyr_socket_t *sockrecv = (nros_zephyr_socket_t *) sockrecv_raw;
    nros_zephyr_socket_t *socksend = (nros_zephyr_socket_t *) socksend_raw;
    const nros_zephyr_endpoint_t *rep = (const nros_zephyr_endpoint_t *) rep_raw;
    const nros_zephyr_endpoint_t *lep = (const nros_zephyr_endpoint_t *) lep_raw;

    if (sockrecv != NULL && sockrecv->fd >= 0 && rep != NULL && rep->iptcp != NULL) {
        struct zsock_addrinfo *ai = rep->iptcp;
#ifdef NROS_NET_USE_IGMP_HELPER
        struct in_addr mcast =
            ((const struct sockaddr_in *) ai->ai_addr)->sin_addr;
        struct net_if *nif = net_if_get_default();
        if (nif != NULL) {
            (void) net_ipv4_igmp_leave(nif, &mcast);
        }
#else
        nros_mcast_membership_t mreq;
        memset(&mreq, 0, sizeof(mreq));
        mreq.imr_multiaddr = ((const struct sockaddr_in *) ai->ai_addr)->sin_addr;
        NROS_MCAST_IFACE(mreq).s_addr = htonl(INADDR_ANY);
        (void) zsock_setsockopt(sockrecv->fd, IPPROTO_IP, IP_DROP_MEMBERSHIP,
                                &mreq, sizeof(mreq));
#endif
    }
    if (lep != NULL && lep->iptcp != NULL) {
        struct zsock_addrinfo *laddr = lep->iptcp;
        k_free(laddr->ai_addr);
        k_free(laddr);
    }
    if (sockrecv != NULL && sockrecv->fd >= 0) {
        zsock_close(sockrecv->fd); sockrecv->fd = -1;
    }
    if (socksend != NULL && socksend->fd >= 0) {
        zsock_close(socksend->fd); socksend->fd = -1;
    }
}

size_t nros_platform_udp_mcast_read(const void *sock_raw, uint8_t *buf,
                                    size_t len, const void *lep_raw,
                                    void *addr) {
    if (sock_raw == NULL || lep_raw == NULL) return NROS_PLATFORM_NET_SOCKET_ERROR;
    const nros_zephyr_socket_t *sock = (const nros_zephyr_socket_t *) sock_raw;
    const nros_zephyr_endpoint_t *lep = (const nros_zephyr_endpoint_t *) lep_raw;
    if (lep->iptcp == NULL) return NROS_PLATFORM_NET_SOCKET_ERROR;
    struct zsock_addrinfo *ai = lep->iptcp;

    for (;;) {
        struct sockaddr_storage raddr;
        socklen_t replen = (socklen_t) sizeof(raddr);
        ssize_t rb = zsock_recvfrom(sock->fd, buf, len, 0,
                                    (struct sockaddr *) &raddr, &replen);
        if (rb < 0) return NROS_PLATFORM_NET_SOCKET_ERROR;

        int is_loopback = 0;
        if (ai->ai_family == AF_INET) {
            const struct sockaddr_in *local  = (const struct sockaddr_in *) ai->ai_addr;
            const struct sockaddr_in *remote = (const struct sockaddr_in *) &raddr;
            is_loopback = (local->sin_port == remote->sin_port
                           && local->sin_addr.s_addr == remote->sin_addr.s_addr);
        }
        if (is_loopback) continue;

        if (addr != NULL && ai->ai_family == AF_INET) {
            nros_z_slice_t *slice = (nros_z_slice_t *) addr;
            const struct sockaddr_in *remote = (const struct sockaddr_in *) &raddr;
            size_t ip_size   = sizeof(remote->sin_addr.s_addr);
            size_t port_size = sizeof(remote->sin_port);
            if (slice->len >= ip_size + port_size) {
                slice->len = ip_size + port_size;
                memcpy((uint8_t *) slice->start,
                       &remote->sin_addr.s_addr, ip_size);
                memcpy((uint8_t *) slice->start + ip_size,
                       &remote->sin_port, port_size);
            }
        }
        return (size_t) rb;
    }
}

size_t nros_platform_udp_mcast_read_exact(const void *sock_raw, uint8_t *buf,
                                          size_t len, const void *lep,
                                          void *addr) {
    size_t n = 0;
    while (n < len) {
        size_t r = nros_platform_udp_mcast_read(sock_raw, buf + n, len - n,
                                                 lep, addr);
        if (r == NROS_PLATFORM_NET_SOCKET_ERROR) return r;
        if (r == 0) return 0;
        n += r;
    }
    return n;
}

size_t nros_platform_udp_mcast_send(const void *sock, const uint8_t *buf,
                                    size_t len, const void *endpoint) {
    return nros_platform_udp_send(sock, buf, len, endpoint);
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

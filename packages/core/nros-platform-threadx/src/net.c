/*
 * Phase 121.6.threadx-c — ThreadX (NetX Duo BSD) implementation of
 * the canonical networking ABI declared in `<nros/platform_net.h>`.
 *
 * Backed by NetX Duo's BSD socket addon (`<nxd_bsd.h>`):
 *
 *   nx_bsd_socket / connect / bind / listen / accept /
 *   send / recv / sendto / recvfrom / setsockopt / fcntl /
 *   getaddrinfo / freeaddrinfo / close
 *
 * NetX Duo's BSD layer must be initialised once at application
 * startup via `bsd_initialize(...)`; the application provides the
 * NX_IP, NX_PACKET_POOL, byte pool, and the BSD task stack. This
 * port assumes that wiring is in place.
 *
 * Storage layouts mirror `nros-platform-threadx::net`:
 *   endpoint = { struct nx_bsd_addrinfo *iptcp; }
 *   socket   = { INT fd; }
 *
 * `SO_RCVTIMEO` on NetX Duo BSD takes `struct nx_bsd_timeval *`
 * (NOT `INT ms` as the Rust impl notes); we honour that here.
 */

#include <nros/platform_net.h>

#include <nx_api.h>
#include <nxd_bsd.h>
#include <tx_api.h>

#include <stddef.h>

typedef struct {
    struct nx_bsd_addrinfo *iptcp;
} nros_threadx_endpoint_t;

typedef struct {
    INT fd;
} nros_threadx_socket_t;

#define TRANSPORT_LEASE_MS 10000u

static void set_rcv_timeout(INT fd, uint32_t timeout_ms) {
    /* Phase 127.B.5 — same fix as nros-platform-posix: `timeout_ms == 0`
     * means "non-blocking" per the platform-net ABI (cooperative recv
     * loops poll + yield). NetX BSD's SO_RCVTIMEO with `{0, 0}` is
     * "block forever" — the inverse. Map timeout==0 to O_NONBLOCK via
     * fcntl so dust-dds's multicast_recv_loop yields cleanly instead
     * of blocking the cooperative async runtime. */
    if (timeout_ms == 0) {
        (void) nx_bsd_fcntl(fd, F_SETFL, O_NONBLOCK);
        return;
    }
    struct nx_bsd_timeval tv;
    tv.tv_sec  = (LONG) (timeout_ms / 1000u);
    tv.tv_usec = (LONG) ((timeout_ms % 1000u) * 1000u);
    (void) nx_bsd_setsockopt(fd, SOL_SOCKET, SO_RCVTIMEO, &tv, sizeof(tv));
}

static void set_int_opt(INT fd, INT level, INT optname, INT value) {
    (void) nx_bsd_setsockopt(fd, level, optname, &value, sizeof(INT));
}

static void apply_tcp_common_options(INT fd, uint32_t recv_timeout_ms) {
    set_rcv_timeout(fd, recv_timeout_ms);
    set_int_opt(fd, SOL_SOCKET, SO_KEEPALIVE, 1);
    set_int_opt(fd, IPPROTO_TCP, TCP_NODELAY, 1);
    struct nx_bsd_linger ling;
    ling.l_onoff  = 1;
    ling.l_linger = (INT) (TRANSPORT_LEASE_MS / 1000u);
    (void) nx_bsd_setsockopt(fd, SOL_SOCKET, SO_LINGER, &ling, sizeof(ling));
}

/* ---- TCP ---- */

int8_t nros_platform_tcp_create_endpoint(void *ep_raw,
                                         const uint8_t *address,
                                         const uint8_t *port) {
    if (ep_raw == NULL) return -1;
    nros_threadx_endpoint_t *ep = (nros_threadx_endpoint_t *) ep_raw;
    struct nx_bsd_addrinfo hints;
    memset(&hints, 0, sizeof(hints));
    hints.ai_family   = AF_UNSPEC;
    hints.ai_socktype = SOCK_STREAM;
    hints.ai_protocol = IPPROTO_TCP;
    return nx_bsd_getaddrinfo((const CHAR *) address, (const CHAR *) port,
                              &hints, &ep->iptcp) == 0 ? 0 : -1;
}

void nros_platform_tcp_free_endpoint(void *ep_raw) {
    if (ep_raw == NULL) return;
    nros_threadx_endpoint_t *ep = (nros_threadx_endpoint_t *) ep_raw;
    if (ep->iptcp != NULL) {
        nx_bsd_freeaddrinfo(ep->iptcp);
        ep->iptcp = NULL;
    }
}

int8_t nros_platform_tcp_open(void *sock_raw, const void *endpoint, uint32_t timeout_ms) {
    if (sock_raw == NULL || endpoint == NULL) return -1;
    nros_threadx_socket_t *sock = (nros_threadx_socket_t *) sock_raw;
    const nros_threadx_endpoint_t *ep = (const nros_threadx_endpoint_t *) endpoint;
    if (ep->iptcp == NULL) return -1;

    struct nx_bsd_addrinfo *first = ep->iptcp;
    INT fd = nx_bsd_socket(first->ai_family, first->ai_socktype, first->ai_protocol);
    if (fd < 0) return -1;
    sock->fd = fd;
    apply_tcp_common_options(fd, timeout_ms);

    for (struct nx_bsd_addrinfo *it = ep->iptcp; it != NULL; it = it->ai_next) {
        if (nx_bsd_connect(fd, it->ai_addr, it->ai_addrlen) == 0) {
            return 0;
        }
    }
    nx_bsd_soc_close(fd);
    sock->fd = -1;
    return -1;
}

int8_t nros_platform_tcp_listen(void *sock_raw, const void *endpoint) {
    if (sock_raw == NULL || endpoint == NULL) return -1;
    nros_threadx_socket_t *sock = (nros_threadx_socket_t *) sock_raw;
    const nros_threadx_endpoint_t *ep = (const nros_threadx_endpoint_t *) endpoint;
    if (ep->iptcp == NULL) return -1;

    struct nx_bsd_addrinfo *first = ep->iptcp;
    INT fd = nx_bsd_socket(first->ai_family, first->ai_socktype, first->ai_protocol);
    if (fd < 0) return -1;
    sock->fd = fd;
    set_int_opt(fd, SOL_SOCKET, SO_REUSEADDR, 1);
    apply_tcp_common_options(fd, 0);

    for (struct nx_bsd_addrinfo *it = ep->iptcp; it != NULL; it = it->ai_next) {
        if (nx_bsd_bind(fd, it->ai_addr, it->ai_addrlen) == 0
            && nx_bsd_listen(fd, 16) == 0) {
            return 0;
        }
    }
    nx_bsd_soc_close(fd);
    sock->fd = -1;
    return -1;
}

void nros_platform_tcp_close(void *sock_raw) {
    if (sock_raw == NULL) return;
    nros_threadx_socket_t *sock = (nros_threadx_socket_t *) sock_raw;
    if (sock->fd >= 0) {
        (void) nx_bsd_soc_close(sock->fd);
        sock->fd = -1;
    }
}

size_t nros_platform_tcp_read(const void *sock_raw, uint8_t *buf, size_t len) {
    if (sock_raw == NULL) return NROS_PLATFORM_NET_SOCKET_ERROR;
    const nros_threadx_socket_t *sock = (const nros_threadx_socket_t *) sock_raw;
    INT r = nx_bsd_recv(sock->fd, buf, (INT) len, 0);
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
    const nros_threadx_socket_t *sock = (const nros_threadx_socket_t *) sock_raw;
    INT r = nx_bsd_send(sock->fd, (const CHAR *) buf, (INT) len, 0);
    return r < 0 ? NROS_PLATFORM_NET_SOCKET_ERROR : (size_t) r;
}

/* ---- UDP unicast ---- */

int8_t nros_platform_udp_create_endpoint(void *ep_raw,
                                         const uint8_t *address,
                                         const uint8_t *port) {
    if (ep_raw == NULL) return -1;
    nros_threadx_endpoint_t *ep = (nros_threadx_endpoint_t *) ep_raw;
    struct nx_bsd_addrinfo hints;
    memset(&hints, 0, sizeof(hints));
    hints.ai_family   = AF_UNSPEC;
    hints.ai_socktype = SOCK_DGRAM;
    hints.ai_protocol = IPPROTO_UDP;
    return nx_bsd_getaddrinfo((const CHAR *) address, (const CHAR *) port,
                              &hints, &ep->iptcp) == 0 ? 0 : -1;
}

void nros_platform_udp_free_endpoint(void *ep_raw) {
    nros_platform_tcp_free_endpoint(ep_raw);
}

int8_t nros_platform_udp_open(void *sock_raw, const void *endpoint, uint32_t timeout_ms) {
    if (sock_raw == NULL || endpoint == NULL) return -1;
    nros_threadx_socket_t *sock = (nros_threadx_socket_t *) sock_raw;
    const nros_threadx_endpoint_t *ep = (const nros_threadx_endpoint_t *) endpoint;
    if (ep->iptcp == NULL) return -1;
    struct nx_bsd_addrinfo *ai = ep->iptcp;
    INT fd = nx_bsd_socket(ai->ai_family, ai->ai_socktype, ai->ai_protocol);
    if (fd < 0) return -1;
    sock->fd = fd;
    set_rcv_timeout(fd, timeout_ms);
    return 0;
}

int8_t nros_platform_udp_listen(void *sock_raw, const void *endpoint, uint32_t timeout_ms) {
    if (sock_raw == NULL || endpoint == NULL) return -1;
    nros_threadx_socket_t *sock = (nros_threadx_socket_t *) sock_raw;
    const nros_threadx_endpoint_t *ep = (const nros_threadx_endpoint_t *) endpoint;
    if (ep->iptcp == NULL) return -1;

    struct nx_bsd_addrinfo *first = ep->iptcp;
    INT fd = nx_bsd_socket(first->ai_family, first->ai_socktype, first->ai_protocol);
    if (fd < 0) return -1;
    sock->fd = fd;
    set_int_opt(fd, SOL_SOCKET, SO_REUSEADDR, 1);
    set_rcv_timeout(fd, timeout_ms);

    for (struct nx_bsd_addrinfo *it = ep->iptcp; it != NULL; it = it->ai_next) {
        if (nx_bsd_bind(fd, it->ai_addr, it->ai_addrlen) == 0) {
            return 0;
        }
    }
    nx_bsd_soc_close(fd);
    sock->fd = -1;
    return -1;
}

void nros_platform_udp_close(void *sock_raw) {
    if (sock_raw == NULL) return;
    nros_threadx_socket_t *sock = (nros_threadx_socket_t *) sock_raw;
    if (sock->fd >= 0) {
        (void) nx_bsd_soc_close(sock->fd);
        sock->fd = -1;
    }
}

size_t nros_platform_udp_read(const void *sock_raw, uint8_t *buf, size_t len) {
    if (sock_raw == NULL) return NROS_PLATFORM_NET_SOCKET_ERROR;
    const nros_threadx_socket_t *sock = (const nros_threadx_socket_t *) sock_raw;
    struct nx_bsd_sockaddr raddr;
    INT addrlen = (INT) sizeof(raddr);
    INT r = nx_bsd_recvfrom(sock->fd, (CHAR *) buf, (INT) len, 0, &raddr, &addrlen);
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
    const nros_threadx_socket_t *sock = (const nros_threadx_socket_t *) sock_raw;
    const nros_threadx_endpoint_t *ep = (const nros_threadx_endpoint_t *) endpoint;
    if (ep->iptcp == NULL) return NROS_PLATFORM_NET_SOCKET_ERROR;
    struct nx_bsd_addrinfo *ai = ep->iptcp;
    INT r = nx_bsd_sendto(sock->fd, (CHAR *) buf, (INT) len, 0,
                          ai->ai_addr, (INT) ai->ai_addrlen);
    return r < 0 ? NROS_PLATFORM_NET_SOCKET_ERROR : (size_t) r;
}

void nros_platform_udp_set_recv_timeout(const void *sock_raw, uint32_t timeout_ms) {
    if (sock_raw == NULL) return;
    const nros_threadx_socket_t *sock = (const nros_threadx_socket_t *) sock_raw;
    if (timeout_ms == 0) {
        /* Non-blocking via fcntl. */
        (void) nx_bsd_fcntl(sock->fd, F_SETFL, O_NONBLOCK);
        return;
    }
    set_rcv_timeout(sock->fd, timeout_ms);
}

/* ---- UDP multicast ----
 *
 * NetX Duo's BSD addon exposes IP_ADD_MEMBERSHIP / IP_DROP_MEMBERSHIP
 * via nx_bsd_setsockopt + struct nx_bsd_ip_mreq. iface resolution
 * skipped — NetX maps interfaces by index via NX_IP_INTERFACE
 * structures; applications that need iface-pinned multicast can
 * post-set IP_MULTICAST_IF.
 */

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
    nros_threadx_socket_t *sock = (nros_threadx_socket_t *) sock_raw;
    nros_threadx_endpoint_t *lep = (nros_threadx_endpoint_t *) lep_raw;
    const nros_threadx_endpoint_t *ep = (const nros_threadx_endpoint_t *) endpoint;
    if (ep->iptcp == NULL) return -1;
    struct nx_bsd_addrinfo *ai = ep->iptcp;

    INT fd = nx_bsd_socket(ai->ai_family, ai->ai_socktype, ai->ai_protocol);
    if (fd < 0) return -1;
    sock->fd = fd;
    set_rcv_timeout(fd, timeout_ms);

    struct nx_bsd_sockaddr_in addr;
    memset(&addr, 0, sizeof(addr));
    addr.sin_family      = AF_INET;
    addr.sin_port        = ((const struct nx_bsd_sockaddr_in *) ai->ai_addr)->sin_port;
    addr.sin_addr.s_addr = 0;  /* INADDR_ANY */
    if (nx_bsd_bind(fd, (struct nx_bsd_sockaddr *) &addr, sizeof(addr)) < 0) {
        nx_bsd_soc_close(fd); sock->fd = -1; return -1;
    }

    /* Stash a copy of the bound local address in lep for read-side
     * loopback filtering. */
    struct nx_bsd_sockaddr *lsockaddr =
        (struct nx_bsd_sockaddr *) nros_platform_alloc(sizeof(addr));
    if (lsockaddr == NULL) {
        nx_bsd_soc_close(fd); sock->fd = -1; return -1;
    }
    memcpy(lsockaddr, &addr, sizeof(addr));

    struct nx_bsd_addrinfo *laddr =
        (struct nx_bsd_addrinfo *) nros_platform_alloc(sizeof(struct nx_bsd_addrinfo));
    if (laddr == NULL) {
        nros_platform_dealloc(lsockaddr);
        nx_bsd_soc_close(fd); sock->fd = -1; return -1;
    }
    memset(laddr, 0, sizeof(*laddr));
    laddr->ai_family   = ai->ai_family;
    laddr->ai_socktype = ai->ai_socktype;
    laddr->ai_protocol = ai->ai_protocol;
    laddr->ai_addrlen  = (INT) sizeof(addr);
    laddr->ai_addr     = lsockaddr;
    lep->iptcp = laddr;
    return 0;
}

int8_t nros_platform_udp_mcast_listen(void *sock_raw, const void *endpoint,
                                      uint32_t timeout_ms,
                                      const uint8_t *iface,
                                      const uint8_t *join) {
    /* Phase 127.B.5 — same fix as nros-platform-posix: use the `join`
     * dotted-quad (e.g. "239.255.0.1") for `imr_multiaddr`, NOT the
     * local endpoint's sin_addr which is always 0.0.0.0 on the
     * dust-dds SPDP bind path (`create_endpoint("0.0.0.0", port)`).
     * Joining 0.0.0.0 silently fails (NetX adds a sentinel grp entry
     * that never matches any real incoming mcast frame, so SPDP
     * discovery silently fails). */
    (void) iface;
    if (sock_raw == NULL || endpoint == NULL || join == NULL) return -1;
    nros_threadx_socket_t *sock = (nros_threadx_socket_t *) sock_raw;
    const nros_threadx_endpoint_t *ep = (const nros_threadx_endpoint_t *) endpoint;
    if (ep->iptcp == NULL) return -1;
    struct nx_bsd_addrinfo *ai = ep->iptcp;

    INT fd = nx_bsd_socket(ai->ai_family, ai->ai_socktype, ai->ai_protocol);
    if (fd < 0) return -1;
    sock->fd = fd;
    set_rcv_timeout(fd, timeout_ms);
    set_int_opt(fd, SOL_SOCKET, SO_REUSEADDR, 1);

    struct nx_bsd_sockaddr_in addr;
    memset(&addr, 0, sizeof(addr));
    addr.sin_family      = AF_INET;
    addr.sin_port        = ((const struct nx_bsd_sockaddr_in *) ai->ai_addr)->sin_port;
    addr.sin_addr.s_addr = 0;  /* INADDR_ANY */
    if (nx_bsd_bind(fd, (struct nx_bsd_sockaddr *) &addr, sizeof(addr)) < 0) {
        nx_bsd_soc_close(fd); sock->fd = -1; return -1;
    }

    struct nx_bsd_ip_mreq mreq;
    memset(&mreq, 0, sizeof(mreq));
    mreq.imr_multiaddr.s_addr = nx_bsd_inet_addr((CHAR *) join);
    if (mreq.imr_multiaddr.s_addr == 0xffffffffu /* INADDR_NONE */) {
        nx_bsd_soc_close(fd); sock->fd = -1; return -1;
    }
    mreq.imr_interface.s_addr = 0;  /* INADDR_ANY */
    if (nx_bsd_setsockopt(fd, IPPROTO_IP, IP_ADD_MEMBERSHIP,
                          &mreq, sizeof(mreq)) < 0) {
        nx_bsd_soc_close(fd); sock->fd = -1; return -1;
    }
    return 0;
}

void nros_platform_udp_mcast_close(void *sockrecv_raw, void *socksend_raw,
                                   const void *rep_raw, const void *lep_raw) {
    nros_threadx_socket_t *sockrecv = (nros_threadx_socket_t *) sockrecv_raw;
    nros_threadx_socket_t *socksend = (nros_threadx_socket_t *) socksend_raw;
    const nros_threadx_endpoint_t *rep = (const nros_threadx_endpoint_t *) rep_raw;
    const nros_threadx_endpoint_t *lep = (const nros_threadx_endpoint_t *) lep_raw;

    if (sockrecv != NULL && sockrecv->fd >= 0 && rep != NULL && rep->iptcp != NULL) {
        struct nx_bsd_addrinfo *ai = rep->iptcp;
        struct nx_bsd_ip_mreq mreq;
        memset(&mreq, 0, sizeof(mreq));
        mreq.imr_multiaddr =
            ((const struct nx_bsd_sockaddr_in *) ai->ai_addr)->sin_addr;
        mreq.imr_interface.s_addr = 0;
        (void) nx_bsd_setsockopt(sockrecv->fd, IPPROTO_IP, IP_DROP_MEMBERSHIP,
                                 &mreq, sizeof(mreq));
    }
    if (lep != NULL && lep->iptcp != NULL) {
        struct nx_bsd_addrinfo *laddr = lep->iptcp;
        nros_platform_dealloc(laddr->ai_addr);
        nros_platform_dealloc(laddr);
    }
    if (sockrecv != NULL && sockrecv->fd >= 0) {
        (void) nx_bsd_soc_close(sockrecv->fd); sockrecv->fd = -1;
    }
    if (socksend != NULL && socksend->fd >= 0) {
        (void) nx_bsd_soc_close(socksend->fd); socksend->fd = -1;
    }
}

size_t nros_platform_udp_mcast_read(const void *sock_raw, uint8_t *buf,
                                    size_t len, const void *lep_raw,
                                    void *addr) {
    if (sock_raw == NULL || lep_raw == NULL) return NROS_PLATFORM_NET_SOCKET_ERROR;
    const nros_threadx_socket_t *sock = (const nros_threadx_socket_t *) sock_raw;
    const nros_threadx_endpoint_t *lep = (const nros_threadx_endpoint_t *) lep_raw;
    if (lep->iptcp == NULL) return NROS_PLATFORM_NET_SOCKET_ERROR;
    struct nx_bsd_addrinfo *ai = lep->iptcp;

    for (;;) {
        struct nx_bsd_sockaddr_in raddr;
        INT replen = (INT) sizeof(raddr);
        INT rb = nx_bsd_recvfrom(sock->fd, (CHAR *) buf, (INT) len, 0,
                                 (struct nx_bsd_sockaddr *) &raddr, &replen);
        if (rb < 0) return NROS_PLATFORM_NET_SOCKET_ERROR;

        const struct nx_bsd_sockaddr_in *local =
            (const struct nx_bsd_sockaddr_in *) ai->ai_addr;
        if (local->sin_port == raddr.sin_port
            && local->sin_addr.s_addr == raddr.sin_addr.s_addr) {
            continue;  /* loopback drop */
        }

        if (addr != NULL) {
            nros_z_slice_t *slice = (nros_z_slice_t *) addr;
            size_t ip_size   = sizeof(raddr.sin_addr.s_addr);
            size_t port_size = sizeof(raddr.sin_port);
            if (slice->len >= ip_size + port_size) {
                slice->len = ip_size + port_size;
                memcpy((uint8_t *) slice->start,
                       &raddr.sin_addr.s_addr, ip_size);
                memcpy((uint8_t *) slice->start + ip_size,
                       &raddr.sin_port, port_size);
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
    const nros_threadx_socket_t *sock = (const nros_threadx_socket_t *) sock_raw;
    /* NetX Duo BSD's fcntl supports F_SETFL with O_NONBLOCK directly. */
    return nx_bsd_fcntl(sock->fd, F_SETFL, O_NONBLOCK) == 0 ? 0 : -1;
}

int8_t nros_platform_socket_accept(const void *in_raw, void *out_raw) {
    if (in_raw == NULL || out_raw == NULL) return -1;
    const nros_threadx_socket_t *in = (const nros_threadx_socket_t *) in_raw;
    nros_threadx_socket_t *out = (nros_threadx_socket_t *) out_raw;
    struct nx_bsd_sockaddr naddr;
    INT nlen = (INT) sizeof(naddr);
    INT con = nx_bsd_accept(in->fd, &naddr, &nlen);
    if (con < 0) return -1;

    struct nx_bsd_timeval tv = { .tv_sec = 10, .tv_usec = 0 };
    (void) nx_bsd_setsockopt(con, SOL_SOCKET, SO_RCVTIMEO, &tv, sizeof(tv));
    INT one = 1;
    (void) nx_bsd_setsockopt(con, SOL_SOCKET, SO_KEEPALIVE, &one, sizeof(one));
    (void) nx_bsd_setsockopt(con, IPPROTO_TCP, TCP_NODELAY, &one, sizeof(one));
    out->fd = con;
    return 0;
}

void nros_platform_socket_close(void *sock_raw) {
    nros_platform_tcp_close(sock_raw);
}

int8_t nros_platform_socket_wait_event(void *peers, void *mutex) {
    (void) peers; (void) mutex;
    tx_thread_relinquish();
    return 0;
}

/* ---- Network poll ---- */

void nros_platform_network_poll(void) {
    /* NetX runs in dedicated threads. No user-space poll. */
}

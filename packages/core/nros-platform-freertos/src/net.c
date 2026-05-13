/*
 * Phase 121.6.freertos-c — FreeRTOS implementation of the canonical
 * networking ABI declared in `<nros/platform_net.h>`.
 *
 * Backed by lwIP's BSD socket API (`<lwip/sockets.h>` +
 * `<lwip/netdb.h>`). lwIP must be built with `LWIP_SOCKET=1` and
 * `LWIP_DNS=1` (for getaddrinfo); the application's lwIP options
 * header (`lwipopts.h`) controls both.
 *
 * Storage layouts match the Rust `nros-platform-freertos::net` impl:
 *   endpoint = { struct addrinfo *iptcp; }   — 1 pointer
 *   socket   = { int fd; }                   — 4 bytes
 *
 * UDP multicast is stubbed (returns -1 / (size_t) -1); a full
 * IP_ADD_MEMBERSHIP path lands as a follow-up.
 */

#include <nros/platform_net.h>

#include <lwip/sockets.h>
#include <lwip/netdb.h>

#include <stddef.h>
#include <stdlib.h>
#include <string.h>

typedef struct {
    struct addrinfo *iptcp;
} nros_freertos_endpoint_t;

typedef struct {
    int fd;
} nros_freertos_socket_t;

#define TRANSPORT_LEASE_MS 10000u

static void set_rcv_timeout(int fd, uint32_t timeout_ms) {
    struct timeval tv = {
        .tv_sec  = (long) (timeout_ms / 1000u),
        .tv_usec = (long) ((timeout_ms % 1000u) * 1000u),
    };
    (void) lwip_setsockopt(fd, SOL_SOCKET, SO_RCVTIMEO, &tv, sizeof(tv));
}

static void set_int_opt(int fd, int level, int optname, int value) {
    (void) lwip_setsockopt(fd, level, optname, &value, sizeof(int));
}

static void apply_tcp_common_options(int fd, uint32_t recv_timeout_ms) {
    set_rcv_timeout(fd, recv_timeout_ms);
    set_int_opt(fd, SOL_SOCKET, SO_KEEPALIVE, 1);
    set_int_opt(fd, IPPROTO_TCP, TCP_NODELAY, 1);
    struct linger ling = { .l_onoff = 1, .l_linger = (int) (TRANSPORT_LEASE_MS / 1000u) };
    (void) lwip_setsockopt(fd, SOL_SOCKET, SO_LINGER, &ling, sizeof(ling));
}

/* ---- TCP ---- */

int8_t nros_platform_tcp_create_endpoint(void *ep_raw,
                                         const uint8_t *address,
                                         const uint8_t *port) {
    if (ep_raw == NULL) return -1;
    nros_freertos_endpoint_t *ep = (nros_freertos_endpoint_t *) ep_raw;
    struct addrinfo hints = {0};
    hints.ai_family   = AF_UNSPEC;
    hints.ai_socktype = SOCK_STREAM;
    hints.ai_protocol = IPPROTO_TCP;
    int rc = lwip_getaddrinfo((const char *) address, (const char *) port, &hints, &ep->iptcp);
    return rc == 0 ? 0 : -1;
}

void nros_platform_tcp_free_endpoint(void *ep_raw) {
    if (ep_raw == NULL) return;
    nros_freertos_endpoint_t *ep = (nros_freertos_endpoint_t *) ep_raw;
    if (ep->iptcp != NULL) {
        lwip_freeaddrinfo(ep->iptcp);
        ep->iptcp = NULL;
    }
}

int8_t nros_platform_tcp_open(void *sock_raw, const void *endpoint, uint32_t timeout_ms) {
    if (sock_raw == NULL || endpoint == NULL) return -1;
    nros_freertos_socket_t *sock = (nros_freertos_socket_t *) sock_raw;
    const nros_freertos_endpoint_t *ep = (const nros_freertos_endpoint_t *) endpoint;
    if (ep->iptcp == NULL) return -1;

    struct addrinfo *first = ep->iptcp;
    int fd = lwip_socket(first->ai_family, first->ai_socktype, first->ai_protocol);
    if (fd < 0) return -1;
    sock->fd = fd;
    apply_tcp_common_options(fd, timeout_ms);

    for (struct addrinfo *it = ep->iptcp; it != NULL; it = it->ai_next) {
        if (lwip_connect(fd, it->ai_addr, it->ai_addrlen) == 0) {
            return 0;
        }
    }
    lwip_close(fd);
    sock->fd = -1;
    return -1;
}

int8_t nros_platform_tcp_listen(void *sock_raw, const void *endpoint) {
    if (sock_raw == NULL || endpoint == NULL) return -1;
    nros_freertos_socket_t *sock = (nros_freertos_socket_t *) sock_raw;
    const nros_freertos_endpoint_t *ep = (const nros_freertos_endpoint_t *) endpoint;
    if (ep->iptcp == NULL) return -1;

    struct addrinfo *first = ep->iptcp;
    int fd = lwip_socket(first->ai_family, first->ai_socktype, first->ai_protocol);
    if (fd < 0) return -1;
    sock->fd = fd;
    set_int_opt(fd, SOL_SOCKET, SO_REUSEADDR, 1);
    apply_tcp_common_options(fd, 0);

    for (struct addrinfo *it = ep->iptcp; it != NULL; it = it->ai_next) {
        if (lwip_bind(fd, it->ai_addr, it->ai_addrlen) == 0
            && lwip_listen(fd, 128) == 0) {
            return 0;
        }
    }
    lwip_close(fd);
    sock->fd = -1;
    return -1;
}

void nros_platform_tcp_close(void *sock_raw) {
    if (sock_raw == NULL) return;
    nros_freertos_socket_t *sock = (nros_freertos_socket_t *) sock_raw;
    if (sock->fd >= 0) {
        lwip_shutdown(sock->fd, SHUT_RDWR);
        lwip_close(sock->fd);
        sock->fd = -1;
    }
}

size_t nros_platform_tcp_read(const void *sock_raw, uint8_t *buf, size_t len) {
    if (sock_raw == NULL) return NROS_PLATFORM_NET_SOCKET_ERROR;
    const nros_freertos_socket_t *sock = (const nros_freertos_socket_t *) sock_raw;
    int r = lwip_recv(sock->fd, buf, len, 0);
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
    const nros_freertos_socket_t *sock = (const nros_freertos_socket_t *) sock_raw;
    int r = lwip_send(sock->fd, buf, len, 0);
    return r < 0 ? NROS_PLATFORM_NET_SOCKET_ERROR : (size_t) r;
}

/* ---- UDP unicast ---- */

int8_t nros_platform_udp_create_endpoint(void *ep_raw,
                                         const uint8_t *address,
                                         const uint8_t *port) {
    if (ep_raw == NULL) return -1;
    nros_freertos_endpoint_t *ep = (nros_freertos_endpoint_t *) ep_raw;
    struct addrinfo hints = {0};
    hints.ai_family   = AF_UNSPEC;
    hints.ai_socktype = SOCK_DGRAM;
    hints.ai_protocol = IPPROTO_UDP;
    int rc = lwip_getaddrinfo((const char *) address, (const char *) port, &hints, &ep->iptcp);
    return rc == 0 ? 0 : -1;
}

void nros_platform_udp_free_endpoint(void *ep_raw) {
    nros_platform_tcp_free_endpoint(ep_raw);
}

int8_t nros_platform_udp_open(void *sock_raw, const void *endpoint, uint32_t timeout_ms) {
    if (sock_raw == NULL || endpoint == NULL) return -1;
    nros_freertos_socket_t *sock = (nros_freertos_socket_t *) sock_raw;
    const nros_freertos_endpoint_t *ep = (const nros_freertos_endpoint_t *) endpoint;
    if (ep->iptcp == NULL) return -1;
    struct addrinfo *ai = ep->iptcp;
    int fd = lwip_socket(ai->ai_family, ai->ai_socktype, ai->ai_protocol);
    if (fd < 0) return -1;
    sock->fd = fd;
    set_rcv_timeout(fd, timeout_ms);
    return 0;
}

int8_t nros_platform_udp_listen(void *sock_raw, const void *endpoint, uint32_t timeout_ms) {
    if (sock_raw == NULL || endpoint == NULL) return -1;
    nros_freertos_socket_t *sock = (nros_freertos_socket_t *) sock_raw;
    const nros_freertos_endpoint_t *ep = (const nros_freertos_endpoint_t *) endpoint;
    if (ep->iptcp == NULL) return -1;

    struct addrinfo *first = ep->iptcp;
    int fd = lwip_socket(first->ai_family, first->ai_socktype, first->ai_protocol);
    if (fd < 0) return -1;
    sock->fd = fd;
    set_int_opt(fd, SOL_SOCKET, SO_REUSEADDR, 1);
    set_rcv_timeout(fd, timeout_ms);

    for (struct addrinfo *it = ep->iptcp; it != NULL; it = it->ai_next) {
        if (lwip_bind(fd, it->ai_addr, it->ai_addrlen) == 0) {
            return 0;
        }
    }
    lwip_close(fd);
    sock->fd = -1;
    return -1;
}

void nros_platform_udp_close(void *sock_raw) {
    if (sock_raw == NULL) return;
    nros_freertos_socket_t *sock = (nros_freertos_socket_t *) sock_raw;
    if (sock->fd >= 0) {
        lwip_close(sock->fd);
        sock->fd = -1;
    }
}

size_t nros_platform_udp_read(const void *sock_raw, uint8_t *buf, size_t len) {
    if (sock_raw == NULL) return NROS_PLATFORM_NET_SOCKET_ERROR;
    const nros_freertos_socket_t *sock = (const nros_freertos_socket_t *) sock_raw;
    struct sockaddr_storage raddr;
    socklen_t addrlen = (socklen_t) sizeof(raddr);
    int r = lwip_recvfrom(sock->fd, buf, len, 0,
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
    const nros_freertos_socket_t *sock = (const nros_freertos_socket_t *) sock_raw;
    const nros_freertos_endpoint_t *ep = (const nros_freertos_endpoint_t *) endpoint;
    if (ep->iptcp == NULL) return NROS_PLATFORM_NET_SOCKET_ERROR;
    struct addrinfo *ai = ep->iptcp;
    int r = lwip_sendto(sock->fd, buf, len, 0, ai->ai_addr, ai->ai_addrlen);
    return r < 0 ? NROS_PLATFORM_NET_SOCKET_ERROR : (size_t) r;
}

void nros_platform_udp_set_recv_timeout(const void *sock_raw, uint32_t timeout_ms) {
    if (sock_raw == NULL) return;
    const nros_freertos_socket_t *sock = (const nros_freertos_socket_t *) sock_raw;
    if (timeout_ms == 0) {
        int flags = lwip_fcntl(sock->fd, F_GETFL, 0);
        if (flags >= 0) {
            (void) lwip_fcntl(sock->fd, F_SETFL, flags | O_NONBLOCK);
        }
        return;
    }
    set_rcv_timeout(sock->fd, timeout_ms);
}

/* ---- UDP multicast ----
 *
 * lwIP provides IP_ADD_MEMBERSHIP + struct ip_mreq via
 * <lwip/sockets.h>. lwIP doesn't ship `getifaddrs` in default
 * builds; the `iface` parameter from the canonical ABI is
 * therefore advisory on this port — we always bind to INADDR_ANY
 * and accept membership on any interface. Applications that need
 * iface-pinned multicast can post-process via
 * lwip_setsockopt(IP_MULTICAST_IF) after open.
 *
 * ZSlice layout mirrors the Rust ZSlice in nros-platform-posix.
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
    nros_freertos_socket_t *sock = (nros_freertos_socket_t *) sock_raw;
    nros_freertos_endpoint_t *lep = (nros_freertos_endpoint_t *) lep_raw;
    const nros_freertos_endpoint_t *ep = (const nros_freertos_endpoint_t *) endpoint;
    if (ep->iptcp == NULL) return -1;
    struct addrinfo *ai = ep->iptcp;

    int fd = lwip_socket(ai->ai_family, ai->ai_socktype, ai->ai_protocol);
    if (fd < 0) return -1;
    sock->fd = fd;
    set_rcv_timeout(fd, timeout_ms);

    /* Bind to INADDR_ANY on the multicast port. Application can
     * override IP_MULTICAST_IF if it needs a specific iface. */
    struct sockaddr_in addr;
    memset(&addr, 0, sizeof(addr));
    addr.sin_family      = AF_INET;
    addr.sin_port        = ((const struct sockaddr_in *) ai->ai_addr)->sin_port;
    addr.sin_addr.s_addr = htonl(INADDR_ANY);
    if (lwip_bind(fd, (struct sockaddr *) &addr, sizeof(addr)) < 0) {
        lwip_close(fd); sock->fd = -1; return -1;
    }

    /* Wrap a copy of the bound local sockaddr in an addrinfo so
     * mcast_read can do loopback filtering. */
    socklen_t bound_len = (socklen_t) sizeof(addr);
    (void) lwip_getsockname(fd, (struct sockaddr *) &addr, &bound_len);

    struct sockaddr *lsockaddr = (struct sockaddr *) malloc(sizeof(addr));
    if (lsockaddr == NULL) {
        lwip_close(fd); sock->fd = -1; return -1;
    }
    memcpy(lsockaddr, &addr, sizeof(addr));

    struct addrinfo *laddr = (struct addrinfo *) calloc(1, sizeof(struct addrinfo));
    if (laddr == NULL) {
        free(lsockaddr); lwip_close(fd); sock->fd = -1; return -1;
    }
    laddr->ai_flags    = 0;
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
    (void) iface; (void) join;
    if (sock_raw == NULL || endpoint == NULL) return -1;
    nros_freertos_socket_t *sock = (nros_freertos_socket_t *) sock_raw;
    const nros_freertos_endpoint_t *ep = (const nros_freertos_endpoint_t *) endpoint;
    if (ep->iptcp == NULL) return -1;
    struct addrinfo *ai = ep->iptcp;

    int fd = lwip_socket(ai->ai_family, ai->ai_socktype, ai->ai_protocol);
    if (fd < 0) return -1;
    sock->fd = fd;
    set_rcv_timeout(fd, timeout_ms);
    set_int_opt(fd, SOL_SOCKET, SO_REUSEADDR, 1);

    /* Bind INADDR_ANY:mcast_port. */
    struct sockaddr_in addr;
    memset(&addr, 0, sizeof(addr));
    addr.sin_family      = AF_INET;
    addr.sin_port        = ((const struct sockaddr_in *) ai->ai_addr)->sin_port;
    addr.sin_addr.s_addr = htonl(INADDR_ANY);
    if (lwip_bind(fd, (struct sockaddr *) &addr, sizeof(addr)) < 0) {
        lwip_close(fd); sock->fd = -1; return -1;
    }

    /* IP_ADD_MEMBERSHIP on default interface. */
    struct ip_mreq mreq;
    memset(&mreq, 0, sizeof(mreq));
    mreq.imr_multiaddr = ((const struct sockaddr_in *) ai->ai_addr)->sin_addr;
    mreq.imr_interface.s_addr = htonl(INADDR_ANY);
    if (lwip_setsockopt(fd, IPPROTO_IP, IP_ADD_MEMBERSHIP,
                        &mreq, sizeof(mreq)) < 0) {
        lwip_close(fd); sock->fd = -1; return -1;
    }
    return 0;
}

void nros_platform_udp_mcast_close(void *sockrecv_raw, void *socksend_raw,
                                   const void *rep_raw, const void *lep_raw) {
    nros_freertos_socket_t *sockrecv = (nros_freertos_socket_t *) sockrecv_raw;
    nros_freertos_socket_t *socksend = (nros_freertos_socket_t *) socksend_raw;
    const nros_freertos_endpoint_t *rep = (const nros_freertos_endpoint_t *) rep_raw;
    const nros_freertos_endpoint_t *lep = (const nros_freertos_endpoint_t *) lep_raw;

    if (sockrecv != NULL && sockrecv->fd >= 0 && rep != NULL && rep->iptcp != NULL) {
        struct addrinfo *ai = rep->iptcp;
        struct ip_mreq mreq;
        memset(&mreq, 0, sizeof(mreq));
        mreq.imr_multiaddr = ((const struct sockaddr_in *) ai->ai_addr)->sin_addr;
        mreq.imr_interface.s_addr = htonl(INADDR_ANY);
        (void) lwip_setsockopt(sockrecv->fd, IPPROTO_IP, IP_DROP_MEMBERSHIP,
                               &mreq, sizeof(mreq));
    }
    if (lep != NULL && lep->iptcp != NULL) {
        struct addrinfo *laddr = lep->iptcp;
        free(laddr->ai_addr);
        free(laddr);
    }
    if (sockrecv != NULL && sockrecv->fd >= 0) {
        lwip_close(sockrecv->fd); sockrecv->fd = -1;
    }
    if (socksend != NULL && socksend->fd >= 0) {
        lwip_close(socksend->fd); socksend->fd = -1;
    }
}

size_t nros_platform_udp_mcast_read(const void *sock_raw, uint8_t *buf,
                                    size_t len, const void *lep_raw,
                                    void *addr) {
    if (sock_raw == NULL || lep_raw == NULL) return NROS_PLATFORM_NET_SOCKET_ERROR;
    const nros_freertos_socket_t *sock = (const nros_freertos_socket_t *) sock_raw;
    const nros_freertos_endpoint_t *lep = (const nros_freertos_endpoint_t *) lep_raw;
    if (lep->iptcp == NULL) return NROS_PLATFORM_NET_SOCKET_ERROR;
    struct addrinfo *ai = lep->iptcp;

    for (;;) {
        struct sockaddr_storage raddr;
        socklen_t replen = (socklen_t) sizeof(raddr);
        int rb = lwip_recvfrom(sock->fd, buf, len, 0,
                               (struct sockaddr *) &raddr, &replen);
        if (rb < 0) return NROS_PLATFORM_NET_SOCKET_ERROR;

        /* Loopback filter — drop packets whose source == local. */
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
    /* Kernel routes by destination address — identical to unicast. */
    return nros_platform_udp_send(sock, buf, len, endpoint);
}

/* ---- Socket helpers ---- */

int8_t nros_platform_socket_set_non_blocking(const void *sock_raw) {
    if (sock_raw == NULL) return -1;
    const nros_freertos_socket_t *sock = (const nros_freertos_socket_t *) sock_raw;
    int flags = lwip_fcntl(sock->fd, F_GETFL, 0);
    if (flags < 0) return -1;
    if (lwip_fcntl(sock->fd, F_SETFL, flags | O_NONBLOCK) < 0) return -1;
    return 0;
}

int8_t nros_platform_socket_accept(const void *in_raw, void *out_raw) {
    if (in_raw == NULL || out_raw == NULL) return -1;
    const nros_freertos_socket_t *in = (const nros_freertos_socket_t *) in_raw;
    nros_freertos_socket_t *out = (nros_freertos_socket_t *) out_raw;
    struct sockaddr_storage naddr;
    socklen_t nlen = (socklen_t) sizeof(naddr);
    int con = lwip_accept(in->fd, (struct sockaddr *) &naddr, &nlen);
    if (con < 0) return -1;
    struct timeval tv = { .tv_sec = 10, .tv_usec = 0 };
    (void) lwip_setsockopt(con, SOL_SOCKET, SO_RCVTIMEO, &tv, sizeof(tv));
    set_int_opt(con, SOL_SOCKET, SO_KEEPALIVE, 1);
    set_int_opt(con, IPPROTO_TCP, TCP_NODELAY, 1);
    out->fd = con;
    return 0;
}

void nros_platform_socket_close(void *sock_raw) {
    nros_platform_tcp_close(sock_raw);
}

int8_t nros_platform_socket_wait_event(void *peers, void *mutex) {
    (void) peers; (void) mutex;
    /* Cooperative yield via FreeRTOS scheduler. */
    extern void vTaskDelay(uint32_t);
    vTaskDelay(1);
    return 0;
}

/* ---- Network poll ---- */

void nros_platform_network_poll(void) {
    /* lwIP runs its TCP/IP thread internally — no user-space poll. */
}

/*
 * Phase 121.6.posix-c — POSIX implementation of the canonical
 * networking ABI declared in `<nros/platform_net.h>`.
 *
 * Mirrors `nros-platform-posix/src/net.rs` byte-for-byte:
 *
 *   - Endpoint storage = `{ struct addrinfo *iptcp; }` (one pointer)
 *     — same layout as zenoh-pico's `_z_sys_net_endpoint_t` (unix.h)
 *     with TLS disabled.
 *   - Socket storage   = `{ int fd; }` (4 bytes) — same as zenoh-pico
 *     `_z_sys_net_socket_t` with TLS disabled.
 *
 * The two layouts MUST stay byte-equal because zenoh-pico's transport
 * code passes the same buffers through whichever provider is linked.
 *
 * Coverage: full TCP + UDP unicast + socket helpers + network_poll.
 * UDP multicast is stubbed (returns -1 / `(size_t) -1`) — the full
 * getifaddrs / IP_ADD_MEMBERSHIP plumbing lands as a follow-up.
 */

#define _POSIX_C_SOURCE 200809L
#define _DEFAULT_SOURCE

#include <nros/platform_net.h>

#include <arpa/inet.h>
#include <errno.h>
#include <fcntl.h>
#include <netdb.h>
#include <netinet/in.h>
#include <netinet/tcp.h>
#include <sched.h>
#include <stddef.h>
#include <stdlib.h>
#include <string.h>
#include <sys/socket.h>
#include <sys/time.h>
#include <unistd.h>

/* ---- Storage layouts ---- */

typedef struct {
    struct addrinfo *iptcp;
} nros_posix_endpoint_t;

typedef struct {
    int fd;
} nros_posix_socket_t;

/* zenoh-pico's lease used as SO_LINGER fallback. */
#define NROS_POSIX_TRANSPORT_LEASE_MS 10000u

/* ---- Internal helpers ---- */

static void set_recv_timeout_ms(int fd, uint32_t timeout_ms) {
    /* Phase 127.B.5 — `timeout_ms == 0` means "non-blocking" per the
     * platform-net ABI (cooperative recv loops poll + yield). POSIX
     * `SO_RCVTIMEO` with `{0, 0}` is the OPPOSITE — it means "block
     * forever". Map timeout==0 to O_NONBLOCK so the dust-dds recv
     * loops actually yield to the async runtime instead of starving
     * `Executor::open` / `create_publisher`. */
    if (timeout_ms == 0) {
        int flags = fcntl(fd, F_GETFL, 0);
        if (flags >= 0) {
            (void) fcntl(fd, F_SETFL, flags | O_NONBLOCK);
        }
        return;
    }
    struct timeval tv = {
        .tv_sec  = (time_t)      (timeout_ms / 1000u),
        .tv_usec = (suseconds_t) ((timeout_ms % 1000u) * 1000u),
    };
    (void) setsockopt(fd, SOL_SOCKET, SO_RCVTIMEO, &tv, sizeof(tv));
}

static void set_int_option(int fd, int level, int optname, int value) {
    (void) setsockopt(fd, level, optname, &value, sizeof(int));
}

static void apply_tcp_common_options(int fd, uint32_t recv_timeout_ms) {
    set_recv_timeout_ms(fd, recv_timeout_ms);
    set_int_option(fd, SOL_SOCKET, SO_KEEPALIVE, 1);
    set_int_option(fd, IPPROTO_TCP, TCP_NODELAY, 1);
    struct linger ling = {
        .l_onoff  = 1,
        .l_linger = (int) (NROS_POSIX_TRANSPORT_LEASE_MS / 1000u),
    };
    (void) setsockopt(fd, SOL_SOCKET, SO_LINGER, &ling, sizeof(ling));
}

/* ---- TCP ---- */

int8_t nros_platform_tcp_create_endpoint(void *ep_raw,
                                         const uint8_t *address,
                                         const uint8_t *port) {
    if (ep_raw == NULL) return -1;
    nros_posix_endpoint_t *ep = (nros_posix_endpoint_t *) ep_raw;
    struct addrinfo hints = {
        .ai_family   = PF_UNSPEC,
        .ai_socktype = SOCK_STREAM,
        .ai_protocol = IPPROTO_TCP,
    };
    int rc = getaddrinfo((const char *) address, (const char *) port,
                         &hints, &ep->iptcp);
    return rc == 0 ? 0 : -1;
}

void nros_platform_tcp_free_endpoint(void *ep_raw) {
    if (ep_raw == NULL) return;
    nros_posix_endpoint_t *ep = (nros_posix_endpoint_t *) ep_raw;
    if (ep->iptcp != NULL) {
        freeaddrinfo(ep->iptcp);
        ep->iptcp = NULL;
    }
}

int8_t nros_platform_tcp_open(void *sock_raw, const void *endpoint, uint32_t timeout_ms) {
    if (sock_raw == NULL || endpoint == NULL) return -1;
    nros_posix_socket_t *sock = (nros_posix_socket_t *) sock_raw;
    const nros_posix_endpoint_t *ep = (const nros_posix_endpoint_t *) endpoint;
    if (ep->iptcp == NULL) return -1;

    struct addrinfo *first = ep->iptcp;
    int fd = socket(first->ai_family, first->ai_socktype, first->ai_protocol);
    if (fd < 0) return -1;
    sock->fd = fd;

    /* Phase 156 — apply socket options AFTER connect, not before.
     * Previously `apply_tcp_common_options(fd, timeout_ms)` ran
     * pre-connect, which on `timeout_ms == 0` flipped the socket
     * to O_NONBLOCK via `set_recv_timeout_ms`. A non-blocking
     * connect returns -1 with EINPROGRESS, which this code
     * treated as failure → close → kernel still completed
     * the TCP handshake async then sent FIN. zenoh-pico
     * bubbled up _Z_ERR_TRANSPORT_OPEN_FAILED (-102), nros
     * surfaced `Transport(ConnectionFailed)`. Phase 127.B.5's
     * non-blocking remap was intended for the recv loop on
     * a NON-zenoh consumer (dust-dds); zenoh-pico's
     * `_z_link_send_t_msg` does single send + checks ret,
     * so a non-blocking socket here trips
     * `_Z_ERR_TRANSPORT_TX_FAILED` (-100) when send() returns
     * EAGAIN. Keep recv-timeout blocking semantics for the
     * TCP connect path by coercing `timeout_ms=0` to a
     * conservative blocking-recv default for zenoh-pico
     * compatibility. */
    uint32_t effective_tout = (timeout_ms == 0) ? 5000u : timeout_ms;
    for (struct addrinfo *it = ep->iptcp; it != NULL; it = it->ai_next) {
        if (connect(fd, it->ai_addr, it->ai_addrlen) == 0) {
            apply_tcp_common_options(fd, effective_tout);
            return 0;
        }
    }

    close(fd);
    sock->fd = -1;
    return -1;
}

int8_t nros_platform_tcp_listen(void *sock_raw, const void *endpoint) {
    if (sock_raw == NULL || endpoint == NULL) return -1;
    nros_posix_socket_t *sock = (nros_posix_socket_t *) sock_raw;
    const nros_posix_endpoint_t *ep = (const nros_posix_endpoint_t *) endpoint;
    if (ep->iptcp == NULL) return -1;

    struct addrinfo *first = ep->iptcp;
    int fd = socket(first->ai_family, first->ai_socktype, first->ai_protocol);
    if (fd < 0) return -1;
    sock->fd = fd;

    set_int_option(fd, SOL_SOCKET, SO_REUSEADDR, 1);
    apply_tcp_common_options(fd, 0);

    for (struct addrinfo *it = ep->iptcp; it != NULL; it = it->ai_next) {
        if (bind(fd, it->ai_addr, it->ai_addrlen) == 0
            && listen(fd, 128) == 0) {
            return 0;
        }
    }

    close(fd);
    sock->fd = -1;
    return -1;
}

void nros_platform_tcp_close(void *sock_raw) {
    if (sock_raw == NULL) return;
    nros_posix_socket_t *sock = (nros_posix_socket_t *) sock_raw;
    if (sock->fd >= 0) {
        shutdown(sock->fd, SHUT_RDWR);
        close(sock->fd);
        sock->fd = -1;
    }
}

size_t nros_platform_tcp_read(const void *sock_raw, uint8_t *buf, size_t len) {
    if (sock_raw == NULL) return NROS_PLATFORM_NET_SOCKET_ERROR;
    const nros_posix_socket_t *sock = (const nros_posix_socket_t *) sock_raw;
    ssize_t r = recv(sock->fd, buf, len, 0);
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
    const nros_posix_socket_t *sock = (const nros_posix_socket_t *) sock_raw;
#ifdef __linux__
    int flags = MSG_NOSIGNAL;
#else
    int flags = 0;
#endif
    ssize_t r = send(sock->fd, buf, len, flags);
    return r < 0 ? NROS_PLATFORM_NET_SOCKET_ERROR : (size_t) r;
}

/* ---- UDP unicast ---- */

int8_t nros_platform_udp_create_endpoint(void *ep_raw,
                                         const uint8_t *address,
                                         const uint8_t *port) {
    if (ep_raw == NULL) return -1;
    nros_posix_endpoint_t *ep = (nros_posix_endpoint_t *) ep_raw;
    struct addrinfo hints = {
        .ai_family   = PF_UNSPEC,
        .ai_socktype = SOCK_DGRAM,
        .ai_protocol = IPPROTO_UDP,
    };
    int rc = getaddrinfo((const char *) address, (const char *) port,
                         &hints, &ep->iptcp);
    return rc == 0 ? 0 : -1;
}

void nros_platform_udp_free_endpoint(void *ep_raw) {
    nros_platform_tcp_free_endpoint(ep_raw);
}

int8_t nros_platform_udp_open(void *sock_raw, const void *endpoint, uint32_t timeout_ms) {
    if (sock_raw == NULL || endpoint == NULL) return -1;
    nros_posix_socket_t *sock = (nros_posix_socket_t *) sock_raw;
    const nros_posix_endpoint_t *ep = (const nros_posix_endpoint_t *) endpoint;
    if (ep->iptcp == NULL) return -1;

    struct addrinfo *ai = ep->iptcp;
    int fd = socket(ai->ai_family, ai->ai_socktype, ai->ai_protocol);
    if (fd < 0) return -1;
    sock->fd = fd;
    set_recv_timeout_ms(fd, timeout_ms);
    return 0;
}

int8_t nros_platform_udp_listen(void *sock_raw, const void *endpoint, uint32_t timeout_ms) {
    if (sock_raw == NULL || endpoint == NULL) return -1;
    nros_posix_socket_t *sock = (nros_posix_socket_t *) sock_raw;
    const nros_posix_endpoint_t *ep = (const nros_posix_endpoint_t *) endpoint;
    if (ep->iptcp == NULL) return -1;

    struct addrinfo *first = ep->iptcp;
    int fd = socket(first->ai_family, first->ai_socktype, first->ai_protocol);
    if (fd < 0) return -1;
    sock->fd = fd;

    set_int_option(fd, SOL_SOCKET, SO_REUSEADDR, 1);
    set_recv_timeout_ms(fd, timeout_ms);

    for (struct addrinfo *it = ep->iptcp; it != NULL; it = it->ai_next) {
        if (bind(fd, it->ai_addr, it->ai_addrlen) == 0) {
            return 0;
        }
    }
    close(fd);
    sock->fd = -1;
    return -1;
}

void nros_platform_udp_close(void *sock_raw) {
    if (sock_raw == NULL) return;
    nros_posix_socket_t *sock = (nros_posix_socket_t *) sock_raw;
    if (sock->fd >= 0) {
        close(sock->fd);
        sock->fd = -1;
    }
}

size_t nros_platform_udp_read(const void *sock_raw, uint8_t *buf, size_t len) {
    if (sock_raw == NULL) return NROS_PLATFORM_NET_SOCKET_ERROR;
    const nros_posix_socket_t *sock = (const nros_posix_socket_t *) sock_raw;
    struct sockaddr_storage raddr;
    socklen_t addrlen = (socklen_t) sizeof(raddr);
    ssize_t r = recvfrom(sock->fd, buf, len, 0,
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
    const nros_posix_socket_t *sock = (const nros_posix_socket_t *) sock_raw;
    const nros_posix_endpoint_t *ep = (const nros_posix_endpoint_t *) endpoint;
    if (ep->iptcp == NULL) return NROS_PLATFORM_NET_SOCKET_ERROR;
    struct addrinfo *ai = ep->iptcp;
    ssize_t r = sendto(sock->fd, buf, len, 0, ai->ai_addr, ai->ai_addrlen);
    return r < 0 ? NROS_PLATFORM_NET_SOCKET_ERROR : (size_t) r;
}

void nros_platform_udp_set_recv_timeout(const void *sock_raw, uint32_t timeout_ms) {
    if (sock_raw == NULL) return;
    const nros_posix_socket_t *sock = (const nros_posix_socket_t *) sock_raw;
    if (timeout_ms == 0) {
        /* POSIX SO_RCVTIMEO with {0,0} means "block forever" — opposite
         * of the caller's intent. Flip to non-blocking mode via fcntl. */
        int flags = fcntl(sock->fd, F_GETFL, 0);
        if (flags >= 0) {
            (void) fcntl(sock->fd, F_SETFL, flags | O_NONBLOCK);
        }
        return;
    }
    set_recv_timeout_ms(sock->fd, timeout_ms);
}

/* ---- UDP multicast ----
 *
 * Mirrors the Rust impl in posix/src/net.rs. Uses getifaddrs to
 * resolve the local interface address, IP_ADD_MEMBERSHIP /
 * IPV6_ADD_MEMBERSHIP to join the multicast group, and
 * sender-address filtering on read to drop our own loopback.
 *
 * ZSlice layout (for the `addr` out-param) matches the Rust
 * `#[repr(C)] struct ZSlice { len, start, _deleter, _context }`.
 */

#include <ifaddrs.h>
#include <net/if.h>

typedef struct {
    size_t  len;
    const uint8_t *start;
    void   *_deleter;
    void   *_context;
} nros_z_slice_t;

/* Walk the host's interfaces looking for one whose name matches
 * `iface` and whose address family matches `sa_family`. Returns a
 * malloc'd sockaddr (callee frees with libc free) or NULL.
 * Mirrors the Rust `get_ip_from_iface`.
 *
 * Phase 127.B.5: `iface == NULL` is the common DDS SPDP path — the
 * caller has no preferred interface, "just join on whatever talks IPv4
 * with a non-loopback address". Pick the first such interface so the
 * caller can compute a valid `imr_interface` for IP_ADD_MEMBERSHIP. */
static struct sockaddr *get_ip_from_iface(const uint8_t *iface,
                                          int sa_family,
                                          socklen_t *addrlen_out) {
    struct ifaddrs *ifaddrs_head = NULL;
    if (getifaddrs(&ifaddrs_head) != 0) {
        if (ifaddrs_head != NULL) freeifaddrs(ifaddrs_head);
        return NULL;
    }

    struct sockaddr *result = NULL;
    socklen_t addrlen = 0;
    for (struct ifaddrs *it = ifaddrs_head; it != NULL; it = it->ifa_next) {
        if (it->ifa_addr == NULL) continue;
        if (it->ifa_addr->sa_family != sa_family) continue;
        if (iface != NULL) {
            if (strcmp(it->ifa_name, (const char *) iface) != 0) continue;
        } else {
            /* Skip loopback when picking a default interface so SPDP
             * lands on the actual network. The flags check uses the
             * portable IFF_LOOPBACK bit. */
            if ((it->ifa_flags & IFF_LOOPBACK) != 0) continue;
            /* Skip down / not-running interfaces. */
            if ((it->ifa_flags & IFF_UP) == 0) continue;
        }

        size_t size = 0;
        if (sa_family == AF_INET)       size = sizeof(struct sockaddr_in);
        else if (sa_family == AF_INET6) size = sizeof(struct sockaddr_in6);
        else continue;

        result = (struct sockaddr *) malloc(size);
        if (result != NULL) {
            memcpy(result, it->ifa_addr, size);
            addrlen = (socklen_t) size;
        }
        break;
    }

    freeifaddrs(ifaddrs_head);
    if (addrlen_out != NULL) *addrlen_out = addrlen;
    return result;
}

int8_t nros_platform_udp_mcast_open(void *sock_raw, const void *endpoint,
                                    void *lep_raw, uint32_t timeout_ms,
                                    const uint8_t *iface) {
    if (sock_raw == NULL || endpoint == NULL || lep_raw == NULL) return -1;
    nros_posix_socket_t *sock = (nros_posix_socket_t *) sock_raw;
    nros_posix_endpoint_t *lep = (nros_posix_endpoint_t *) lep_raw;
    const nros_posix_endpoint_t *ep = (const nros_posix_endpoint_t *) endpoint;
    if (ep->iptcp == NULL) return -1;
    struct addrinfo *ai = ep->iptcp;

    socklen_t addrlen = 0;
    struct sockaddr *lsockaddr = get_ip_from_iface(iface, ai->ai_family, &addrlen);
    if (lsockaddr == NULL) return -1;

    int fd = socket(ai->ai_family, ai->ai_socktype, ai->ai_protocol);
    if (fd < 0) { free(lsockaddr); return -1; }
    sock->fd = fd;
    set_recv_timeout_ms(fd, timeout_ms);

    if (bind(fd, lsockaddr, addrlen) < 0) {
        close(fd); free(lsockaddr); sock->fd = -1; return -1;
    }

    /* Retrieve the kernel-assigned port (in-place update of lsockaddr). */
    socklen_t bound_len = addrlen;
    (void) getsockname(fd, lsockaddr, &bound_len);

    /* IP_MULTICAST_IF / IPV6_MULTICAST_IF: route outbound multicast
     * through the chosen interface. */
    if (lsockaddr->sa_family == AF_INET) {
        struct in_addr *addr = &((struct sockaddr_in *) lsockaddr)->sin_addr;
        (void) setsockopt(fd, IPPROTO_IP, IP_MULTICAST_IF,
                          addr, sizeof(struct in_addr));
    } else if (lsockaddr->sa_family == AF_INET6) {
        int ifindex = (int) if_nametoindex((const char *) iface);
        (void) setsockopt(fd, IPPROTO_IPV6, IPV6_MULTICAST_IF,
                          &ifindex, sizeof(ifindex));
    }

    /* Wrap the bound local sockaddr in an addrinfo and stash it in
     * `lep` so the caller can use the local address as the read
     * endpoint identifier (matches the Rust impl's lifetime model). */
    struct addrinfo *laddr = (struct addrinfo *) calloc(1, sizeof(struct addrinfo));
    if (laddr == NULL) {
        close(fd); free(lsockaddr); sock->fd = -1; return -1;
    }
    laddr->ai_flags    = 0;
    laddr->ai_family   = ai->ai_family;
    laddr->ai_socktype = ai->ai_socktype;
    laddr->ai_protocol = ai->ai_protocol;
    laddr->ai_addrlen  = addrlen;
    laddr->ai_addr     = lsockaddr;
    lep->iptcp = laddr;
    return 0;
}

int8_t nros_platform_udp_mcast_listen(void *sock_raw, const void *endpoint,
                                      uint32_t timeout_ms,
                                      const uint8_t *iface,
                                      const uint8_t *join) {
    /* Phase 127.B.5 — `endpoint` is the LOCAL bind endpoint (typically
     * `0.0.0.0:<port>`); `join` is the NUL-terminated dotted-quad
     * multicast group to subscribe to (e.g. `"239.255.0.1"`). Use
     * `join` for `imr_multiaddr` — using the endpoint's address (which
     * is 0.0.0.0 for the SPDP path) silently subscribes to no group on
     * lenient stacks (Linux drops the join with EINVAL but some stacks
     * like NuttX add a sentinel grp entry that never matches any real
     * incoming mcast frame, so SPDP discovery silently fails). */
    if (sock_raw == NULL || endpoint == NULL || join == NULL) return -1;
    nros_posix_socket_t *sock = (nros_posix_socket_t *) sock_raw;
    const nros_posix_endpoint_t *ep = (const nros_posix_endpoint_t *) endpoint;
    if (ep->iptcp == NULL) return -1;
    struct addrinfo *ai = ep->iptcp;

    socklen_t addrlen = 0;
    struct sockaddr *lsockaddr = get_ip_from_iface(iface, ai->ai_family, &addrlen);
    if (lsockaddr == NULL) return -1;

    int fd = socket(ai->ai_family, ai->ai_socktype, ai->ai_protocol);
    if (fd < 0) { free(lsockaddr); return -1; }
    sock->fd = fd;
    set_recv_timeout_ms(fd, timeout_ms);
    set_int_option(fd, SOL_SOCKET, SO_REUSEADDR, 1);
#ifdef SO_REUSEPORT
    set_int_option(fd, SOL_SOCKET, SO_REUSEPORT, 1);
#endif

    int bind_rc;
    if (ai->ai_family == AF_INET) {
        struct sockaddr_in addr;
        memset(&addr, 0, sizeof(addr));
        addr.sin_family = AF_INET;
        addr.sin_port   = ((const struct sockaddr_in *) ai->ai_addr)->sin_port;
        addr.sin_addr.s_addr = htonl(INADDR_ANY);
        bind_rc = bind(fd, (struct sockaddr *) &addr, sizeof(addr));
    } else {
        struct sockaddr_in6 addr;
        memset(&addr, 0, sizeof(addr));
        addr.sin6_family = AF_INET6;
        addr.sin6_port   = ((const struct sockaddr_in6 *) ai->ai_addr)->sin6_port;
        bind_rc = bind(fd, (struct sockaddr *) &addr, sizeof(addr));
    }
    if (bind_rc < 0) {
        close(fd); free(lsockaddr); sock->fd = -1; return -1;
    }

    /* Join the multicast group on the chosen interface. */
    int join_rc;
    if (ai->ai_family == AF_INET) {
        struct ip_mreq mreq;
        memset(&mreq, 0, sizeof(mreq));
        if (inet_pton(AF_INET, (const char *) join, &mreq.imr_multiaddr) != 1) {
            close(fd); free(lsockaddr); sock->fd = -1; return -1;
        }
        mreq.imr_interface = ((const struct sockaddr_in *) lsockaddr)->sin_addr;
        join_rc = setsockopt(fd, IPPROTO_IP, IP_ADD_MEMBERSHIP,
                             &mreq, sizeof(mreq));
    } else {
#ifdef IPV6_ADD_MEMBERSHIP
        struct ipv6_mreq mreq;
        memset(&mreq, 0, sizeof(mreq));
        if (inet_pton(AF_INET6, (const char *) join, &mreq.ipv6mr_multiaddr) != 1) {
            close(fd); free(lsockaddr); sock->fd = -1; return -1;
        }
        mreq.ipv6mr_interface = if_nametoindex((const char *) iface);
        join_rc = setsockopt(fd, IPPROTO_IPV6, IPV6_ADD_MEMBERSHIP,
                             &mreq, sizeof(mreq));
#else
        join_rc = -1;
#endif
    }

    free(lsockaddr);
    if (join_rc < 0) {
        close(fd); sock->fd = -1; return -1;
    }
    return 0;
}

void nros_platform_udp_mcast_close(void *sockrecv_raw, void *socksend_raw,
                                   const void *rep_raw, const void *lep_raw) {
    nros_posix_socket_t *sockrecv = (nros_posix_socket_t *) sockrecv_raw;
    nros_posix_socket_t *socksend = (nros_posix_socket_t *) socksend_raw;
    const nros_posix_endpoint_t *rep = (const nros_posix_endpoint_t *) rep_raw;
    const nros_posix_endpoint_t *lep = (const nros_posix_endpoint_t *) lep_raw;

    /* Drop multicast membership on sockrecv before closing. */
    if (sockrecv != NULL && sockrecv->fd >= 0 && rep != NULL && rep->iptcp != NULL) {
        struct addrinfo *ai = rep->iptcp;
        if (ai->ai_family == AF_INET) {
            struct ip_mreq mreq;
            memset(&mreq, 0, sizeof(mreq));
            mreq.imr_multiaddr =
                ((const struct sockaddr_in *) ai->ai_addr)->sin_addr;
            mreq.imr_interface.s_addr = htonl(INADDR_ANY);
            (void) setsockopt(sockrecv->fd, IPPROTO_IP, IP_DROP_MEMBERSHIP,
                              &mreq, sizeof(mreq));
        } else if (ai->ai_family == AF_INET6) {
#ifdef IPV6_DROP_MEMBERSHIP
            struct ipv6_mreq mreq;
            memset(&mreq, 0, sizeof(mreq));
            mreq.ipv6mr_multiaddr =
                ((const struct sockaddr_in6 *) ai->ai_addr)->sin6_addr;
            (void) setsockopt(sockrecv->fd, IPPROTO_IPV6, IPV6_DROP_MEMBERSHIP,
                              &mreq, sizeof(mreq));
#endif
        }
    }

    /* Free the local-endpoint addrinfo allocated by mcast_open. */
    if (lep != NULL && lep->iptcp != NULL) {
        struct addrinfo *laddr = lep->iptcp;
        free(laddr->ai_addr);
        free(laddr);
    }

    if (sockrecv != NULL && sockrecv->fd >= 0) {
        close(sockrecv->fd);
        sockrecv->fd = -1;
    }
    if (socksend != NULL && socksend->fd >= 0) {
        close(socksend->fd);
        socksend->fd = -1;
    }
}

size_t nros_platform_udp_mcast_read(const void *sock_raw, uint8_t *buf,
                                    size_t len, const void *lep_raw,
                                    void *addr) {
    if (sock_raw == NULL || lep_raw == NULL) return NROS_PLATFORM_NET_SOCKET_ERROR;
    const nros_posix_socket_t *sock = (const nros_posix_socket_t *) sock_raw;
    const nros_posix_endpoint_t *lep = (const nros_posix_endpoint_t *) lep_raw;
    if (lep->iptcp == NULL) return NROS_PLATFORM_NET_SOCKET_ERROR;
    struct addrinfo *ai = lep->iptcp;

    for (;;) {
        struct sockaddr_storage raddr;
        socklen_t replen = (socklen_t) sizeof(raddr);
        ssize_t rb = recvfrom(sock->fd, buf, len, 0,
                              (struct sockaddr *) &raddr, &replen);
        if (rb < 0) return NROS_PLATFORM_NET_SOCKET_ERROR;

        /* Drop our own loopback: skip when sender == local. */
        int is_loopback = 0;
        if (ai->ai_family == AF_INET) {
            const struct sockaddr_in *local  = (const struct sockaddr_in *) ai->ai_addr;
            const struct sockaddr_in *remote = (const struct sockaddr_in *) &raddr;
            is_loopback = (local->sin_port == remote->sin_port
                           && local->sin_addr.s_addr == remote->sin_addr.s_addr);
        } else if (ai->ai_family == AF_INET6) {
            const struct sockaddr_in6 *local  = (const struct sockaddr_in6 *) ai->ai_addr;
            const struct sockaddr_in6 *remote = (const struct sockaddr_in6 *) &raddr;
            is_loopback = (local->sin6_port == remote->sin6_port
                           && memcmp(&local->sin6_addr, &remote->sin6_addr,
                                     sizeof(local->sin6_addr)) == 0);
        } else {
            is_loopback = 1;  /* unknown family — drop */
        }

        if (is_loopback) continue;

        /* Write sender (ip+port) into the caller's ZSlice if supplied. */
        if (addr != NULL) {
            nros_z_slice_t *slice = (nros_z_slice_t *) addr;
            if (ai->ai_family == AF_INET) {
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
            } else if (ai->ai_family == AF_INET6) {
                const struct sockaddr_in6 *remote = (const struct sockaddr_in6 *) &raddr;
                size_t ip_size   = sizeof(remote->sin6_addr);
                size_t port_size = sizeof(remote->sin6_port);
                if (slice->len >= ip_size + port_size) {
                    slice->len = ip_size + port_size;
                    memcpy((uint8_t *) slice->start,
                           &remote->sin6_addr, ip_size);
                    memcpy((uint8_t *) slice->start + ip_size,
                           &remote->sin6_port, port_size);
                }
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
    /* Identical to UDP unicast send — multicast routing happens at
     * the kernel based on the destination address. */
    return nros_platform_udp_send(sock, buf, len, endpoint);
}

/* ---- Socket helpers ---- */

int8_t nros_platform_socket_set_non_blocking(const void *sock_raw) {
    if (sock_raw == NULL) return -1;
    const nros_posix_socket_t *sock = (const nros_posix_socket_t *) sock_raw;
    int flags = fcntl(sock->fd, F_GETFL, 0);
    if (flags < 0) return -1;
    if (fcntl(sock->fd, F_SETFL, flags | O_NONBLOCK) < 0) return -1;
    return 0;
}

int8_t nros_platform_socket_accept(const void *sock_in_raw, void *sock_out_raw) {
    if (sock_in_raw == NULL || sock_out_raw == NULL) return -1;
    const nros_posix_socket_t *in = (const nros_posix_socket_t *) sock_in_raw;
    nros_posix_socket_t *out = (nros_posix_socket_t *) sock_out_raw;

    struct sockaddr_storage naddr;
    socklen_t nlen = (socklen_t) sizeof(naddr);
    int con = accept(in->fd, (struct sockaddr *) &naddr, &nlen);
    if (con < 0) return -1;

    /* Mirror the Rust impl's accepted-socket options. */
    struct timeval tv = { .tv_sec = 10, .tv_usec = 0 };  /* Z_CONFIG_SOCKET_TIMEOUT */
    (void) setsockopt(con, SOL_SOCKET, SO_RCVTIMEO, &tv, sizeof(tv));
    set_int_option(con, SOL_SOCKET, SO_KEEPALIVE, 1);
    set_int_option(con, IPPROTO_TCP, TCP_NODELAY, 1);

    out->fd = con;
    return 0;
}

void nros_platform_socket_close(void *sock_raw) {
    nros_platform_tcp_close(sock_raw);
}

int8_t nros_platform_socket_wait_event(void *peers, void *mutex) {
    /* Mirrors the Rust impl: delegate to a cooperative yield —
     * background read tasks handle actual I/O readiness. */
    (void) peers; (void) mutex;
    sched_yield();
    return 0;
}

/* ---- Network poll ---- */

void nros_platform_network_poll(void) {
    /* POSIX socket layer is kernel-driven — no-op. */
}

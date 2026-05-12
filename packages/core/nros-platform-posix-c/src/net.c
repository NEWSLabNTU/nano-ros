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

    apply_tcp_common_options(fd, timeout_ms);

    for (struct addrinfo *it = ep->iptcp; it != NULL; it = it->ai_next) {
        if (connect(fd, it->ai_addr, it->ai_addrlen) == 0) {
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

/* ---- UDP multicast (stubs) ----
 *
 * Full implementation requires getifaddrs + IP_ADD_MEMBERSHIP +
 * setsockopt(IP_MULTICAST_IF) per address family, mirroring the
 * Rust impl in posix/src/net.rs. Stubbed for now; consumers that
 * need multicast should keep using the Rust path (PosixPlatform).
 */

int8_t nros_platform_udp_mcast_open(void *sock, const void *endpoint,
                                    void *lep, uint32_t timeout_ms,
                                    const uint8_t *iface) {
    (void) sock; (void) endpoint; (void) lep; (void) timeout_ms; (void) iface;
    return -1;
}

int8_t nros_platform_udp_mcast_listen(void *sock, const void *endpoint,
                                      uint32_t timeout_ms,
                                      const uint8_t *iface,
                                      const uint8_t *join) {
    (void) sock; (void) endpoint; (void) timeout_ms; (void) iface; (void) join;
    return -1;
}

void nros_platform_udp_mcast_close(void *sockrecv, void *socksend,
                                   const void *rep, const void *lep) {
    (void) sockrecv; (void) socksend; (void) rep; (void) lep;
}

size_t nros_platform_udp_mcast_read(const void *sock, uint8_t *buf,
                                    size_t len, const void *lep,
                                    void *addr) {
    (void) sock; (void) buf; (void) len; (void) lep; (void) addr;
    return NROS_PLATFORM_NET_SOCKET_ERROR;
}

size_t nros_platform_udp_mcast_read_exact(const void *sock, uint8_t *buf,
                                          size_t len, const void *lep,
                                          void *addr) {
    (void) sock; (void) buf; (void) len; (void) lep; (void) addr;
    return NROS_PLATFORM_NET_SOCKET_ERROR;
}

size_t nros_platform_udp_mcast_send(const void *sock, const uint8_t *buf,
                                    size_t len, const void *endpoint) {
    (void) sock; (void) buf; (void) len; (void) endpoint;
    return NROS_PLATFORM_NET_SOCKET_ERROR;
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

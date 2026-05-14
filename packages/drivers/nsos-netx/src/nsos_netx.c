/**
 * nsos_netx.c — NetX Duo BSD socket compatibility shim over host POSIX
 *
 * Implements the `nx_bsd_*` symbols as thin wrappers around the host
 * kernel's BSD socket API. Lets a ThreadX-Linux simulation use real
 * Linux sockets without running the NetX Duo TCP/IP stack.
 *
 * The Linux board uses NetX Duo's native `nx_bsd_*` names so NetX
 * headers do not redefine host POSIX symbols. This shim translates the
 * small native NetX BSD data structures used by nano-ros into POSIX
 * structures before forwarding to the host kernel.
 *
 * All FD numbers are real kernel file descriptors. NetX-style "socket
 * IDs" no longer exist.
 */

#define _GNU_SOURCE
#include <errno.h>
#include <fcntl.h>
#include <netdb.h>
#include <stdlib.h>
#include <stdint.h>
#include <string.h>
#include <arpa/inet.h>
#include <sys/socket.h>
#include <netinet/in.h>
#include <netinet/tcp.h>
#include <sys/select.h>
#include <sys/time.h>
#include <unistd.h>

#include "nsos_netx.h"

/* NetX integer types — must match tx_port.h. */
typedef int                 INT;
typedef unsigned int        UINT;
typedef short               SHORT;
typedef unsigned short      USHORT;
typedef char                CHAR;
typedef unsigned char       UCHAR;
#define VOID                void
#if defined(__x86_64__) && __x86_64__
typedef int                 LONG;
typedef unsigned int        ULONG;
#else
typedef long                LONG;
typedef unsigned long       ULONG;
#endif

#define NX_AF_UNSPEC        0
#define NX_AF_INET          2
#define NX_AF_INET6         3
#define NX_IPPROTO_IP       2
#define NX_IP_ADD_MEMBERSHIP    32
#define NX_IP_DROP_MEMBERSHIP   33
#define NX_O_NONBLOCK       0x4000

typedef ULONG nx_bsd_socklen_t;

struct nx_bsd_in_addr {
    ULONG s_addr;
};

struct nx_bsd_in6_addr {
    union {
        UCHAR _S6_u8[16];
        ULONG _S6_u32[4];
    } _S6_un;
};

struct nx_bsd_sockaddr {
    USHORT sa_family;
    UCHAR sa_data[14];
};

struct nx_bsd_sockaddr_in {
    USHORT sin_family;
    USHORT sin_port;
    struct nx_bsd_in_addr sin_addr;
    CHAR sin_zero[8];
};

struct nx_bsd_sockaddr_in6 {
    USHORT sin6_family;
    USHORT sin6_port;
    ULONG sin6_flowinfo;
    struct nx_bsd_in6_addr sin6_addr;
    ULONG sin6_scope_id;
};

struct nx_bsd_addrinfo {
    INT ai_flags;
    INT ai_family;
    INT ai_socktype;
    INT ai_protocol;
    nx_bsd_socklen_t ai_addrlen;
    struct nx_bsd_sockaddr *ai_addr;
    CHAR *ai_canonname;
    struct nx_bsd_addrinfo *ai_next;
};

struct nx_bsd_ip_mreq {
    struct nx_bsd_in_addr imr_multiaddr;
    struct nx_bsd_in_addr imr_interface;
};

struct nx_bsd_timeval {
    LONG tv_sec;
    LONG tv_usec;
};

static int nx_to_posix_family(INT family) {
    if (family == NX_AF_INET6) return AF_INET6;
    return family;
}

static INT posix_to_nx_family(int family) {
    if (family == AF_INET6) return NX_AF_INET6;
    if (family == AF_INET) return NX_AF_INET;
    if (family == AF_UNSPEC) return NX_AF_UNSPEC;
    return (INT)family;
}

static int nx_to_posix_sockaddr(const struct nx_bsd_sockaddr *src,
                                struct sockaddr_storage *dst,
                                socklen_t *dst_len) {
    if (src == NULL || dst == NULL || dst_len == NULL) return -1;
    memset(dst, 0, sizeof(*dst));

    if (src->sa_family == NX_AF_INET) {
        const struct nx_bsd_sockaddr_in *nx = (const struct nx_bsd_sockaddr_in *)src;
        struct sockaddr_in *posix = (struct sockaddr_in *)dst;
        posix->sin_family = AF_INET;
        posix->sin_port = nx->sin_port;
        posix->sin_addr.s_addr = (in_addr_t)nx->sin_addr.s_addr;
        *dst_len = (socklen_t)sizeof(*posix);
        return 0;
    }

    if (src->sa_family == NX_AF_INET6) {
        const struct nx_bsd_sockaddr_in6 *nx = (const struct nx_bsd_sockaddr_in6 *)src;
        struct sockaddr_in6 *posix = (struct sockaddr_in6 *)dst;
        posix->sin6_family = AF_INET6;
        posix->sin6_port = nx->sin6_port;
        posix->sin6_flowinfo = nx->sin6_flowinfo;
        memcpy(&posix->sin6_addr, nx->sin6_addr._S6_un._S6_u8, sizeof(posix->sin6_addr));
        posix->sin6_scope_id = nx->sin6_scope_id;
        *dst_len = (socklen_t)sizeof(*posix);
        return 0;
    }

    errno = EAFNOSUPPORT;
    return -1;
}

static int posix_to_nx_sockaddr(const struct sockaddr *src,
                                socklen_t src_len,
                                struct nx_bsd_sockaddr *dst,
                                INT *dst_len) {
    if (src == NULL || dst == NULL || dst_len == NULL) return -1;

    if (src->sa_family == AF_INET && *dst_len >= (INT)sizeof(struct nx_bsd_sockaddr_in)) {
        const struct sockaddr_in *posix = (const struct sockaddr_in *)src;
        struct nx_bsd_sockaddr_in *nx = (struct nx_bsd_sockaddr_in *)dst;
        memset(nx, 0, sizeof(*nx));
        nx->sin_family = NX_AF_INET;
        nx->sin_port = posix->sin_port;
        nx->sin_addr.s_addr = posix->sin_addr.s_addr;
        *dst_len = (INT)sizeof(*nx);
        (void)src_len;
        return 0;
    }

    if (src->sa_family == AF_INET6 && *dst_len >= (INT)sizeof(struct nx_bsd_sockaddr_in6)) {
        const struct sockaddr_in6 *posix = (const struct sockaddr_in6 *)src;
        struct nx_bsd_sockaddr_in6 *nx = (struct nx_bsd_sockaddr_in6 *)dst;
        memset(nx, 0, sizeof(*nx));
        nx->sin6_family = NX_AF_INET6;
        nx->sin6_port = posix->sin6_port;
        nx->sin6_flowinfo = posix->sin6_flowinfo;
        memcpy(nx->sin6_addr._S6_un._S6_u8, &posix->sin6_addr, sizeof(posix->sin6_addr));
        nx->sin6_scope_id = posix->sin6_scope_id;
        *dst_len = (INT)sizeof(*nx);
        (void)src_len;
        return 0;
    }

    errno = EAFNOSUPPORT;
    return -1;
}

static struct nx_bsd_sockaddr *alloc_nx_sockaddr(const struct sockaddr *src,
                                                 socklen_t src_len,
                                                 nx_bsd_socklen_t *nx_len) {
    INT len = src->sa_family == AF_INET6
        ? (INT)sizeof(struct nx_bsd_sockaddr_in6)
        : (INT)sizeof(struct nx_bsd_sockaddr_in);
    struct nx_bsd_sockaddr *dst = calloc(1, (size_t)len);
    if (dst == NULL) return NULL;
    if (posix_to_nx_sockaddr(src, src_len, dst, &len) != 0) {
        free(dst);
        return NULL;
    }
    *nx_len = (nx_bsd_socklen_t)len;
    return dst;
}

VOID nx_bsd_freeaddrinfo(struct nx_bsd_addrinfo *res);

/* ─── Lifecycle ─────────────────────────────────────────────────────── */

int nsos_netx_init(void) {
    return 0;  /* nothing to do — host kernel is always ready */
}

/* ─── Socket calls ──────────────────────────────────────────────────── */

INT nx_bsd_socket(INT family, INT type, INT proto) {
    return socket(nx_to_posix_family(family), type, proto);
}

INT nx_bsd_soc_close(INT sockID) {
    return close(sockID);
}

INT nx_bsd_bind(INT sockID, const struct nx_bsd_sockaddr *addr, INT addrLen) {
    struct sockaddr_storage posix_addr;
    socklen_t posix_len = 0;
    (void)addrLen;
    if (nx_to_posix_sockaddr(addr, &posix_addr, &posix_len) != 0) return -1;
    return bind(sockID, (const struct sockaddr *)&posix_addr, posix_len);
}

INT nx_bsd_connect(INT sockID, struct nx_bsd_sockaddr *addr, INT addrLen) {
    struct sockaddr_storage posix_addr;
    socklen_t posix_len = 0;
    (void)addrLen;
    if (nx_to_posix_sockaddr(addr, &posix_addr, &posix_len) != 0) return -1;
    return connect(sockID, (const struct sockaddr *)&posix_addr, posix_len);
}

INT nx_bsd_listen(INT sockID, INT backlog) {
    return listen(sockID, backlog);
}

INT nx_bsd_accept(INT sockID, struct nx_bsd_sockaddr *clientAddr, INT *addrLen) {
    struct sockaddr_storage posix_addr;
    socklen_t posix_len = sizeof(posix_addr);
    int new_fd = accept(sockID, (struct sockaddr *)&posix_addr,
                        clientAddr ? &posix_len : NULL);
    if (new_fd < 0) {
        return new_fd;
    }
    if (clientAddr != NULL && addrLen != NULL) {
        (void)posix_to_nx_sockaddr((const struct sockaddr *)&posix_addr,
                                   posix_len, clientAddr, addrLen);
    }
    return new_fd;
}

INT nx_bsd_send(INT sockID, const CHAR *msg, INT msgLen, INT flags) {
    return (INT)send(sockID, msg, (size_t)msgLen, flags);
}

INT nx_bsd_recv(INT sockID, VOID *buf, INT bufLen, INT flags) {
    return (INT)recv(sockID, buf, (size_t)bufLen, flags);
}

INT nx_bsd_sendto(INT sockID, CHAR *msg, INT msgLen, INT flags,
                  struct nx_bsd_sockaddr *destAddr, INT destAddrLen) {
    struct sockaddr_storage posix_addr;
    socklen_t posix_len = 0;
    (void)destAddrLen;
    if (nx_to_posix_sockaddr(destAddr, &posix_addr, &posix_len) != 0) return -1;
    return (INT)sendto(sockID, msg, (size_t)msgLen, flags,
                       (const struct sockaddr *)&posix_addr, posix_len);
}

INT nx_bsd_recvfrom(INT sockID, CHAR *buf, INT bufLen, INT flags,
                    struct nx_bsd_sockaddr *srcAddr, INT *addrLen) {
    struct sockaddr_storage posix_addr;
    socklen_t posix_len = sizeof(posix_addr);
    int n = (int)recvfrom(sockID, buf, (size_t)bufLen, flags,
                          srcAddr ? (struct sockaddr *)&posix_addr : NULL,
                          srcAddr ? &posix_len : NULL);
    if (n < 0) {
        return n;
    }
    if (srcAddr != NULL && addrLen != NULL) {
        (void)posix_to_nx_sockaddr((const struct sockaddr *)&posix_addr,
                                   posix_len, srcAddr, addrLen);
    }
    return n;
}

/* Phase 97.4.threadx-linux — NetX BSD's `IPPROTO_IP` and
 * `IP_*MEMBERSHIP` / `IP_MULTICAST_*` constants don't match Linux's
 * (NetX uses `IPPROTO_IP=2`, `IP_ADD_MEMBERSHIP=32`,
 * `IP_MULTICAST_LOOP=29`; Linux uses `IPPROTO_IP=0`,
 * `IP_ADD_MEMBERSHIP=35`, `IP_MULTICAST_LOOP=34`). Translate the
 * level + optname pair before forwarding to the host kernel.
 *
 * SOL_SOCKET / SO_* constants happen to match between NetX and
 * Linux, so we only need the IPPROTO_IP path. */
static int translate_sockopt(INT *level, INT *optName) {
    if (*level == NX_IPPROTO_IP) {
        *level = 0; /* Linux IPPROTO_IP */
        switch (*optName) {
            case 27: *optName = 32; break; /* IP_MULTICAST_IF */
            case 28: *optName = 33; break; /* IP_MULTICAST_TTL */
            case 29: *optName = 34; break; /* IP_MULTICAST_LOOP */
            case NX_IP_ADD_MEMBERSHIP: *optName = 35; break;
            case NX_IP_DROP_MEMBERSHIP: *optName = 36; break;
            default: return -1;
        }
    }
    return 0;
}

INT nx_bsd_setsockopt(INT sockID, INT level, INT optName,
                      const VOID *optValue, INT optLen) {
    if (translate_sockopt(&level, &optName) < 0) return -1;

    /* Phase 97.4.threadx-linux — NetX BSD passes SO_RCVTIMEO /
     * SO_SNDTIMEO as `INT` milliseconds *or* (post Phase 97.4.threadx-
     * riscv64 cleanup) as a packed `struct nx_bsd_timeval` (LONG-typed
     * tv_sec / tv_usec — 8 bytes total under bindgen's INT-as-c_int
     * remap). Linux's POSIX socket layer expects `struct timeval`
     * (16 bytes on LP64). Either input shape needs translation —
     * forwarding verbatim makes Linux interpret the 4 / 8 byte buffer
     * as a truncated 16-byte timeval and either yields a 1-second
     * block (the INT path) or returns EINVAL silently (the 8-byte
     * path). */
    if (level == SOL_SOCKET && (optName == SO_RCVTIMEO || optName == SO_SNDTIMEO)) {
        if (optLen == (INT)sizeof(INT)) {
            INT ms = *(const INT *)optValue;
            struct timeval tv = {
                .tv_sec = ms / 1000,
                .tv_usec = (ms % 1000) * 1000,
            };
            return setsockopt(sockID, level, optName, &tv, (socklen_t)sizeof(tv));
        }
        if (optLen == (INT)sizeof(struct nx_bsd_timeval)) {
            const struct nx_bsd_timeval *nx_tv = (const struct nx_bsd_timeval *)optValue;
            struct timeval tv = {
                .tv_sec = (long)nx_tv->tv_sec,
                .tv_usec = (long)nx_tv->tv_usec,
            };
            return setsockopt(sockID, level, optName, &tv, (socklen_t)sizeof(tv));
        }
        if (optLen == (INT)(2 * sizeof(INT))) {
            const INT *fields = (const INT *)optValue;
            struct timeval tv = {
                .tv_sec = (long)fields[0],
                .tv_usec = (long)fields[1],
            };
            return setsockopt(sockID, level, optName, &tv, (socklen_t)sizeof(tv));
        }
    }

    return setsockopt(sockID, level, optName, optValue, (socklen_t)optLen);
}

INT nx_bsd_getsockopt(INT sockID, INT level, INT optName,
                      VOID *optValue, INT *optLen) {
    if (translate_sockopt(&level, &optName) < 0) return -1;
    socklen_t posix_len = (socklen_t)*optLen;
    int rc = getsockopt(sockID, level, optName, optValue, &posix_len);
    *optLen = (INT)posix_len;
    return rc;
}

INT nx_bsd_select(INT nfds, fd_set *readfds, fd_set *writefds,
                  fd_set *exceptfds, struct timeval *timeout) {
    return select(nfds, readfds, writefds, exceptfds, timeout);
}

INT nx_bsd_fcntl(INT sockID, UINT flagType, UINT options) {
    if (flagType == F_SETFL && (options & NX_O_NONBLOCK) != 0) {
        options &= ~NX_O_NONBLOCK;
        options |= O_NONBLOCK;
    }
    return fcntl(sockID, (int)flagType, (int)options);
}

INT nx_bsd_shutdown(INT sockID, INT how) {
    return shutdown(sockID, how);
}

INT nx_bsd_getaddrinfo(const CHAR *node,
                       const CHAR *service,
                       const struct nx_bsd_addrinfo *hints,
                       struct nx_bsd_addrinfo **res) {
    struct addrinfo posix_hints;
    struct addrinfo *posix_res = NULL;
    struct nx_bsd_addrinfo *head = NULL;
    struct nx_bsd_addrinfo **tail = &head;

    if (res == NULL) return EAI_FAIL;
    *res = NULL;
    memset(&posix_hints, 0, sizeof(posix_hints));
    if (hints != NULL) {
        posix_hints.ai_flags = hints->ai_flags;
        posix_hints.ai_family = nx_to_posix_family(hints->ai_family);
        posix_hints.ai_socktype = hints->ai_socktype;
        posix_hints.ai_protocol = hints->ai_protocol;
    }

    int rc = getaddrinfo((const char *)node, (const char *)service,
                         hints ? &posix_hints : NULL, &posix_res);
    if (rc != 0) return rc;

    for (const struct addrinfo *it = posix_res; it != NULL; it = it->ai_next) {
        if (it->ai_family != AF_INET && it->ai_family != AF_INET6) continue;

        struct nx_bsd_addrinfo *nx = calloc(1, sizeof(*nx));
        if (nx == NULL) {
            nx_bsd_freeaddrinfo(head);
            freeaddrinfo(posix_res);
            return EAI_MEMORY;
        }

        nx->ai_flags = it->ai_flags;
        nx->ai_family = posix_to_nx_family(it->ai_family);
        nx->ai_socktype = it->ai_socktype;
        nx->ai_protocol = it->ai_protocol;
        nx->ai_addr = alloc_nx_sockaddr(it->ai_addr, it->ai_addrlen, &nx->ai_addrlen);
        if (nx->ai_addr == NULL) {
            free(nx);
            nx_bsd_freeaddrinfo(head);
            freeaddrinfo(posix_res);
            return EAI_MEMORY;
        }
        if (it->ai_canonname != NULL) {
            nx->ai_canonname = strdup(it->ai_canonname);
            if (nx->ai_canonname == NULL) {
                free(nx->ai_addr);
                free(nx);
                nx_bsd_freeaddrinfo(head);
                freeaddrinfo(posix_res);
                return EAI_MEMORY;
            }
        }

        *tail = nx;
        tail = &nx->ai_next;
    }

    freeaddrinfo(posix_res);
    if (head == NULL) return EAI_NONAME;
    *res = head;
    return 0;
}

VOID nx_bsd_freeaddrinfo(struct nx_bsd_addrinfo *res) {
    while (res != NULL) {
        struct nx_bsd_addrinfo *next = res->ai_next;
        free(res->ai_addr);
        free(res->ai_canonname);
        free(res);
        res = next;
    }
}

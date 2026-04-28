/**
 * nsos_netx.c — NetX Duo BSD socket compatibility shim over host POSIX
 *
 * Implements the `nx_bsd_*` symbols as thin wrappers around the host
 * kernel's BSD socket API. Lets a ThreadX-Linux simulation use real
 * Linux sockets without running the NetX Duo TCP/IP stack.
 *
 * Type compatibility: NetX Duo's `nxd_bsd.h` aliases its `nx_bsd_*`
 * types directly to the POSIX equivalents when `NX_BSD_ENABLE_NATIVE_API`
 * is NOT defined (the default). Under that mode `nx_bsd_sockaddr` is
 * `sockaddr`, `nx_bsd_sockaddr_in` is `sockaddr_in`, etc., so we can
 * forward calls with no struct translation.
 *
 * All FD numbers are real kernel file descriptors. NetX-style "socket
 * IDs" no longer exist.
 */

#define _GNU_SOURCE
#include <errno.h>
#include <fcntl.h>
#include <stdint.h>
#include <string.h>
#include <sys/socket.h>
#include <netinet/in.h>
#include <netinet/tcp.h>
#include <sys/select.h>
#include <sys/time.h>
#include <unistd.h>

#include "nsos_netx.h"

/* NetX integer types — must match tx_port.h (Linux x86_64). */
typedef int                 INT;
typedef unsigned int        UINT;
typedef short               SHORT;
typedef unsigned short      USHORT;
typedef unsigned long       ULONG;
typedef char                CHAR;
typedef unsigned char       UCHAR;
#define VOID                void

/* When NX_BSD_ENABLE_NATIVE_API is not defined (the default on POSIX),
 * NetX Duo's `nxd_bsd.h` resolves these names to the POSIX types via
 * `#define nx_bsd_sockaddr sockaddr` etc. Our shim follows the same
 * convention, so we can forward directly. */

/* ─── Lifecycle ─────────────────────────────────────────────────────── */

int nsos_netx_init(void) {
    return 0;  /* nothing to do — host kernel is always ready */
}

/* ─── Socket calls ──────────────────────────────────────────────────── */

INT nx_bsd_socket(INT family, INT type, INT proto) {
    return socket(family, type, proto);
}

INT nx_bsd_soc_close(INT sockID) {
    return close(sockID);
}

INT nx_bsd_bind(INT sockID, const struct sockaddr *addr, INT addrLen) {
    return bind(sockID, addr, (socklen_t)addrLen);
}

INT nx_bsd_connect(INT sockID, struct sockaddr *addr, INT addrLen) {
    return connect(sockID, addr, (socklen_t)addrLen);
}

INT nx_bsd_listen(INT sockID, INT backlog) {
    return listen(sockID, backlog);
}

INT nx_bsd_accept(INT sockID, struct sockaddr *clientAddr, INT *addrLen) {
    socklen_t posix_len = clientAddr ? (socklen_t)*addrLen : 0;
    int new_fd = accept(sockID, clientAddr, clientAddr ? &posix_len : NULL);
    if (new_fd < 0) {
        return new_fd;
    }
    if (clientAddr != NULL && addrLen != NULL) {
        *addrLen = (INT)posix_len;
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
                  struct sockaddr *destAddr, INT destAddrLen) {
    return (INT)sendto(sockID, msg, (size_t)msgLen, flags,
                       destAddr, (socklen_t)destAddrLen);
}

INT nx_bsd_recvfrom(INT sockID, CHAR *buf, INT bufLen, INT flags,
                    struct sockaddr *srcAddr, INT *addrLen) {
    socklen_t posix_len = (srcAddr && addrLen) ? (socklen_t)*addrLen : 0;
    int n = (int)recvfrom(sockID, buf, (size_t)bufLen, flags,
                          srcAddr, (srcAddr && addrLen) ? &posix_len : NULL);
    if (n < 0) {
        return n;
    }
    if (srcAddr != NULL && addrLen != NULL) {
        *addrLen = (INT)posix_len;
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
    if (*level == 2 /* NetX IPPROTO_IP */) {
        *level = 0; /* Linux IPPROTO_IP */
        switch (*optName) {
            case 27: *optName = 32; break; /* IP_MULTICAST_IF */
            case 28: *optName = 33; break; /* IP_MULTICAST_TTL */
            case 29: *optName = 34; break; /* IP_MULTICAST_LOOP */
            case 32: *optName = 35; break; /* IP_ADD_MEMBERSHIP */
            case 33: *optName = 36; break; /* IP_DROP_MEMBERSHIP */
            default: return -1;
        }
    }
    return 0;
}

INT nx_bsd_setsockopt(INT sockID, INT level, INT optName,
                      const VOID *optValue, INT optLen) {
    if (translate_sockopt(&level, &optName) < 0) return -1;

    /* Phase 97.4.threadx-linux — NetX BSD passes SO_RCVTIMEO /
     * SO_SNDTIMEO as `INT` milliseconds, but Linux's POSIX socket
     * layer expects `struct timeval`. Forwarding the 4-byte INT
     * verbatim makes Linux interpret the milliseconds as
     * `tv_sec` (so a NetX 1 ms timeout becomes a 1-second Linux
     * block), starving the cooperative recv loops. Convert here
     * — `tv_usec` carries the millisecond fraction. */
    if (level == SOL_SOCKET && (optName == SO_RCVTIMEO || optName == SO_SNDTIMEO)
        && optLen == (INT)sizeof(INT))
    {
        INT ms = *(const INT *)optValue;
        struct timeval tv = {
            .tv_sec = ms / 1000,
            .tv_usec = (ms % 1000) * 1000,
        };
        return setsockopt(sockID, level, optName, &tv, (socklen_t)sizeof(tv));
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
    return fcntl(sockID, (int)flagType, (int)options);
}

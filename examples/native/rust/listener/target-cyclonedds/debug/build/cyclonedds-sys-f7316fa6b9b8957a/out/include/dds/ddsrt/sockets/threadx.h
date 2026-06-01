/*
 * Copyright(c) 2026 ZettaScale Technology and others
 *
 * SPDX-License-Identifier: EPL-2.0 OR BSD-3-Clause
 */
#ifndef DDSRT_SOCKETS_THREADX_H
#define DDSRT_SOCKETS_THREADX_H

#include <nxd_bsd.h>
#include <stddef.h>

#include "dds/ddsrt/iovec.h"

#if defined(__cplusplus)
extern "C" {
#endif

typedef INT ddsrt_socket_t;
#define DDSRT_INVALID_SOCKET (-1)
#define PRIdSOCK "d"

#define DDSRT_HAVE_SSM 0

#ifndef IFF_UP
#define IFF_UP 0x1
#endif
#ifndef IFF_BROADCAST
#define IFF_BROADCAST 0x2
#endif
#ifndef IFF_LOOPBACK
#define IFF_LOOPBACK 0x8
#endif
#ifndef IFF_POINTOPOINT
#define IFF_POINTOPOINT 0x10
#endif
#ifndef IFF_MULTICAST
#define IFF_MULTICAST 0x1000
#endif
#ifndef MSG_NOSIGNAL
#define MSG_NOSIGNAL 0
#endif
#ifndef MSG_TRUNC
#define MSG_TRUNC 0x20
#endif
#ifndef INADDR_LOOPBACK
#define INADDR_LOOPBACK 0x7f000001UL
#endif
#ifndef IN_MULTICAST
#define IN_MULTICAST(a) ((((uint32_t)(a)) & 0xf0000000u) == 0xe0000000u)
#endif

typedef struct nx_bsd_msghdr ddsrt_msghdr_t;
#define DDSRT_MSGHDR_FLAGS 1

#define sockaddr nx_bsd_sockaddr
#define sockaddr_storage nx_bsd_sockaddr_storage
#define sockaddr_in nx_bsd_sockaddr_in
#define sockaddr_in6 nx_bsd_sockaddr_in6
#define in_addr nx_bsd_in_addr
#define in_addr_t nx_bsd_in_addr_t
#define iovec nx_bsd_iovec
#define msghdr nx_bsd_msghdr
#define addrinfo nx_bsd_addrinfo
#define socklen_t nx_bsd_socklen_t
#define fd_set nx_bsd_fd_set
#define timeval nx_bsd_timeval
#define ip_mreq nx_bsd_ip_mreq
#define linger nx_bsd_linger

#define inet_addr nx_bsd_inet_addr
#define inet_aton nx_bsd_inet_aton
#define inet_ntoa nx_bsd_inet_ntoa
#define inet_pton nx_bsd_inet_pton
#define inet_ntop nx_bsd_inet_ntop
#define getaddrinfo nx_bsd_getaddrinfo
#define freeaddrinfo nx_bsd_freeaddrinfo
#define getnameinfo nx_bsd_getnameinfo
#define listen nx_bsd_listen
#define getpeername nx_bsd_getpeername
#define shutdown(sock, how) (0)

#define FD_SET NX_BSD_FD_SET
#define FD_CLR NX_BSD_FD_CLR
#define FD_ISSET NX_BSD_FD_ISSET
#define FD_ZERO NX_BSD_FD_ZERO

#if defined(__cplusplus)
}
#endif

#endif /* DDSRT_SOCKETS_THREADX_H */

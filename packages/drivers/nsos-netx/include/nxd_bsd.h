#ifndef NSOS_NXD_BSD_H
#define NSOS_NXD_BSD_H

#include <fcntl.h>
#include <netinet/in.h>
#include <netinet/tcp.h>
#include <sys/select.h>
#include <sys/socket.h>
#include <sys/time.h>

#include "tx_api.h"

#ifdef __cplusplus
extern "C" {
#endif

#define NX_AF_UNSPEC 0
#define NX_AF_INET 2
#define NX_AF_INET6 3
#define NX_IPPROTO_IP 2
#define NX_IP_ADD_MEMBERSHIP 32
#define NX_IP_DROP_MEMBERSHIP 33
#define NX_O_NONBLOCK 0x4000

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

struct nx_bsd_linger {
    INT l_onoff;
    INT l_linger;
};

INT nx_bsd_socket(INT domain, INT type, INT protocol);
INT nx_bsd_soc_close(INT sockID);
INT nx_bsd_bind(INT sockID, const struct nx_bsd_sockaddr *addr, INT addrLen);
INT nx_bsd_connect(INT sockID, struct nx_bsd_sockaddr *addr, INT addrLen);
INT nx_bsd_listen(INT sockID, INT backlog);
INT nx_bsd_accept(INT sockID, struct nx_bsd_sockaddr *clientAddr, INT *addrLen);
INT nx_bsd_send(INT sockID, const CHAR *msg, INT msgLen, INT flags);
INT nx_bsd_recv(INT sockID, VOID *buf, INT bufLen, INT flags);
INT nx_bsd_sendto(INT sockID, CHAR *msg, INT msgLen, INT flags,
                  struct nx_bsd_sockaddr *destAddr, INT destAddrLen);
INT nx_bsd_recvfrom(INT sockID, CHAR *buf, INT bufLen, INT flags,
                    struct nx_bsd_sockaddr *srcAddr, INT *addrLen);
INT nx_bsd_setsockopt(INT sockID, INT level, INT optName,
                      const VOID *optValue, INT optLen);
INT nx_bsd_getsockopt(INT sockID, INT level, INT optName,
                      VOID *optValue, INT *optLen);
INT nx_bsd_select(INT nfds, fd_set *readfds, fd_set *writefds,
                  fd_set *exceptfds, struct timeval *timeout);
INT nx_bsd_fcntl(INT sockID, UINT flagType, UINT options);
INT nx_bsd_shutdown(INT sockID, INT how);
INT nx_bsd_getaddrinfo(const CHAR *node,
                       const CHAR *service,
                       const struct nx_bsd_addrinfo *hints,
                       struct nx_bsd_addrinfo **res);
VOID nx_bsd_freeaddrinfo(struct nx_bsd_addrinfo *res);

#ifdef __cplusplus
}
#endif

#endif /* NSOS_NXD_BSD_H */

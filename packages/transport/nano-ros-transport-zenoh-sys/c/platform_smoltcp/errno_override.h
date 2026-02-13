#ifndef _ERRNO_OVERRIDE_H
#define _ERRNO_OVERRIDE_H

/* Minimal errno.h for bare-metal RISC-V (no TLS).
 * Shadows picolibc's errno.h which uses __thread. */
extern int errno;

#define EPERM    1
#define ENOENT   2
#define EIO      5
#define ENOMEM  12
#define EACCES  13
#define EFAULT  14
#define EBUSY   16
#define EEXIST  17
#define EINVAL  22
#define ENOSPC  28
#define ERANGE  34
#define ENOSYS  88
#define ENOMSG  91
#define ENOTSUP 95
#define EADDRINUSE 98
#define EADDRNOTAVAIL 99
#define ENETUNREACH 101
#define ECONNABORTED 103
#define ECONNRESET 104
#define ENOBUFS 105
#define EISCONN 106
#define ENOTCONN 107
#define ETIMEDOUT 110
#define ECONNREFUSED 111
#define EALREADY 114
#define EINPROGRESS 115
#define EAGAIN  11
#define EWOULDBLOCK EAGAIN

#endif

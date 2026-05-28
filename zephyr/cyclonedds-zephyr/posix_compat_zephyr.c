/*
 * Copyright (c) 2026, NEWSLab NTU.
 * SPDX-License-Identifier: EPL-2.0 OR BSD-3-Clause
 *
 * Phase 190.G — POSIX symbols Cyclone DDS' `posix` ddsrt backend
 * (src/ddsrt/src/sockets/posix/socket.c + threads/posix/threads.c, both kept
 * in the Zephyr build) references but that the Zephyr POSIX layer does not
 * provide on every line — notably the 3.5/3.7-LTS profile used by the
 * Autoware safety-island fvp_baser_aemv8r target. native_sim links these from
 * the host libc; a bare Zephyr target has nothing.
 *
 * All definitions are `__attribute__((weak))` so a Zephyr that DOES ship a
 * real implementation (4.x grew several of these) wins the link and these
 * become inert. They are only consulted when the strong symbol is absent.
 */
#ifdef __ZEPHYR__

#include <errno.h>
#include <pthread.h>
#include <signal.h>
#include <sys/socket.h>

/* newlib's `_open_r` (pulled into the link by libc init / a stray fopen path)
 * needs the `_open` syscall stub. Zephyr's newlib libc-hooks only defines
 * `_open`/`_read`/`_write` under `#ifndef CONFIG_POSIX_API`; this profile sets
 * CONFIG_POSIX_API=y, so they are gated out, yet the SDK libc.a still
 * references `_open`. Cyclone runs with DDSRT_HAVE_FILESYSTEM=0, so this is
 * never exercised for real file I/O — a -1/ENOSYS stub satisfies the link. */
__attribute__((weak)) int _open(const char *name, int mode)
{
    (void) name;
    (void) mode;
    errno = ENOSYS;
    return -1;
}

/* recvmsg: Cyclone's socket.c carries this exact single-iovec shim under
 * `#if LWIP_SOCKET`, but Zephyr is not lwIP, so it is compiled out and the
 * external `recvmsg` is unresolved. Cyclone only ever calls it with one iovec
 * and no control data (it asserts so), so forward to recvfrom — identical to
 * Cyclone's own lwIP path. */
__attribute__((weak)) ssize_t recvmsg(int sockfd, struct msghdr *msg, int flags)
{
    if (msg == NULL || msg->msg_iovlen != 1) {
        errno = EINVAL;
        return -1;
    }
    msg->msg_flags = 0;
    return recvfrom(sockfd, msg->msg_iov[0].iov_base, msg->msg_iov[0].iov_len,
                    flags, (struct sockaddr *) msg->msg_name, &msg->msg_namelen);
}

/* pthread scheduling-attribute setters: Zephyr threads are all system-scope
 * with explicit priorities, so these POSIX scheduling knobs are advisory.
 * Cyclone sets them when creating its threads; accept and ignore. */
__attribute__((weak)) int pthread_attr_setscope(pthread_attr_t *attr, int scope)
{
    (void) attr;
    (void) scope;
    return 0;
}

__attribute__((weak)) int pthread_attr_setinheritsched(pthread_attr_t *attr,
                                                        int inheritsched)
{
    (void) attr;
    (void) inheritsched;
    return 0;
}

/* pthread_sigmask: Zephyr has no POSIX signal delivery to threads, so masking
 * is a no-op. Cyclone blocks signals on its worker threads defensively. */
__attribute__((weak)) int pthread_sigmask(int how, const sigset_t *set,
                                          sigset_t *oldset)
{
    (void) how;
    (void) set;
    (void) oldset;
    return 0;
}

#endif /* __ZEPHYR__ */

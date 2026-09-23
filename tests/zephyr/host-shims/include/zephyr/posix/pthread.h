/*
 * Host stand-in for <zephyr/posix/pthread.h> (phase-460 W6).
 *
 * Nothing to emulate: Zephyr's POSIX option IS pthreads, and the host has the
 * real thing. Forwarding rather than stubbing is the point of the whole
 * harness -- the slot table is exercised against genuine
 * pthread_create/join/detach semantics, including a stack handed back to a
 * second thread after the first was joined.
 */
#ifndef NROS_HOST_SHIMS_ZEPHYR_POSIX_PTHREAD_H
#define NROS_HOST_SHIMS_ZEPHYR_POSIX_PTHREAD_H

#include <pthread.h>

#endif /* NROS_HOST_SHIMS_ZEPHYR_POSIX_PTHREAD_H */

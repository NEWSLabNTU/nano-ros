/*
 * Host stand-in for <zephyr/random/random.h> (phase-460 W6). See
 * tests/zephyr/host-shims/include/zephyr/kernel.h for the rule this tree
 * follows.
 */
#ifndef NROS_HOST_SHIMS_ZEPHYR_RANDOM_RANDOM_H
#define NROS_HOST_SHIMS_ZEPHYR_RANDOM_RANDOM_H

#include <stddef.h>
#include <stdlib.h>

/* Not entropy, and deliberately not pretending to be: nothing in the
 * thread-slot test reads a random byte. Zeroing rather than aborting only
 * because a caller would be in a fill loop, not a control path. */
static inline void sys_rand_get(void* dst, size_t len) {
    unsigned char* p = (unsigned char*)dst;
    for (size_t i = 0; i < len; i++) {
        p[i] = 0u;
    }
}

#endif /* NROS_HOST_SHIMS_ZEPHYR_RANDOM_RANDOM_H */

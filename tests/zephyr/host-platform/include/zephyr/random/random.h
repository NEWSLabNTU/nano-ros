/*
 * Host stand-in for <zephyr/random/random.h> (phase-460 W7). See
 * tests/zephyr/host-platform/include/zephyr/kernel.h for the rule this tree
 * follows.
 */
#ifndef NROS_HOST_PLATFORM_ZEPHYR_RANDOM_RANDOM_H
#define NROS_HOST_PLATFORM_ZEPHYR_RANDOM_RANDOM_H

#include <stddef.h>
#include <stdint.h>

uint32_t sys_rand32_get(void);
void sys_rand_get(void* dst, size_t len);

#endif /* NROS_HOST_PLATFORM_ZEPHYR_RANDOM_RANDOM_H */

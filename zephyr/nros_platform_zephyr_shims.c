/**
 * @file nros_platform_zephyr_shims.c
 * @brief Real-symbol wrappers around Zephyr kernel inlines.
 *
 * Several Zephyr APIs are declared `static inline` in headers
 * (`k_msleep`, `k_uptime_get`, `sys_rand_get`, etc.) and have no exported
 * symbol. Rust FFI can only call real symbols, so we wrap them here —
 * this TU is compiled by the Zephyr module build and exports the real
 * functions that `nros-platform-zephyr` declares as `extern "C"`.
 *
 * Real-function Zephyr APIs (`k_malloc`, `k_free`, `k_usleep`,
 * `sys_rand32_get`, `pthread_*`) are called directly from Rust and do
 * not need wrappers.
 */

#include <stddef.h>
#include <stdint.h>

#include <zephyr/kernel.h>
#include <zephyr/random/random.h>
#include <zephyr/posix/pthread.h>

int64_t nros_zephyr_uptime_ms(void) {
    return k_uptime_get();
}

int32_t nros_zephyr_msleep(int32_t ms) {
    return k_msleep(ms);
}

void nros_zephyr_rand_fill(void *dst, size_t len) {
    sys_rand_get(dst, len);
}

/* ── Thread creation with Zephyr-managed stacks ──────────────────────────
 *
 * Zephyr's pthread_create requires a pthread_attr_t with an explicit
 * stack (pthread_attr_setstack) — NULL attr returns EINVAL.
 *
 * We pre-allocate a stack array using K_THREAD_STACK_ARRAY_DEFINE
 * (same approach as zenoh-pico's zephyr/system.c) and hand one stack
 * to each pthread_create call. The Rust shim calls this wrapper
 * instead of pthread_create directly.
 */

#ifndef NROS_ZEPHYR_MAX_THREADS
#define NROS_ZEPHYR_MAX_THREADS 8
#endif

#ifndef NROS_ZEPHYR_STACK_SIZE
#define NROS_ZEPHYR_STACK_SIZE CONFIG_MAIN_STACK_SIZE
#endif

K_THREAD_STACK_ARRAY_DEFINE(nros_thread_stacks, NROS_ZEPHYR_MAX_THREADS,
                            NROS_ZEPHYR_STACK_SIZE);
static int nros_thread_index;

int nros_zephyr_task_create(pthread_t *thread,
                            void *(*entry)(void *),
                            void *arg) {
    if (nros_thread_index >= NROS_ZEPHYR_MAX_THREADS) {
        return -1; /* no more stack slots */
    }

    pthread_attr_t attr;
    (void)pthread_attr_init(&attr);
    (void)pthread_attr_setstack(&attr,
                                &nros_thread_stacks[nros_thread_index++],
                                NROS_ZEPHYR_STACK_SIZE);

    int ret = pthread_create(thread, &attr, entry, arg);
    (void)pthread_attr_destroy(&attr);
    return ret;
}

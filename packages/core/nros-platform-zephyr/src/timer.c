/*
 * Phase 121.6.zephyr-c — Zephyr implementation of the canonical
 * timer ABI declared in `<nros/platform_timer.h>`.
 *
 * Backed by `k_timer_init` / `k_timer_start` / `k_timer_stop`.
 * Zephyr's k_timer callback runs in the system clock ISR context;
 * the trampoline below dispatches the user callback there. Bodies
 * must be short + use atomics for shared state (per the standard
 * <nros/platform_timer.h> contract).
 */

#include <nros/platform_timer.h>

#include <zephyr/kernel.h>

#include <stdatomic.h>
#include <stddef.h>
#include <stdint.h>

typedef struct {
    struct k_timer                 kernel;
    nros_platform_timer_callback_t callback;
    void                          *user_data;
    atomic_int                     fired;
    atomic_int                     cancelled;
    int                             periodic;
} nros_zephyr_timer_t;

static void timer_trampoline(struct k_timer *kernel) {
    nros_zephyr_timer_t *t = CONTAINER_OF(kernel, nros_zephyr_timer_t, kernel);
    if (atomic_load_explicit(&t->cancelled, memory_order_acquire)) {
        return;
    }
    atomic_store_explicit(&t->fired, 1, memory_order_release);
    if (t->callback != NULL) {
        t->callback(t->user_data);
    }
}

static void *create_timer(uint32_t value_us, int periodic,
                          nros_platform_timer_callback_t callback,
                          void *user_data) {
    if (callback == NULL || value_us == 0) return NULL;
    nros_zephyr_timer_t *t = (nros_zephyr_timer_t *) k_malloc(sizeof(*t));
    if (t == NULL) return NULL;

    t->callback  = callback;
    t->user_data = user_data;
    t->periodic  = periodic;
    atomic_init(&t->fired, 0);
    atomic_init(&t->cancelled, 0);

    k_timer_init(&t->kernel, timer_trampoline, NULL);

    k_timeout_t duration = K_USEC((int32_t) value_us);
    k_timeout_t period   = periodic ? duration : K_NO_WAIT;
    k_timer_start(&t->kernel, duration, period);
    return (void *) t;
}

void *nros_platform_timer_create_periodic(uint32_t period_us,
                                          nros_platform_timer_callback_t callback,
                                          void *user_data) {
    return create_timer(period_us, /* periodic = */ 1, callback, user_data);
}

void *nros_platform_timer_create_oneshot(uint32_t timeout_us,
                                         nros_platform_timer_callback_t callback,
                                         void *user_data) {
    return create_timer(timeout_us, /* periodic = */ 0, callback, user_data);
}

void nros_platform_timer_destroy(void *handle) {
    if (handle == NULL) return;
    nros_zephyr_timer_t *t = (nros_zephyr_timer_t *) handle;
    atomic_store_explicit(&t->cancelled, 1, memory_order_release);
    k_timer_stop(&t->kernel);
    k_free(t);
}

int8_t nros_platform_timer_cancel(void *handle) {
    if (handle == NULL) return -1;
    nros_zephyr_timer_t *t = (nros_zephyr_timer_t *) handle;
    int prev_fired = atomic_load_explicit(&t->fired, memory_order_acquire);
    atomic_store_explicit(&t->cancelled, 1, memory_order_release);
    k_timer_stop(&t->kernel);
    return prev_fired == 0 ? 1 : 0;
}

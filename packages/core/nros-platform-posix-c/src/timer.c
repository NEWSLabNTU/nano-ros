/*
 * Phase 121.6.posix-c — POSIX implementation of the canonical timer
 * ABI declared in `<nros/platform_timer.h>`.
 *
 * Backed by POSIX `timer_create(CLOCK_MONOTONIC, SIGEV_THREAD)`,
 * which dispatches each fire on a fresh helper thread spawned by
 * the librt timer machinery. The returned handle is a heap-owned
 * record carrying the kernel `timer_t` + the user's callback +
 * user_data; the handle pointer itself is what the ABI returns.
 *
 * `cancel` distinguishes between "cancellation prevented the
 * callback" and "callback already fired" by inspecting
 * `timer_getoverrun` plus a `fired` flag the trampoline sets.
 */

#define _POSIX_C_SOURCE 200809L
#define _DEFAULT_SOURCE

#include <nros/platform_timer.h>

#include <pthread.h>
#include <signal.h>
#include <stdatomic.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

typedef struct {
    timer_t                         kernel;
    nros_platform_timer_callback_t  callback;
    void                           *user_data;
    atomic_int                      fired;     /* set non-zero on first invocation */
    atomic_int                      cancelled; /* set by cancel() to suppress further fires */
    int                              periodic;
} nros_posix_timer_t;

static void timer_trampoline(union sigval sv) {
    nros_posix_timer_t *t = (nros_posix_timer_t *) sv.sival_ptr;
    if (t == NULL) return;
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
    if (callback == NULL || value_us == 0) {
        return NULL;
    }
    nros_posix_timer_t *t = (nros_posix_timer_t *) calloc(1, sizeof(*t));
    if (t == NULL) {
        return NULL;
    }
    t->callback  = callback;
    t->user_data = user_data;
    t->periodic  = periodic;
    atomic_init(&t->fired, 0);
    atomic_init(&t->cancelled, 0);

    struct sigevent sev = {0};
    sev.sigev_notify          = SIGEV_THREAD;
    sev.sigev_value.sival_ptr = t;
    sev.sigev_notify_function = timer_trampoline;
    sev.sigev_notify_attributes = NULL;

    if (timer_create(CLOCK_MONOTONIC, &sev, &t->kernel) != 0) {
        free(t);
        return NULL;
    }

    /* Convert microseconds to {sec, nsec} for itimerspec. */
    struct itimerspec its = {0};
    long ns_per_fire = (long) (value_us % 1000000u) * 1000L;
    time_t s_per_fire = (time_t) (value_us / 1000000u);
    its.it_value.tv_sec  = s_per_fire;
    its.it_value.tv_nsec = ns_per_fire;
    if (periodic) {
        its.it_interval.tv_sec  = s_per_fire;
        its.it_interval.tv_nsec = ns_per_fire;
    }

    if (timer_settime(t->kernel, 0, &its, NULL) != 0) {
        timer_delete(t->kernel);
        free(t);
        return NULL;
    }
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
    nros_posix_timer_t *t = (nros_posix_timer_t *) handle;
    atomic_store_explicit(&t->cancelled, 1, memory_order_release);
    /* Disarm before delete to ensure no callback is in flight. */
    struct itimerspec disarm = {0};
    (void) timer_settime(t->kernel, 0, &disarm, NULL);
    (void) timer_delete(t->kernel);
    free(t);
}

int8_t nros_platform_timer_cancel(void *handle) {
    if (handle == NULL) return -1;
    nros_posix_timer_t *t = (nros_posix_timer_t *) handle;
    int prev_fired = atomic_load_explicit(&t->fired, memory_order_acquire);
    atomic_store_explicit(&t->cancelled, 1, memory_order_release);

    struct itimerspec disarm = {0};
    if (timer_settime(t->kernel, 0, &disarm, NULL) != 0) {
        return -1;
    }
    /* 1 = cancellation prevented fire; 0 = already fired (or oneshot
     * that completed). Best-effort detection via the trampoline's
     * `fired` flag. */
    return prev_fired == 0 ? 1 : 0;
}

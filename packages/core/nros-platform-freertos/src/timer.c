/*
 * Phase 121.6.freertos-c — FreeRTOS implementation of the canonical
 * timer ABI declared in `<nros/platform_timer.h>`.
 *
 * Backed by FreeRTOS software timers (xTimerCreate / xTimerStart /
 * xTimerStop / xTimerDelete). The timer's ID field carries the
 * caller's (callback, user_data) pair; the trampoline reads it back
 * inside the timer-task context and dispatches.
 *
 * FreeRTOSConfig.h requirements:
 *   - configUSE_TIMERS                 1
 *   - configTIMER_TASK_PRIORITY        (whatever your app uses)
 *   - configTIMER_QUEUE_LENGTH         ≥ 8 (recommended)
 *   - configTIMER_TASK_STACK_DEPTH     ≥ configMINIMAL_STACK_SIZE * 2
 */

#include <nros/platform_timer.h>

#include <FreeRTOS.h>
#include <timers.h>

#include <stddef.h>
#include <stdint.h>
#include <stdlib.h>

typedef struct {
    TimerHandle_t                   kernel;
    nros_platform_timer_callback_t  callback;
    void                           *user_data;
    /* atomic_int is not portable across all FreeRTOS ports; the
     * timer-task is the only thread mutating these flags, callers
     * read via cancel() which also runs in the timer-task because
     * xTimerStop is queued. Plain volatile ints suffice. */
    volatile int                    fired;
    volatile int                    cancelled;
    int                              periodic;
} nros_freertos_timer_t;

static void timer_trampoline(TimerHandle_t kernel) {
    nros_freertos_timer_t *t = (nros_freertos_timer_t *) pvTimerGetTimerID(kernel);
    if (t == NULL) return;
    if (t->cancelled) return;
    t->fired = 1;
    if (t->callback != NULL) {
        t->callback(t->user_data);
    }
}

static void *create_timer(uint32_t value_us, int periodic,
                          nros_platform_timer_callback_t callback,
                          void *user_data) {
    if (callback == NULL || value_us == 0) return NULL;

    nros_freertos_timer_t *t = (nros_freertos_timer_t *) pvPortMalloc(sizeof(*t));
    if (t == NULL) return NULL;
    t->callback  = callback;
    t->user_data = user_data;
    t->fired     = 0;
    t->cancelled = 0;
    t->periodic  = periodic;

    /* Round up sub-millisecond periods to one tick. */
    uint32_t period_ms = (value_us + 999u) / 1000u;
    if (period_ms == 0) period_ms = 1;

    t->kernel = xTimerCreate(
        "nros_timer",
        pdMS_TO_TICKS(period_ms),
        periodic ? pdTRUE : pdFALSE,
        (void *) t,
        timer_trampoline);
    if (t->kernel == NULL) {
        vPortFree(t);
        return NULL;
    }
    if (xTimerStart(t->kernel, 0) != pdPASS) {
        xTimerDelete(t->kernel, 0);
        vPortFree(t);
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
    nros_freertos_timer_t *t = (nros_freertos_timer_t *) handle;
    t->cancelled = 1;
    if (t->kernel != NULL) {
        (void) xTimerStop(t->kernel, 0);
        (void) xTimerDelete(t->kernel, 0);
    }
    vPortFree(t);
}

int8_t nros_platform_timer_cancel(void *handle) {
    if (handle == NULL) return -1;
    nros_freertos_timer_t *t = (nros_freertos_timer_t *) handle;
    int prev_fired = t->fired;
    t->cancelled = 1;
    if (t->kernel == NULL) return -1;
    if (xTimerStop(t->kernel, 0) != pdPASS) return -1;
    return prev_fired == 0 ? 1 : 0;
}

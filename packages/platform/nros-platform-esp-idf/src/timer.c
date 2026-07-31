/*
 * Phase 121.6.esp-idf-c — ESP-IDF implementation of the canonical
 * timer ABI declared in `<nros/platform_timer.h>`.
 *
 * Backed by `esp_timer_create` / `esp_timer_start_periodic` /
 * `esp_timer_start_once` / `esp_timer_stop` / `esp_timer_delete`.
 * Callbacks fire from the esp_timer task by default (lower latency
 * than FreeRTOS software timers).
 */

#include <nros/platform_timer.h>

#include <esp_timer.h>

#include <stdatomic.h>
#include <stddef.h>
#include <stdint.h>
#include <stdlib.h>

typedef struct {
    esp_timer_handle_t              kernel;
    nros_platform_timer_callback_t  callback;
    void                           *user_data;
    atomic_int                      fired;
    atomic_int                      cancelled;
    int                              periodic;
} nros_esp_timer_t;

static void timer_trampoline(void *arg) {
    nros_esp_timer_t *t = (nros_esp_timer_t *) arg;
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
    if (callback == NULL || value_us == 0) return NULL;
    nros_esp_timer_t *t = (nros_esp_timer_t *) calloc(1, sizeof(*t));
    if (t == NULL) return NULL;

    t->callback  = callback;
    t->user_data = user_data;
    t->periodic  = periodic;
    atomic_init(&t->fired, 0);
    atomic_init(&t->cancelled, 0);

    esp_timer_create_args_t args = {
        .callback = timer_trampoline,
        .arg = t,
        .dispatch_method = ESP_TIMER_TASK,
        .name = "nros_timer",
        .skip_unhandled_events = false,
    };
    if (esp_timer_create(&args, &t->kernel) != 0) {
        free(t);
        return NULL;
    }
    int rc = periodic
        ? esp_timer_start_periodic(t->kernel, (uint64_t) value_us)
        : esp_timer_start_once(t->kernel, (uint64_t) value_us);
    if (rc != 0) {
        esp_timer_delete(t->kernel);
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
    nros_esp_timer_t *t = (nros_esp_timer_t *) handle;
    atomic_store_explicit(&t->cancelled, 1, memory_order_release);
    (void) esp_timer_stop(t->kernel);
    (void) esp_timer_delete(t->kernel);
    free(t);
}

int8_t nros_platform_timer_cancel(void *handle) {
    if (handle == NULL) return -1;
    nros_esp_timer_t *t = (nros_esp_timer_t *) handle;
    int prev_fired = atomic_load_explicit(&t->fired, memory_order_acquire);
    atomic_store_explicit(&t->cancelled, 1, memory_order_release);
    if (esp_timer_stop(t->kernel) != 0) return -1;
    return prev_fired == 0 ? 1 : 0;
}

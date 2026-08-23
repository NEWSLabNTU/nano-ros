/*
 * Phase 121.3.esp-idf — native C implementation of the canonical
 * platform ABI for Espressif ESP-IDF (FreeRTOS-based).
 *
 * The threading layer is FreeRTOS (ESP-IDF ships its own fork with
 * SMP support on ESP32/ESP32-S3); the rest uses ESP-IDF-specific
 * helpers where they're better than the FreeRTOS defaults:
 *
 *   - Clock     — esp_timer_get_time() gives microsecond resolution
 *                 (FreeRTOS xTaskGetTickCount is tick-granular only).
 *   - Allocation — malloc / free; ESP-IDF redirects these to
 *                 heap_caps_malloc(MALLOC_CAP_DEFAULT) so they work
 *                 portably for both internal and PSRAM heaps.
 *   - Sleep     — vTaskDelay (FreeRTOS), with esp_rom_delay_us for
 *                 sub-tick spins.
 *   - Yield     — taskYIELD() (ESP-IDF cooperative yield).
 *   - Random    — esp_random() — wraps the hardware RNG when WiFi
 *                 / BT are active; falls back to a PRNG otherwise.
 *                 esp_fill_random for byte fills.
 *   - Time      — time(NULL) reads the system clock; returns 0 when
 *                 no time source is configured (SNTP / RTC).
 *   - Tasks     — xTaskCreate (same as FreeRTOS-C).
 *   - Mutexes   — xSemaphoreCreate{Mutex,RecursiveMutex} (FreeRTOS).
 *   - Condvars  — same mutex + counting-semaphore pattern as
 *                 FreeRTOS-C.
 *
 * Storage layouts (`ZTask`, `ZMutex`, `ZCondvar`) match the Rust
 * `nros-platform-freertos`'s types exactly.
 */

#include <nros/platform.h>

#include <freertos/FreeRTOS.h>
#include <freertos/task.h>
#include <freertos/semphr.h>

#include <esp_heap_caps.h>
#include <esp_random.h>
#include <esp_timer.h>
#include <esp_rom_sys.h>
/* phase-366 — `esp_system_abort` for the fatal path. */
#include <esp_system.h>

#include <stddef.h>
#include <stdint.h>
#include <string.h>
#include <stdlib.h>
#include <time.h>

/* ---- Clock — esp_timer_get_time is monotonic, microseconds ---- */

/* RFC-0073 — esp_timer is a free-running 64-bit hardware counter with a
 * microsecond timebase, so nanoseconds are a constant multiply and the
 * resolution it can honestly claim is 1 us. */
uint64_t nros_platform_clock_ns(void) {
    int64_t us = esp_timer_get_time();
    return us < 0 ? 0 : (uint64_t) us * 1000ULL;
}

uint64_t nros_platform_clock_resolution_ns(void) {
    return 1000ULL;
}

/* issue 0758 — no wall-clock source on this platform, so `0` is the honest
 * answer, not a placeholder. The header makes `0` mean "no epoch here", which
 * lets a caller keep stamping boot-relative time knowingly instead of
 * publishing a confidently wrong absolute one.
 *
 * A port gains a real epoch by acquiring one (SNTP, an RTC handoff) and
 * returning it here; until then this is correct rather than unfinished.
 * Monotonic time is `nros_platform_clock_ns` and is unaffected. */
uint64_t nros_platform_epoch_us(void) {
    return 0;
}

/* ---- Allocation — ESP-IDF redirects libc malloc to heap_caps ---- */

void *nros_platform_alloc(size_t size) {
    return size == 0 ? NULL : malloc(size);
}

void *nros_platform_realloc(void *ptr, size_t size) {
    if (size == 0) {
        free(ptr);
        return NULL;
    }
    return realloc(ptr, size);
}

void nros_platform_dealloc(void *ptr) {
    free(ptr);
}

/* ---- Heap stats (phase-230 1b / RFC-0034 D7) ----
 * ESP-IDF heap_caps: used = total − free for the default (8-bit) caps. */
size_t nros_platform_heap_used_bytes(void) {
    size_t total = heap_caps_get_total_size(MALLOC_CAP_DEFAULT);
    size_t freeb = heap_caps_get_free_size(MALLOC_CAP_DEFAULT);
    return total >= freeb ? total - freeb : 0u;
}

size_t nros_platform_heap_total_bytes(void) {
    return heap_caps_get_total_size(MALLOC_CAP_DEFAULT);
}

/* ---- Sleep ---- */

void nros_platform_sleep_us(size_t us) {
    if (us == 0) return;
    /* Sub-tick sleep via the ROM busy-wait helper. */
    if (us < 1000U * (1000U / configTICK_RATE_HZ)) {
        esp_rom_delay_us((uint32_t) us);
        return;
    }
    vTaskDelay(pdMS_TO_TICKS((us + 999U) / 1000U));
}

void nros_platform_sleep_ms(size_t ms) {
    if (ms == 0) return;
    vTaskDelay(pdMS_TO_TICKS(ms));
}

void nros_platform_sleep_s(size_t s) {
    if (s == 0) return;
    vTaskDelay(pdMS_TO_TICKS(s * 1000U));
}

/* ---- Yield ---- */

void nros_platform_yield_now(void) {
    taskYIELD();
}

/* ---- Random ---- */

uint8_t  nros_platform_random_u8(void)   { return (uint8_t)  esp_random(); }
uint16_t nros_platform_random_u16(void)  { return (uint16_t) esp_random(); }
uint32_t nros_platform_random_u32(void)  { return esp_random(); }

uint64_t nros_platform_random_u64(void) {
    uint64_t hi = esp_random();
    uint64_t lo = esp_random();
    return (hi << 32) | lo;
}

void nros_platform_random_fill(void *buf, size_t len) {
    esp_fill_random(buf, len);
}

/* ---- Wall clock ---- */

uint64_t nros_platform_time_now_ns(void) {
    /* `time()` carries whole seconds only, so the sub-second field was always
     * 0 here — the ns ABI states that honestly rather than implying a
     * precision this source does not have. */
    time_t t = time(NULL);
    return t < 0 ? 0 : (uint64_t) t * 1000000000ULL;
}

/* ---- Tasks / Mutex / Condvar — FreeRTOS, same as FreeRTOS-C ----
 *
 * Storage layouts match the Rust `nros-platform-freertos::types`.
 */

typedef struct {
    void *handle;
    void *join_event;
    void *(*entry)(void *);
    void *arg;
} nros_esp_task_t;

/* phase-364 W3 — private task attr struct deleted; the ABI defines
 * `nros_platform_task_attr_t` for every port. */

typedef struct {
    void *handle;
} nros_esp_mutex_t;

typedef struct {
    void *mutex;
    void *sem;
    int32_t waiters;
} nros_esp_condvar_t;

static void esp_task_trampoline(void *raw) {
    nros_esp_task_t *t = (nros_esp_task_t *) raw;
    if (t->entry != NULL) {
        (void) t->entry(t->arg);
    }
    vTaskDelete(NULL);
}

void nros_platform_task_attr_init(nros_platform_task_attr_t *attr) {
    if (attr == NULL) {
        return;
    }
    memset(attr, 0, sizeof(*attr));
    attr->priority = INT32_MIN;
    attr->core = -1;
}

int8_t nros_platform_task_init(void *task, void *attr,
                               void *(*entry)(void *), void *arg) {
    /* phase-364 W1 — INVALID: a NULL where storage or an entry is required. */
    if (task == NULL || entry == NULL) return NROS_PLATFORM_RET_INVALID;
    nros_esp_task_t *t = (nros_esp_task_t *) task;
    t->handle = NULL;
    t->join_event = NULL;
    t->entry = entry;
    t->arg = arg;

    /* phase-364 W3 — one ABI-defined attribute struct; `NULL` = every default. */
    const nros_platform_task_attr_t *a = (const nros_platform_task_attr_t *) attr;
    const char *name = (a != NULL && a->name != NULL) ? a->name : "nros";
    /* phase-364 W5 — map the normalised band onto this kernel's range.
     *
     * FreeRTOS runs the SAME direction as the band (larger = more urgent), so
     * this is a scale, not an inversion — unlike ThreadX, where the identical
     * authored number means the opposite. `configMAX_PRIORITIES` is commonly 5
     * to 32, so distinct band values do collapse onto one level; the band is a
     * portable ORDERING, not a promise of 256 levels. */
    uint32_t priority = (uint32_t) (configMAX_PRIORITIES / 2);
    if (a != NULL) {
        if (NROS_PLATFORM_PRIORITY_IS_RAW(a->priority)) {
            priority = (uint32_t) NROS_PLATFORM_PRIORITY_RAW_VALUE(a->priority);
        } else if (a->priority != NROS_PLATFORM_PRIORITY_INHERIT && a->priority >= 0) {
            int32_t band = a->priority > NROS_PLATFORM_PRIORITY_MAX
                               ? NROS_PLATFORM_PRIORITY_MAX
                               : a->priority;
            priority = (uint32_t) ((band * (configMAX_PRIORITIES - 1)) / NROS_PLATFORM_PRIORITY_MAX);
        }
    }
    if (priority >= (uint32_t) configMAX_PRIORITIES) {
        priority = (uint32_t) configMAX_PRIORITIES - 1u;
    }
    size_t stack_bytes = (a != NULL && a->stack_bytes > 0u)
                             ? a->stack_bytes
                             : (size_t) (configMINIMAL_STACK_SIZE * sizeof(StackType_t));
    /* issue 0612 — raise a below-floor request rather than pass it through; see
     * the FreeRTOS port for why the quiet form of this is the worse one. This
     * port's `xTaskCreate` takes BYTES, so the floor is expressed in bytes too. */
    if (stack_bytes < (size_t) (configMINIMAL_STACK_SIZE * sizeof(StackType_t))) {
        stack_bytes = (size_t) (configMINIMAL_STACK_SIZE * sizeof(StackType_t));
    }
    /* phase-364 W3 — ESP-IDF's `xTaskCreate` takes the stack size in BYTES,
     * unlike vanilla FreeRTOS which takes words. The ABI speaks bytes, so this
     * port passes the value straight through — and that difference is exactly
     * why a shared field called `stack_depth` was a hazard. */
    uint32_t stack_depth = (uint32_t) stack_bytes;

    TaskHandle_t handle = NULL;
    if (xTaskCreate(esp_task_trampoline, name, stack_depth,
                    (void *) t, priority, &handle) != pdPASS) {
        /* phase-364 W1 — NOMEM; see the FreeRTOS port. */
        return NROS_PLATFORM_RET_NOMEM;
    }
    t->handle = handle;
    return 0;
}

int8_t nros_platform_task_join(void *task) {
    if (task == NULL) return -1;
    nros_esp_task_t *t = (nros_esp_task_t *) task;
    if (t->handle == NULL) return -1;
    while (eTaskGetState((TaskHandle_t) t->handle) != eDeleted) {
        vTaskDelay(1);
    }
    t->handle = NULL;
    return 0;
}

int8_t nros_platform_task_detach(void *task) {
    if (task == NULL) return -1;
    ((nros_esp_task_t *) task)->handle = NULL;
    return 0;
}

int8_t nros_platform_task_cancel(void *task) {
    if (task == NULL) return -1;
    nros_esp_task_t *t = (nros_esp_task_t *) task;
    if (t->handle == NULL) return -1;
    vTaskDelete((TaskHandle_t) t->handle);
    t->handle = NULL;
    return 0;
}

void nros_platform_task_exit(void) { vTaskDelete(NULL); }
void nros_platform_task_free(void **task) { (void) task; }

/* Phase 121.3.freertos-parity — `mutex_*` is implemented over a
 * recursive mutex so callers that take the same lock twice on the
 * same task don't deadlock. Matches the FreeRTOS C port + the
 * deleted Rust impl. */
int8_t nros_platform_mutex_init(void *m) {
    if (m == NULL) return -1;
    SemaphoreHandle_t h = xSemaphoreCreateRecursiveMutex();
    if (h == NULL) return -1;
    ((nros_esp_mutex_t *) m)->handle = h;
    return 0;
}

int8_t nros_platform_mutex_drop(void *m) {
    if (m == NULL) return -1;
    SemaphoreHandle_t h = ((nros_esp_mutex_t *) m)->handle;
    if (h == NULL) return -1;
    vSemaphoreDelete(h);
    ((nros_esp_mutex_t *) m)->handle = NULL;
    return 0;
}

int8_t nros_platform_mutex_lock(void *m) {
    if (m == NULL) return -1;
    return xSemaphoreTakeRecursive(((nros_esp_mutex_t *) m)->handle, portMAX_DELAY) == pdTRUE
        ? 0 : -1;
}

int8_t nros_platform_mutex_try_lock(void *m) {
    if (m == NULL) return -1;
    return xSemaphoreTakeRecursive(((nros_esp_mutex_t *) m)->handle, 0) == pdTRUE ? 0 : 1;
}

int8_t nros_platform_mutex_unlock(void *m) {
    if (m == NULL) return -1;
    return xSemaphoreGiveRecursive(((nros_esp_mutex_t *) m)->handle) == pdTRUE ? 0 : -1;
}

int8_t nros_platform_mutex_rec_init(void *m) {
    if (m == NULL) return -1;
    SemaphoreHandle_t h = xSemaphoreCreateRecursiveMutex();
    if (h == NULL) return -1;
    ((nros_esp_mutex_t *) m)->handle = h;
    return 0;
}

int8_t nros_platform_mutex_rec_drop(void *m)     { return nros_platform_mutex_drop(m); }

int8_t nros_platform_mutex_rec_lock(void *m) {
    if (m == NULL) return -1;
    return xSemaphoreTakeRecursive(((nros_esp_mutex_t *) m)->handle, portMAX_DELAY) == pdTRUE
        ? 0 : -1;
}

int8_t nros_platform_mutex_rec_try_lock(void *m) {
    if (m == NULL) return -1;
    return xSemaphoreTakeRecursive(((nros_esp_mutex_t *) m)->handle, 0) == pdTRUE ? 0 : 1;
}

int8_t nros_platform_mutex_rec_unlock(void *m) {
    if (m == NULL) return -1;
    return xSemaphoreGiveRecursive(((nros_esp_mutex_t *) m)->handle) == pdTRUE ? 0 : -1;
}

int8_t nros_platform_condvar_init(void *cv) {
    if (cv == NULL) return -1;
    nros_esp_condvar_t *c = (nros_esp_condvar_t *) cv;
    c->mutex = (void *) xSemaphoreCreateMutex();
    c->sem   = (void *) xSemaphoreCreateCounting(UINT32_MAX, 0);
    c->waiters = 0;
    if (c->mutex == NULL || c->sem == NULL) {
        if (c->mutex != NULL) vSemaphoreDelete((SemaphoreHandle_t) c->mutex);
        if (c->sem   != NULL) vSemaphoreDelete((SemaphoreHandle_t) c->sem);
        c->mutex = NULL;
        c->sem   = NULL;
        return -1;
    }
    return 0;
}

int8_t nros_platform_condvar_drop(void *cv) {
    if (cv == NULL) return -1;
    nros_esp_condvar_t *c = (nros_esp_condvar_t *) cv;
    if (c->sem   != NULL) vSemaphoreDelete((SemaphoreHandle_t) c->sem);
    if (c->mutex != NULL) vSemaphoreDelete((SemaphoreHandle_t) c->mutex);
    c->sem = NULL;
    c->mutex = NULL;
    return 0;
}

int8_t nros_platform_condvar_signal(void *cv) {
    if (cv == NULL) return -1;
    nros_esp_condvar_t *c = (nros_esp_condvar_t *) cv;
    xSemaphoreTake((SemaphoreHandle_t) c->mutex, portMAX_DELAY);
    if (c->waiters > 0) {
        xSemaphoreGive((SemaphoreHandle_t) c->sem);
        c->waiters--;
    }
    xSemaphoreGive((SemaphoreHandle_t) c->mutex);
    return 0;
}

int8_t nros_platform_condvar_signal_all(void *cv) {
    if (cv == NULL) return -1;
    nros_esp_condvar_t *c = (nros_esp_condvar_t *) cv;
    xSemaphoreTake((SemaphoreHandle_t) c->mutex, portMAX_DELAY);
    while (c->waiters > 0) {
        xSemaphoreGive((SemaphoreHandle_t) c->sem);
        c->waiters--;
    }
    xSemaphoreGive((SemaphoreHandle_t) c->mutex);
    return 0;
}

/* Phase 124.B.7.a — ISR-safe signal. ESP-IDF FreeRTOS port:
 * xSemaphoreGiveFromISR is the ISR-safe variant. We can't take the
 * mutex from ISR; waiters re-arm on the next wait. */
int8_t nros_platform_condvar_signal_from_isr(void *cv) {
    if (cv == NULL) return -1;
    nros_esp_condvar_t *c = (nros_esp_condvar_t *) cv;
    BaseType_t higher_pri = pdFALSE;
    xSemaphoreGiveFromISR((SemaphoreHandle_t) c->sem, &higher_pri);
    portYIELD_FROM_ISR(higher_pri);
    return 0;
}

int8_t nros_platform_condvar_wait(void *cv, void *m) {
    if (cv == NULL || m == NULL) return -1;
    nros_esp_condvar_t *c = (nros_esp_condvar_t *) cv;
    xSemaphoreTake((SemaphoreHandle_t) c->mutex, portMAX_DELAY);
    c->waiters++;
    xSemaphoreGive((SemaphoreHandle_t) c->mutex);

    nros_platform_mutex_unlock(m);
    xSemaphoreTake((SemaphoreHandle_t) c->sem, portMAX_DELAY);
    nros_platform_mutex_lock(m);
    return 0;
}

int8_t nros_platform_condvar_wait_until(void *cv, void *m, uint64_t abstime_ms) {
    if (cv == NULL || m == NULL) return -1;
    nros_esp_condvar_t *c = (nros_esp_condvar_t *) cv;

    uint64_t now = (nros_platform_clock_ns() / 1000000ULL);
    uint32_t rel_ms = abstime_ms > now ? (uint32_t) (abstime_ms - now) : 0;

    xSemaphoreTake((SemaphoreHandle_t) c->mutex, portMAX_DELAY);
    c->waiters++;
    xSemaphoreGive((SemaphoreHandle_t) c->mutex);

    nros_platform_mutex_unlock(m);
    BaseType_t ret = xSemaphoreTake((SemaphoreHandle_t) c->sem, pdMS_TO_TICKS(rel_ms));
    nros_platform_mutex_lock(m);

    if (ret != pdTRUE) {
        xSemaphoreTake((SemaphoreHandle_t) c->mutex, portMAX_DELAY);
        c->waiters--;
        xSemaphoreGive((SemaphoreHandle_t) c->mutex);
        return 1;
    }
    return 0;
}

/* ============================================================
 *   Wake primitive (Phase 130)
 *
 *   ESP-IDF ships its own FreeRTOS fork; binary semaphore +
 *   `xSemaphoreGiveFromISR` are available unchanged.
 * ============================================================ */

typedef struct {
    void *handle;  /* SemaphoreHandle_t */
} nros_wake_t;

int8_t nros_platform_wake_init(void *w) {
    if (w == NULL) return -1;
    nros_wake_t *wp = (nros_wake_t *) w;
    wp->handle = (void *) xSemaphoreCreateBinary();
    return wp->handle != NULL ? 0 : -1;
}

int8_t nros_platform_wake_drop(void *w) {
    if (w == NULL) return 0;
    nros_wake_t *wp = (nros_wake_t *) w;
    if (wp->handle != NULL) {
        vSemaphoreDelete((SemaphoreHandle_t) wp->handle);
        wp->handle = NULL;
    }
    return 0;
}

int8_t nros_platform_wake_wait_ms(void *w, uint32_t timeout_ms) {
    if (w == NULL) return -1;
    nros_wake_t *wp = (nros_wake_t *) w;
    if (wp->handle == NULL) return -1;
    TickType_t ticks = (timeout_ms == 0u) ? 0 : pdMS_TO_TICKS(timeout_ms);
    BaseType_t rc = xSemaphoreTake((SemaphoreHandle_t) wp->handle, ticks);
    return rc == pdTRUE ? 0 : 1;
}

int8_t nros_platform_wake_signal(void *w) {
    if (w == NULL) return -1;
    nros_wake_t *wp = (nros_wake_t *) w;
    if (wp->handle == NULL) return -1;
    (void) xSemaphoreGive((SemaphoreHandle_t) wp->handle);
    return 0;
}

int8_t nros_platform_wake_signal_from_isr(void *w) {
    if (w == NULL) return -1;
    nros_wake_t *wp = (nros_wake_t *) w;
    if (wp->handle == NULL) return -1;
    BaseType_t higher_pri = pdFALSE;
    (void) xSemaphoreGiveFromISR((SemaphoreHandle_t) wp->handle, &higher_pri);
    /* ESP-IDF wraps portYIELD_FROM_ISR for both single-core and
     * multicore SoCs. */
    if (higher_pri == pdTRUE) {
        portYIELD_FROM_ISR();
    }
    return 0;
}

size_t nros_platform_wake_storage_size(void) {
    return sizeof(nros_wake_t);
}

size_t nros_platform_wake_storage_align(void) {
    return __alignof__(nros_wake_t);
}

/* phase-359 W10 — opaque-storage sizing for `task`, the sibling of the wake
 * probes above. `task_init`'s contract says the implementor decides the size;
 * these let a caller ASK instead of hard-coding it (issue 0570's trap). */
size_t nros_platform_task_storage_size(void) {
    return sizeof(nros_esp_task_t);
}

size_t nros_platform_task_storage_align(void) {
    return _Alignof(nros_esp_task_t);
}
/* phase-364 W2 (RFC-0076 D1) — opaque-storage sizing for the lock family, the
 * siblings of the `wake` and `task` probes.
 *
 * Two forms because callers need two. A Rust or otherwise-dynamic caller asks
 * at RUNTIME and allocates; zenoh-pico embeds `_z_mutex_t` BY VALUE and needs
 * the number at COMPILE time, which a function call cannot provide. The
 * `_Static_assert`s below are what stop the two from drifting: the macro and
 * the type are checked against each other in the port that owns both, so a
 * wrong macro is a compile error here rather than a buffer overrun in a
 * consumer.
 *
 * This replaces a hand-computed table in `zpico-sys` that guessed OTHER
 * platforms' struct sizes with `≈` and a "2× safety margin". */
size_t nros_platform_mutex_storage_size(void) { return sizeof(nros_esp_mutex_t); }
size_t nros_platform_mutex_storage_align(void) { return _Alignof(nros_esp_mutex_t); }
size_t nros_platform_mutex_rec_storage_size(void) { return sizeof(nros_esp_mutex_t); }
size_t nros_platform_mutex_rec_storage_align(void) { return _Alignof(nros_esp_mutex_t); }
size_t nros_platform_condvar_storage_size(void) { return sizeof(nros_esp_condvar_t); }
size_t nros_platform_condvar_storage_align(void) { return _Alignof(nros_esp_condvar_t); }

_Static_assert(NROS_PLATFORM_MUTEX_STORAGE_SIZE >= sizeof(nros_esp_mutex_t),
               "NROS_PLATFORM_MUTEX_STORAGE_SIZE too small for this port");
_Static_assert(NROS_PLATFORM_MUTEX_REC_STORAGE_SIZE >= sizeof(nros_esp_mutex_t),
               "NROS_PLATFORM_MUTEX_REC_STORAGE_SIZE too small for this port");
_Static_assert(NROS_PLATFORM_CONDVAR_STORAGE_SIZE >= sizeof(nros_esp_condvar_t),
               "NROS_PLATFORM_CONDVAR_STORAGE_SIZE too small for this port");
_Static_assert(NROS_PLATFORM_TASK_STORAGE_SIZE >= sizeof(nros_esp_task_t),
               "NROS_PLATFORM_TASK_STORAGE_SIZE too small for this port");


/* ============================================================
 *   Critical section (Phase 121.9)
 * ============================================================ */
/* ESP-IDF uses a spinlock-based critical section on multicore SoCs
 * (ESP32, ESP32-S3) and a plain interrupt-disable on single-core SoCs
 * (ESP32-C3, ESP32-C6). `portENTER_CRITICAL` / `portEXIT_CRITICAL`
 * wrap both; the FreeRTOS port owns the bookkeeping. */
static portMUX_TYPE s_cs_lock = portMUX_INITIALIZER_UNLOCKED;

uint32_t nros_platform_critical_section_acquire(void) {
    portENTER_CRITICAL(&s_cs_lock);
    return 0;
}

void nros_platform_critical_section_release(uint32_t token) {
    (void) token;
    portEXIT_CRITICAL(&s_cs_lock);
}

/* ============================================================
 *   Logging (Phase 88)
 *
 *   Route through `esp_log_write`. Logger name → ESP TAG;
 *   message body → "%s" arg so ESP-IDF's timestamp + colour
 *   prefix wraps the rendered line.
 *
 *   ISR-safety: "partial" — `esp_log_write` is NOT IRAM-safe in
 *   the flash-cache-enabled default. Callers logging from IRAM
 *   handlers should call `esp_rom_printf` directly; nros-log
 *   does not surface that distinction in v1.
 * ============================================================ */
#include "esp_log.h"
#include <string.h>

#define NROS_PLATFORM_LOG_TAG_SZ  64
#define NROS_PLATFORM_LOG_BODY_SZ 1280

static esp_log_level_t severity_to_esp(uint8_t s) {
    switch (s) {
    case 5: /* Fatal — ESP has no fatal; map to ERROR */
    case 4: return ESP_LOG_ERROR;
    case 3: return ESP_LOG_WARN;
    case 2: return ESP_LOG_INFO;
    case 1: return ESP_LOG_DEBUG;
    case 0: return ESP_LOG_VERBOSE;
    default: return ESP_LOG_INFO;
    }
}

void nros_platform_log_write(uint8_t severity,
                             const uint8_t *name_ptr, uintptr_t name_len,
                             const uint8_t *msg_ptr,  uintptr_t msg_len) {
    if (msg_ptr == NULL && msg_len > 0) {
        return;
    }
    char tag[NROS_PLATFORM_LOG_TAG_SZ];
    if (name_ptr != NULL && name_len > 0) {
        size_t copy = name_len < (sizeof(tag) - 1) ? name_len : (sizeof(tag) - 1);
        memcpy(tag, name_ptr, copy);
        tag[copy] = '\0';
    } else {
        tag[0] = 'n'; tag[1] = 'r'; tag[2] = 'o'; tag[3] = 's'; tag[4] = '\0';
    }
    char body[NROS_PLATFORM_LOG_BODY_SZ];
    size_t copy = msg_len < (sizeof(body) - 1) ? msg_len : (sizeof(body) - 1);
    if (msg_ptr != NULL && msg_len > 0) {
        memcpy(body, msg_ptr, copy);
    }
    body[copy] = '\0';
    esp_log_write(severity_to_esp(severity), tag, "%s\n", body);
}

void nros_platform_log_flush(void) {
    /* esp_log_write is synchronous to UART; nothing to flush. */
}

/* ---- Fatal error (phase-366 / RFC-0077) ----
 *
 * ESP-IDF is the closest match to what this API wants: the MECHANISM is the
 * platform's and the POLICY is already configuration. `esp_system_abort()`
 * enters IDF's panic handler, which prints a backtrace and then honours
 * `CONFIG_ESP_SYSTEM_PANIC_*` — print-and-reboot, halt, or hand over to a GDB
 * stub. Reimplementing any of that here would take the choice away from the
 * user's sdkconfig, which is exactly the defect this API removes.
 *
 * `esp_system_abort` takes a NUL-terminated string and our `msg` is a
 * length-delimited diagnostic, so it is copied into a bounded stack buffer and
 * truncated rather than heap-allocated — this runs when the heap may be the
 * thing that failed.
 */
__attribute__((weak))
_Noreturn void nros_platform_panic(const char *msg, size_t len) {
    char line[192];
    size_t n = 0;
    const char kPrefix[] = "nros: PANIC ";
    memcpy(line, kPrefix, sizeof(kPrefix) - 1);
    n = sizeof(kPrefix) - 1;
    if (msg != NULL && len > 0) {
        size_t copy = len < (sizeof(line) - n - 1) ? len : (sizeof(line) - n - 1);
        memcpy(line + n, msg, copy);
        n += copy;
    }
    line[n] = '\0';
    esp_system_abort(line);
}

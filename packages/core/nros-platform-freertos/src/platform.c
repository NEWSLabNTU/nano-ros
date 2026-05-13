/*
 * Phase 121.3.freertos — native C implementation of the canonical
 * platform ABI for FreeRTOS.
 *
 * Behavioural parity with `nros-platform-freertos`'s Rust impl:
 *
 *   - Clock        — xTaskGetTickCount() scaled to ms / us by
 *                    configTICK_RATE_HZ.
 *   - Allocation   — pvPortMalloc / vPortFree. realloc is emulated
 *                    via malloc + memcpy + free since FreeRTOS has
 *                    no `pvPortRealloc`.
 *   - Sleep        — vTaskDelay(ms_to_ticks).
 *   - Yield        — vTaskDelay(1). taskYIELD() is a tick-quantum
 *                    busy-spin on cooperative scheduling; one tick
 *                    is the closest portable cross-port behaviour.
 *   - Tasks        — xTaskCreate + vTaskDelete. ZTask storage shape
 *                    matches zenoh-pico's `_z_task_t` (4 pointers).
 *   - Mutexes      — xSemaphoreCreate{Mutex,RecursiveMutex} stored
 *                    inside a `ZMutex` (single-pointer storage).
 *   - Condvars     — mutex + counting-semaphore + waiters counter
 *                    (matches the Rust impl + zenoh-pico's _z_condvar_t
 *                    layout).
 *   - Random       — deterministic xorshift seeded from
 *                    nros_platform_freertos_seed_rng() (optional;
 *                    defaults to a fixed seed).
 *   - Time         — wall clock not provided by FreeRTOS; returns 0.
 *
 * Build verification requires a FreeRTOS-Kernel checkout and a
 * `FreeRTOSConfig.h` for the target board; CMakeLists.txt parametrises
 * both. The integration test for this port lives at the application
 * level (per-board, see examples/).
 */

#include <nros/platform.h>

#include <FreeRTOS.h>
#include <task.h>
#include <semphr.h>

#include <stddef.h>
#include <stdint.h>
#include <string.h>

/* ---- Tick-rate scaling ---- */

#ifndef configTICK_RATE_HZ
#  error "FreeRTOSConfig.h must define configTICK_RATE_HZ before including this source"
#endif

#define MS_PER_TICK ((uint64_t) (1000U / configTICK_RATE_HZ))
#define US_PER_TICK ((uint64_t) (1000000U / configTICK_RATE_HZ))

/* ---- Clock ---- */

uint64_t nros_platform_clock_ms(void) {
    return (uint64_t) xTaskGetTickCount() * MS_PER_TICK;
}

uint64_t nros_platform_clock_us(void) {
    return (uint64_t) xTaskGetTickCount() * US_PER_TICK;
}

/* ---- Allocation ---- */

void *nros_platform_alloc(size_t size) {
    if (size == 0) {
        return NULL;
    }
    return pvPortMalloc(size);
}

void nros_platform_dealloc(void *ptr) {
    if (ptr != NULL) {
        vPortFree(ptr);
    }
}

/*
 * FreeRTOS has no `pvPortRealloc` in stock builds. Emulate it with
 * malloc + memcpy + free. The caller must keep the original `size`
 * available out-of-band if it needs to preserve more than the
 * minimum of (old, new); we have no way to query the old size from
 * the heap_4 free-list, so we conservatively copy up to `size`.
 */
void *nros_platform_realloc(void *ptr, size_t size) {
    if (size == 0) {
        nros_platform_dealloc(ptr);
        return NULL;
    }
    if (ptr == NULL) {
        return nros_platform_alloc(size);
    }
    void *out = pvPortMalloc(size);
    if (out == NULL) {
        return NULL;
    }
    /* Best-effort copy. FreeRTOS heap_4 doesn't expose the original
     * block size; the caller is expected to track that out-of-band
     * if a precise copy is required. */
    memcpy(out, ptr, size);
    vPortFree(ptr);
    return out;
}

/* ---- Sleep ---- */

static inline TickType_t ms_to_ticks(size_t ms) {
    /* `pdMS_TO_TICKS(ms)` for portability; expands to
     *    ((TickType_t)(((TickType_t)(ms) * (TickType_t)configTICK_RATE_HZ) / (TickType_t)1000))
     * which is what we want. */
    return pdMS_TO_TICKS(ms);
}

void nros_platform_sleep_us(size_t us) {
    /* FreeRTOS tick is typically 1 ms; sub-millisecond sleep can't
     * be honored portably. Round up to 1 tick if non-zero. */
    if (us == 0) {
        return;
    }
    TickType_t ticks = (TickType_t) ((us + 999) / 1000);
    if (ticks == 0) {
        ticks = 1;
    }
    vTaskDelay(ticks);
}

void nros_platform_sleep_ms(size_t ms) {
    vTaskDelay(ms_to_ticks(ms));
}

void nros_platform_sleep_s(size_t s) {
    vTaskDelay(ms_to_ticks(s * 1000U));
}

/* ---- Yield ---- */

void nros_platform_yield_now(void) {
    /* Mirror the Rust impl's choice: vTaskDelay(1) gives the
     * scheduler a clean tick boundary. taskYIELD() is also valid
     * but doesn't always re-schedule under cooperative configs. */
    vTaskDelay(1);
}

/* ---- Random — deterministic xorshift64 ---- */

static uint64_t s_rng_state = 0x9E3779B97F4A7C15ULL;

void nros_platform_freertos_seed_rng(uint32_t value) {
    s_rng_state = ((uint64_t) value) | (((uint64_t) value) << 32) | 1ULL;
}

static uint64_t rng_next(void) {
    uint64_t x = s_rng_state;
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    s_rng_state = x;
    return x;
}

uint8_t  nros_platform_random_u8(void)   { return (uint8_t)  rng_next(); }
uint16_t nros_platform_random_u16(void)  { return (uint16_t) rng_next(); }
uint32_t nros_platform_random_u32(void)  { return (uint32_t) rng_next(); }
uint64_t nros_platform_random_u64(void)  { return rng_next(); }

void nros_platform_random_fill(void *buf, size_t len) {
    uint8_t *p = (uint8_t *) buf;
    while (len >= 8) {
        uint64_t v = rng_next();
        memcpy(p, &v, 8);
        p += 8;
        len -= 8;
    }
    if (len > 0) {
        uint64_t v = rng_next();
        memcpy(p, &v, len);
    }
}

/* ---- Wall clock — not provided by FreeRTOS ---- */

uint64_t nros_platform_time_now_ms(void)              { return 0; }
uint32_t nros_platform_time_since_epoch_secs(void)    { return 0; }
uint32_t nros_platform_time_since_epoch_nanos(void)   { return 0; }

/* ---- Tasks ----
 *
 * Layout must match `nros-platform-freertos::types::ZTask` and
 * zenoh-pico's `_z_task_t` (16 bytes on ARM32).
 */

typedef struct {
    void *handle;          /* TaskHandle_t */
    void *join_event;      /* unused on FreeRTOS C port; reserved for future use */
    void *(*entry)(void *);
    void *arg;
} nros_freertos_task_t;

typedef struct {
    const char *name;
    uint32_t priority;
    size_t stack_depth;
} nros_freertos_task_attr_t;

static void freertos_task_trampoline(void *raw) {
    nros_freertos_task_t *t = (nros_freertos_task_t *) raw;
    if (t->entry != NULL) {
        (void) t->entry(t->arg);
    }
    /* Self-delete; the caller's task_join treats vTaskDelete as the
     * exit signal. */
    vTaskDelete(NULL);
}

int8_t nros_platform_task_init(void *task, void *attr,
                               void *(*entry)(void *), void *arg) {
    if (task == NULL || entry == NULL) {
        return -1;
    }
    nros_freertos_task_t *t = (nros_freertos_task_t *) task;
    t->handle = NULL;
    t->join_event = NULL;
    t->entry = entry;
    t->arg = arg;

    const char *name = "nros";
    uint32_t priority = tskIDLE_PRIORITY + 1;
    uint32_t stack_depth = configMINIMAL_STACK_SIZE;
    if (attr != NULL) {
        const nros_freertos_task_attr_t *a = (const nros_freertos_task_attr_t *) attr;
        if (a->name != NULL)      name = a->name;
        if (a->priority != 0)     priority = a->priority;
        if (a->stack_depth != 0)  stack_depth = (uint32_t) a->stack_depth;
    }

    TaskHandle_t handle = NULL;
    BaseType_t rc = xTaskCreate(
        freertos_task_trampoline,
        name,
        stack_depth,
        (void *) t,
        priority,
        &handle);
    if (rc != pdPASS) {
        return -1;
    }
    t->handle = handle;
    return 0;
}

int8_t nros_platform_task_join(void *task) {
    /* FreeRTOS provides no native join. Spin on the task handle
     * being deleted: vTaskDelete(NULL) zeroes the eTaskGetState
     * eventually (eDeleted). Poll. */
    if (task == NULL) return -1;
    nros_freertos_task_t *t = (nros_freertos_task_t *) task;
    if (t->handle == NULL) return -1;
    while (eTaskGetState((TaskHandle_t) t->handle) != eDeleted) {
        vTaskDelay(1);
    }
    t->handle = NULL;
    return 0;
}

int8_t nros_platform_task_detach(void *task) {
    if (task == NULL) return -1;
    /* No FreeRTOS-side detach; the trampoline already self-deletes. */
    ((nros_freertos_task_t *) task)->handle = NULL;
    return 0;
}

int8_t nros_platform_task_cancel(void *task) {
    if (task == NULL) return -1;
    nros_freertos_task_t *t = (nros_freertos_task_t *) task;
    if (t->handle == NULL) return -1;
    vTaskDelete((TaskHandle_t) t->handle);
    t->handle = NULL;
    return 0;
}

void nros_platform_task_exit(void) {
    vTaskDelete(NULL);
}

void nros_platform_task_free(void **task) {
    (void) task; /* caller-owned storage */
}

/* ---- Mutex ----
 *
 * Layout matches `nros-platform-freertos::types::ZMutex` (single
 * pointer).
 */

typedef struct {
    void *handle;  /* SemaphoreHandle_t */
} nros_freertos_mutex_t;

int8_t nros_platform_mutex_init(void *m) {
    if (m == NULL) return -1;
    SemaphoreHandle_t h = xSemaphoreCreateMutex();
    if (h == NULL) return -1;
    ((nros_freertos_mutex_t *) m)->handle = h;
    return 0;
}

int8_t nros_platform_mutex_drop(void *m) {
    if (m == NULL) return -1;
    SemaphoreHandle_t h = (SemaphoreHandle_t) ((nros_freertos_mutex_t *) m)->handle;
    if (h == NULL) return -1;
    vSemaphoreDelete(h);
    ((nros_freertos_mutex_t *) m)->handle = NULL;
    return 0;
}

int8_t nros_platform_mutex_lock(void *m) {
    if (m == NULL) return -1;
    SemaphoreHandle_t h = (SemaphoreHandle_t) ((nros_freertos_mutex_t *) m)->handle;
    return xSemaphoreTake(h, portMAX_DELAY) == pdTRUE ? 0 : -1;
}

int8_t nros_platform_mutex_try_lock(void *m) {
    if (m == NULL) return -1;
    SemaphoreHandle_t h = (SemaphoreHandle_t) ((nros_freertos_mutex_t *) m)->handle;
    return xSemaphoreTake(h, 0) == pdTRUE ? 0 : 1;
}

int8_t nros_platform_mutex_unlock(void *m) {
    if (m == NULL) return -1;
    SemaphoreHandle_t h = (SemaphoreHandle_t) ((nros_freertos_mutex_t *) m)->handle;
    return xSemaphoreGive(h) == pdTRUE ? 0 : -1;
}

int8_t nros_platform_mutex_rec_init(void *m) {
    if (m == NULL) return -1;
    SemaphoreHandle_t h = xSemaphoreCreateRecursiveMutex();
    if (h == NULL) return -1;
    ((nros_freertos_mutex_t *) m)->handle = h;
    return 0;
}

int8_t nros_platform_mutex_rec_drop(void *m) {
    return nros_platform_mutex_drop(m);
}

int8_t nros_platform_mutex_rec_lock(void *m) {
    if (m == NULL) return -1;
    SemaphoreHandle_t h = (SemaphoreHandle_t) ((nros_freertos_mutex_t *) m)->handle;
    return xSemaphoreTakeRecursive(h, portMAX_DELAY) == pdTRUE ? 0 : -1;
}

int8_t nros_platform_mutex_rec_try_lock(void *m) {
    if (m == NULL) return -1;
    SemaphoreHandle_t h = (SemaphoreHandle_t) ((nros_freertos_mutex_t *) m)->handle;
    return xSemaphoreTakeRecursive(h, 0) == pdTRUE ? 0 : 1;
}

int8_t nros_platform_mutex_rec_unlock(void *m) {
    if (m == NULL) return -1;
    SemaphoreHandle_t h = (SemaphoreHandle_t) ((nros_freertos_mutex_t *) m)->handle;
    return xSemaphoreGiveRecursive(h) == pdTRUE ? 0 : -1;
}

/* ---- Condvar ----
 *
 * Layout matches `nros-platform-freertos::types::ZCondvar` (12 bytes
 * on ARM32: { mutex, sem, waiters }). The pattern follows zenoh-pico's
 * FreeRTOS system.c.
 */

typedef struct {
    void *mutex;     /* SemaphoreHandle_t guarding the waiter count */
    void *sem;       /* SemaphoreHandle_t counting semaphore */
    int32_t waiters;
} nros_freertos_condvar_t;

int8_t nros_platform_condvar_init(void *cv) {
    if (cv == NULL) return -1;
    nros_freertos_condvar_t *c = (nros_freertos_condvar_t *) cv;
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
    nros_freertos_condvar_t *c = (nros_freertos_condvar_t *) cv;
    if (c->sem   != NULL) vSemaphoreDelete((SemaphoreHandle_t) c->sem);
    if (c->mutex != NULL) vSemaphoreDelete((SemaphoreHandle_t) c->mutex);
    c->sem = NULL;
    c->mutex = NULL;
    return 0;
}

int8_t nros_platform_condvar_signal(void *cv) {
    if (cv == NULL) return -1;
    nros_freertos_condvar_t *c = (nros_freertos_condvar_t *) cv;
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
    nros_freertos_condvar_t *c = (nros_freertos_condvar_t *) cv;
    xSemaphoreTake((SemaphoreHandle_t) c->mutex, portMAX_DELAY);
    while (c->waiters > 0) {
        xSemaphoreGive((SemaphoreHandle_t) c->sem);
        c->waiters--;
    }
    xSemaphoreGive((SemaphoreHandle_t) c->mutex);
    return 0;
}

int8_t nros_platform_condvar_wait(void *cv, void *m) {
    if (cv == NULL || m == NULL) return -1;
    nros_freertos_condvar_t *c = (nros_freertos_condvar_t *) cv;

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
    nros_freertos_condvar_t *c = (nros_freertos_condvar_t *) cv;

    uint64_t now = nros_platform_clock_ms();
    uint32_t rel_ms = abstime_ms > now ? (uint32_t) (abstime_ms - now) : 0;

    xSemaphoreTake((SemaphoreHandle_t) c->mutex, portMAX_DELAY);
    c->waiters++;
    xSemaphoreGive((SemaphoreHandle_t) c->mutex);

    nros_platform_mutex_unlock(m);
    BaseType_t ret = xSemaphoreTake((SemaphoreHandle_t) c->sem, pdMS_TO_TICKS(rel_ms));
    nros_platform_mutex_lock(m);

    if (ret != pdTRUE) {
        /* Timed out — decrement waiter count. */
        xSemaphoreTake((SemaphoreHandle_t) c->mutex, portMAX_DELAY);
        c->waiters--;
        xSemaphoreGive((SemaphoreHandle_t) c->mutex);
        return 1;
    }
    return 0;
}

/* ============================================================
 *   Critical section (Phase 121.9)
 * ============================================================ */
/* Cortex-M PRIMASK + nested-call counter (taskENTER_CRITICAL /
 * taskEXIT_CRITICAL already track nesting at the FreeRTOS port
 * level via uxCriticalNesting). The canonical ABI uses the FreeRTOS
 * primitive directly so kernel-aware bookkeeping stays consistent.
 *
 * Token is unused (returns 0); FreeRTOS's port layer handles the
 * restore posture internally. */
uint32_t nros_platform_critical_section_acquire(void) {
    taskENTER_CRITICAL();
    return 0;
}

void nros_platform_critical_section_release(uint32_t token) {
    (void) token;
    taskEXIT_CRITICAL();
}

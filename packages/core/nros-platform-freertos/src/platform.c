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

/* Issue #44 — on esp-idf (riscv), `freertos/config/riscv/include/freertos/
 * FreeRTOSConfig_arch.h` defines
 *     #define configTOTAL_HEAP_SIZE ( &_heap_end - &_heap_start )
 * using the linker heap-region symbols but does NOT declare them; esp-idf's own
 * build pulls a prior header that does. This TU includes <FreeRTOS.h> directly
 * (no esp-idf system headers), so the symbols are undeclared and the *compile*
 * fails. Declare them ahead of the FreeRTOS include with the SAME type esp-idf
 * uses (`extern int`, e.g. `components/heap/port/esp32c3/memory_layout.c`), so
 * `&_heap_end - &_heap_start` is a well-formed `int*` pointer subtraction and the
 * declaration can't clash with esp-idf's. Gated to `ESP_PLATFORM` (esp-idf's
 * compiler define) so no other FreeRTOS port is touched. */
#if defined(ESP_PLATFORM)
extern int _heap_start;
extern int _heap_end;
#endif

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

/* Phase 121.3.freertos-parity — semihosting trace for diagnostic
 * builds. Enable by setting `NROS_PLATFORM_FREERTOS_TRACE` on the
 * compile line. ARM Cortex-M only (uses SYS_WRITE0 / BKPT 0xAB). */
#ifdef NROS_PLATFORM_FREERTOS_TRACE
static void _trace(const char *s) {
    register unsigned r0 __asm__("r0") = 0x04; /* SYS_WRITE0 */
    register const char *r1 __asm__("r1") = s;
    __asm__ volatile("bkpt #0xAB" : : "r"(r0), "r"(r1) : "memory");
}
#define TRACE(s) _trace(s)
#else
#define TRACE(s) ((void) 0)
#endif

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

/* ---- Heap stats (phase-230 1b / RFC-0034 D7) ----
 * FreeRTOS heap_4/5: used = configTOTAL_HEAP_SIZE − current free. FreeRTOS
 * is a Mode-A platform (nano-ros owns the allocator; zenoh-pico's z_malloc
 * → pvPortMalloc once Wave 1c funnels it), so this tracks the nano-ros +
 * RMW heap. `xPortGetFreeHeapSize` is available on heap_4/heap_5. */
size_t nros_platform_heap_used_bytes(void) {
    return (size_t) (configTOTAL_HEAP_SIZE - xPortGetFreeHeapSize());
}

size_t nros_platform_heap_total_bytes(void) {
    return (size_t) configTOTAL_HEAP_SIZE;
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

/* Phase 121.3.freertos-parity — pin storage layout. zenoh-pico's
 * `_z_task_t` allocates exactly `4 * sizeof(void*)` bytes (handle,
 * join_event, fun, arg) when `configSUPPORT_STATIC_ALLOCATION = 0`;
 * the trait-side opaque buffer in `zpico-platform-shim` is
 * `[u8; 64]` which is an upper bound. A `_Static_assert` catches
 * accidental field reordering or alignment drift at compile time
 * without resorting to hand-math comments. */
_Static_assert(sizeof(nros_freertos_task_t) == 4 * sizeof(void *),
               "nros_freertos_task_t must be 4 pointers (handle, join_event, entry, arg)");
_Static_assert(offsetof(nros_freertos_task_t, handle) == 0,
               "handle must be the first field (matches zenoh-pico _z_task_t)");
_Static_assert(offsetof(nros_freertos_task_t, join_event) == sizeof(void *),
               "join_event must follow handle");
_Static_assert(offsetof(nros_freertos_task_t, entry) == 2 * sizeof(void *),
               "entry/fun must be the third field");
_Static_assert(offsetof(nros_freertos_task_t, arg) == 3 * sizeof(void *),
               "arg must be the fourth field");

static void freertos_task_trampoline(void *raw) {
    nros_freertos_task_t *t = (nros_freertos_task_t *) raw;
    if (t->entry != NULL) {
        (void) t->entry(t->arg);
    }
    /* Self-delete — task_join below polls `eTaskGetState` until the
     * task hits `eDeleted`. Matches zpico's own _z_task_wrapper
     * semantics for non-static-allocation builds. */
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

    /* Phase 121.3.freertos-parity — defaults match the deleted Rust
     * impl (`DEFAULT_PRIORITY=3`, `DEFAULT_STACK_DEPTH=5120` words).
     * configMINIMAL_STACK_SIZE = 256 words is too small for zenoh-pico
     * RTPS / message parsing — task overflows the stack silently and
     * the binary appears to hang in zenoh-pico's read loop. */
    const char *name = "nros";
    uint32_t priority = 3;
    uint32_t stack_depth = 5120;
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
    /* Poll `eTaskGetState` until the trampoline has called
     * `vTaskDelete(NULL)`. After vTaskDelete the TCB is queued for
     * idle-task cleanup; eTaskGetState returns `eDeleted` until the
     * idle task frees the memory. */
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
    /* No FreeRTOS-side detach. */
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

_Static_assert(sizeof(nros_freertos_mutex_t) == sizeof(void *),
               "nros_freertos_mutex_t must be one pointer (matches zenoh-pico _z_mutex_t)");
_Static_assert(offsetof(nros_freertos_mutex_t, handle) == 0,
               "handle must be the first / only field");

/* Phase 121.3.freertos-parity — `mutex_*` (non-recursive in name) is
 * implemented over `xSemaphoreCreateRecursiveMutex` so it matches the
 * deleted Rust impl byte-for-byte. zenoh-pico holds the same `_z_mutex_t`
 * recursively in several read-task code paths; a strict non-recursive
 * mutex deadlocks the task on the second take and the listener never
 * receives a message. Both `mutex_*` and `mutex_rec_*` share the same
 * underlying primitive — the trait split exists for callers that need
 * the distinction, but FreeRTOS recursive mutexes satisfy both. */
int8_t nros_platform_mutex_init(void *m) {
    if (m == NULL) return -1;
    SemaphoreHandle_t h = xSemaphoreCreateRecursiveMutex();
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
    return xSemaphoreTakeRecursive(h, portMAX_DELAY) == pdTRUE ? 0 : -1;
}

int8_t nros_platform_mutex_try_lock(void *m) {
    if (m == NULL) return -1;
    SemaphoreHandle_t h = (SemaphoreHandle_t) ((nros_freertos_mutex_t *) m)->handle;
    return xSemaphoreTakeRecursive(h, 0) == pdTRUE ? 0 : 1;
}

int8_t nros_platform_mutex_unlock(void *m) {
    if (m == NULL) return -1;
    SemaphoreHandle_t h = (SemaphoreHandle_t) ((nros_freertos_mutex_t *) m)->handle;
    return xSemaphoreGiveRecursive(h) == pdTRUE ? 0 : -1;
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

/* zenoh-pico's `_z_condvar_t` layout: { mutex, sem, int waiters }.
 * On ARM32 with int=32-bit and ptr=32-bit, that's 12 bytes — matches
 * `sizeof(void*) * 2 + sizeof(int32_t)`. The trailing alignment
 * padding is implementation-defined; we don't pin total size, only
 * field offsets. */
_Static_assert(offsetof(nros_freertos_condvar_t, mutex) == 0,
               "mutex must be the first field");
_Static_assert(offsetof(nros_freertos_condvar_t, sem) == sizeof(void *),
               "sem must follow mutex");
_Static_assert(offsetof(nros_freertos_condvar_t, waiters) == 2 * sizeof(void *),
               "waiters must follow sem");

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

/* Phase 124.B.7.a — ISR-safe signal.
 *
 * Uses xSemaphoreGiveFromISR which is the FreeRTOS ISR-safe
 * primitive. We can't take the mutex from ISR context, so we skip
 * the waiter-count decrement (it's an optimisation — waiters will
 * either consume the semaphore or re-arm on the next wait).
 *
 * After yielding any pending higher-priority task, portYIELD_FROM_ISR
 * triggers a context switch on return from the ISR if a higher-pri
 * task became runnable. */
int8_t nros_platform_condvar_signal_from_isr(void *cv) {
    if (cv == NULL) return -1;
    nros_freertos_condvar_t *c = (nros_freertos_condvar_t *) cv;
    BaseType_t higher_pri = pdFALSE;
    xSemaphoreGiveFromISR((SemaphoreHandle_t) c->sem, &higher_pri);
    portYIELD_FROM_ISR(higher_pri);
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
        /* Phase 121.3.freertos-parity — return -1 (non-zero, matches
         * the deleted Rust impl byte-for-byte) so zenoh-pico's
         * `_z_condvar_wait_until` callers see the same error-shaped
         * return value they used to. Returning +1 (positive non-zero)
         * subtly diverges and lets some callers treat it as success-
         * with-spurious-wake. */
        xSemaphoreTake((SemaphoreHandle_t) c->mutex, portMAX_DELAY);
        c->waiters--;
        xSemaphoreGive((SemaphoreHandle_t) c->mutex);
        return -1;
    }
    return 0;
}

/* ============================================================
 *   Wake primitive (Phase 130)
 *
 *   Binary semaphore backed by `xSemaphoreCreateBinary`. ISR
 *   signal uses `xSemaphoreGiveFromISR` (FreeRTOS ISR-safe by
 *   spec). Storage holds a `SemaphoreHandle_t` (pointer); the
 *   actual semaphore object is allocated by FreeRTOS on init.
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
    /* Binary semaphore: give is a no-op when already signaled. */
    (void) xSemaphoreGive((SemaphoreHandle_t) wp->handle);
    return 0;
}

int8_t nros_platform_wake_signal_from_isr(void *w) {
    if (w == NULL) return -1;
    nros_wake_t *wp = (nros_wake_t *) w;
    if (wp->handle == NULL) return -1;
    BaseType_t higher_pri = pdFALSE;
    (void) xSemaphoreGiveFromISR((SemaphoreHandle_t) wp->handle, &higher_pri);
    portYIELD_FROM_ISR(higher_pri);
    return 0;
}

size_t nros_platform_wake_storage_size(void) {
    return sizeof(nros_wake_t);
}

size_t nros_platform_wake_storage_align(void) {
    return __alignof__(nros_wake_t);
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
    /* ESP-IDF's FreeRTOS port redefines `taskENTER_CRITICAL` to take
     * a `portMUX_TYPE *` arg (SMP-aware). The vanilla zero-arg form
     * doesn't compile under IDF. Drop to the underlying
     * `vPortEnterCritical()` primitive which is portable across
     * vanilla and IDF FreeRTOS. */
#if defined(ESP_PLATFORM)
    vPortEnterCritical();
#else
    taskENTER_CRITICAL();
#endif
    return 0;
}

void nros_platform_critical_section_release(uint32_t token) {
    (void) token;
#if defined(ESP_PLATFORM)
    vPortExitCritical();
#else
    taskEXIT_CRITICAL();
#endif
}

/* ============================================================
 *   Logging (Phase 88)
 *
 *   FreeRTOS has no native text logger. The board crate registers
 *   a writer fn-ptr at startup via `nros_platform_register_log_writer`
 *   (e.g. mps2-an385-freertos hands over its semihosting helper;
 *   another board might hand over a `configPRINTF` adapter).
 *
 *   Until a writer is registered, `nros_platform_log_write` is a
 *   no-op. This keeps the ABI total — every consumer call returns
 *   immediately on a board that hasn't wired logging.
 *
 *   ISR safety inherits from the registered writer.
 * ============================================================ */
#include <string.h>

typedef void (*nros_platform_log_writer_fn)(
    uint8_t        severity,
    const uint8_t *name_ptr, uintptr_t name_len,
    const uint8_t *msg_ptr,  uintptr_t msg_len);

typedef void (*nros_platform_log_flush_fn)(void);

/* Phase 166 — promoted from `static` to external linkage so the
 * linker dedups the storage across multiple TU compilations of
 * this file (cmake's `libnros_platform_freertos.a` + cargo's
 * `nros-board-freertos` build script both compile `platform.c`).
 * With file-static linkage each TU got its own private slot;
 * `nros_platform_register_log_writer` from one archive would
 * write its slot, and `nros_platform_log_write` from the other
 * archive would read a NULL slot. That broke the Phase 88.16.H
 * C/C++ FreeRTOS log path. Single external symbol = single slot. */
nros_platform_log_writer_fn nros_platform_freertos_log_writer = NULL;
nros_platform_log_flush_fn  nros_platform_freertos_log_flusher = NULL;

/* Board-crate hook. Pass NULL for `flusher` if the writer is fully
 * synchronous. Re-calling replaces the current writer; the swap
 * is plain pointer store — boards must call this BEFORE any task
 * starts logging. */
void nros_platform_register_log_writer(nros_platform_log_writer_fn writer,
                                       nros_platform_log_flush_fn  flusher) {
    nros_platform_freertos_log_writer  = writer;
    nros_platform_freertos_log_flusher = flusher;
}

void nros_platform_log_write(uint8_t severity,
                             const uint8_t *name_ptr, uintptr_t name_len,
                             const uint8_t *msg_ptr,  uintptr_t msg_len) {
    nros_platform_log_writer_fn writer = nros_platform_freertos_log_writer;
    if (writer == NULL) {
        return;
    }
    writer(severity, name_ptr, name_len, msg_ptr, msg_len);
}

void nros_platform_log_flush(void) {
    nros_platform_log_flush_fn flusher = nros_platform_freertos_log_flusher;
    if (flusher != NULL) {
        flusher();
    }
}

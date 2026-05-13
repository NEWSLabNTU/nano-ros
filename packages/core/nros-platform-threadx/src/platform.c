/*
 * Phase 121.3.threadx — native C implementation of the canonical
 * platform ABI for Azure RTOS ThreadX.
 *
 * Behavioural parity with `nros-platform-threadx`'s Rust impl:
 *
 *   - Clock    — tx_time_get() scaled by TX_TIMER_TICKS_PER_SECOND.
 *   - Alloc    — tx_byte_allocate / tx_byte_release against a
 *                caller-provided byte pool. The application sets the
 *                pool pointer once via `nros_platform_threadx_set_byte_pool`
 *                before the first allocation.
 *   - Sleep    — tx_thread_sleep(ms_to_ticks).
 *   - Yield    — tx_thread_relinquish() (ThreadX's native
 *                cooperative yield).
 *   - Random   — deterministic xorshift64; seedable via
 *                `nros_platform_threadx_seed_rng(u32)`.
 *   - Time     — wall clock unsupported; returns 0.
 *   - Tasks    — tx_thread_create + tx_thread_delete.
 *   - Mutexes  — tx_mutex_create with TX_INHERIT=1. ThreadX mutexes
 *                are recursive by design, so mutex_* and mutex_rec_*
 *                share the same primitive.
 *   - Condvars — tx_semaphore. tx_semaphore_get / tx_semaphore_put
 *                with the caller's mutex released around the wait
 *                (matches the Rust impl).
 *
 * Build verification requires ThreadX headers + a configured port;
 * CMakeLists.txt parametrises THREADX_KERNEL_TARGET. Integration
 * tests live at the application level (per-board).
 */

#include <nros/platform.h>

#include <tx_api.h>

#include <stddef.h>
#include <stdint.h>
#include <string.h>

#ifndef TX_TIMER_TICKS_PER_SECOND
#  define TX_TIMER_TICKS_PER_SECOND 100u
#endif

#define MS_PER_TICK ((uint64_t) (1000U / TX_TIMER_TICKS_PER_SECOND))

/* ---- Clock ---- */

uint64_t nros_platform_clock_ms(void) {
    return (uint64_t) tx_time_get() * MS_PER_TICK;
}

uint64_t nros_platform_clock_us(void) {
    return (uint64_t) tx_time_get() * MS_PER_TICK * 1000ULL;
}

/* ---- Byte-pool wiring ----
 *
 * ThreadX has no global heap; allocations come out of a caller-owned
 * `TX_BYTE_POOL`. The application initialises the pool, calls
 * `nros_platform_threadx_set_byte_pool` once, and from then on the
 * canonical alloc/realloc/dealloc symbols route through it.
 */

static TX_BYTE_POOL *s_byte_pool = NULL;

void nros_platform_threadx_set_byte_pool(void *pool) {
    s_byte_pool = (TX_BYTE_POOL *) pool;
}

/* ---- Alloc ---- */

void *nros_platform_alloc(size_t size) {
    if (size == 0 || s_byte_pool == NULL) {
        return NULL;
    }
    void *p = NULL;
    if (tx_byte_allocate(s_byte_pool, &p, (ULONG) size, TX_WAIT_FOREVER) != TX_SUCCESS) {
        return NULL;
    }
    return p;
}

void nros_platform_dealloc(void *ptr) {
    if (ptr != NULL) {
        (void) tx_byte_release(ptr);
    }
}

/*
 * tx_byte_allocate has no "remaining size" query; mirror the Rust
 * impl's strategy of malloc + memcpy + free with a best-effort copy
 * up to the new size.
 */
void *nros_platform_realloc(void *ptr, size_t size) {
    if (size == 0) {
        nros_platform_dealloc(ptr);
        return NULL;
    }
    if (ptr == NULL) {
        return nros_platform_alloc(size);
    }
    void *out = nros_platform_alloc(size);
    if (out == NULL) {
        return NULL;
    }
    memcpy(out, ptr, size);
    nros_platform_dealloc(ptr);
    return out;
}

/* ---- Sleep ---- */

static inline ULONG ms_to_ticks(size_t ms) {
    return (ULONG) ((ms * TX_TIMER_TICKS_PER_SECOND + 999U) / 1000U);
}

void nros_platform_sleep_us(size_t us) {
    if (us == 0) return;
    ULONG ticks = (ULONG) ((us + 9999U) / 10000U);  /* assumes 100Hz tick */
    if (ticks == 0) ticks = 1;
    tx_thread_sleep(ticks);
}

void nros_platform_sleep_ms(size_t ms) {
    tx_thread_sleep(ms_to_ticks(ms));
}

void nros_platform_sleep_s(size_t s) {
    tx_thread_sleep(ms_to_ticks(s * 1000U));
}

/* ---- Yield ---- */

void nros_platform_yield_now(void) {
    tx_thread_relinquish();
}

/* ---- Random — deterministic xorshift64 ---- */

static uint64_t s_rng_state = 0x9E3779B97F4A7C15ULL;

void nros_platform_threadx_seed_rng(uint32_t value) {
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

/* ---- Wall clock — unsupported ---- */

uint64_t nros_platform_time_now_ms(void)              { return 0; }
uint32_t nros_platform_time_since_epoch_secs(void)    { return 0; }
uint32_t nros_platform_time_since_epoch_nanos(void)   { return 0; }

/* ---- Tasks ----
 *
 * Storage is a caller-allocated `TX_THREAD` instance (size known to
 * the application that supplies the stack as well; we forward both
 * via the `attr` argument's `stack_depth`).
 */

typedef struct {
    const char *name;
    UINT priority;
    size_t stack_depth;
    void  *stack_base; /* required: caller supplies stack memory */
} nros_threadx_task_attr_t;

int8_t nros_platform_task_init(void *task, void *attr,
                               void *(*entry)(void *), void *arg) {
    if (task == NULL || attr == NULL || entry == NULL) {
        return -1;
    }
    const nros_threadx_task_attr_t *a = (const nros_threadx_task_attr_t *) attr;
    if (a->stack_base == NULL || a->stack_depth == 0) {
        return -1;
    }
    /* ThreadX entry signature is `void(*)(ULONG)`. We forward our
     * pointer-shaped `arg` via reinterpretation; same trick the Rust
     * impl uses. */
    /* Reinterpret the user's `void *(*)(void *)` entry as the
     * ThreadX-shaped `void(*)(ULONG)`. The double-cast via
     * `void *` defeats `-Werror=cast-function-type`; ABI parity is
     * the caller's responsibility (matches the Rust impl). */
    union { void *(*src)(void *); VOID (*dst)(ULONG); } _entry_cvt;
    _entry_cvt.src = entry;
    UINT rc = tx_thread_create(
        (TX_THREAD *) task,
        a->name != NULL ? (char *) a->name : (char *) "nros",
        _entry_cvt.dst,
        (ULONG) (uintptr_t) arg,
        a->stack_base,
        (ULONG) a->stack_depth,
        a->priority != 0 ? a->priority : 16,  /* TX_MAX_PRIORITIES / 2 */
        a->priority != 0 ? a->priority : 16,
        TX_NO_TIME_SLICE,
        TX_AUTO_START);
    return rc == TX_SUCCESS ? 0 : -1;
}

int8_t nros_platform_task_join(void *task) {
    if (task == NULL) return -1;
    /* ThreadX has no native join. Poll the thread state until it
     * reports completed/terminated. */
    UINT state = 0;
    while (1) {
        if (tx_thread_info_get((TX_THREAD *) task,
                               TX_NULL, &state,
                               TX_NULL, TX_NULL, TX_NULL,
                               TX_NULL, TX_NULL, TX_NULL) != TX_SUCCESS) {
            return -1;
        }
        if (state == TX_COMPLETED || state == TX_TERMINATED) {
            return 0;
        }
        tx_thread_sleep(1);
    }
}

int8_t nros_platform_task_detach(void *task) {
    (void) task;
    return 0;  /* ThreadX threads don't need detach */
}

int8_t nros_platform_task_cancel(void *task) {
    if (task == NULL) return -1;
    return tx_thread_terminate((TX_THREAD *) task) == TX_SUCCESS ? 0 : -1;
}

void nros_platform_task_exit(void) {
    /* ThreadX threads exit by returning from their entry function.
     * A no-op here lets the caller's `return` propagate. */
}

void nros_platform_task_free(void **task) {
    if (task == NULL || *task == NULL) return;
    (void) tx_thread_delete((TX_THREAD *) *task);
}

/* ---- Mutex (non-recursive + recursive share the same primitive) ----
 *
 * ThreadX mutexes are recursive by design: the owner thread may
 * tx_mutex_get the same mutex multiple times and must tx_mutex_put
 * matching times. Both API families forward to the same code.
 */

int8_t nros_platform_mutex_init(void *m) {
    if (m == NULL) return -1;
    return tx_mutex_create((TX_MUTEX *) m, (char *) "nros", TX_INHERIT) == TX_SUCCESS
        ? 0 : -1;
}

int8_t nros_platform_mutex_drop(void *m) {
    if (m == NULL) return -1;
    return tx_mutex_delete((TX_MUTEX *) m) == TX_SUCCESS ? 0 : -1;
}

int8_t nros_platform_mutex_lock(void *m) {
    if (m == NULL) return -1;
    return tx_mutex_get((TX_MUTEX *) m, TX_WAIT_FOREVER) == TX_SUCCESS ? 0 : -1;
}

int8_t nros_platform_mutex_try_lock(void *m) {
    if (m == NULL) return -1;
    UINT rc = tx_mutex_get((TX_MUTEX *) m, TX_NO_WAIT);
    if (rc == TX_SUCCESS)         return 0;
    if (rc == TX_NOT_AVAILABLE)   return 1;
    return -1;
}

int8_t nros_platform_mutex_unlock(void *m) {
    if (m == NULL) return -1;
    return tx_mutex_put((TX_MUTEX *) m) == TX_SUCCESS ? 0 : -1;
}

int8_t nros_platform_mutex_rec_init(void *m)     { return nros_platform_mutex_init(m); }
int8_t nros_platform_mutex_rec_drop(void *m)     { return nros_platform_mutex_drop(m); }
int8_t nros_platform_mutex_rec_lock(void *m)     { return nros_platform_mutex_lock(m); }
int8_t nros_platform_mutex_rec_try_lock(void *m) { return nros_platform_mutex_try_lock(m); }
int8_t nros_platform_mutex_rec_unlock(void *m)   { return nros_platform_mutex_unlock(m); }

/* ---- Condvar — tx_semaphore-backed ----
 *
 * Storage is a `TX_SEMAPHORE`. Signal does tx_semaphore_put; wait
 * does tx_semaphore_get with the caller's mutex released around the
 * blocking call. Matches the Rust impl's behaviour.
 */

int8_t nros_platform_condvar_init(void *cv) {
    if (cv == NULL) return -1;
    return tx_semaphore_create((TX_SEMAPHORE *) cv, (char *) "nros_cv", 0) == TX_SUCCESS
        ? 0 : -1;
}

int8_t nros_platform_condvar_drop(void *cv) {
    if (cv == NULL) return -1;
    return tx_semaphore_delete((TX_SEMAPHORE *) cv) == TX_SUCCESS ? 0 : -1;
}

int8_t nros_platform_condvar_signal(void *cv) {
    if (cv == NULL) return -1;
    return tx_semaphore_put((TX_SEMAPHORE *) cv) == TX_SUCCESS ? 0 : -1;
}

int8_t nros_platform_condvar_signal_all(void *cv) {
    /* tx_semaphore has no broadcast; the Rust impl issues a single
     * put. Match that behaviour. Callers needing broadcast can
     * loop, but the semantic is "wake at least one". */
    return nros_platform_condvar_signal(cv);
}

int8_t nros_platform_condvar_wait(void *cv, void *m) {
    if (cv == NULL || m == NULL) return -1;
    nros_platform_mutex_unlock(m);
    UINT rc = tx_semaphore_get((TX_SEMAPHORE *) cv, TX_WAIT_FOREVER);
    nros_platform_mutex_lock(m);
    return rc == TX_SUCCESS ? 0 : -1;
}

int8_t nros_platform_condvar_wait_until(void *cv, void *m, uint64_t abstime_ms) {
    if (cv == NULL || m == NULL) return -1;
    uint64_t now = nros_platform_clock_ms();
    ULONG timeout_ticks = abstime_ms > now
        ? (ULONG) ((abstime_ms - now) * TX_TIMER_TICKS_PER_SECOND / 1000U)
        : 0;
    nros_platform_mutex_unlock(m);
    UINT rc = tx_semaphore_get((TX_SEMAPHORE *) cv, timeout_ticks);
    nros_platform_mutex_lock(m);
    if (rc == TX_SUCCESS)       return 0;
    if (rc == TX_NO_INSTANCE)   return 1;  /* timeout */
    return -1;
}

/* ============================================================
 *   Critical section (Phase 121.9)
 * ============================================================ */
/* `tx_interrupt_control(TX_INT_DISABLE)` returns the prior posture
 * (TX_INT_ENABLE or TX_INT_DISABLE); pass the same value back via
 * `tx_interrupt_control(token)` to restore. ThreadX's port already
 * stacks interrupt state across nested acquire/release pairs. */
uint32_t nros_platform_critical_section_acquire(void) {
    return (uint32_t) tx_interrupt_control(TX_INT_DISABLE);
}

void nros_platform_critical_section_release(uint32_t token) {
    (void) tx_interrupt_control((UINT) token);
}

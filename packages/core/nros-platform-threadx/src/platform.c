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
#include <sys/types.h>

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

/* ---- Heap stats (phase-230 1b / RFC-0034 D7) ----
 * Query the byte pool: used = pool size − available. ThreadX is a Mode-A
 * platform (both zenoh-pico's z_malloc and nano-ros allocations funnel
 * through nros_platform_alloc → tx_byte_allocate), so this is the exact
 * unified figure. Returns 0 before the pool is registered. */
size_t nros_platform_heap_used_bytes(void) {
    if (s_byte_pool == NULL) {
        return 0u;
    }
    ULONG available = 0;
    if (tx_byte_pool_info_get(s_byte_pool, TX_NULL, &available, TX_NULL, TX_NULL, TX_NULL,
                              TX_NULL) != TX_SUCCESS) {
        return 0u;
    }
    ULONG total = s_byte_pool->tx_byte_pool_size;
    return (size_t) (total >= available ? total - available : 0u);
}

size_t nros_platform_heap_total_bytes(void) {
    if (s_byte_pool == NULL) {
        return 0u;
    }
    return (size_t) s_byte_pool->tx_byte_pool_size;
}

/* phase-230 1f (RFC-0034): the `z_malloc`/`z_free` funnel on ThreadX is owned
 * by zpico-sys's `platform_aliases.c` (the `platform-aliases` feature, on by
 * default for the ThreadX boards) — a STRONG `z_malloc`/`z_free` →
 * `nros_platform_alloc`/`_dealloc`. ThreadX uses zenoh-pico's generic
 * `system/common` platform, which defines NO `z_malloc`, so there is no
 * vendored bypass to guard (unlike FreeRTOS) and the alias is the sole
 * definition on the link. The earlier `__attribute__((weak)) z_malloc` here
 * (RFC-0034's "footgun") was silently shadowed by that alias and is removed:
 * a ThreadX zenoh build without `platform-aliases` should fail to link loudly
 * (no `z_malloc` provider) rather than fall back to a hidden weak forwarder. */

/*
 * Minimal POSIX/picolibc hooks for freestanding ThreadX links. Cyclone DDS
 * avoids file I/O here; its ThreadX socket waitset path still references a
 * few POSIX names, so provide weak stubs until the backend supplies native
 * waitset plumbing.
 *
 * HOSTED EXCEPTION (`__linux__`): the ThreadX *linux* port (threadx-linux)
 * runs as a real Linux process linked against glibc, which already provides
 * strong open/close/read/write/lseek/pipe and a real `stdin`. A *weak*
 * definition living in the main executable still shadows the glibc public
 * symbol for the dynamic lookup, so these stubs would hijack every public
 * `write(2)` etc. C/C++ stdio escapes this because glibc routes printf
 * through the internal `__write` alias, but Rust's `std::io::Stdout` calls
 * the public `write`, gets the stub's unconditional `-1`, and panics
 * ("failed printing to stdout"); with `panic = "abort"` that SIGABRTs the
 * whole node before it prints its readiness banner. So compile these only
 * for the freestanding (bare-metal) ThreadX targets — the riscv64 cross
 * toolchain does not define `__linux__`; the hosted linux port does.
 */
#if !defined(__linux__)
__attribute__((weak)) void *stdin = NULL;

__attribute__((weak)) int open(const char *path, int flags, ...) {
    (void) path;
    (void) flags;
    return -1;
}

__attribute__((weak)) int close(int fd) {
    (void) fd;
    return -1;
}

__attribute__((weak)) ssize_t read(int fd, void *buf, size_t count) {
    (void) fd;
    (void) buf;
    (void) count;
    return -1;
}

__attribute__((weak)) ssize_t write(int fd, const void *buf, size_t count) {
    (void) fd;
    (void) buf;
    (void) count;
    return -1;
}

__attribute__((weak)) off_t lseek(int fd, off_t offset, int whence) {
    (void) fd;
    (void) offset;
    (void) whence;
    return (off_t) -1;
}

__attribute__((weak)) int pipe(int fds[2]) {
    if (fds != NULL) {
        fds[0] = -1;
        fds[1] = -1;
    }
    return -1;
}
#endif /* !__linux__ */

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

/* Phase 124.B.7.a — ISR-safe signal.
 *
 * tx_semaphore_put is ISR-safe under ThreadX (callable from any
 * context, including ISRs). Same impl as the thread-context path. */
int8_t nros_platform_condvar_signal_from_isr(void *cv) {
    if (cv == NULL) return -1;
    return tx_semaphore_put((TX_SEMAPHORE *) cv) == TX_SUCCESS ? 0 : -1;
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
 *   Wake primitive (Phase 130)
 *
 *   Binary semaphore backed by `tx_semaphore`. `tx_semaphore_put`
 *   is documented ISR-safe by ThreadX (callable from ISRs without
 *   a separate `_from_isr` variant).
 * ============================================================ */

/* TX_SEMAPHORE control block lives inline in caller storage. */
typedef TX_SEMAPHORE nros_wake_t;

int8_t nros_platform_wake_init(void *w) {
    if (w == NULL) return -1;
    /* Initial count 0 (waiter blocks until first put). */
    UINT rc = tx_semaphore_create((TX_SEMAPHORE *) w, (CHAR *) "nros_wake", 0u);
    return rc == TX_SUCCESS ? 0 : -1;
}

int8_t nros_platform_wake_drop(void *w) {
    if (w == NULL) return 0;
    (void) tx_semaphore_delete((TX_SEMAPHORE *) w);
    return 0;
}

int8_t nros_platform_wake_wait_ms(void *w, uint32_t timeout_ms) {
    if (w == NULL) return -1;
    /* ThreadX ticks come from `TX_TIMER_TICKS_PER_SECOND`; convert
     * ms via the same formula nros_platform_clock_ms uses. */
    ULONG ticks;
    if (timeout_ms == 0u) {
        ticks = TX_NO_WAIT;
    } else {
        ULONG tps = TX_TIMER_TICKS_PER_SECOND;
        if (tps == 0u) tps = 100u;  /* defensive fallback */
        ticks = (ULONG) (((uint64_t) timeout_ms * tps + 999u) / 1000u);
        if (ticks == 0u) ticks = 1u;
    }
    UINT rc = tx_semaphore_get((TX_SEMAPHORE *) w, ticks);
    if (rc == TX_SUCCESS)            return 0;
    if (rc == TX_NO_INSTANCE
        || rc == TX_WAIT_ABORTED)    return 1;
    return -1;
}

int8_t nros_platform_wake_signal(void *w) {
    if (w == NULL) return -1;
    UINT rc = tx_semaphore_ceiling_put((TX_SEMAPHORE *) w, 1u);
    /* Ceiling-put with limit 1 = binary semaphore semantics:
     * subsequent puts coalesce instead of stacking. */
    return rc == TX_SUCCESS ? 0 : -1;
}

int8_t nros_platform_wake_signal_from_isr(void *w) {
    /* tx_semaphore_put / _ceiling_put are ISR-safe per ThreadX spec. */
    return nros_platform_wake_signal(w);
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

/* ============================================================
 *   Logging (Phase 88)
 *
 *   ThreadX has no native text logger. Same fn-ptr pattern as
 *   FreeRTOS: board crate registers a writer at startup. Without
 *   one, the ABI is a no-op.
 * ============================================================ */
#include <string.h>

typedef void (*nros_platform_log_writer_fn)(
    uint8_t        severity,
    const uint8_t *name_ptr, uintptr_t name_len,
    const uint8_t *msg_ptr,  uintptr_t msg_len);

typedef void (*nros_platform_log_flush_fn)(void);

static nros_platform_log_writer_fn s_log_writer = NULL;
static nros_platform_log_flush_fn  s_log_flusher = NULL;

/* Board-crate hook. NULL flusher = writer is fully synchronous. */
void nros_platform_register_log_writer(nros_platform_log_writer_fn writer,
                                       nros_platform_log_flush_fn  flusher) {
    s_log_writer  = writer;
    s_log_flusher = flusher;
}

void nros_platform_log_write(uint8_t severity,
                             const uint8_t *name_ptr, uintptr_t name_len,
                             const uint8_t *msg_ptr,  uintptr_t msg_len) {
    nros_platform_log_writer_fn writer = s_log_writer;
    if (writer == NULL) {
        return;
    }
    writer(severity, name_ptr, name_len, msg_ptr, msg_len);
}

void nros_platform_log_flush(void) {
    nros_platform_log_flush_fn flusher = s_log_flusher;
    if (flusher != NULL) {
        flusher();
    }
}

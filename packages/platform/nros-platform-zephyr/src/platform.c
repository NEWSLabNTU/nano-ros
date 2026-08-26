/*
 * Phase 121.3.zephyr — native C implementation of the canonical
 * platform ABI for Zephyr RTOS.
 *
 * Behavioural parity with `nros-platform-zephyr`'s Rust impl. The
 * Rust port had to go through C shims for Zephyr's static-inline
 * macros (`k_uptime_get`, `k_msleep`, `k_yield`, …); the native C
 * port can call them directly.
 *
 *   - Clock    — k_uptime_get() returns int64_t milliseconds since
 *                boot; us from the 64-bit cycle counter where the board
 *                provides one, else from the tick clock (issue #531).
 *   - Alloc    — k_malloc / k_realloc / k_free against the kernel heap.
 *   - Sleep    — k_msleep / k_usleep / k_sleep.
 *   - Yield    — k_yield().
 *   - Random   — sys_rand32_get() / sys_rand_get(). Default Zephyr
 *                build provides a PRNG; CONFIG_ENTROPY_GENERATOR
 *                upgrades to hardware entropy.
 *   - Time     — wall clock unsupported unless the user enables
 *                CONFIG_RTC; defaults return 0.
 *   - Tasks    — pthread_create via the module's stack-provisioning shim.
 *   - Mutexes  — pthread_mutex_t handles, matching zenoh-pico's Zephyr ABI.
 *   - Condvars — pthread_cond_t handles, matching zenoh-pico's Zephyr ABI.
 *
 * Build verification requires a Zephyr workspace; CMakeLists.txt
 * is designed to be consumed as a Zephyr module or as an external
 * project linked via the Zephyr interface library.
 */

#include <nros/platform.h>

#include <zephyr/kernel.h>
#include <zephyr/random/random.h>
#ifdef CONFIG_POSIX_API
#include <zephyr/posix/pthread.h>
#endif

#include <errno.h>
#include <stddef.h>
#include <stdint.h>
#include <string.h>

/* ---- Clock ---- */

/* RFC-0073 — nanoseconds, from whichever source this board actually has.
 *
 * Issue #531: `k_cycle_get_64()` returns 0 unless
 * CONFIG_TIMER_HAS_64BIT_CYCLE_COUNTER is set (its __ASSERT compiles out in
 * release), and the Cortex-M SysTick driver only selects that symbol
 * `default y if (SYS_CLOCK_HW_CYCLES_PER_SEC > 60000000)`. So the cycle
 * counter is used only where the board provides one; everywhere else the
 * tick clock, which every board has. `IS_ENABLED` rather than `#ifdef` so
 * both arms keep compiling everywhere. */
uint64_t nros_platform_clock_ns(void) {
    if (IS_ENABLED(CONFIG_TIMER_HAS_64BIT_CYCLE_COUNTER)) {
        return (uint64_t) k_cyc_to_ns_floor64(k_cycle_get_64());
    }
    return (uint64_t) k_ticks_to_ns_floor64(k_uptime_ticks());
}

uint64_t nros_platform_clock_resolution_ns(void) {
    if (IS_ENABLED(CONFIG_TIMER_HAS_64BIT_CYCLE_COUNTER)) {
        /* One cycle. `sys_clock_hw_cycles_per_sec()` rather than the
         * Kconfig constant because a driver may only know its frequency
         * at runtime (CONFIG_TIMER_READS_ITS_FREQUENCY_AT_RUNTIME). */
        const uint64_t hz = (uint64_t) sys_clock_hw_cycles_per_sec();
        if (hz != 0U) {
            const uint64_t ns = 1000000000ULL / hz;
            return ns == 0U ? 1U : ns;
        }
    }
    return (uint64_t) k_ticks_to_ns_floor64(1);
}

/* issue 0758 — the Zephyr wall-clock epoch, acquired once over SNTP.
 *
 * WHY AN OFFSET AND NOT A QUERY PER CALL. `epoch_us` is on the message-stamp
 * path, so a network round trip per call is not an option — and SNTP's own
 * accuracy is worse than the monotonic clock's between acquisitions anyway. We
 * take ONE reading, subtract the monotonic clock at that instant to get a fixed
 * offset, and derive every later answer from `nros_platform_clock_ns()`. That
 * makes reads cheap, monotonic between acquisitions, and free of any failure
 * mode the network has.
 *
 * The jump the header warns about happens exactly once, when the offset is
 * installed: before it, callers get `0` (no wall clock) and stamp boot-relative
 * time knowingly; after it, absolute time. There is no window where a wrong
 * absolute value is published.
 *
 * Gated on CONFIG_SNTP so an image that does not want a network clock does not
 * link one, and answers `0` — which the header defines as "no wall clock here"
 * rather than an error. */
#ifdef CONFIG_SNTP
#include <zephyr/net/sntp.h>

/* Microseconds to add to `nros_platform_clock_ns()/1000` to get UNIX time.
 * Zero means "not acquired"; written once by the acquirer, read by every
 * `epoch_us` caller. `atomic` because the acquirer runs on the boot thread
 * while tiers may already be stamping. */
static atomic_t nros_zephyr_epoch_offset_lo = ATOMIC_INIT(0);
static atomic_t nros_zephyr_epoch_offset_hi = ATOMIC_INIT(0);

static uint64_t nros_zephyr_epoch_offset_get(void) {
    /* Read hi/lo twice and retry on a torn pair. A 64-bit offset does not fit
     * one atomic_t on 32-bit targets, and the value is written exactly once, so
     * a single retry is sufficient — this is not a general seqlock. */
    for (int attempt = 0; attempt < 2; attempt++) {
        uint32_t hi1 = (uint32_t) atomic_get(&nros_zephyr_epoch_offset_hi);
        uint32_t lo = (uint32_t) atomic_get(&nros_zephyr_epoch_offset_lo);
        uint32_t hi2 = (uint32_t) atomic_get(&nros_zephyr_epoch_offset_hi);
        if (hi1 == hi2) {
            return ((uint64_t) hi1 << 32) | (uint64_t) lo;
        }
    }
    return 0;
}

int nros_platform_epoch_acquire_sntp(const char *server, uint32_t timeout_ms);
int nros_platform_epoch_acquire_sntp(const char *server, uint32_t timeout_ms) {
    if (server == NULL || server[0] == '\0') {
        return -EINVAL;
    }
    struct sntp_time ts;
    int rc = sntp_simple(server, timeout_ms, &ts);
    if (rc != 0) {
        /* Leave the offset unset: the caller keeps getting 0 and keeps
         * stamping boot-relative time, which is the honest degradation. */
        return rc;
    }
    /* `fraction` is a 32-bit binary fraction of a second. Scale to us via a
     * 64-bit intermediate; >> 32 rather than / 2^32 so no division is emitted
     * on targets without one. */
    uint64_t frac_us = ((uint64_t) ts.fraction * 1000000ULL) >> 32;
    uint64_t now_us = ts.seconds * 1000000ULL + frac_us;
    uint64_t mono_us = nros_platform_clock_ns() / 1000ULL;
    uint64_t offset = now_us - mono_us;
    atomic_set(&nros_zephyr_epoch_offset_lo, (atomic_val_t) (uint32_t) offset);
    atomic_set(&nros_zephyr_epoch_offset_hi, (atomic_val_t) (uint32_t) (offset >> 32));
    return 0;
}

uint64_t nros_platform_epoch_us(void) {
    uint64_t offset = nros_zephyr_epoch_offset_get();
    if (offset == 0ULL) {
        return 0ULL; /* not acquired — no wall clock */
    }
    return offset + nros_platform_clock_ns() / 1000ULL;
}
#else  /* !CONFIG_SNTP */
/* No network clock compiled in. `0` is the honest answer, not a placeholder:
 * the header makes it mean "no epoch here", so a caller keeps stamping
 * boot-relative time knowingly rather than publishing a wrong absolute one. */
uint64_t nros_platform_epoch_us(void) {
    return 0;
}
#endif /* CONFIG_SNTP */

/* ---- Allocation ---- */

void *nros_platform_alloc(size_t size) {
    if (size == 0) return NULL;
    return k_malloc(size);
}

void *nros_platform_realloc(void *ptr, size_t size) {
    if (size == 0) {
        k_free(ptr);
        return NULL;
    }
    if (ptr == NULL) {
        return k_malloc(size);
    }
    /* Zephyr has no `k_realloc`; emulate. Same caveat as the
     * FreeRTOS port (best-effort copy up to new size). */
    void *out = k_malloc(size);
    if (out == NULL) return NULL;
    memcpy(out, ptr, size);
    k_free(ptr);
    return out;
}

void nros_platform_dealloc(void *ptr) {
    k_free(ptr);
}

/* ---- Heap stats (phase-230 Z5 / RFC-0034 D7) ----
 *
 * The true unified heap total on Zephyr: `k_malloc` (and thus
 * `nros_platform_alloc`, which backs zenoh-pico's `z_malloc`) AND
 * zephyr-lang-rust's `#[global_allocator]` (`malloc`) both draw from the
 * kernel system heap `_system_heap`. Querying its runtime stats gives the
 * exact C+Rust figure without owning the Rust allocator (D7 Mode B).
 * Requires CONFIG_SYS_HEAP_RUNTIME_STATS + a non-zero CONFIG_HEAP_MEM_POOL_SIZE
 * (which is what defines `_system_heap`); returns 0 ("unknown") otherwise. */
#if defined(CONFIG_SYS_HEAP_RUNTIME_STATS) && (CONFIG_HEAP_MEM_POOL_SIZE > 0)
extern struct k_heap _system_heap;

size_t nros_platform_heap_used_bytes(void) {
    struct sys_memory_stats st;
    if (sys_heap_runtime_stats_get(&_system_heap.heap, &st) != 0) return 0u;
    return (size_t) st.allocated_bytes;
}

size_t nros_platform_heap_total_bytes(void) {
    struct sys_memory_stats st;
    if (sys_heap_runtime_stats_get(&_system_heap.heap, &st) != 0) return 0u;
    return (size_t) (st.allocated_bytes + st.free_bytes);
}
#else
size_t nros_platform_heap_used_bytes(void) { return 0u; }
size_t nros_platform_heap_total_bytes(void) { return 0u; }
#endif

/* ---- Sleep ---- */

void nros_platform_sleep_us(size_t us) {
    if (us == 0) return;
    k_usleep((int32_t) us);
}

void nros_platform_sleep_ms(size_t ms) {
    if (ms == 0) return;
    k_msleep((int32_t) ms);
}

void nros_platform_sleep_s(size_t s) {
    k_sleep(K_SECONDS((int32_t) s));
}

/* ---- Yield ---- */

void nros_platform_yield_now(void) {
    k_yield();
}

/* ---- Random ---- */

uint8_t  nros_platform_random_u8(void)   { return (uint8_t)  sys_rand32_get(); }
uint16_t nros_platform_random_u16(void)  { return (uint16_t) sys_rand32_get(); }
uint32_t nros_platform_random_u32(void)  { return sys_rand32_get(); }

uint64_t nros_platform_random_u64(void) {
    uint64_t hi = sys_rand32_get();
    uint64_t lo = sys_rand32_get();
    return (hi << 32) | lo;
}

void nros_platform_random_fill(void *buf, size_t len) {
    sys_rand_get(buf, len);
}

/* ---- Wall clock — unsupported without CONFIG_RTC ---- */

/* No real-time clock on this port: 0 means "no wall clock", per the ABI. */
uint64_t nros_platform_time_now_ns(void)              { return 0; }

/* ---- Tasks ---- */

#ifdef CONFIG_POSIX_API

int nros_zephyr_task_create(pthread_t *thread,
                            void *(*entry)(void *),
                            void *arg);

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
    /* phase-364 W1 — see the posix port: INVALID for a caller-side
     * impossibility, NOMEM for a refused create (this port reaches Zephyr's
     * pthread layer, so it inherits EAGAIN-on-exhaustion semantics). */
    if (task == NULL || entry == NULL) return NROS_PLATFORM_RET_INVALID;

    /* phase-364 W3 — `attr` is accepted rather than ignored, but only the
     * fields this port can honour are read.
     *
     * Zephyr's native `k_thread_create` needs a `K_THREAD_STACK_DEFINE` region,
     * which carries MPU alignment requirements a caller cannot satisfy with a
     * plain allocation. This port therefore goes through Zephyr's POSIX layer,
     * where the stack comes from `CONFIG_PTHREAD_DYNAMIC_STACK` — so a
     * requested `stack_bytes` cannot be applied here and is deliberately not
     * silently pretended to be. A caller needing an exact Zephyr stack should
     * declare the thread in the image, which is the Zephyr-native answer. */
    const nros_platform_task_attr_t *a = (const nros_platform_task_attr_t *) attr;
    (void) a;

    return nros_zephyr_task_create((pthread_t *) task, entry, arg) == 0
               ? NROS_PLATFORM_RET_OK
               : NROS_PLATFORM_RET_NOMEM;
}


int8_t nros_platform_task_join(void *task) {
    if (task == NULL) return -1;
    return pthread_join(*(pthread_t *) task, NULL) == 0 ? 0 : -1;
}

int8_t nros_platform_task_detach(void *task) {
    if (task == NULL) return -1;
    return pthread_detach(*(pthread_t *) task) == 0 ? 0 : -1;
}

int8_t nros_platform_task_cancel(void *task) {
    if (task == NULL) return -1;
    return pthread_cancel(*(pthread_t *) task) == 0 ? 0 : -1;
}

void nros_platform_task_exit(void) {
    pthread_exit(NULL);
}

void nros_platform_task_free(void **task) {
    (void) task;  /* caller-owned pthread_t storage */
}

/* phase-360 W5 follow-up — the probe lives WITH the implementation that owns
 * the storage. It used to sit outside every arm and hardcode `pthread_t`, so
 * the non-POSIX builds (the default for these fixtures since issue 0566) could
 * not compile it at all: the type is not declared there. A size stated in one
 * place and allocated in another is issue 0570's trap in the mechanism built to
 * avoid it. */
size_t nros_platform_task_storage_size(void) {
    return sizeof(pthread_t);
}

size_t nros_platform_task_storage_align(void) {
    return _Alignof(pthread_t);
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
size_t nros_platform_mutex_storage_size(void) { return sizeof(pthread_mutex_t); }
size_t nros_platform_mutex_storage_align(void) { return _Alignof(pthread_mutex_t); }
size_t nros_platform_mutex_rec_storage_size(void) { return sizeof(pthread_mutex_t); }
size_t nros_platform_mutex_rec_storage_align(void) { return _Alignof(pthread_mutex_t); }
size_t nros_platform_condvar_storage_size(void) { return sizeof(pthread_cond_t); }
size_t nros_platform_condvar_storage_align(void) { return _Alignof(pthread_cond_t); }

_Static_assert(NROS_PLATFORM_MUTEX_STORAGE_SIZE >= sizeof(pthread_mutex_t),
               "NROS_PLATFORM_MUTEX_STORAGE_SIZE too small for this port");
_Static_assert(NROS_PLATFORM_MUTEX_REC_STORAGE_SIZE >= sizeof(pthread_mutex_t),
               "NROS_PLATFORM_MUTEX_REC_STORAGE_SIZE too small for this port");
_Static_assert(NROS_PLATFORM_CONDVAR_STORAGE_SIZE >= sizeof(pthread_cond_t),
               "NROS_PLATFORM_CONDVAR_STORAGE_SIZE too small for this port");
_Static_assert(NROS_PLATFORM_TASK_STORAGE_SIZE >= sizeof(pthread_t),
               "NROS_PLATFORM_TASK_STORAGE_SIZE too small for this port");


/* ---- Mutex ---- */

int8_t nros_platform_mutex_init(void *m) {
    if (m == NULL) return -1;
    return pthread_mutex_init((pthread_mutex_t *) m, NULL) == 0 ? 0 : -1;
}

int8_t nros_platform_mutex_drop(void *m) {
    if (m == NULL) return 0;
    return pthread_mutex_destroy((pthread_mutex_t *) m) == 0 ? 0 : -1;
}

int8_t nros_platform_mutex_lock(void *m) {
    if (m == NULL) return -1;
    return pthread_mutex_lock((pthread_mutex_t *) m) == 0 ? 0 : -1;
}

int8_t nros_platform_mutex_try_lock(void *m) {
    if (m == NULL) return -1;
    int rc = pthread_mutex_trylock((pthread_mutex_t *) m);
    if (rc == 0)       return 0;
    if (rc == EBUSY)   return 1;
    return -1;
}

int8_t nros_platform_mutex_unlock(void *m) {
    if (m == NULL) return -1;
    return pthread_mutex_unlock((pthread_mutex_t *) m) == 0 ? 0 : -1;
}

int8_t nros_platform_mutex_rec_init(void *m) {
    if (m == NULL) return -1;
    pthread_mutexattr_t attr;
    if (pthread_mutexattr_init(&attr) != 0) return -1;
    if (pthread_mutexattr_settype(&attr, PTHREAD_MUTEX_RECURSIVE) != 0) {
        (void) pthread_mutexattr_destroy(&attr);
        return -1;
    }
    int rc = pthread_mutex_init((pthread_mutex_t *) m, &attr);
    (void) pthread_mutexattr_destroy(&attr);
    return rc == 0 ? 0 : -1;
}
int8_t nros_platform_mutex_rec_drop(void *m)     { return nros_platform_mutex_drop(m); }
int8_t nros_platform_mutex_rec_lock(void *m)     { return nros_platform_mutex_lock(m); }
int8_t nros_platform_mutex_rec_try_lock(void *m) { return nros_platform_mutex_try_lock(m); }
int8_t nros_platform_mutex_rec_unlock(void *m)   { return nros_platform_mutex_unlock(m); }

/* ---- Condvars ---- */

int8_t nros_platform_condvar_init(void *cv) {
    if (cv == NULL) return -1;
    pthread_condattr_t attr;
    if (pthread_condattr_init(&attr) != 0) return -1;
    (void) pthread_condattr_setclock(&attr, CLOCK_MONOTONIC);
    int rc = pthread_cond_init((pthread_cond_t *) cv, &attr);
    (void) pthread_condattr_destroy(&attr);
    return rc == 0 ? 0 : -1;
}

int8_t nros_platform_condvar_drop(void *cv) {
    if (cv == NULL) return 0;
    return pthread_cond_destroy((pthread_cond_t *) cv) == 0 ? 0 : -1;
}

int8_t nros_platform_condvar_signal(void *cv) {
    if (cv == NULL) return -1;
    return pthread_cond_signal((pthread_cond_t *) cv) == 0 ? 0 : -1;
}

int8_t nros_platform_condvar_signal_all(void *cv) {
    if (cv == NULL) return -1;
    return pthread_cond_broadcast((pthread_cond_t *) cv) == 0 ? 0 : -1;
}

/* Phase 124.B.7.a — ISR-safe signal.
 *
 * Zephyr's k_condvar_signal documents that it MAY be called from
 * ISR context (the doc is split — newer kernels enforce thread
 * context). Use k_condvar_signal directly; if a backend exercises
 * the ISR path on a kernel build that rejects it, we'll need a
 * dedicated k_sem fallback. Track in the platform integration
 * tests. */
int8_t nros_platform_condvar_signal_from_isr(void *cv) {
    if (cv == NULL) return -1;
    return pthread_cond_signal((pthread_cond_t *) cv) == 0 ? 0 : -1;
}

int8_t nros_platform_condvar_wait(void *cv, void *m) {
    if (cv == NULL || m == NULL) return -1;
    return pthread_cond_wait((pthread_cond_t *) cv,
                             (pthread_mutex_t *) m) == 0 ? 0 : -1;
}

int8_t nros_platform_condvar_wait_until(void *cv, void *m, uint64_t abstime_ms) {
    if (cv == NULL || m == NULL) return -1;
    struct timespec ts = {
        .tv_sec = (time_t) (abstime_ms / 1000U),
        .tv_nsec = (long) ((abstime_ms % 1000U) * 1000000U),
    };
    int rc = pthread_cond_timedwait((pthread_cond_t *) cv,
                                    (pthread_mutex_t *) m,
                                    &ts);
    if (rc == 0)         return 0;
    if (rc == ETIMEDOUT) return 1;
    return -1;
}

#else

/* Issue #566 — the non-POSIX arm used to stub ~20 functions to -1, on a
 * kernel that has all of these natively. A -1 nobody checks is worse than
 * no implementation at all: it hands the caller a mutex that does not
 * lock. These are now backed by `k_mutex` / `k_condvar` / `k_thread`.
 *
 * Storage: the ABI's objects are caller-provided and opaque, and the
 * smallest consumer on this platform sizes them from `pthread_mutex_t`
 * (a `uint32_t`), which cannot hold a `struct k_mutex` inline. So the
 * caller's storage holds a POINTER to a heap-allocated kernel object —
 * the same shape the FreeRTOS port uses for its `SemaphoreHandle_t`.
 * That needs storage of at least `sizeof(void *)`, which every caller
 * provides on this 32-bit ABI. */

#define NROS_Z_HANDLE(ptr) (*(void **) (ptr))

int8_t nros_platform_mutex_init(void *m) {
    if (m == NULL) return -1;
/* RFC-0034 D6 -- allocations here go through `nros_platform_alloc`, NOT
 * `k_malloc` directly. On Zephyr both reach `_system_heap` today, so the
 * distinction looks cosmetic; it is not. The funnel is what lets the arena's
 * ALGORITHM be swapped (a constant-time allocator for the real-time tier)
 * without hunting every allocation site in the tree. A direct `k_malloc` here
 * would keep allocating from the kernel heap after the funnel moved, silently
 * splitting the one arena D6 exists to keep whole. */
    struct k_mutex *mu = nros_platform_alloc(sizeof(struct k_mutex));
    if (mu == NULL) return -1;
    if (k_mutex_init(mu) != 0) {
        nros_platform_dealloc(mu);
        return -1;
    }
    NROS_Z_HANDLE(m) = mu;
    return 0;
}

int8_t nros_platform_mutex_drop(void *m) {
    if (m == NULL) return 0;
    struct k_mutex *mu = NROS_Z_HANDLE(m);
    if (mu == NULL) return 0;
    nros_platform_dealloc(mu);
    NROS_Z_HANDLE(m) = NULL;
    return 0;
}

int8_t nros_platform_mutex_lock(void *m) {
    if (m == NULL) return -1;
    struct k_mutex *mu = NROS_Z_HANDLE(m);
    if (mu == NULL) return -1;
    return k_mutex_lock(mu, K_FOREVER) == 0 ? 0 : -1;
}

int8_t nros_platform_mutex_try_lock(void *m) {
    if (m == NULL) return -1;
    struct k_mutex *mu = NROS_Z_HANDLE(m);
    if (mu == NULL) return -1;
    /* 0 = acquired, 1 = would block (ABI's `try` contract), -1 = error. */
    int rc = k_mutex_lock(mu, K_NO_WAIT);
    if (rc == 0)      return 0;
    if (rc == -EBUSY) return 1;
    return -1;
}

int8_t nros_platform_mutex_unlock(void *m) {
    if (m == NULL) return -1;
    struct k_mutex *mu = NROS_Z_HANDLE(m);
    if (mu == NULL) return -1;
    return k_mutex_unlock(mu) == 0 ? 0 : -1;
}

/* `k_mutex` is recursive for the owning thread (`lock_count`, see
 * zephyr/kernel/mutex.c), which is exactly what the ABI requires of
 * `mutex_rec_*` — zenoh-pico deadlocks on a non-recursive one. */
int8_t nros_platform_mutex_rec_init(void *m)     { return nros_platform_mutex_init(m); }
int8_t nros_platform_mutex_rec_drop(void *m)     { return nros_platform_mutex_drop(m); }
int8_t nros_platform_mutex_rec_lock(void *m)     { return nros_platform_mutex_lock(m); }
int8_t nros_platform_mutex_rec_try_lock(void *m) { return nros_platform_mutex_try_lock(m); }
int8_t nros_platform_mutex_rec_unlock(void *m)   { return nros_platform_mutex_unlock(m); }

int8_t nros_platform_condvar_init(void *cv) {
    if (cv == NULL) return -1;
    struct k_condvar *c = nros_platform_alloc(sizeof(struct k_condvar));
    if (c == NULL) return -1;
    if (k_condvar_init(c) != 0) {
        nros_platform_dealloc(c);
        return -1;
    }
    NROS_Z_HANDLE(cv) = c;
    return 0;
}

int8_t nros_platform_condvar_drop(void *cv) {
    if (cv == NULL) return 0;
    struct k_condvar *c = NROS_Z_HANDLE(cv);
    if (c == NULL) return 0;
    nros_platform_dealloc(c);
    NROS_Z_HANDLE(cv) = NULL;
    return 0;
}

int8_t nros_platform_condvar_signal(void *cv) {
    if (cv == NULL) return -1;
    struct k_condvar *c = NROS_Z_HANDLE(cv);
    if (c == NULL) return -1;
    return k_condvar_signal(c) == 0 ? 0 : -1;
}

int8_t nros_platform_condvar_signal_all(void *cv) {
    if (cv == NULL) return -1;
    struct k_condvar *c = NROS_Z_HANDLE(cv);
    if (c == NULL) return -1;
    return k_condvar_broadcast(c) >= 0 ? 0 : -1;
}

int8_t nros_platform_condvar_wait(void *cv, void *m) {
    if (cv == NULL || m == NULL) return -1;
    struct k_condvar *c = NROS_Z_HANDLE(cv);
    struct k_mutex *mu = NROS_Z_HANDLE(m);
    if (c == NULL || mu == NULL) return -1;
    return k_condvar_wait(c, mu, K_FOREVER) == 0 ? 0 : -1;
}

int8_t nros_platform_condvar_wait_until(void *cv, void *m, uint64_t abstime_ms) {
    if (cv == NULL || m == NULL) return -1;
    struct k_condvar *c = NROS_Z_HANDLE(cv);
    struct k_mutex *mu = NROS_Z_HANDLE(m);
    if (c == NULL || mu == NULL) return -1;
    /* `abstime_ms` is on the same monotonic epoch as the platform clock;
     * k_condvar_wait takes a RELATIVE timeout. */
    uint64_t now_ms = nros_platform_clock_ns() / 1000000ULL;
    k_timeout_t rel = (abstime_ms > now_ms)
                          ? K_MSEC((uint32_t) (abstime_ms - now_ms))
                          : K_NO_WAIT;
    int rc = k_condvar_wait(c, mu, rel);
    if (rc == 0)         return 0;
    if (rc == -EAGAIN)   return 1; /* timed out — the ABI's `1` */
    return -1;
}

/* Tasks need a stack, and a dynamically-created thread needs
 * CONFIG_DYNAMIC_THREAD to allocate one. Where that is unavailable this
 * still cannot spawn — but it says so at the call rather than pretending,
 * and mutexes/condvars above no longer go down with it. */
#if defined(CONFIG_DYNAMIC_THREAD) && defined(CONFIG_THREAD_STACK_INFO)

#ifndef NROS_ZEPHYR_TASK_STACK_SIZE
#define NROS_ZEPHYR_TASK_STACK_SIZE 4096
#endif

struct nros_z_task {
    struct k_thread thread;
    k_thread_stack_t *stack;
    void *(*entry)(void *);
    void *arg;
};

static void nros_z_task_trampoline(void *p1, void *p2, void *p3) {
    (void) p2;
    (void) p3;
    struct nros_z_task *t = (struct nros_z_task *) p1;
    (void) t->entry(t->arg);
}

int8_t nros_platform_task_init(void *task, void *attr,
                               void *(*entry)(void *), void *arg) {
    (void) attr;
    if (task == NULL || entry == NULL) return -1;
    struct nros_z_task *t = nros_platform_alloc(sizeof(struct nros_z_task));
    if (t == NULL) return -1;
    t->stack = k_thread_stack_alloc(NROS_ZEPHYR_TASK_STACK_SIZE, 0);
    if (t->stack == NULL) {
        nros_platform_dealloc(t);
        return -1;
    }
    t->entry = entry;
    t->arg = arg;
    k_thread_create(&t->thread, t->stack, NROS_ZEPHYR_TASK_STACK_SIZE,
                    nros_z_task_trampoline, t, NULL, NULL,
                    K_PRIO_PREEMPT(5), 0, K_NO_WAIT);
    NROS_Z_HANDLE(task) = t;
    return 0;
}

int8_t nros_platform_task_join(void *task) {
    if (task == NULL) return -1;
    struct nros_z_task *t = NROS_Z_HANDLE(task);
    if (t == NULL) return -1;
    return k_thread_join(&t->thread, K_FOREVER) == 0 ? 0 : -1;
}

int8_t nros_platform_task_detach(void *task) {
    (void) task; /* Zephyr threads need no detach; storage is freed by _free. */
    return 0;
}

int8_t nros_platform_task_cancel(void *task) {
    if (task == NULL) return -1;
    struct nros_z_task *t = NROS_Z_HANDLE(task);
    if (t == NULL) return -1;
    k_thread_abort(&t->thread);
    return 0;
}

void nros_platform_task_exit(void) {
    k_thread_abort(k_current_get());
}

void nros_platform_task_free(void **task) {
    if (task == NULL || *task == NULL) return;
    struct nros_z_task *t = (struct nros_z_task *) *task;
    if (t->stack != NULL) {
        (void) k_thread_stack_free(t->stack);
    }
    nros_platform_dealloc(t);
    *task = NULL;
}

/* The k_thread-backed storage this arm actually allocates. */
size_t nros_platform_task_storage_size(void) {
    return sizeof(struct nros_z_task);
}

size_t nros_platform_task_storage_align(void) {
    return _Alignof(struct nros_z_task);
}

#else /* no CONFIG_DYNAMIC_THREAD */

int8_t nros_platform_task_init(void *task, void *attr,
                               void *(*entry)(void *), void *arg) {
    (void) task; (void) attr; (void) entry; (void) arg;
    /* Needs CONFIG_POSIX_API, or CONFIG_DYNAMIC_THREAD +
     * CONFIG_THREAD_STACK_INFO for a dynamically allocated stack. */
    return -1;
}
int8_t nros_platform_task_join(void *task)   { (void) task; return -1; }
int8_t nros_platform_task_detach(void *task) { (void) task; return -1; }
int8_t nros_platform_task_cancel(void *task) { (void) task; return -1; }
void nros_platform_task_exit(void) {}
void nros_platform_task_free(void **task)    { (void) task; }

/* This arm cannot start a task at all (`task_init` returns -1), so there is no
 * storage to size. Zero is the honest answer, not a guess that would let a
 * caller allocate for a task it can never create. */
size_t nros_platform_task_storage_size(void)  { return 0; }
size_t nros_platform_task_storage_align(void) { return 1; }

#endif /* CONFIG_DYNAMIC_THREAD */

#endif

/* ============================================================
 *   Wake primitive (Phase 130)
 *
 *   Binary semaphore backed by `k_sem`. Bypasses libc pthread
 *   so the executor's spin_once wake is not subject to the
 *   Zephyr libc `pthread_cond_timedwait` deadline-hang
 *   (Phase 127.C.4). `k_sem_give` is ISR-safe per Zephyr spec.
 *   Available unconditionally — `k_sem` ships in every Zephyr
 *   kernel build, no Kconfig gate.
 * ============================================================ */

int8_t nros_platform_wake_init(void *w) {
    if (w == NULL) return -1;
    k_sem_init((struct k_sem *) w, 0u, 1u);
    return 0;
}

int8_t nros_platform_wake_drop(void *w) {
    /* k_sem has no destructor; reset to a known-empty state so any
     * stale waiter (impossible if the caller respects ownership)
     * sees -EAGAIN on the next take. */
    if (w == NULL) return 0;
    k_sem_reset((struct k_sem *) w);
    return 0;
}

int8_t nros_platform_wake_wait_ms(void *w, uint32_t timeout_ms) {
    if (w == NULL) return -1;
    k_timeout_t to = (timeout_ms == 0u) ? K_NO_WAIT : K_MSEC(timeout_ms);
    int rc = k_sem_take((struct k_sem *) w, to);
    if (rc == 0)        return 0;
    if (rc == -EAGAIN)  return 1;
    return -1;
}

int8_t nros_platform_wake_signal(void *w) {
    if (w == NULL) return -1;
    k_sem_give((struct k_sem *) w);
    return 0;
}

int8_t nros_platform_wake_signal_from_isr(void *w) {
    if (w == NULL) return -1;
    /* k_sem_give is documented ISR-safe on Zephyr. */
    k_sem_give((struct k_sem *) w);
    return 0;
}

size_t nros_platform_wake_storage_size(void) {
    return sizeof(struct k_sem);
}

size_t nros_platform_wake_storage_align(void) {
    return __alignof__(struct k_sem);
}

/* phase-359 W10 — opaque-storage sizing for `task`, the sibling of the wake
 * probes above. `task_init`'s contract says the implementor decides the size;
 * these let a caller ASK instead of hard-coding it (issue 0570's trap). */


/* ============================================================
 *   Critical section (Phase 121.9)
 * ============================================================ */
/* Zephyr's `irq_lock` returns the prior IRQ posture; `irq_unlock`
 * accepts the same value back. Reentrant: Zephyr's port layer stacks
 * the key word correctly across nested calls. */
uint32_t nros_platform_critical_section_acquire(void) {
    return (uint32_t) irq_lock();
}

void nros_platform_critical_section_release(uint32_t token) {
    irq_unlock((unsigned int) token);
}

/* ============================================================
 *   Logging (Phase 88)
 *
 *   When CONFIG_LOG=y, route through Zephyr's logging subsystem
 *   (`LOG_INF` / `LOG_WRN` etc., backed by `log_msg_runtime_create`).
 *   Falls back to `printk` when CONFIG_LOG is disabled so the
 *   message still reaches the system console.
 *
 *   Module name `nros` is registered with `LOG_MODULE_REGISTER` so
 *   Zephyr's shell `log enable warn nros` filters at the platform
 *   layer (in addition to the per-Logger threshold on the nros-log
 *   side). ISR-safe: Zephyr LOG queues for deferred processing.
 * ============================================================ */
#ifdef CONFIG_LOG
#include <zephyr/logging/log.h>
LOG_MODULE_REGISTER(nros, CONFIG_LOG_DEFAULT_LEVEL);
#endif

#include <stdio.h>

#define NROS_PLATFORM_LOG_BUFSZ 1280

static void nros_platform_log_format(char *out, size_t out_sz,
                                     const uint8_t *name_ptr, uintptr_t name_len,
                                     const uint8_t *msg_ptr,  uintptr_t msg_len) {
    if (name_ptr != NULL && name_len > 0) {
        snprintf(out, out_sz, "%.*s: %.*s",
                 (int) name_len, (const char *) name_ptr,
                 (int) msg_len,  (const char *) msg_ptr);
    } else {
        snprintf(out, out_sz, "%.*s",
                 (int) msg_len, (const char *) msg_ptr);
    }
}

void nros_platform_log_write(uint8_t severity,
                             const uint8_t *name_ptr, uintptr_t name_len,
                             const uint8_t *msg_ptr,  uintptr_t msg_len) {
    if (msg_ptr == NULL && msg_len > 0) {
        return;
    }
    char buf[NROS_PLATFORM_LOG_BUFSZ];
    nros_platform_log_format(buf, sizeof(buf), name_ptr, name_len, msg_ptr, msg_len);
#ifdef CONFIG_LOG
    switch (severity) {
    case 5: /* Fatal */
    case 4: /* Error */ LOG_ERR("%s", buf); break;
    case 3: /* Warn  */ LOG_WRN("%s", buf); break;
    case 2: /* Info  */ LOG_INF("%s", buf); break;
    case 1: /* Debug */
    case 0: /* Trace */ LOG_DBG("%s", buf); break;
    default:            LOG_INF("%s", buf); break;
    }
#else
    static const char *labels[] = {
        "[TRACE]", "[DEBUG]", "[INFO]", "[WARN]", "[ERROR]", "[FATAL]",
    };
    const char *label = severity <= 5 ? labels[severity] : "[?]";
    printk("%s %s\n", label, buf);
#endif
}

void nros_platform_log_flush(void) {
#ifdef CONFIG_LOG
    /* Best-effort: yield so the log thread drains its deferred queue. */
    k_yield();
#endif
}

/* ============================================================
 * Runtime locator override — nano-ros #166 / phase-286 W1.
 *
 * native_sim test parallelism: the test harness starts a per-test zenohd on an
 * ephemeral port and launches the image with `-testargs --nros-locator=<loc>`.
 * Reading that here (preferred over the build-time-baked
 * `CONFIG_NROS_ZENOH_LOCATOR`) gives every test a distinct router port, so the
 * zenoh e2e lanes stop serializing on one shared baked port.
 *
 * Why `-testargs`: native_sim's own option parser ABORTS on an unregistered
 * option ("Incorrect option '--nros-locator=…'"). Everything after `-testargs`
 * is instead collected into the native-simulator "test args" argv, bypassing
 * that parser; the app reads it via the native-simulator public API
 * `nsi_get_test_cmd_line_args`. No NSI_TASK / option-struct registration needed.
 *
 * native_sim / native_posix only (`CONFIG_ARCH_POSIX`): on real embedded there
 * is no host argv channel, so the hook returns NULL and the baked locator
 * stands. The `loc` form matches the bake — `tcp/host:port` (zenoh) or bare
 * `host:port` (xrce), exactly as the example `build.rs` unifies `NROS_LOCATOR`.
 * ============================================================ */
#if defined(CONFIG_ARCH_POSIX)
/* Provided by the native-simulator runtime (linked into every native_sim
 * image). Prototype declared locally so this module does not couple to the
 * board-local `<nsi_cmdline.h>` include path. */
extern void nsi_get_test_cmd_line_args(int *argc, char ***argv);

const char *nros_runtime_locator_override(void) {
    static const char *cached;
    static int resolved;
    if (resolved) {
        return cached;
    }
    resolved = 1;
    cached = NULL;
    int argc = 0;
    char **argv = NULL;
    nsi_get_test_cmd_line_args(&argc, &argv);
    static const char prefix[] = "--nros-locator=";
    const size_t plen = sizeof(prefix) - 1;
    for (int i = 0; argv != NULL && i < argc; i++) {
        if (argv[i] != NULL && strncmp(argv[i], prefix, plen) == 0 && argv[i][plen] != '\0') {
            cached = argv[i] + plen;
        }
    }
    return cached;
}
#else
const char *nros_runtime_locator_override(void) {
    return NULL;
}
#endif

/* ---- Fatal error (phase-366 / RFC-0077) ----
 *
 * `printk` then `k_panic()`. Both halves are deliberate.
 *
 * `printk` rather than the `LOG_*` macros: the logging subsystem may be
 * deferred (`CONFIG_LOG_MODE_DEFERRED`), in which case a message logged here is
 * processed by a thread that will never run again. `printk` is synchronous and
 * works with the scheduler locked or from an ISR, which is the contract this
 * function has to honour.
 *
 * `k_panic()` rather than a spin: it routes into Zephyr's own fatal path, so an
 * image that installed `k_sys_fatal_error_handler` — the RTOS's own weak
 * override hook — still gets to run it. Spinning here would silently defeat
 * that.
 */
__attribute__((weak))
_Noreturn void nros_platform_panic(const char *msg, size_t len) {
    if (msg != NULL && len > 0) {
        printk("nros: PANIC %.*s\n", (int) len, msg);
    } else {
        printk("nros: PANIC\n");
    }
    k_panic();
    /* k_panic() is noreturn, but it is a macro on some lines and the compiler
     * cannot always see that; keep the contract explicit. */
    for (;;) {
    }
}

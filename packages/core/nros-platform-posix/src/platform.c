/*
 * Phase 121.3.posix — native C implementation of the canonical platform ABI.
 *
 * Header source of truth: `<nros/platform.h>` (`nros-platform-cffi`).
 *
 * Each `nros_platform_*` symbol below maps to the closest POSIX
 * primitive. The intent is parity with `PosixPlatform`'s Rust impl
 * (`packages/core/nros-platform-posix/src/lib.rs`); the two share
 * the same canonical ABI and may not be linked into the same binary
 * (duplicate `#[no_mangle]` symbols / `extern "C"` definitions).
 *
 * Build standalone via the sibling `CMakeLists.txt`, or let
 * `nros-platform-cffi`'s `posix-c-port` feature compile this file
 * through the `cc` build dep.
 */

#define _POSIX_C_SOURCE 200809L
#define _DEFAULT_SOURCE

#include <nros/platform.h>

#include <errno.h>
#include <pthread.h>
#include <sched.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>
#include <sys/time.h>
#include <time.h>
#include <unistd.h>

/* ---- Clock (monotonic) ---- */

uint64_t nros_platform_clock_ms(void) {
    struct timespec ts;
    if (clock_gettime(CLOCK_MONOTONIC, &ts) != 0) {
        return 0;
    }
    return (uint64_t) ts.tv_sec * 1000ULL + (uint64_t) ts.tv_nsec / 1000000ULL;
}

uint64_t nros_platform_clock_us(void) {
    struct timespec ts;
    if (clock_gettime(CLOCK_MONOTONIC, &ts) != 0) {
        return 0;
    }
    return (uint64_t) ts.tv_sec * 1000000ULL + (uint64_t) ts.tv_nsec / 1000ULL;
}

/* ---- Allocation ---- */

void *nros_platform_alloc(size_t size) {
    if (size == 0) {
        return NULL;
    }
    return malloc(size);
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

/* ---- Sleep ---- */

void nros_platform_sleep_us(size_t us) {
    struct timespec ts = {
        .tv_sec  = (time_t) (us / 1000000),
        .tv_nsec = (long)   ((us % 1000000) * 1000),
    };
    while (nanosleep(&ts, &ts) == -1 && errno == EINTR) {
        /* continue with remaining time */
    }
}

void nros_platform_sleep_ms(size_t ms) {
    struct timespec ts = {
        .tv_sec  = (time_t) (ms / 1000),
        .tv_nsec = (long)   ((ms % 1000) * 1000000),
    };
    while (nanosleep(&ts, &ts) == -1 && errno == EINTR) {
    }
}

void nros_platform_sleep_s(size_t s) {
    struct timespec ts = { .tv_sec = (time_t) s, .tv_nsec = 0 };
    while (nanosleep(&ts, &ts) == -1 && errno == EINTR) {
    }
}

/* ---- Cooperative yield ---- */

void nros_platform_yield_now(void) {
    sched_yield();
}

/* ---- Random ---- */
/*
 * The Rust `PosixPlatform` uses a deterministic xorshift seeded from
 * a fixed constant for reproducibility; matching that exactly keeps
 * the two ports observable-equivalent for tests.
 */

static uint64_t s_rng_state = 0x9E3779B97F4A7C15ULL;

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

/* ---- Wall clock ---- */

uint64_t nros_platform_time_now_ms(void) {
    struct timespec ts;
    if (clock_gettime(CLOCK_REALTIME, &ts) != 0) {
        return 0;
    }
    return (uint64_t) ts.tv_sec * 1000ULL + (uint64_t) ts.tv_nsec / 1000000ULL;
}

uint32_t nros_platform_time_since_epoch_secs(void) {
    struct timespec ts;
    if (clock_gettime(CLOCK_REALTIME, &ts) != 0) {
        return 0;
    }
    return (uint32_t) ts.tv_sec;
}

uint32_t nros_platform_time_since_epoch_nanos(void) {
    struct timespec ts;
    if (clock_gettime(CLOCK_REALTIME, &ts) != 0) {
        return 0;
    }
    return (uint32_t) ts.tv_nsec;
}

/* ---- Tasks ----
 *
 * Task storage is `pthread_t`. Caller allocates `sizeof(pthread_t)`
 * bytes; we trust the caller-supplied buffer.
 */

typedef struct {
    void *(*entry)(void *);
    void *arg;
} nros_posix_task_arg_t;

static void *nros_posix_task_trampoline(void *raw) {
    /* The Rust trait signature uses `Option<unsafe extern "C" fn(*mut c_void) -> *mut c_void>`
     * which lowers to the same shape as a pthread start_routine, so
     * we can forward directly. */
    return raw;
}

int8_t nros_platform_task_init(void *task, void *attr,
                               void *(*entry)(void *), void *arg) {
    (void) attr;
    if (task == NULL || entry == NULL) {
        return -1;
    }
    pthread_t *t = (pthread_t *) task;
    /* The simple path: forward directly to pthread_create. */
    if (pthread_create(t, NULL, entry, arg) != 0) {
        return -1;
    }
    /* Reference the trampoline so the compiler doesn't strip it; a
     * future signature change (e.g. argument repacking) will route
     * through it. */
    (void) nros_posix_task_trampoline;
    return 0;
}

int8_t nros_platform_task_join(void *task) {
    if (task == NULL) {
        return -1;
    }
    return pthread_join(*(pthread_t *) task, NULL) == 0 ? 0 : -1;
}

int8_t nros_platform_task_detach(void *task) {
    if (task == NULL) {
        return -1;
    }
    return pthread_detach(*(pthread_t *) task) == 0 ? 0 : -1;
}

int8_t nros_platform_task_cancel(void *task) {
    if (task == NULL) {
        return -1;
    }
    return pthread_cancel(*(pthread_t *) task) == 0 ? 0 : -1;
}

void nros_platform_task_exit(void) {
    pthread_exit(NULL);
}

void nros_platform_task_free(void **task) {
    (void) task;
    /* Storage is caller-owned; nothing to free here. */
}

/* ---- Non-recursive mutex ---- */

int8_t nros_platform_mutex_init(void *m) {
    if (m == NULL) {
        return -1;
    }
    return pthread_mutex_init((pthread_mutex_t *) m, NULL) == 0 ? 0 : -1;
}

int8_t nros_platform_mutex_drop(void *m) {
    if (m == NULL) {
        return -1;
    }
    return pthread_mutex_destroy((pthread_mutex_t *) m) == 0 ? 0 : -1;
}

int8_t nros_platform_mutex_lock(void *m) {
    if (m == NULL) {
        return -1;
    }
    return pthread_mutex_lock((pthread_mutex_t *) m) == 0 ? 0 : -1;
}

int8_t nros_platform_mutex_try_lock(void *m) {
    if (m == NULL) {
        return -1;
    }
    int r = pthread_mutex_trylock((pthread_mutex_t *) m);
    if (r == 0)        return 0;
    if (r == EBUSY)    return 1;
    return -1;
}

int8_t nros_platform_mutex_unlock(void *m) {
    if (m == NULL) {
        return -1;
    }
    return pthread_mutex_unlock((pthread_mutex_t *) m) == 0 ? 0 : -1;
}

/* ---- Recursive mutex ---- */

int8_t nros_platform_mutex_rec_init(void *m) {
    if (m == NULL) {
        return -1;
    }
    pthread_mutexattr_t attr;
    if (pthread_mutexattr_init(&attr) != 0) {
        return -1;
    }
    int8_t rc = -1;
    if (pthread_mutexattr_settype(&attr, PTHREAD_MUTEX_RECURSIVE) == 0
        && pthread_mutex_init((pthread_mutex_t *) m, &attr) == 0) {
        rc = 0;
    }
    pthread_mutexattr_destroy(&attr);
    return rc;
}

int8_t nros_platform_mutex_rec_drop(void *m) {
    return nros_platform_mutex_drop(m);
}

int8_t nros_platform_mutex_rec_lock(void *m) {
    return nros_platform_mutex_lock(m);
}

int8_t nros_platform_mutex_rec_try_lock(void *m) {
    return nros_platform_mutex_try_lock(m);
}

int8_t nros_platform_mutex_rec_unlock(void *m) {
    return nros_platform_mutex_unlock(m);
}

/* ---- Condition variables ---- */

int8_t nros_platform_condvar_init(void *cv) {
    if (cv == NULL) {
        return -1;
    }
    return pthread_cond_init((pthread_cond_t *) cv, NULL) == 0 ? 0 : -1;
}

int8_t nros_platform_condvar_drop(void *cv) {
    if (cv == NULL) {
        return -1;
    }
    return pthread_cond_destroy((pthread_cond_t *) cv) == 0 ? 0 : -1;
}

int8_t nros_platform_condvar_signal(void *cv) {
    if (cv == NULL) {
        return -1;
    }
    return pthread_cond_signal((pthread_cond_t *) cv) == 0 ? 0 : -1;
}

int8_t nros_platform_condvar_signal_all(void *cv) {
    if (cv == NULL) {
        return -1;
    }
    return pthread_cond_broadcast((pthread_cond_t *) cv) == 0 ? 0 : -1;
}

/* Phase 124.B.7.a — ISR-safe signal.
 *
 * pthread_cond_signal is NOT async-signal-safe per POSIX (and glibc
 * gives no stronger guarantee), so callers from a POSIX signal
 * handler MUST NOT use this directly. The intended impl is a
 * `signalfd`/`eventfd` write forwarded by a runtime-owned worker
 * thread (Phase 124.B.7.c). For now, callers from thread context
 * (Rust panic handler, executor halt path) keep working through the
 * regular cond_signal — the signal-handler case returns -1 so the
 * caller can route through a self-pipe.
 *
 * Detecting "are we in a signal handler" portably is not possible;
 * caller discipline is the contract. Documented in the header. */
int8_t nros_platform_condvar_signal_from_isr(void *cv) {
    if (cv == NULL) {
        return -1;
    }
    /* TODO(124.B.7.c): forward via signalfd/eventfd self-pipe to a
     * worker thread that calls pthread_cond_signal under the wake
     * mutex. Today: same as condvar_signal — safe from any non-
     * signal-handler thread, undefined behaviour from a signal
     * handler. */
    return pthread_cond_signal((pthread_cond_t *) cv) == 0 ? 0 : -1;
}

int8_t nros_platform_condvar_wait(void *cv, void *m) {
    if (cv == NULL || m == NULL) {
        return -1;
    }
    return pthread_cond_wait((pthread_cond_t *) cv, (pthread_mutex_t *) m) == 0
        ? 0 : -1;
}

int8_t nros_platform_condvar_wait_until(void *cv, void *m, uint64_t abstime_ms) {
    if (cv == NULL || m == NULL) {
        return -1;
    }
    /* `abstime_ms` is in the same epoch as `nros_platform_clock_ms`
     * (monotonic). pthread_cond_timedwait uses CLOCK_REALTIME by
     * default; we convert the monotonic deadline into a relative
     * delay and re-anchor against REALTIME. */
    uint64_t now_mono_ms = nros_platform_clock_ms();
    uint64_t rel_ms = abstime_ms > now_mono_ms ? abstime_ms - now_mono_ms : 0;

    struct timespec realtime;
    if (clock_gettime(CLOCK_REALTIME, &realtime) != 0) {
        return -1;
    }
    realtime.tv_sec  += (time_t) (rel_ms / 1000);
    realtime.tv_nsec += (long)   ((rel_ms % 1000) * 1000000);
    if (realtime.tv_nsec >= 1000000000L) {
        realtime.tv_sec  += 1;
        realtime.tv_nsec -= 1000000000L;
    }
    int r = pthread_cond_timedwait((pthread_cond_t *) cv,
                                   (pthread_mutex_t *) m,
                                   &realtime);
    if (r == 0)         return 0;
    if (r == ETIMEDOUT) return 1;
    return -1;
}

/* ============================================================
 *   Critical section (Phase 121.9)
 * ============================================================ */
/* Process-wide recursive mutex. Lazy-initialised on first use via
 * pthread_once. Token is unused (returns 0) because the recursive
 * mutex already tracks nesting. */
static pthread_mutex_t s_cs_mutex;
static pthread_once_t  s_cs_once = PTHREAD_ONCE_INIT;

static void cs_init(void) {
    pthread_mutexattr_t attr;
    pthread_mutexattr_init(&attr);
    pthread_mutexattr_settype(&attr, PTHREAD_MUTEX_RECURSIVE);
    pthread_mutex_init(&s_cs_mutex, &attr);
    pthread_mutexattr_destroy(&attr);
}

uint32_t nros_platform_critical_section_acquire(void) {
    pthread_once(&s_cs_once, cs_init);
    pthread_mutex_lock(&s_cs_mutex);
    return 0;
}

void nros_platform_critical_section_release(uint32_t token) {
    (void) token;
    pthread_mutex_unlock(&s_cs_mutex);
}

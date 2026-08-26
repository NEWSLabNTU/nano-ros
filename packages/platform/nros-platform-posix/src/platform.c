/*
 * Phase 121.3.posix — native C implementation of the canonical platform ABI.
 *
 * Header source of truth: `<nros/platform.h>` (`nros-platform-cffi`).
 *
 * Each `nros_platform_*` symbol below maps to the closest POSIX
 * primitive. The intent is parity with `PosixPlatform`'s Rust impl
 * (`packages/platform/nros-platform-posix/src/lib.rs`); the two share
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
#include <semaphore.h>
#include <stdint.h>
/* issue 0636 — `printf` for the priority read-back below. STDOUT, not stderr:
 * the NuttX guest's stderr does not reach the serial console, which is where
 * every other boot diagnostic this repo relies on lands. */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/time.h>
#include <time.h>
#include <unistd.h>

/* ---- Clock (monotonic) ---- */

/* RFC-0073 — the source is already nanoseconds, so this is the one port
 * that never had to truncate and now does not. */
uint64_t nros_platform_clock_ns(void) {
    struct timespec ts;
    if (clock_gettime(CLOCK_MONOTONIC, &ts) != 0) {
        return 0;
    }
    return (uint64_t) ts.tv_sec * 1000000000ULL + (uint64_t) ts.tv_nsec;
}

uint64_t nros_platform_clock_resolution_ns(void) {
    struct timespec res;
    if (clock_getres(CLOCK_MONOTONIC, &res) != 0) {
        return 1; /* unknown: claim the finest, never zero */
    }
    uint64_t ns = (uint64_t) res.tv_sec * 1000000000ULL + (uint64_t) res.tv_nsec;
    return ns == 0 ? 1 : ns;
}

/* issue 0758 — the one port where a wall clock is free. CLOCK_REALTIME is
 * the host's (or NuttX's) notion of absolute time; no acquisition step, no
 * SNTP, nothing to configure.
 *
 * NOTE this file is ALSO the NuttX port: `nros-platform-nuttx/CMakeLists.txt`
 * compiles `../nros-platform-posix/src/platform.c` verbatim rather than
 * carrying a copy. NuttX implements CLOCK_REALTIME, so both get a real epoch
 * from this one definition — and a change here is a change to two platforms.
 *
 * Returns 0 on failure, per the header's rule, which also covers a NuttX
 * build whose clock has never been set. */
uint64_t nros_platform_epoch_us(void) {
    struct timespec ts;
    if (clock_gettime(CLOCK_REALTIME, &ts) != 0) {
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

/* ---- Heap stats (phase-230 1b / RFC-0034 D7) ----
 * Host best-effort via glibc mallinfo2 (uordblks = in-use bytes). Not all
 * libcs provide it; return 0 ("unknown") elsewhere. POSIX is a Mode-A
 * platform (nano-ros owns the allocator: malloc), so this reflects the
 * process's nano-ros + zenoh-pico heap use. */
#if defined(__GLIBC__)
#include <malloc.h>
size_t nros_platform_heap_used_bytes(void) {
    struct mallinfo2 mi = mallinfo2();
    return (size_t) mi.uordblks;
}
size_t nros_platform_heap_total_bytes(void) {
    struct mallinfo2 mi = mallinfo2();
    return (size_t) (mi.arena + mi.hblkhd);
}
#else
size_t nros_platform_heap_used_bytes(void) { return 0u; }
size_t nros_platform_heap_total_bytes(void) { return 0u; }
#endif

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

uint64_t nros_platform_time_now_ns(void) {
    struct timespec ts;
    if (clock_gettime(CLOCK_REALTIME, &ts) != 0) {
        return 0;
    }
    /* Issue 0532 item 5 — ONE sample. This port used to issue a separate
     * `clock_gettime` per symbol, which is exactly what let the seconds and
     * the sub-second remainder come from different instants. */
    return (uint64_t) ts.tv_sec * 1000000000ULL + (uint64_t) ts.tv_nsec;
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

void nros_platform_task_attr_init(nros_platform_task_attr_t *attr) {
    if (attr == NULL) {
        return;
    }
    memset(attr, 0, sizeof(*attr));
    attr->priority = INT32_MIN; /* inherit */
    attr->core = -1;            /* unpinned */
}

/* issue 0636 — one spelling of "what native priority does this attribute ask
 * for", shared by the pthread_attr path (before create) and the running-thread
 * path (after). Returns < 0 for "no priority requested", which both callers
 * read as "leave it inherited".
 *
 * POSIX runs the same direction as the band (higher = more urgent) but only
 * under a real-time policy: under the default SCHED_OTHER the value is ignored,
 * and selecting SCHED_FIFO needs privilege the process may not have. */
static int nros_posix_native_priority(int32_t priority) {
    if (priority == NROS_PLATFORM_PRIORITY_INHERIT) {
        return -1;
    }
    if (NROS_PLATFORM_PRIORITY_IS_RAW(priority)) {
        return (int) NROS_PLATFORM_PRIORITY_RAW_VALUE(priority);
    }
    int lo = sched_get_priority_min(SCHED_FIFO);
    int hi = sched_get_priority_max(SCHED_FIFO);
    if (priority >= 0 && hi > lo) {
        int32_t band =
            priority > NROS_PLATFORM_PRIORITY_MAX ? NROS_PLATFORM_PRIORITY_MAX : priority;
        return lo + (int) (((int64_t) band * (hi - lo)) / NROS_PLATFORM_PRIORITY_MAX);
    }
    return -1;
}

int8_t nros_platform_task_init(void *task, void *attr,
                               void *(*entry)(void *), void *arg) {
    /* phase-364 W1 — INVALID: the caller passed a NULL where storage or an
     * entry point is required, which retrying cannot fix. */
    if (task == NULL || entry == NULL) {
        return NROS_PLATFORM_RET_INVALID;
    }

    /* phase-364 W3 — `attr` is read now rather than ignored. This port used to
     * do `(void) attr;`, which is why a caller could not ask for a stack size
     * and why phase-359 W7 wrote `nros_nuttx_spawn_tier` in C to get one: NuttX
     * runs this very port, so the shim existed to reach `pthread_attr_t` from a
     * caller the ABI gave no way to express it. */
    const nros_platform_task_attr_t *a = (const nros_platform_task_attr_t *) attr;

    pthread_attr_t pattr;
    pthread_attr_t *pattr_p = NULL;
    if (a != NULL && (a->stack_bytes > 0u || (a->flags & NROS_PLATFORM_TASK_DETACHED) != 0u)) {
        if (pthread_attr_init(&pattr) != 0) {
            return NROS_PLATFORM_RET_NOMEM;
        }
        pattr_p = &pattr;
        if (a->stack_bytes > 0u) {
            /* issue 0612 — `stack_bytes` is a FLOOR the caller needs, not an
             * exact size, so a request under this port's own minimum is raised
             * to it rather than refused.
             *
             * Refusing was a live bug, not a theoretical one: the signalfd
             * worker asks for 8192 ("the smallest stack any port will honour"),
             * glibc's PTHREAD_STACK_MIN on x86_64 is 16384, so
             * `pthread_attr_setstacksize` failed, `task_init` returned INVALID,
             * and `Executor::signal_fd()` returned `NotInitialized` on EVERY
             * Linux host. The capability was dead from the day it moved to a
             * platform task, and the one test that would have said so could not
             * run (issue 0612's other half).
             *
             * The floor is not a constant across POSIX either — it is 16384 on
             * glibc/x86_64 and 131072 on glibc/aarch64 — so no caller can pick a
             * portable number by hand. Only the port knows, which is why the
             * port is where the clamp belongs. Same "ask, do not assume"
             * reasoning as the storage-size probes (issue 0570). */
            size_t want = a->stack_bytes;
            if (want < (size_t) PTHREAD_STACK_MIN) {
                want = (size_t) PTHREAD_STACK_MIN;
            }
            if (pthread_attr_setstacksize(&pattr, want) != 0) {
                (void) pthread_attr_destroy(&pattr);
                return NROS_PLATFORM_RET_INVALID;
            }
        }
        if ((a->flags & NROS_PLATFORM_TASK_DETACHED) != 0u) {
            (void) pthread_attr_setdetachstate(&pattr, PTHREAD_CREATE_DETACHED);
        }
        /* issue 0636 — set the priority ON THE ATTRIBUTE, so the task is BORN
         * with it. The block after `pthread_create` below applied it to a
         * running thread instead, which leaves a window where the child holds
         * the SPAWNER's priority — and under SCHED_FIFO on a uniprocessor an
         * equal-priority peer never preempts, so a child that should have
         * outranked the owner ran only when the owner happened to block.
         *
         * That window is what `sched_dims_applied`'s NuttX cell kept losing:
         * measured 4 of 8 runs with the owner at the child's own priority (the
         * two are equal exactly when the attribute did not take), against 8 of
         * 8 once the attribute carries it.
         *
         * `PTHREAD_EXPLICIT_SCHED` is the half that makes the other two
         * settings mean anything: without it POSIX says the child INHERITS the
         * creator's policy and priority and the attribute is ignored, which is
         * a silent no-op rather than an error. */
        int want_native = nros_posix_native_priority(a->priority);
        if (want_native >= 0) {
            struct sched_param sp;
            memset(&sp, 0, sizeof(sp));
            sp.sched_priority = want_native;
            if (pthread_attr_setinheritsched(&pattr, PTHREAD_EXPLICIT_SCHED) == 0
                && pthread_attr_setschedpolicy(&pattr, SCHED_FIFO) == 0) {
                (void) pthread_attr_setschedparam(&pattr, &sp);
            }
        }
    }

    pthread_t *t = (pthread_t *) task;
    int rc = pthread_create(t, pattr_p, entry, arg);
    if (pattr_p != NULL) {
        (void) pthread_attr_destroy(pattr_p);
    }
    if (rc != 0) {
        /* phase-364 W1 — NOMEM, not ERROR. `pthread_create` fails with EAGAIN
         * when the system is out of thread resources, and issue 0246 is exactly
         * that: a TRANSIENT failure on NuttX under load, which the tier spawn
         * retries. A caller that reads this as permanent turns a momentary
         * shortage into a dead feature. */
        return NROS_PLATFORM_RET_NOMEM;
    }

    /* phase-364 W5 — priority.
     *
     * POSIX runs the same direction as the band (higher = more urgent) but only
     * under a real-time policy: under the default SCHED_OTHER the value is
     * ignored, and setting SCHED_FIFO needs privilege the process may not have.
     * So this is applied AFTER create, best-effort, and a refusal is not a
     * spawn failure — the task runs at the inherited priority, which is what it
     * did before this field existed.
     *
     * NuttX runs this port, and phase-296/issue 0263 already established that a
     * tier self-applies its priority at entry through
     * `nros_nuttx_apply_current_priority`; this covers the create-time case the
     * ABI can express portably. */
    if (a != NULL) {
        int native = nros_posix_native_priority(a->priority);
        if (native >= 0) {
            /* Belt and braces: the attribute above is what makes the task BORN
             * with this priority (issue 0636). This repeats it on the running
             * thread for the case the attribute did not take — a kernel that
             * declines `PTHREAD_EXPLICIT_SCHED`, or a host without the
             * privilege to select SCHED_FIFO. Still best-effort: a refusal is
             * not a spawn failure, because the task runs at the inherited
             * priority, which is what it did before this field existed. */
            struct sched_param sp;
            memset(&sp, 0, sizeof(sp));
            sp.sched_priority = native;
            int prio_rc = pthread_setschedparam(*t, SCHED_FIFO, &sp);
            /* issue 0803 — report what the task GOT, not what it asked for.
             *
             * Everything above reports the request. That is how the transport
             * band could announce "SCHED_FIFO 90" at boot while the kernel ran
             * those threads at 1 for four weeks: the attribute was read back
             * through the same lens that wrote it, so every check agreed with
             * itself and none of them asked the kernel. A mismatch here is
             * exactly the shape that hid — policy applied, value wrong — so it
             * is read back from the thread and reported when it differs. */
            if (prio_rc == 0) {
                int got_policy = -1;
                struct sched_param got;
                memset(&got, 0, sizeof(got));
                if (pthread_getschedparam(*t, &got_policy, &got) == 0
                    && (got_policy != SCHED_FIFO || got.sched_priority != native)) {
                    fprintf(stderr,
                            "[warn] nros: task `%s` asked for SCHED_FIFO %d and the kernel "
                            "gave it policy=%d priority=%d. A transport or tier that lands "
                            "BELOW what it declared inverts the ordering it exists to "
                            "state (issue 0803).\n",
                            (a->name != NULL) ? a->name : "?", native, got_policy,
                            got.sched_priority);
                }
            }
            /* RFC-0079 / issue 0765 — a REFUSAL is reported, once.
             *
             * This used to be `(void) pthread_setschedparam(...)`. On a Linux
             * host without `CAP_SYS_NICE` (or an `RLIMIT_RTPRIO` allowance)
             * every one of those calls returns EPERM, so a tier that declared
             * `priority = 80` ran at the default policy and NOTHING said so —
             * eleven such pins exist in the tree. Silently honouring a
             * scheduling declaration that was not applied is the same
             * silent-drop shape RFC-0052 makes fail-loud everywhere else.
             *
             * Once per process, not per task: an image with six tiers would
             * otherwise print six identical lines saying one thing about the
             * process. `pthread_setschedparam` is called under the spawn path,
             * which is already serialised by the caller, so a plain static is
             * enough here and avoids dragging <stdatomic.h> into a file that
             * compiles for several hosted targets.
             *
             * Not a spawn failure: the task runs at the inherited priority, and
             * turning a missing capability into a boot failure would make
             * every unprivileged `just ci` run stop dead. The point is that the
             * operator learns the declaration did not take. */
            if (prio_rc != 0) {
                static int reported;
                if (!reported) {
                    reported = 1;
                    if (prio_rc == EPERM) {
                        fprintf(stderr,
                                "[warn] nros: SCHED_FIFO priority %d was REFUSED (EPERM) — this "
                                "process may not request real-time scheduling, so every tier's "
                                "declared priority is INERT and the kernel runs them all at the "
                                "default policy.\n"
                                "       Grant the capability to the binary that runs the tiers:\n"
                                "           sudo setcap cap_sys_nice+ep <the executable>\n"
                                "       (re-run it after EVERY rebuild — a file capability is "
                                "bound to the file's CONTENTS and cannot survive a replaced "
                                "binary), or raise RLIMIT_RTPRIO for the user.\n",
                                native);
                    } else {
                        fprintf(stderr,
                                "[warn] nros: SCHED_FIFO priority %d was refused (rc=%d) — the "
                                "tier runs at its inherited priority.\n",
                                native, prio_rc);
                    }
                }
            }
            /* issue 0636 — a read-back diagnostic stood here and was REMOVED as
             * unverified. It reported `high` at prio=1 policy=1 on a run where
             * that task demonstrably ran at 110 (it self-applied, printed its
             * marker and set its sporadic budget), so either
             * `pthread_getschedparam` does not reflect the attribute on this
             * NuttX config or SCHED_FIFO's numeric value there is not what the
             * comparison assumed. A diagnostic that cries wolf costs more than
             * the silence it replaces; the evidence that mattered came from the
             * cell's own assertion naming which tier lost its marker. */
        }
    }

    /* Naming is best-effort: it reaches crash dumps and `ps`, and a kernel that
     * declines must not fail the spawn.
     *
     * Gated on `_GNU_SOURCE` rather than on the OS: `pthread_setname_np` is an
     * extension, and this file deliberately compiles under `_POSIX_C_SOURCE`
     * + `_DEFAULT_SOURCE`, which do not declare it. Testing the OS instead
     * would compile to an implicit declaration — which is an ERROR here, not a
     * warning — on exactly the platforms it was meant to help. */
#ifdef _GNU_SOURCE
    if (a != NULL && a->name != NULL) {
        (void) pthread_setname_np(*t, a->name);
    }
#else
    (void) 0; /* naming unavailable under this feature-test level */
#endif

    /* Reference the trampoline so the compiler doesn't strip it; a
     * future signature change (e.g. argument repacking) will route
     * through it. */
    (void) nros_posix_task_trampoline;
    return NROS_PLATFORM_RET_OK;
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
    uint64_t now_mono_ms = (nros_platform_clock_ns() / 1000000ULL);
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
 *   Wake primitive (Phase 130)
 *
 *   Binary semaphore backed by `sem_t`. macOS deprecates unnamed
 *   POSIX semaphores, so darwin falls back to a `pthread_cond_t`
 *   + flag pair; the surface is the same. ISR-safety is not
 *   meaningful on a hosted POSIX target — `signal_from_isr`
 *   aliases to `signal`.
 * ============================================================ */

#if defined(__APPLE__)
typedef struct {
    pthread_mutex_t mu;
    pthread_cond_t  cv;
    int             flag;  /* 0 = no signal pending, 1 = signaled */
} nros_wake_t;
#else
typedef struct {
    sem_t sem;
} nros_wake_t;
#endif

int8_t nros_platform_wake_init(void *w) {
    if (w == NULL) return -1;
    nros_wake_t *wp = (nros_wake_t *) w;
#if defined(__APPLE__)
    if (pthread_mutex_init(&wp->mu, NULL) != 0) return -1;
    pthread_condattr_t attr;
    if (pthread_condattr_init(&attr) != 0) {
        pthread_mutex_destroy(&wp->mu);
        return -1;
    }
    int rc = pthread_cond_init(&wp->cv, &attr);
    pthread_condattr_destroy(&attr);
    if (rc != 0) {
        pthread_mutex_destroy(&wp->mu);
        return -1;
    }
    wp->flag = 0;
    return 0;
#else
    return sem_init(&wp->sem, 0, 0) == 0 ? 0 : -1;
#endif
}

int8_t nros_platform_wake_drop(void *w) {
    if (w == NULL) return 0;
    nros_wake_t *wp = (nros_wake_t *) w;
#if defined(__APPLE__)
    pthread_cond_destroy(&wp->cv);
    pthread_mutex_destroy(&wp->mu);
    return 0;
#else
    return sem_destroy(&wp->sem) == 0 ? 0 : -1;
#endif
}

int8_t nros_platform_wake_wait_ms(void *w, uint32_t timeout_ms) {
    if (w == NULL) return -1;
    nros_wake_t *wp = (nros_wake_t *) w;
#if defined(__APPLE__)
    pthread_mutex_lock(&wp->mu);
    if (wp->flag == 0) {
        struct timespec ts;
        clock_gettime(CLOCK_REALTIME, &ts);
        uint64_t add_ns = (uint64_t) timeout_ms * 1000000ULL;
        ts.tv_sec  += (time_t) (add_ns / 1000000000ULL);
        ts.tv_nsec += (long)   (add_ns % 1000000000ULL);
        if (ts.tv_nsec >= 1000000000L) {
            ts.tv_sec  += 1;
            ts.tv_nsec -= 1000000000L;
        }
        int rc = pthread_cond_timedwait(&wp->cv, &wp->mu, &ts);
        if (rc == ETIMEDOUT && wp->flag == 0) {
            pthread_mutex_unlock(&wp->mu);
            return 1;
        }
        if (rc != 0 && rc != ETIMEDOUT) {
            pthread_mutex_unlock(&wp->mu);
            return -1;
        }
    }
    wp->flag = 0;
    pthread_mutex_unlock(&wp->mu);
    return 0;
#else
    struct timespec ts;
    if (clock_gettime(CLOCK_REALTIME, &ts) != 0) return -1;
    uint64_t add_ns = (uint64_t) timeout_ms * 1000000ULL;
    ts.tv_sec  += (time_t) (add_ns / 1000000000ULL);
    ts.tv_nsec += (long)   (add_ns % 1000000000ULL);
    if (ts.tv_nsec >= 1000000000L) {
        ts.tv_sec  += 1;
        ts.tv_nsec -= 1000000000L;
    }
    while (sem_timedwait(&wp->sem, &ts) != 0) {
        if (errno == ETIMEDOUT) return 1;
        if (errno == EINTR)     continue;
        return -1;
    }
    return 0;
#endif
}

int8_t nros_platform_wake_signal(void *w) {
    if (w == NULL) return -1;
    nros_wake_t *wp = (nros_wake_t *) w;
#if defined(__APPLE__)
    pthread_mutex_lock(&wp->mu);
    wp->flag = 1;
    pthread_cond_signal(&wp->cv);
    pthread_mutex_unlock(&wp->mu);
    return 0;
#else
    /* Coalesce signals: only post if not already pending so the
     * binary semaphore stays at value <= 1. EAGAIN means already
     * signaled (POSIX SEM_VALUE_MAX overflow not relevant here
     * because we never exceed 1 with the getvalue guard). */
    int val = 0;
    if (sem_getvalue(&wp->sem, &val) != 0) return -1;
    if (val > 0) return 0;
    return sem_post(&wp->sem) == 0 ? 0 : -1;
#endif
}

int8_t nros_platform_wake_signal_from_isr(void *w) {
    /* POSIX hosted: ISR semantics not meaningful. Alias to signal. */
    return nros_platform_wake_signal(w);
}

size_t nros_platform_wake_storage_size(void) {
    return sizeof(nros_wake_t);
}

size_t nros_platform_wake_storage_align(void) {
    return _Alignof(nros_wake_t);
}

/* phase-359 W10 — opaque-storage sizing for `task`, the sibling of the wake
 * probes above. `task_init`'s contract says the implementor decides the size;
 * these let a caller ASK instead of hard-coding it (issue 0570's trap). */
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

/* ============================================================
 *   Logging (Phase 88)
 *
 *   Render as `[<LEVEL>] <name>: <message>\n`. Body is pre-formatted
 *   by `nros-log`; we only prepend severity + name and append the
 *   newline. Mutex guards the sink so multi-thread writes land one line
 *   at a time. Not ISR-safe (POSIX has no ISR).
 * ============================================================ */
#include <stdio.h>
#ifdef __NuttX__
#include <syslog.h>
#endif

static const char *severity_label_log(uint8_t s) {
    switch (s) {
    case 0: return "TRACE";
    case 1: return "DEBUG";
    case 2: return "INFO";
    case 3: return "WARN";
    case 4: return "ERROR";
    case 5: return "FATAL";
    default: return "?";
    }
}

static pthread_mutex_t s_log_mutex = PTHREAD_MUTEX_INITIALIZER;

/* RFC-0079 / issue 0765 — adopt a tier's declared priority on the CALLING
 * thread, and SAY what happened.
 *
 * The sibling of `nros_nuttx_apply_current_priority` and
 * `freertos_apply_tier_priority`: one implementation per port, one marker, and
 * a refusal that is reported rather than absorbed. It exists because the Linux
 * board spawns its tiers with `std::thread::scope`, which never reaches this
 * file's `task_init` — so the attribute path that gives NuttX its priorities
 * cannot reach a native tier, and eleven `[tiers.*.posix] priority` pins in the
 * tree were inert with nothing saying so.
 *
 * `priority` is the RAW SCHED_FIFO value, the vocabulary issue 0623 settled on
 * after a normalised scale silently inverted a tier against the transport band.
 * 0 means "undeclared" — keep what was inherited, as on NuttX.
 *
 * Returns 1 when applied, 0 otherwise. Never fatal: an unprivileged host is the
 * common case for `just ci`, and refusing to boot there would trade a silent
 * degradation for a stopped test suite. The operator gets a line instead. */
int nros_posix_apply_current_priority(const char *name, uint32_t priority);
int nros_posix_apply_current_priority(const char *name, uint32_t priority) {
    const char *tname = (name != NULL) ? name : "?";
    if (priority == 0u) {
        return 0;
    }
    int lo = sched_get_priority_min(SCHED_FIFO);
    int hi = sched_get_priority_max(SCHED_FIFO);
    int want = (int) priority;
    if (want < lo) {
        want = lo;
    }
    if (want > hi) {
        want = hi;
    }
    struct sched_param sp;
    memset(&sp, 0, sizeof(sp));
    sp.sched_priority = want;
    int rc = pthread_setschedparam(pthread_self(), SCHED_FIFO, &sp);
    if (rc == 0) {
        printf("nros: tier priority set tier=`%s` prio=%d\n", tname, want);
        /* A tier's spin loop never returns, and stdout to a PIPE is
         * block-buffered — so without this the marker sits in a buffer that is
         * flushed at exit, i.e. never, and every piped reader (every e2e
         * harness) sees nothing. Measured: invisible under a plain pipe,
         * visible under `stdbuf -o0`. A diagnostic nobody can read is not a
         * diagnostic, which is the whole failure this line exists to fix. */
        fflush(stdout);
        return 1;
    }
    if (rc == EPERM) {
        static int reported;
        if (!reported) {
            reported = 1;
            fprintf(stderr,
                    "[warn] nros: SCHED_FIFO is REFUSED for this process (EPERM), so every "
                    "tier's declared priority is INERT and the kernel runs them all at the "
                    "default policy. Tier ordering is then whatever SCHED_OTHER decides.\n"
                    "       Grant the capability to the binary that runs the tiers:\n"
                    "           sudo setcap cap_sys_nice+ep <executable>\n"
                    "       Re-run it after EVERY rebuild: a file capability is bound to the "
                    "file's CONTENTS, so replacing the binary drops it. Or raise "
                    "RLIMIT_RTPRIO for the user.\n");
        }
    }
    printf("nros: tier priority FAILED tier=`%s` prio=%d rc=%d — tier runs at inherited "
           "priority\n",
           tname, want, rc);
    fflush(stdout); /* see the note above */
    return 0;
}

void nros_platform_log_write(uint8_t severity,
                             const uint8_t *name_ptr, uintptr_t name_len,
                             const uint8_t *msg_ptr,  uintptr_t msg_len) {
    if (msg_ptr == NULL && msg_len > 0) {
        return;
    }
    const char *label = severity_label_log(severity);
    pthread_mutex_lock(&s_log_mutex);
#ifdef __NuttX__
    if (name_ptr != NULL && name_len > 0) {
        syslog(LOG_INFO, "[%s] %.*s: %.*s",
               label,
               (int) name_len, (const char *) name_ptr,
               (int) msg_len,  (const char *) msg_ptr);
    } else {
        syslog(LOG_INFO, "[%s] %.*s",
               label,
               (int) msg_len, (const char *) msg_ptr);
    }
#else
    if (name_ptr != NULL && name_len > 0) {
        fprintf(stderr, "[%s] %.*s: %.*s\n",
                label,
                (int) name_len, (const char *) name_ptr,
                (int) msg_len,  (const char *) msg_ptr);
    } else {
        fprintf(stderr, "[%s] %.*s\n",
                label,
                (int) msg_len, (const char *) msg_ptr);
    }
#endif
    pthread_mutex_unlock(&s_log_mutex);
}

void nros_platform_log_flush(void) {
#ifndef __NuttX__
    fflush(stderr);
#endif
}

/* ---- Fatal error (phase-366 / RFC-0077) ----
 *
 * Hosted, so the honest ending is the libc one: say what happened on stderr,
 * then `abort()` — which raises SIGABRT, so a debugger stops here and a shell
 * sees the signal rather than a plain non-zero status.
 *
 * `__attribute__((weak))` so a C/C++ image can define this symbol strongly and
 * take the decision back; that is the whole point of the API. A Rust image
 * declares its own `#[panic_handler]` in the entry package instead.
 *
 * `fwrite` rather than the log ABI: this must work from any context, including
 * one where the log mutex is already held by the thread that is dying.
 */
__attribute__((weak))
_Noreturn void nros_platform_panic(const char *msg, size_t len) {
    fputs("nros: PANIC ", stderr);
    if (msg != NULL && len > 0) {
        (void) fwrite(msg, 1, len, stderr);
    }
    fputc('\n', stderr);
    fflush(stderr);
    abort();
}

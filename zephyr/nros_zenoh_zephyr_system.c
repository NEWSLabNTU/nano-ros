/*
 * Zephyr-specific zenoh-pico system ABI.
 *
 * The Rust zpico-platform-shim uses compact integer clock/thread placeholders
 * for several RTOS ports. Zephyr's zenoh-pico headers expose POSIX-shaped
 * types instead: pthread_t, pthread_mutex_t, pthread_cond_t, and struct
 * timespec. These symbols must therefore be compiled in C with those exact
 * signatures.
 */

#include <zenoh-pico/config.h>
#include <zenoh-pico/system/common/system_error.h>
#include <zenoh-pico/system/platform.h>

#include <nros/platform.h>

#include <zephyr/kernel.h>
#include <zephyr/posix/pthread.h>

#include <errno.h>
#include <stdint.h>
#include <stdio.h>
#include <sys/time.h>
#include <time.h>

#if Z_FEATURE_MULTI_THREAD == 1

int nros_zephyr_task_create(pthread_t *thread,
                            void *(*entry)(void *),
                            void *arg);

/* issue 0852 — `attr` is READ, not discarded.
 *
 * The pointer is an `nros_platform_task_attr_t *`, not a `pthread_attr_t *`,
 * despite the `z_task_attr_t *` in the signature. That is the contract
 * `zpico_set_task_config` states and issue 0803 already relied on:
 *
 *     g_default_read_task_opts.task_attributes =
 *         (z_task_attr_t *) &g_default_read_nros_attr;
 *
 * zenoh-pico's core never dereferences the pointer -- `_zp_start_read_task`
 * forwards it here untouched -- so the port at the other end decides the type,
 * and on this platform that type is the nros attr.
 *
 * This function used to be `(void)attr;`. Everything above it worked: the
 * Kconfig knob, the CMake wiring, the RAW encoding, issue 0626's fix to
 * `zpico_set_task_config`. The priority reached this line and stopped, so the
 * zenoh READ task was born inheriting the executor's priority and the polled
 * serial RX lost a full timeslice to it on every `k_yield()`. */
z_result_t _z_task_init(_z_task_t *task,
                        z_task_attr_t *attr,
                        void *(*fun)(void *),
                        void *arg) {
    /* NULL is the documented "every default" case, and it must stay
     * indistinguishable from the pre-issue behaviour. */
    if (attr == NULL) {
        return nros_zephyr_task_create(task, fun, arg) == 0 ? 0 : -1;
    }
    return nros_platform_task_init((void *)task, (void *)attr, fun, arg)
                   == NROS_PLATFORM_RET_OK
               ? 0
               : -1;
}

extern void nros_zephyr_task_slot_release(pthread_t owner);

z_result_t _z_task_join(_z_task_t *task) {
    pthread_t owner = *task;
    if (pthread_join(owner, NULL) != 0) {
        return -1;
    }
    /* The join RETURNED, so the thread is gone and its stack slot can be
     * handed out again. This is the only safe release point -- `_z_task_detach`
     * deliberately does NOT release, because a detached thread may still be
     * running (issue 0839; same rule as issue 0822 upstream). */
    nros_zephyr_task_slot_release(owner);
    return 0;
}

/* Task identity -- new in zenoh-pico 1.10. The background executor compares the
 * calling thread against the executor's own so it can tell a re-entrant call
 * (a callback asking the executor to do something) from an outside one, and
 * refuse to self-join. `_z_task_id_t` is `pthread_t` on Zephyr, so this is the
 * same identity the rest of this shim already keys task slots on. */
_z_task_id_t _z_task_get_id(const _z_task_t *task) { return *task; }

_z_task_id_t _z_task_current_id(void) { return pthread_self(); }

bool _z_task_id_equal(const _z_task_id_t *l, const _z_task_id_t *r) { return pthread_equal(*l, *r) != 0; }

z_result_t _z_task_detach(_z_task_t *task) {
    return pthread_detach(*task) == 0 ? 0 : -1;
}

z_result_t _z_task_cancel(_z_task_t *task) {
    return pthread_cancel(*task) == 0 ? 0 : -1;
}

void _z_task_exit(void) {
    pthread_exit(NULL);
}

/* issue 0882 — FREE WITH THE ALLOCATOR THAT ALLOCATED, and never free the
 * storage this task is RUNNING ON.
 *
 * The handle comes from zenoh-pico:
 *
 *     _zp_start_read_task():  _z_task_t *task = z_malloc(sizeof(_z_task_t));
 *
 * and `z_malloc` is `nros_platform_alloc`, i.e. the nano-ros TLSF arena. This
 * function used `k_free`, which is Zephyr's `_system_heap` -- a DIFFERENT
 * allocator. `k_free` reads a `struct k_heap *` from the words preceding the
 * block, so on a TLSF block it reads that block's metadata as a heap pointer
 * and then takes a spinlock inside it. Caught under gdb:
 *
 *     k_heap_free (heap=0x2040f4f0 <HEAP+656>, mem=0x2040f504 <HEAP+676>)
 *     k_free                       mempool.c:70
 *     _z_task_free                 nros_zenoh_zephyr_system.c
 *     _zp_unicast_failed           lease.c:63
 *
 * which asserts "Invalid spinlock 0x2040f504" -- an address inside our own
 * arena, because that is exactly where the bogus heap pointer pointed.
 *
 * The mismatch was invisible for as long as the transport never failed: this
 * is the only path that frees a task handle.
 *
 * Second, unrelated hazard on the same function, kept:
 *
 * zenoh-pico's `_zp_unicast_failed` executes on the lease task and calls
 * `_z_unicast_transport_clear(ztu, true)`, which detaches and frees
 * `_lease_task` -- the caller's own handle. It then calls `_z_reopen`, which
 * starts a NEW lease task, and `z_malloc` hands back the block that was just
 * freed. Two live threads then share one `pthread_t` slot.
 *
 * That is fatal here rather than merely untidy, because
 * `nros_zephyr_task_slot_release` keys the stack-slot table on `pthread_t`
 * (issues 0822, 0839): a second thread holding the same handle releases the
 * first thread's stack while it is still running on it, and the next task to
 * claim that slot writes over a live stack. The observed symptom is an
 * assertion on a `k_spinlock` that landed in the reused memory:
 *
 *     ASSERTION FAIL [z_spin_lock_valid(l)]  Invalid spinlock
 *     lr -> z_spinlock_validate_post / _z_task_free / _zp_unicast_failed
 *
 * Fixing this upstream means not freeing the caller's own task in
 * `_z_common_transport_clear`, which is a change to shared code on a path
 * every platform takes. Refusing the self-free HERE is the same guarantee at
 * the only layer that can identify the caller, and it cannot regress a port
 * that never had the problem.
 *
 * The handle is deferred, not leaked: it is released on the next free that
 * comes from a different thread, so at most one stale handle is outstanding.
 * It cannot be freed here at any later point in this function either -- the
 * thread is about to call `pthread_exit`, and pthread internals still need it.
 */
static void *nros_deferred_task_handle;

void _z_task_free(_z_task_t **task) {
    if (task == NULL || *task == NULL) {
        return;
    }

    if (pthread_equal(*(*task), pthread_self()) != 0) {
        /* Self-free. Hand the previous deferral to the allocator -- that thread
         * is long gone -- and hold this one in its place. */
        void *stale = nros_deferred_task_handle;
        nros_deferred_task_handle = (void *) *task;
        *task = NULL;
        if (stale != NULL) {
            z_free(stale);
        }
        return;
    }

    z_free(*task);
    *task = NULL;
}

z_result_t _z_mutex_init(_z_mutex_t *m) {
    return pthread_mutex_init(m, NULL) == 0 ? 0 : -1;
}

z_result_t _z_mutex_drop(_z_mutex_t *m) {
    return m == NULL || pthread_mutex_destroy(m) == 0 ? 0 : -1;
}

z_result_t _z_mutex_lock(_z_mutex_t *m) {
    return pthread_mutex_lock(m) == 0 ? 0 : -1;
}

z_result_t _z_mutex_try_lock(_z_mutex_t *m) {
    return pthread_mutex_trylock(m) == 0 ? 0 : -1;
}

z_result_t _z_mutex_unlock(_z_mutex_t *m) {
    return pthread_mutex_unlock(m) == 0 ? 0 : -1;
}

z_result_t _z_mutex_rec_init(_z_mutex_rec_t *m) {
    pthread_mutexattr_t attr;
    if (pthread_mutexattr_init(&attr) != 0) return -1;
    if (pthread_mutexattr_settype(&attr, PTHREAD_MUTEX_RECURSIVE) != 0) {
        (void)pthread_mutexattr_destroy(&attr);
        return -1;
    }
    int rc = pthread_mutex_init(m, &attr);
    (void)pthread_mutexattr_destroy(&attr);
    return rc == 0 ? 0 : -1;
}

z_result_t _z_mutex_rec_drop(_z_mutex_rec_t *m) {
    return m == NULL || pthread_mutex_destroy(m) == 0 ? 0 : -1;
}

z_result_t _z_mutex_rec_lock(_z_mutex_rec_t *m) {
    return pthread_mutex_lock(m) == 0 ? 0 : -1;
}

z_result_t _z_mutex_rec_try_lock(_z_mutex_rec_t *m) {
    return pthread_mutex_trylock(m) == 0 ? 0 : -1;
}

z_result_t _z_mutex_rec_unlock(_z_mutex_rec_t *m) {
    return pthread_mutex_unlock(m) == 0 ? 0 : -1;
}

z_result_t _z_condvar_init(_z_condvar_t *cv) {
    /* MUST be zeroed. Zephyr's `pthread_condattr_init` refuses an attribute
     * that already looks initialised:
     *
     *     if (attr->initialized) { return EINVAL; }
     *
     * `initialized` is a bit inside the caller's object, so on an
     * uninitialised stack variable this is a read of whatever the previous
     * frame left there. When that garbage happened to have the bit set, every
     * condvar init failed, `_z_session_init` returned this platform's -1, and
     * the session never opened:
     *
     *     ERROR ::_z_session_rc_init] _z_open failed: -1   (x10, then give up)
     *
     * Which way the bit fell depended on what had run before, so the failure
     * tracked code layout: adding an unrelated `printk` elsewhere in this file
     * was enough to make a dead board boot cleanly. zenoh-pico 1.10 is what
     * made it bite -- its executor changed what sits in that stack slot -- but
     * the bug was here all along. `pthread_mutexattr_init` has no equivalent
     * check, which is why the recursive-mutex path never showed it. */
    pthread_condattr_t attr = {0};
    if (pthread_condattr_init(&attr) != 0) return -1;
    (void)pthread_condattr_setclock(&attr, CLOCK_MONOTONIC);
    int rc = pthread_cond_init(cv, &attr);
    (void)pthread_condattr_destroy(&attr);
    return rc == 0 ? 0 : -1;
}

z_result_t _z_condvar_drop(_z_condvar_t *cv) {
    return pthread_cond_destroy(cv) == 0 ? 0 : -1;
}

z_result_t _z_condvar_signal(_z_condvar_t *cv) {
    return pthread_cond_signal(cv) == 0 ? 0 : -1;
}

z_result_t _z_condvar_signal_all(_z_condvar_t *cv) {
    return pthread_cond_broadcast(cv) == 0 ? 0 : -1;
}

z_result_t _z_condvar_wait(_z_condvar_t *cv, _z_mutex_t *m) {
    return pthread_cond_wait(cv, m) == 0 ? 0 : -1;
}

z_result_t _z_condvar_wait_until(_z_condvar_t *cv,
                                 _z_mutex_t *m,
                                 const z_clock_t *abstime) {
    int rc = pthread_cond_timedwait(cv, m, abstime);
    if (rc == ETIMEDOUT) return Z_ETIMEDOUT;
    return rc == 0 ? 0 : -1;
}

#endif /* Z_FEATURE_MULTI_THREAD == 1 */

z_clock_t z_clock_now(void) {
    z_clock_t now;
    clock_gettime(CLOCK_MONOTONIC, &now);
    return now;
}

/* Nanoseconds must be accumulated in 64 bits.
 *
 * `unsigned long` is 32 bits on this target, so a nanosecond count wraps at
 * 2^32 ns = 4.295 SECONDS. Every elapsed-time helper below is built on this
 * one, so past that point they all returned garbage -- and because they wrap
 * rather than saturate, the garbage looks like a small, plausible duration.
 *
 * What that broke: zenoh-pico's executor schedules a future's wake-up as
 * "milliseconds since the executor epoch" and compares deadlines with these
 * helpers. Once the epoch was more than ~4.29 s old the comparison inverted,
 * the lease future fired early, and the lease task saw a peer that had not
 * received anything since the last wake-up and closed the session:
 *
 *     [4.124000 INFO ::_zp_unicast_lease_task_fn] Closing session because it
 *                                                 has expired after 10000ms
 *
 * -- a 10 s lease expiring at 4.1 s, on a link that was working. The board then
 * reconnected and died again on the same clock, forever.
 *
 * The return type is fixed by zenoh-pico's platform ABI and stays `unsigned
 * long`, so the caller-visible ranges are what they are (~71 min for the us
 * variants, ~49 days for ms). The point is that the INTERMEDIATE must not be
 * the thing that overflows. */
static uint64_t elapsed_ns(const z_clock_t *start, const z_clock_t *now) {
    time_t sec = now->tv_sec - start->tv_sec;
    long nsec = now->tv_nsec - start->tv_nsec;
    if (nsec < 0) {
        sec -= 1;
        nsec += 1000000000L;
    }
    if (sec < 0) return 0;
    return (uint64_t)sec * 1000000000ULL + (uint64_t)nsec;
}

unsigned long z_clock_elapsed_us(z_clock_t *instant) {
    z_clock_t now = z_clock_now();
    return (unsigned long)(elapsed_ns(instant, &now) / 1000ULL);
}

unsigned long z_clock_elapsed_ms(z_clock_t *instant) {
    z_clock_t now = z_clock_now();
    return (unsigned long)(elapsed_ns(instant, &now) / 1000000ULL);
}

unsigned long z_clock_elapsed_s(z_clock_t *instant) {
    z_clock_t now = z_clock_now();
    return (unsigned long)(elapsed_ns(instant, &now) / 1000000000ULL);
}

/* zenoh-pico 1.10 added the `*_since` family: the same elapsed calculation, but
 * between two captured instants rather than against "now". The background
 * executor uses it to age deadlines it captured earlier, so it must not re-read
 * the clock. Clamped at zero like the `elapsed_ns` helper above -- a negative
 * interval means the caller passed the instants the wrong way round, and the
 * callers treat the result as an unsigned duration. */
unsigned long zp_clock_elapsed_us_since(z_clock_t *instant, z_clock_t *epoch) {
    return (unsigned long)(elapsed_ns(epoch, instant) / 1000ULL);
}

unsigned long zp_clock_elapsed_ms_since(z_clock_t *instant, z_clock_t *epoch) {
    return (unsigned long)(elapsed_ns(epoch, instant) / 1000000ULL);
}

unsigned long zp_clock_elapsed_s_since(z_clock_t *instant, z_clock_t *epoch) {
    return (unsigned long)(elapsed_ns(epoch, instant) / 1000000000ULL);
}

void z_clock_advance_us(z_clock_t *clock, unsigned long duration) {
    clock->tv_sec += (time_t)(duration / 1000000UL);
    clock->tv_nsec += (long)((duration % 1000000UL) * 1000UL);
    if (clock->tv_nsec >= 1000000000L) {
        clock->tv_sec += 1;
        clock->tv_nsec -= 1000000000L;
    }
}

void z_clock_advance_ms(z_clock_t *clock, unsigned long duration) {
    clock->tv_sec += (time_t)(duration / 1000UL);
    clock->tv_nsec += (long)((duration % 1000UL) * 1000000UL);
    if (clock->tv_nsec >= 1000000000L) {
        clock->tv_sec += 1;
        clock->tv_nsec -= 1000000000L;
    }
}

void z_clock_advance_s(z_clock_t *clock, unsigned long duration) {
    clock->tv_sec += (time_t)duration;
}

z_time_t z_time_now(void) {
    z_time_t now;
    if (gettimeofday(&now, NULL) != 0) {
        now.tv_sec = 0;
        now.tv_usec = 0;
    }
    return now;
}

const char *z_time_now_as_str(char *const buf, unsigned long buflen) {
    z_time_t tv = z_time_now();
    snprintf(buf, buflen, "%ld.%06ld", (long)tv.tv_sec, (long)tv.tv_usec);
    return buf;
}

unsigned long z_time_elapsed_us(z_time_t *time) {
    z_time_t now = z_time_now();
    return (unsigned long)((now.tv_sec - time->tv_sec) * 1000000L
                           + (now.tv_usec - time->tv_usec));
}

unsigned long z_time_elapsed_ms(z_time_t *time) {
    return z_time_elapsed_us(time) / 1000UL;
}

unsigned long z_time_elapsed_s(z_time_t *time) {
    return z_time_elapsed_us(time) / 1000000UL;
}

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

z_result_t _z_task_detach(_z_task_t *task) {
    return pthread_detach(*task) == 0 ? 0 : -1;
}

z_result_t _z_task_cancel(_z_task_t *task) {
    return pthread_cancel(*task) == 0 ? 0 : -1;
}

void _z_task_exit(void) {
    pthread_exit(NULL);
}

void _z_task_free(_z_task_t **task) {
    if (task != NULL && *task != NULL) {
        k_free(*task);
        *task = NULL;
    }
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
    pthread_condattr_t attr;
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

static unsigned long elapsed_ns(const z_clock_t *start, const z_clock_t *now) {
    time_t sec = now->tv_sec - start->tv_sec;
    long nsec = now->tv_nsec - start->tv_nsec;
    if (nsec < 0) {
        sec -= 1;
        nsec += 1000000000L;
    }
    if (sec < 0) return 0;
    return (unsigned long)sec * 1000000000UL + (unsigned long)nsec;
}

unsigned long z_clock_elapsed_us(z_clock_t *instant) {
    z_clock_t now = z_clock_now();
    return elapsed_ns(instant, &now) / 1000UL;
}

unsigned long z_clock_elapsed_ms(z_clock_t *instant) {
    z_clock_t now = z_clock_now();
    return elapsed_ns(instant, &now) / 1000000UL;
}

unsigned long z_clock_elapsed_s(z_clock_t *instant) {
    z_clock_t now = z_clock_now();
    return elapsed_ns(instant, &now) / 1000000000UL;
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

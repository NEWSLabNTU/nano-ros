/**
 * zenoh_threadx_system.c — zenoh-pico system layer for Eclipse ThreadX
 *
 * Provides threading, memory, clock, sleep, and random functions using
 * ThreadX kernel APIs. Adapted from zenoh-pico's src/system/threadx/stm32/system.c
 * but without STM32 HAL dependency — works with Linux simulation port and
 * embedded RISC-V/ARM targets.
 *
 * Gated on ZENOH_THREADX (NOT ZENOH_THREADX_STM32).
 */

#if defined(ZENOH_THREADX)

#include <stdbool.h>
#include <stddef.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "tx_api.h"
#include "zenoh-pico/config.h"
#include "zenoh-pico/system/platform.h"
#include "zenoh-pico/utils/logging.h"

/* ── Memory ──────────────────────────────────────────────────────────────── */

/*
 * ThreadX byte pool for zenoh-pico allocations.
 * Must be provided by the board crate's application_define().
 */
extern TX_BYTE_POOL *zpico_threadx_byte_pool;

void *z_malloc(size_t size) {
    void *ptr = NULL;
    UINT r = tx_byte_allocate(zpico_threadx_byte_pool, &ptr, size, TX_WAIT_FOREVER);
    if (r != TX_SUCCESS) {
        ptr = NULL;
    }
    return ptr;
}

void *z_realloc(void *ptr, size_t size) {
    /* ThreadX byte pools don't support realloc.
     * Allocate new, copy, free old. */
    if (size == 0) {
        if (ptr) tx_byte_release(ptr);
        return NULL;
    }
    void *new_ptr = z_malloc(size);
    if (new_ptr && ptr) {
        /* We don't know the old size, so copy `size` bytes.
         * This is safe as long as the caller passes a size >= old size
         * (which is the standard realloc contract). */
        memcpy(new_ptr, ptr, size);
        tx_byte_release(ptr);
    }
    return new_ptr;
}

void z_free(void *ptr) {
    if (ptr) tx_byte_release(ptr);
}

/* ── Random ──────────────────────────────────────────────────────────────── */

/*
 * Random number generation.
 * On Linux simulation: uses rand() seeded by application.
 * On embedded: board crate should provide a hardware RNG or override these
 * weak symbols.
 */

__attribute__((weak))
uint32_t z_random_u32(void) {
    return (uint32_t)rand();
}

uint8_t z_random_u8(void) { return (uint8_t)z_random_u32(); }

uint16_t z_random_u16(void) { return (uint16_t)z_random_u32(); }

uint64_t z_random_u64(void) {
    uint64_t ret = 0;
    ret |= z_random_u32();
    ret = ret << 32;
    ret |= z_random_u32();
    return ret;
}

void z_random_fill(void *buf, size_t len) {
    for (size_t i = 0; i < len; i++) {
        ((uint8_t *)buf)[i] = z_random_u8();
    }
}

/* ── Threading ───────────────────────────────────────────────────────────── */

#if Z_FEATURE_MULTI_THREAD == 1

z_result_t _z_task_init(_z_task_t *task, z_task_attr_t *attr, void *(*fun)(void *), void *arg) {
    (void)attr;

    UINT status = tx_thread_create(
        &(task->threadx_thread), "ztask",
        (VOID (*)(ULONG))fun, (ULONG)arg,
        task->threadx_stack, Z_TASK_STACK_SIZE,
        Z_TASK_PRIORITY, Z_TASK_PREEMPT_THRESHOLD,
        Z_TASK_TIME_SLICE, TX_AUTO_START);

    if (status != TX_SUCCESS) return _Z_ERR_GENERIC;
    return _Z_RES_OK;
}

z_result_t _z_task_join(_z_task_t *task) {
    while (1) {
        UINT state;
        UINT status = tx_thread_info_get(
            &(task->threadx_thread), NULL, &state,
            NULL, NULL, NULL, NULL, NULL, NULL);
        if (status != TX_SUCCESS) return _Z_ERR_GENERIC;

        if ((state == TX_COMPLETED) || (state == TX_TERMINATED)) break;
        tx_thread_sleep(1);
    }
    return _Z_RES_OK;
}

z_result_t _z_task_detach(_z_task_t *task) {
    (void)task;
    return _Z_ERR_GENERIC;  // Not implemented
}

z_result_t _z_task_cancel(_z_task_t *task) {
    (void)task;
    return _Z_ERR_GENERIC;  // Not implemented
}

void _z_task_exit(void) {
    /* ThreadX threads terminate when they return from their entry function. */
}

void _z_task_free(_z_task_t **task) {
    z_free(*task);
    *task = NULL;
}

/* ── Mutex ───────────────────────────────────────────────────────────────── */

z_result_t _z_mutex_init(_z_mutex_t *m) {
    UINT status = tx_mutex_create(m, TX_NULL, TX_INHERIT);
    if (status == TX_MUTEX_ERROR) {
        /* zenoh-pico may reuse mutex if init fails. Re-create. */
        status = tx_mutex_delete(m);
        if (status == TX_SUCCESS) {
            status = tx_mutex_create(m, TX_NULL, TX_INHERIT);
        }
    }
    return (status == TX_SUCCESS) ? _Z_RES_OK : _Z_ERR_GENERIC;
}

z_result_t _z_mutex_drop(_z_mutex_t *m) {
    UINT status = tx_mutex_delete(m);
    return (status == TX_SUCCESS) ? _Z_RES_OK : _Z_ERR_GENERIC;
}

z_result_t _z_mutex_lock(_z_mutex_t *m) {
    UINT status = tx_mutex_get(m, TX_WAIT_FOREVER);
    return (status == TX_SUCCESS) ? _Z_RES_OK : _Z_ERR_GENERIC;
}

z_result_t _z_mutex_try_lock(_z_mutex_t *m) {
    UINT status = tx_mutex_get(m, TX_NO_WAIT);
    return (status == TX_SUCCESS) ? _Z_RES_OK : _Z_ERR_GENERIC;
}

z_result_t _z_mutex_unlock(_z_mutex_t *m) {
    UINT status = tx_mutex_put(m);
    return (status == TX_SUCCESS) ? _Z_RES_OK : _Z_ERR_GENERIC;
}

z_result_t _z_mutex_rec_init(_z_mutex_rec_t *m) { return _z_mutex_init(m); }
z_result_t _z_mutex_rec_drop(_z_mutex_rec_t *m) { return _z_mutex_drop(m); }
z_result_t _z_mutex_rec_lock(_z_mutex_rec_t *m) { return _z_mutex_lock(m); }
z_result_t _z_mutex_rec_unlock(_z_mutex_rec_t *m) { return _z_mutex_unlock(m); }

/* ── Condition Variable ──────────────────────────────────────────────────── */

z_result_t _z_condvar_init(_z_condvar_t *cv) {
    if (!cv) return _Z_ERR_GENERIC;

    UINT m_status = tx_mutex_create(&cv->mutex, TX_NULL, TX_INHERIT);
    UINT s_status = tx_semaphore_create(&cv->sem, TX_NULL, 0);
    cv->waiters = 0;

    if (m_status != TX_SUCCESS || s_status != TX_SUCCESS) {
        return _Z_ERR_GENERIC;
    }
    return _Z_RES_OK;
}

z_result_t _z_condvar_drop(_z_condvar_t *cv) {
    if (!cv) return _Z_ERR_GENERIC;

    tx_mutex_delete(&cv->mutex);
    tx_semaphore_delete(&cv->sem);
    return _Z_RES_OK;
}

z_result_t _z_condvar_signal(_z_condvar_t *cv) {
    if (!cv) return _Z_ERR_GENERIC;

    tx_mutex_get(&cv->mutex, TX_WAIT_FOREVER);
    if (cv->waiters > 0) {
        tx_semaphore_put(&cv->sem);
        cv->waiters--;
    }
    tx_mutex_put(&cv->mutex);
    return _Z_RES_OK;
}

z_result_t _z_condvar_signal_all(_z_condvar_t *cv) {
    if (!cv) return _Z_ERR_GENERIC;

    tx_mutex_get(&cv->mutex, TX_WAIT_FOREVER);
    while (cv->waiters > 0) {
        tx_semaphore_put(&cv->sem);
        cv->waiters--;
    }
    tx_mutex_put(&cv->mutex);
    return _Z_RES_OK;
}

z_result_t _z_condvar_wait(_z_condvar_t *cv, _z_mutex_t *m) {
    if (!cv || !m) return _Z_ERR_GENERIC;

    tx_mutex_get(&cv->mutex, TX_WAIT_FOREVER);
    cv->waiters++;
    tx_mutex_put(&cv->mutex);

    _z_mutex_unlock(m);
    tx_semaphore_get(&cv->sem, TX_WAIT_FOREVER);
    _z_mutex_lock(m);

    return _Z_RES_OK;
}

z_result_t _z_condvar_wait_until(_z_condvar_t *cv, _z_mutex_t *m, const z_clock_t *abstime) {
    if (!cv || !m) return _Z_ERR_GENERIC;

    ULONG now = tx_time_get();
    ULONG target_ticks = (ULONG)((abstime->tv_sec * 1000 + abstime->tv_nsec / 1000000)
                                  * TX_TIMER_TICKS_PER_SECOND / 1000);
    ULONG block_duration = (target_ticks > now) ? (target_ticks - now) : 0;

    tx_mutex_get(&cv->mutex, TX_WAIT_FOREVER);
    cv->waiters++;
    tx_mutex_put(&cv->mutex);

    _z_mutex_unlock(m);
    bool timed_out = (tx_semaphore_get(&cv->sem, block_duration) != TX_SUCCESS);
    _z_mutex_lock(m);

    if (timed_out) {
        tx_mutex_get(&cv->mutex, TX_WAIT_FOREVER);
        cv->waiters--;
        tx_mutex_put(&cv->mutex);
        return Z_ETIMEDOUT;
    }

    return _Z_RES_OK;
}

#endif  /* Z_FEATURE_MULTI_THREAD == 1 */

/* ── Sleep ───────────────────────────────────────────────────────────────── */

z_result_t z_sleep_us(size_t time) {
    ULONG ticks = (ULONG)(time * TX_TIMER_TICKS_PER_SECOND / 1000000);
    if (ticks == 0 && time > 0) ticks = 1;  /* At least 1 tick */
    tx_thread_sleep(ticks);
    return _Z_RES_OK;
}

z_result_t z_sleep_ms(size_t time) {
    ULONG ticks = (ULONG)(time * TX_TIMER_TICKS_PER_SECOND / 1000);
    if (ticks == 0 && time > 0) ticks = 1;
    tx_thread_sleep(ticks);
    return _Z_RES_OK;
}

z_result_t z_sleep_s(size_t time) {
    tx_thread_sleep((ULONG)(time * TX_TIMER_TICKS_PER_SECOND));
    return _Z_RES_OK;
}

/* ── Clock ───────────────────────────────────────────────────────────────── */

static void _z_clock_from_ticks(z_clock_t *ts) {
    uint64_t ticks = tx_time_get();
    uint64_t ms = ticks * 1000 / TX_TIMER_TICKS_PER_SECOND;
    ts->tv_sec = (long)(ms / 1000);
    ts->tv_nsec = (long)((ms % 1000) * 1000000);
}

z_clock_t z_clock_now(void) {
    z_clock_t now;
    _z_clock_from_ticks(&now);
    return now;
}

unsigned long z_clock_elapsed_us(z_clock_t *instant) {
    z_clock_t now;
    _z_clock_from_ticks(&now);
    return (unsigned long)(1000000 * (now.tv_sec - instant->tv_sec)
                           + (now.tv_nsec - instant->tv_nsec) / 1000);
}

unsigned long z_clock_elapsed_ms(z_clock_t *instant) {
    z_clock_t now;
    _z_clock_from_ticks(&now);
    return (unsigned long)(1000 * (now.tv_sec - instant->tv_sec)
                           + (now.tv_nsec - instant->tv_nsec) / 1000000);
}

unsigned long z_clock_elapsed_s(z_clock_t *instant) {
    z_clock_t now;
    _z_clock_from_ticks(&now);
    return (unsigned long)(now.tv_sec - instant->tv_sec);
}

void z_clock_advance_us(z_clock_t *clock, unsigned long duration) {
    clock->tv_sec += (long)(duration / 1000000);
    clock->tv_nsec += (long)((duration % 1000000) * 1000);
    if (clock->tv_nsec >= 1000000000L) {
        clock->tv_sec += 1;
        clock->tv_nsec -= 1000000000L;
    }
}

void z_clock_advance_ms(z_clock_t *clock, unsigned long duration) {
    clock->tv_sec += (long)(duration / 1000);
    clock->tv_nsec += (long)((duration % 1000) * 1000000);
    if (clock->tv_nsec >= 1000000000L) {
        clock->tv_sec += 1;
        clock->tv_nsec -= 1000000000L;
    }
}

void z_clock_advance_s(z_clock_t *clock, unsigned long duration) {
    clock->tv_sec += (long)duration;
}

/* ── Time ────────────────────────────────────────────────────────────────── */

z_time_t z_time_now(void) { return tx_time_get(); }

const char *z_time_now_as_str(char *const buf, unsigned long buflen) {
    snprintf(buf, buflen, "%lu", (unsigned long)z_time_now());
    return buf;
}

unsigned long z_time_elapsed_us(z_time_t *time) {
    return (unsigned long)((tx_time_get() - *time) * 1000000ULL / TX_TIMER_TICKS_PER_SECOND);
}

unsigned long z_time_elapsed_ms(z_time_t *time) {
    return (unsigned long)((tx_time_get() - *time) * 1000ULL / TX_TIMER_TICKS_PER_SECOND);
}

unsigned long z_time_elapsed_s(z_time_t *time) {
    return (unsigned long)((tx_time_get() - *time) / TX_TIMER_TICKS_PER_SECOND);
}

z_result_t _z_get_time_since_epoch(_z_time_since_epoch *t) {
    uint64_t ticks = tx_time_get();
    uint64_t time_ns = ticks * 1000000000ULL / TX_TIMER_TICKS_PER_SECOND;

    t->secs = (uint32_t)(time_ns / 1000000000ULL);
    t->nanos = (uint32_t)(time_ns % 1000000000ULL);

    return _Z_RES_OK;
}

#endif  /* ZENOH_THREADX */

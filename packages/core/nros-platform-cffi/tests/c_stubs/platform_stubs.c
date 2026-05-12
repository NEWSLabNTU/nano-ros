/*
 * Phase 121.4.a — C-stub harness defining every nros_platform_*
 * symbol declared in <nros/platform.h>.
 *
 * Each stub bumps a per-category counter (plus the TOTAL counter).
 * The Rust integration test invokes every symbol through the
 * `nros_platform_cffi` extern wrappers and verifies all categories
 * advanced as expected.
 *
 * The stub implementations themselves return zero / null sentinels
 * — semantically inert. We are exercising the ABI surface and the
 * macro / extern declarations, not any platform behaviour.
 */

#include "platform_stubs.h"

#include <stddef.h>
#include <stdint.h>

static uint32_t counters[NROS_STUB_CATEGORY_COUNT];

static void bump(nros_platform_stub_category_t category) {
    counters[NROS_STUB_TOTAL]++;
    counters[category]++;
}

uint32_t nros_platform_stub_counter(nros_platform_stub_category_t category) {
    if ((unsigned) category >= (unsigned) NROS_STUB_CATEGORY_COUNT) {
        return 0;
    }
    return counters[category];
}

void nros_platform_stub_reset_counters(void) {
    for (size_t i = 0; i < (size_t) NROS_STUB_CATEGORY_COUNT; ++i) {
        counters[i] = 0;
    }
}

/* ---- Clock ---- */

uint64_t nros_platform_clock_ms(void)                           { bump(NROS_STUB_CLOCK); return 0; }
uint64_t nros_platform_clock_us(void)                           { bump(NROS_STUB_CLOCK); return 0; }

/* ---- Alloc ---- */

void *nros_platform_alloc(size_t size)                          { (void) size; bump(NROS_STUB_ALLOC); return NULL; }
void *nros_platform_realloc(void *ptr, size_t size)             { (void) ptr; (void) size; bump(NROS_STUB_ALLOC); return NULL; }
void  nros_platform_dealloc(void *ptr)                          { (void) ptr; bump(NROS_STUB_ALLOC); }

/* ---- Sleep ---- */

void nros_platform_sleep_us(size_t us)                          { (void) us; bump(NROS_STUB_SLEEP); }
void nros_platform_sleep_ms(size_t ms)                          { (void) ms; bump(NROS_STUB_SLEEP); }
void nros_platform_sleep_s(size_t s)                            { (void) s; bump(NROS_STUB_SLEEP); }

/* ---- Yield ---- */

void nros_platform_yield_now(void)                              { bump(NROS_STUB_YIELD); }

/* ---- Random ---- */

uint8_t  nros_platform_random_u8(void)                          { bump(NROS_STUB_RANDOM); return 0; }
uint16_t nros_platform_random_u16(void)                         { bump(NROS_STUB_RANDOM); return 0; }
uint32_t nros_platform_random_u32(void)                         { bump(NROS_STUB_RANDOM); return 0; }
uint64_t nros_platform_random_u64(void)                         { bump(NROS_STUB_RANDOM); return 0; }
void     nros_platform_random_fill(void *buf, size_t len)       { (void) buf; (void) len; bump(NROS_STUB_RANDOM); }

/* ---- Time ---- */

uint64_t nros_platform_time_now_ms(void)                        { bump(NROS_STUB_TIME); return 0; }
uint32_t nros_platform_time_since_epoch_secs(void)              { bump(NROS_STUB_TIME); return 0; }
uint32_t nros_platform_time_since_epoch_nanos(void)             { bump(NROS_STUB_TIME); return 0; }

/* ---- Task ---- */

int8_t nros_platform_task_init(void *task, void *attr,
                               void *(*entry)(void *), void *arg) {
    (void) task; (void) attr; (void) entry; (void) arg;
    bump(NROS_STUB_TASK);
    return -1;
}
int8_t nros_platform_task_join(void *task)                      { (void) task; bump(NROS_STUB_TASK); return -1; }
int8_t nros_platform_task_detach(void *task)                    { (void) task; bump(NROS_STUB_TASK); return -1; }
int8_t nros_platform_task_cancel(void *task)                    { (void) task; bump(NROS_STUB_TASK); return -1; }
void   nros_platform_task_exit(void)                            { bump(NROS_STUB_TASK); }
void   nros_platform_task_free(void **task)                     { (void) task; bump(NROS_STUB_TASK); }

/* ---- Mutex (non-recursive + recursive share the same counter) ---- */

int8_t nros_platform_mutex_init(void *m)                        { (void) m; bump(NROS_STUB_MUTEX); return 0; }
int8_t nros_platform_mutex_drop(void *m)                        { (void) m; bump(NROS_STUB_MUTEX); return 0; }
int8_t nros_platform_mutex_lock(void *m)                        { (void) m; bump(NROS_STUB_MUTEX); return 0; }
int8_t nros_platform_mutex_try_lock(void *m)                    { (void) m; bump(NROS_STUB_MUTEX); return 0; }
int8_t nros_platform_mutex_unlock(void *m)                      { (void) m; bump(NROS_STUB_MUTEX); return 0; }
int8_t nros_platform_mutex_rec_init(void *m)                    { (void) m; bump(NROS_STUB_MUTEX); return 0; }
int8_t nros_platform_mutex_rec_drop(void *m)                    { (void) m; bump(NROS_STUB_MUTEX); return 0; }
int8_t nros_platform_mutex_rec_lock(void *m)                    { (void) m; bump(NROS_STUB_MUTEX); return 0; }
int8_t nros_platform_mutex_rec_try_lock(void *m)                { (void) m; bump(NROS_STUB_MUTEX); return 0; }
int8_t nros_platform_mutex_rec_unlock(void *m)                  { (void) m; bump(NROS_STUB_MUTEX); return 0; }

/* ---- Condvar ---- */

int8_t nros_platform_condvar_init(void *cv)                     { (void) cv; bump(NROS_STUB_CONDVAR); return 0; }
int8_t nros_platform_condvar_drop(void *cv)                     { (void) cv; bump(NROS_STUB_CONDVAR); return 0; }
int8_t nros_platform_condvar_signal(void *cv)                   { (void) cv; bump(NROS_STUB_CONDVAR); return 0; }
int8_t nros_platform_condvar_signal_all(void *cv)               { (void) cv; bump(NROS_STUB_CONDVAR); return 0; }
int8_t nros_platform_condvar_wait(void *cv, void *m)            { (void) cv; (void) m; bump(NROS_STUB_CONDVAR); return 0; }
int8_t nros_platform_condvar_wait_until(void *cv, void *m, uint64_t abstime) {
    (void) cv; (void) m; (void) abstime;
    bump(NROS_STUB_CONDVAR);
    return 0;
}

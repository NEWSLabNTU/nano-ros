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
 *                boot; us derived by k_cyc_to_us_floor64(k_cycle_get_64()).
 *   - Alloc    — k_malloc / k_realloc / k_free against the kernel heap.
 *   - Sleep    — k_msleep / k_usleep / k_sleep.
 *   - Yield    — k_yield().
 *   - Random   — sys_rand32_get() / sys_rand_get(). Default Zephyr
 *                build provides a PRNG; CONFIG_ENTROPY_GENERATOR
 *                upgrades to hardware entropy.
 *   - Time     — wall clock unsupported unless the user enables
 *                CONFIG_RTC; defaults return 0.
 *   - Tasks    — k_thread_create + k_thread_join + k_thread_abort.
 *                attr carries the stack pointer, stack size, priority.
 *   - Mutexes  — k_mutex is recursive by design; mutex_* and
 *                mutex_rec_* share the same primitive.
 *   - Condvars — k_condvar_init / signal / broadcast / wait /
 *                wait-with-timeout (Zephyr 2.5+).
 *
 * Build verification requires a Zephyr workspace; CMakeLists.txt
 * is designed to be consumed as a Zephyr module or as an external
 * project linked via the Zephyr interface library.
 */

#include <nros/platform.h>

#include <zephyr/kernel.h>
#include <zephyr/random/random.h>

#include <stddef.h>
#include <stdint.h>
#include <string.h>

/* ---- Clock ---- */

uint64_t nros_platform_clock_ms(void) {
    int64_t ms = k_uptime_get();
    return ms < 0 ? 0 : (uint64_t) ms;
}

uint64_t nros_platform_clock_us(void) {
    return (uint64_t) k_cyc_to_us_floor64(k_cycle_get_64());
}

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

uint64_t nros_platform_time_now_ms(void)              { return 0; }
uint32_t nros_platform_time_since_epoch_secs(void)    { return 0; }
uint32_t nros_platform_time_since_epoch_nanos(void)   { return 0; }

/* ---- Tasks ----
 *
 * Storage is `struct k_thread`. attr carries name, priority, and
 * the caller-allocated stack region. Zephyr requires
 * `K_THREAD_STACK_DEFINE(stack_name, size)` at file scope; we
 * receive a pointer to that storage.
 */

typedef struct {
    const char *name;
    int priority;
    size_t stack_depth;
    void  *stack_base;
} nros_zephyr_task_attr_t;

int8_t nros_platform_task_init(void *task, void *attr,
                               void *(*entry)(void *), void *arg) {
    if (task == NULL || attr == NULL || entry == NULL) return -1;
    const nros_zephyr_task_attr_t *a = (const nros_zephyr_task_attr_t *) attr;
    if (a->stack_base == NULL || a->stack_depth == 0) return -1;

    k_tid_t tid = k_thread_create(
        (struct k_thread *) task,
        (k_thread_stack_t *) a->stack_base,
        a->stack_depth,
        (k_thread_entry_t) entry,
        arg, NULL, NULL,
        a->priority,
        0,
        K_NO_WAIT);
    if (a->name != NULL) {
        (void) k_thread_name_set(tid, a->name);
    }
    return tid == NULL ? -1 : 0;
}

int8_t nros_platform_task_join(void *task) {
    if (task == NULL) return -1;
    return k_thread_join((struct k_thread *) task, K_FOREVER) == 0 ? 0 : -1;
}

int8_t nros_platform_task_detach(void *task) {
    (void) task;
    return 0;  /* Zephyr threads run independently once created */
}

int8_t nros_platform_task_cancel(void *task) {
    if (task == NULL) return -1;
    k_thread_abort((struct k_thread *) task);
    return 0;
}

void nros_platform_task_exit(void) {
    /* k_thread_abort(k_current_get()) is the documented self-exit
     * primitive; some Zephyr versions accept a fall-through return
     * from the entry point instead. */
    k_thread_abort(k_current_get());
}

void nros_platform_task_free(void **task) {
    (void) task;  /* caller-owned struct k_thread storage */
}

/* ---- Mutex (recursive; non-recursive uses same primitive) ---- */

int8_t nros_platform_mutex_init(void *m) {
    if (m == NULL) return -1;
    return k_mutex_init((struct k_mutex *) m) == 0 ? 0 : -1;
}

int8_t nros_platform_mutex_drop(void *m) {
    (void) m;  /* Zephyr k_mutex has no destroy */
    return 0;
}

int8_t nros_platform_mutex_lock(void *m) {
    if (m == NULL) return -1;
    return k_mutex_lock((struct k_mutex *) m, K_FOREVER) == 0 ? 0 : -1;
}

int8_t nros_platform_mutex_try_lock(void *m) {
    if (m == NULL) return -1;
    int rc = k_mutex_lock((struct k_mutex *) m, K_NO_WAIT);
    if (rc == 0)       return 0;
    if (rc == -EBUSY)  return 1;
    return -1;
}

int8_t nros_platform_mutex_unlock(void *m) {
    if (m == NULL) return -1;
    return k_mutex_unlock((struct k_mutex *) m) == 0 ? 0 : -1;
}

int8_t nros_platform_mutex_rec_init(void *m)     { return nros_platform_mutex_init(m); }
int8_t nros_platform_mutex_rec_drop(void *m)     { return nros_platform_mutex_drop(m); }
int8_t nros_platform_mutex_rec_lock(void *m)     { return nros_platform_mutex_lock(m); }
int8_t nros_platform_mutex_rec_try_lock(void *m) { return nros_platform_mutex_try_lock(m); }
int8_t nros_platform_mutex_rec_unlock(void *m)   { return nros_platform_mutex_unlock(m); }

/* ---- Condvars (Zephyr 2.5+: k_condvar_*) ---- */

int8_t nros_platform_condvar_init(void *cv) {
    if (cv == NULL) return -1;
    return k_condvar_init((struct k_condvar *) cv) == 0 ? 0 : -1;
}

int8_t nros_platform_condvar_drop(void *cv) {
    (void) cv;  /* no destroy */
    return 0;
}

int8_t nros_platform_condvar_signal(void *cv) {
    if (cv == NULL) return -1;
    return k_condvar_signal((struct k_condvar *) cv) == 0 ? 0 : -1;
}

int8_t nros_platform_condvar_signal_all(void *cv) {
    if (cv == NULL) return -1;
    return k_condvar_broadcast((struct k_condvar *) cv) >= 0 ? 0 : -1;
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
    return k_condvar_signal((struct k_condvar *) cv) == 0 ? 0 : -1;
}

int8_t nros_platform_condvar_wait(void *cv, void *m) {
    if (cv == NULL || m == NULL) return -1;
    return k_condvar_wait((struct k_condvar *) cv,
                          (struct k_mutex *) m,
                          K_FOREVER) == 0 ? 0 : -1;
}

int8_t nros_platform_condvar_wait_until(void *cv, void *m, uint64_t abstime_ms) {
    if (cv == NULL || m == NULL) return -1;
    uint64_t now = nros_platform_clock_ms();
    k_timeout_t to = abstime_ms > now
        ? K_MSEC((int64_t) (abstime_ms - now))
        : K_NO_WAIT;
    int rc = k_condvar_wait((struct k_condvar *) cv,
                            (struct k_mutex *) m,
                            to);
    if (rc == 0)         return 0;
    if (rc == -EAGAIN)   return 1;  /* Zephyr returns -EAGAIN on timeout */
    return -1;
}

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

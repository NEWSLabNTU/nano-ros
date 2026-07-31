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

uint64_t nros_platform_time_now_ms(void)              { return 0; }
uint32_t nros_platform_time_since_epoch_secs(void)    { return 0; }
uint32_t nros_platform_time_since_epoch_nanos(void)   { return 0; }

/* ---- Tasks ---- */

#ifdef CONFIG_POSIX_API

int nros_zephyr_task_create(pthread_t *thread,
                            void *(*entry)(void *),
                            void *arg);

int8_t nros_platform_task_init(void *task, void *attr,
                               void *(*entry)(void *), void *arg) {
    (void) attr;
    if (task == NULL || entry == NULL) return -1;
    return nros_zephyr_task_create((pthread_t *) task, entry, arg) == 0 ? 0 : -1;
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

int8_t nros_platform_task_init(void *task, void *attr,
                               void *(*entry)(void *), void *arg) {
    (void) task;
    (void) attr;
    (void) entry;
    (void) arg;
    return -1;
}

int8_t nros_platform_task_join(void *task) {
    (void) task;
    return -1;
}

int8_t nros_platform_task_detach(void *task) {
    (void) task;
    return -1;
}

int8_t nros_platform_task_cancel(void *task) {
    (void) task;
    return -1;
}

void nros_platform_task_exit(void) {}

void nros_platform_task_free(void **task) {
    (void) task;
}

/* ---- Mutex ---- */

int8_t nros_platform_mutex_init(void *m) {
    (void) m;
    return -1;
}

int8_t nros_platform_mutex_drop(void *m) {
    (void) m;
    return 0;
}

int8_t nros_platform_mutex_lock(void *m) {
    (void) m;
    return -1;
}

int8_t nros_platform_mutex_try_lock(void *m) {
    (void) m;
    return -1;
}

int8_t nros_platform_mutex_unlock(void *m) {
    (void) m;
    return -1;
}

int8_t nros_platform_mutex_rec_init(void *m)       { return nros_platform_mutex_init(m); }
int8_t nros_platform_mutex_rec_drop(void *m)       { return nros_platform_mutex_drop(m); }
int8_t nros_platform_mutex_rec_lock(void *m)       { return nros_platform_mutex_lock(m); }
int8_t nros_platform_mutex_rec_try_lock(void *m)   { return nros_platform_mutex_try_lock(m); }
int8_t nros_platform_mutex_rec_unlock(void *m)     { return nros_platform_mutex_unlock(m); }

/* ---- Condvars ---- */

int8_t nros_platform_condvar_init(void *cv) {
    (void) cv;
    return -1;
}

int8_t nros_platform_condvar_drop(void *cv) {
    (void) cv;
    return 0;
}

int8_t nros_platform_condvar_signal(void *cv) {
    (void) cv;
    return -1;
}

int8_t nros_platform_condvar_signal_all(void *cv) {
    (void) cv;
    return -1;
}

int8_t nros_platform_condvar_signal_from_isr(void *cv) {
    (void) cv;
    return -1;
}

int8_t nros_platform_condvar_wait(void *cv, void *m) {
    (void) cv;
    (void) m;
    return -1;
}

int8_t nros_platform_condvar_wait_until(void *cv, void *m, uint64_t abstime_ms) {
    (void) cv;
    (void) m;
    (void) abstime_ms;
    return -1;
}

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

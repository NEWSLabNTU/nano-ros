/**
 * @file platform_impl.c
 * @brief Reference implementation of the canonical nros platform ABI in C.
 *
 * This file shows how you bring up the platform abstraction layer for your own
 * target. It implements every `nros_platform_*` symbol declared in the single
 * canonical header `<nros/platform.h>` (owned by `nros-platform-api`,
 * RFC-0042 D1 / phase-243). Every nros binary links exactly ONE platform
 * implementation; resolution is at link time, no runtime registration.
 *
 * The bodies below are POSIX-backed so the reference compiles and runs on a
 * desktop for testing — mirroring the authoritative Rust/C POSIX port
 * (`packages/core/nros-platform-posix/`). Each section also notes the
 * bare-metal alternative (Cortex-M shown) so a real port is a mechanical
 * substitution.
 *
 * ## How this file is wired into the build
 *
 * The sibling `CMakeLists.txt` compiles this file into a stand-alone
 * `baremetal_platform_ref` library purely for COMPILE COVERAGE — it proves the
 * reference stays in lockstep with the canonical header. It is deliberately
 * NOT linked into the `baremetal_demo` executable, because that binary already
 * links the Rust POSIX port (via `NanoRos::NanoRos` / `DEPLOY native`), and two
 * implementations of the same `nros_platform_*` symbols cannot coexist in one
 * link (duplicate-definition error).
 *
 * To make THIS file the platform for a real target you would instead:
 *   1. build nros WITHOUT a Rust platform port (no `platform-posix` /
 *      `platform-<rtos>` feature — the C side provides the symbols), and
 *   2. link this translation unit (or your edited copy) into the firmware.
 *
 * ## What you must implement vs what the header gives you for free
 *
 * `nros_platform_malloc` / `nros_platform_free` (thin forwards to
 * `alloc`/`dealloc`) and `nros_platform_atomic_{store,load}_bool` (built on the
 * `__atomic_*` builtins) are `static inline` in the header — do NOT redefine
 * them. Everything declared `extern` there is your job; all of it is below.
 *
 * `nros_platform_register_log_writer` is intentionally omitted: like the POSIX
 * port, this reference writes logs directly (to stderr), so it has no
 * board-registered writer hook. Platforms whose `log_write` is a thin
 * dispatcher (FreeRTOS / ThreadX / bare-metal UART) implement it instead.
 */

#define _POSIX_C_SOURCE 200809L
#define _DEFAULT_SOURCE

#include <nros/platform.h>

#include <errno.h>
#include <pthread.h>
#include <sched.h>
#include <semaphore.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

/* ============================================================================
 * Clock (monotonic)
 *
 * BARE-METAL: drive a free-running counter from SysTick (1 kHz → ms) or the
 * DWT cycle counter (→ sub-µs), e.g.
 *   uint64_t nros_platform_clock_us(void) {
 *       return (uint64_t) DWT->CYCCNT * 1000000ULL / SystemCoreClock;  // +wrap
 *   }
 * ==========================================================================*/

uint64_t nros_platform_clock_ms(void) {
    struct timespec ts;
    if (clock_gettime(CLOCK_MONOTONIC, &ts) != 0) {
        return 0;
    }
    return (uint64_t)ts.tv_sec * 1000ULL + (uint64_t)ts.tv_nsec / 1000000ULL;
}

uint64_t nros_platform_clock_us(void) {
    struct timespec ts;
    if (clock_gettime(CLOCK_MONOTONIC, &ts) != 0) {
        return 0;
    }
    return (uint64_t)ts.tv_sec * 1000000ULL + (uint64_t)ts.tv_nsec / 1000ULL;
}

/* ============================================================================
 * Allocation
 *
 * BARE-METAL without an OS heap: a bump or TLSF allocator over a static arena
 * (`static uint8_t heap[N];`). A genuinely heap-less board omits
 * `-DNROS_PLATFORM_HAS_MALLOC` so the header drops the malloc/free shim and any
 * heap-container use becomes a COMPILE error (the issue-0038 gate) instead of a
 * link failure.
 * ==========================================================================*/

void* nros_platform_alloc(size_t size) {
    if (size == 0) {
        return NULL;
    }
    return malloc(size);
}

void* nros_platform_realloc(void* ptr, size_t size) {
    if (size == 0) {
        free(ptr);
        return NULL;
    }
    return realloc(ptr, size);
}

void nros_platform_dealloc(void* ptr) {
    free(ptr);
}

/* Heap instrumentation is optional — return 0 ("unknown") if the allocator
 * does not track it. glibc exposes mallinfo2; a static-arena allocator would
 * report its own used/total counters here. */
#if defined(__GLIBC__)
#include <malloc.h>
size_t nros_platform_heap_used_bytes(void) {
    struct mallinfo2 mi = mallinfo2();
    return (size_t)mi.uordblks;
}
size_t nros_platform_heap_total_bytes(void) {
    struct mallinfo2 mi = mallinfo2();
    return (size_t)(mi.arena + mi.hblkhd);
}
#else
size_t nros_platform_heap_used_bytes(void) {
    return 0u;
}
size_t nros_platform_heap_total_bytes(void) {
    return 0u;
}
#endif

/* ============================================================================
 * Sleep
 *
 * BARE-METAL: busy-wait against the clock, or __WFI() until the next tick.
 *   void nros_platform_sleep_us(size_t us) {
 *       uint64_t end = nros_platform_clock_us() + us;
 *       while (nros_platform_clock_us() < end) { }   // or __WFE() to idle
 *   }
 * ==========================================================================*/

void nros_platform_sleep_us(size_t us) {
    struct timespec ts = {
        .tv_sec = (time_t)(us / 1000000),
        .tv_nsec = (long)((us % 1000000) * 1000),
    };
    while (nanosleep(&ts, &ts) == -1 && errno == EINTR) {
        /* resume with the remaining time written back into ts */
    }
}

void nros_platform_sleep_ms(size_t ms) {
    nros_platform_sleep_us(ms * 1000u);
}

void nros_platform_sleep_s(size_t s) {
    nros_platform_sleep_us(s * 1000000u);
}

/* ============================================================================
 * Cooperative yield
 *
 * BARE-METAL: `core::hint::spin_loop()` equivalent — a `__NOP()` / `__YIELD()`.
 * On an RTOS use the native primitive (k_yield / vPortYield / sched_yield).
 * ==========================================================================*/

void nros_platform_yield_now(void) {
    sched_yield();
}

/* ============================================================================
 * Random
 *
 * Deterministic xorshift seeded from a fixed constant — matches the POSIX port
 * so the two are observably equivalent under tests. A real target seeds from a
 * hardware RNG / unique-device-id / ADC noise.
 * ==========================================================================*/

static uint64_t s_rng_state = 0x9E3779B97F4A7C15ULL;

static uint64_t rng_next(void) {
    uint64_t x = s_rng_state;
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    s_rng_state = x;
    return x;
}

uint8_t nros_platform_random_u8(void) {
    return (uint8_t)rng_next();
}
uint16_t nros_platform_random_u16(void) {
    return (uint16_t)rng_next();
}
uint32_t nros_platform_random_u32(void) {
    return (uint32_t)rng_next();
}
uint64_t nros_platform_random_u64(void) {
    return rng_next();
}

void nros_platform_random_fill(void* buf, size_t len) {
    uint8_t* p = (uint8_t*)buf;
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

/* ============================================================================
 * Wall clock
 *
 * Real-time (Unix epoch). Return 0 if the board has no RTC.
 * ==========================================================================*/

uint64_t nros_platform_time_now_ms(void) {
    struct timespec ts;
    if (clock_gettime(CLOCK_REALTIME, &ts) != 0) {
        return 0;
    }
    return (uint64_t)ts.tv_sec * 1000ULL + (uint64_t)ts.tv_nsec / 1000000ULL;
}

uint32_t nros_platform_time_since_epoch_secs(void) {
    struct timespec ts;
    if (clock_gettime(CLOCK_REALTIME, &ts) != 0) {
        return 0;
    }
    return (uint32_t)ts.tv_sec;
}

uint32_t nros_platform_time_since_epoch_nanos(void) {
    struct timespec ts;
    if (clock_gettime(CLOCK_REALTIME, &ts) != 0) {
        return 0;
    }
    return (uint32_t)ts.tv_nsec;
}

/* ============================================================================
 * Tasks
 *
 * `task` is caller-provided opaque storage; here it holds a `pthread_t`. An
 * RTOS port maps these onto its thread API (k_thread / xTaskCreate /
 * tx_thread_create); a single-core bare-metal port with no scheduler returns
 * non-zero (unsupported) and runs everything from the executor on the main
 * loop.
 * ==========================================================================*/

int8_t nros_platform_task_init(void* task, void* attr, void* (*entry)(void*), void* arg) {
    (void)attr;
    if (task == NULL || entry == NULL) {
        return -1;
    }
    return pthread_create((pthread_t*)task, NULL, entry, arg) == 0 ? 0 : -1;
}

int8_t nros_platform_task_join(void* task) {
    if (task == NULL) {
        return -1;
    }
    return pthread_join(*(pthread_t*)task, NULL) == 0 ? 0 : -1;
}

int8_t nros_platform_task_detach(void* task) {
    if (task == NULL) {
        return -1;
    }
    return pthread_detach(*(pthread_t*)task) == 0 ? 0 : -1;
}

int8_t nros_platform_task_cancel(void* task) {
    if (task == NULL) {
        return -1;
    }
    return pthread_cancel(*(pthread_t*)task) == 0 ? 0 : -1;
}

void nros_platform_task_exit(void) {
    pthread_exit(NULL);
}

void nros_platform_task_free(void** task) {
    (void)task; /* storage is caller-owned — nothing to free */
}

/* ============================================================================
 * Non-recursive mutex (storage = pthread_mutex_t)
 *
 * BARE-METAL single core: a mutex is interrupt-disable/enable around the
 * critical region, or a simple test-and-set spinlock.
 * ==========================================================================*/

int8_t nros_platform_mutex_init(void* m) {
    return (m && pthread_mutex_init((pthread_mutex_t*)m, NULL) == 0) ? 0 : -1;
}

int8_t nros_platform_mutex_drop(void* m) {
    return (m && pthread_mutex_destroy((pthread_mutex_t*)m) == 0) ? 0 : -1;
}

int8_t nros_platform_mutex_lock(void* m) {
    return (m && pthread_mutex_lock((pthread_mutex_t*)m) == 0) ? 0 : -1;
}

int8_t nros_platform_mutex_try_lock(void* m) {
    if (m == NULL) {
        return -1;
    }
    int r = pthread_mutex_trylock((pthread_mutex_t*)m);
    if (r == 0) return 0;     /* acquired */
    if (r == EBUSY) return 1; /* held by someone else */
    return -1;
}

int8_t nros_platform_mutex_unlock(void* m) {
    return (m && pthread_mutex_unlock((pthread_mutex_t*)m) == 0) ? 0 : -1;
}

/* ============================================================================
 * Recursive mutex (same-thread re-entry; required by zenoh-pico)
 * ==========================================================================*/

int8_t nros_platform_mutex_rec_init(void* m) {
    if (m == NULL) {
        return -1;
    }
    pthread_mutexattr_t attr;
    if (pthread_mutexattr_init(&attr) != 0) {
        return -1;
    }
    int8_t rc = -1;
    if (pthread_mutexattr_settype(&attr, PTHREAD_MUTEX_RECURSIVE) == 0 &&
        pthread_mutex_init((pthread_mutex_t*)m, &attr) == 0) {
        rc = 0;
    }
    pthread_mutexattr_destroy(&attr);
    return rc;
}

int8_t nros_platform_mutex_rec_drop(void* m) {
    return nros_platform_mutex_drop(m);
}
int8_t nros_platform_mutex_rec_lock(void* m) {
    return nros_platform_mutex_lock(m);
}
int8_t nros_platform_mutex_rec_try_lock(void* m) {
    return nros_platform_mutex_try_lock(m);
}
int8_t nros_platform_mutex_rec_unlock(void* m) {
    return nros_platform_mutex_unlock(m);
}

/* ============================================================================
 * Condition variables (storage = pthread_cond_t)
 * ==========================================================================*/

int8_t nros_platform_condvar_init(void* cv) {
    return (cv && pthread_cond_init((pthread_cond_t*)cv, NULL) == 0) ? 0 : -1;
}

int8_t nros_platform_condvar_drop(void* cv) {
    return (cv && pthread_cond_destroy((pthread_cond_t*)cv) == 0) ? 0 : -1;
}

int8_t nros_platform_condvar_signal(void* cv) {
    return (cv && pthread_cond_signal((pthread_cond_t*)cv) == 0) ? 0 : -1;
}

int8_t nros_platform_condvar_signal_all(void* cv) {
    return (cv && pthread_cond_broadcast((pthread_cond_t*)cv) == 0) ? 0 : -1;
}

/* ISR-safe signal. pthread_cond_signal is NOT async-signal-safe, so a real
 * POSIX port forwards through an eventfd/self-pipe worker; an RTOS uses its
 * ISR-safe give (k_sem_give / xSemaphoreGiveFromISR / tx_event_flags_set);
 * bare-metal Cortex-M does an atomic flag store + __SEV(). For this reference
 * we alias to the thread-context signal (safe from any non-signal-handler
 * thread). */
int8_t nros_platform_condvar_signal_from_isr(void* cv) {
    return nros_platform_condvar_signal(cv);
}

int8_t nros_platform_condvar_wait(void* cv, void* m) {
    if (cv == NULL || m == NULL) {
        return -1;
    }
    return pthread_cond_wait((pthread_cond_t*)cv, (pthread_mutex_t*)m) == 0 ? 0 : -1;
}

int8_t nros_platform_condvar_wait_until(void* cv, void* m, uint64_t abstime) {
    if (cv == NULL || m == NULL) {
        return -1;
    }
    /* `abstime` is a monotonic deadline in `clock_ms` units. Convert to a
     * relative delay and re-anchor against CLOCK_REALTIME (what
     * pthread_cond_timedwait uses by default). */
    uint64_t now_ms = nros_platform_clock_ms();
    uint64_t rel_ms = abstime > now_ms ? abstime - now_ms : 0;

    struct timespec deadline;
    if (clock_gettime(CLOCK_REALTIME, &deadline) != 0) {
        return -1;
    }
    deadline.tv_sec += (time_t)(rel_ms / 1000);
    deadline.tv_nsec += (long)((rel_ms % 1000) * 1000000);
    if (deadline.tv_nsec >= 1000000000L) {
        deadline.tv_sec += 1;
        deadline.tv_nsec -= 1000000000L;
    }
    int r = pthread_cond_timedwait((pthread_cond_t*)cv, (pthread_mutex_t*)m, &deadline);
    if (r == 0) return 0;
    if (r == ETIMEDOUT) return 1;
    return -1;
}

/* ============================================================================
 * Wake primitive (Phase 130)
 *
 * Binary-semaphore shape used by the executor's wake_flag / spin_once pair.
 * Storage is opaque; the caller sizes it via the probe helpers below. A real
 * port maps it onto a binary semaphore (sem_t / k_sem / xSemaphoreCreateBinary
 * / tx_semaphore) or, on bare-metal, an atomic flag + busy-spin.
 *
 *   wait_ms: 0 = signaled, 1 = timeout, -1 = error.
 * ==========================================================================*/

typedef struct {
    sem_t sem;
} nros_wake_t;

int8_t nros_platform_wake_init(void* w) {
    if (w == NULL) {
        return -1;
    }
    return sem_init(&((nros_wake_t*)w)->sem, 0, 0) == 0 ? 0 : -1;
}

int8_t nros_platform_wake_drop(void* w) {
    if (w == NULL) {
        return 0;
    }
    return sem_destroy(&((nros_wake_t*)w)->sem) == 0 ? 0 : -1;
}

int8_t nros_platform_wake_wait_ms(void* w, uint32_t timeout_ms) {
    if (w == NULL) {
        return -1;
    }
    struct timespec ts;
    if (clock_gettime(CLOCK_REALTIME, &ts) != 0) {
        return -1;
    }
    uint64_t add_ns = (uint64_t)timeout_ms * 1000000ULL;
    ts.tv_sec += (time_t)(add_ns / 1000000000ULL);
    ts.tv_nsec += (long)(add_ns % 1000000000ULL);
    if (ts.tv_nsec >= 1000000000L) {
        ts.tv_sec += 1;
        ts.tv_nsec -= 1000000000L;
    }
    while (sem_timedwait(&((nros_wake_t*)w)->sem, &ts) != 0) {
        if (errno == ETIMEDOUT) return 1;
        if (errno == EINTR) continue;
        return -1;
    }
    return 0;
}

int8_t nros_platform_wake_signal(void* w) {
    if (w == NULL) {
        return -1;
    }
    /* Coalesce so the binary semaphore never exceeds 1. */
    int val = 0;
    if (sem_getvalue(&((nros_wake_t*)w)->sem, &val) != 0) {
        return -1;
    }
    if (val > 0) {
        return 0;
    }
    return sem_post(&((nros_wake_t*)w)->sem) == 0 ? 0 : -1;
}

int8_t nros_platform_wake_signal_from_isr(void* w) {
    /* Hosted POSIX: ISR semantics not meaningful — alias to signal. */
    return nros_platform_wake_signal(w);
}

size_t nros_platform_wake_storage_size(void) {
    return sizeof(nros_wake_t);
}
size_t nros_platform_wake_storage_align(void) {
    return _Alignof(nros_wake_t);
}

/* ============================================================================
 * Critical section (Phase 121.9)
 *
 * Global mutual exclusion against preemption + ISR delivery, reentrant by
 * contract. The returned token restores the prior posture; here a process-wide
 * recursive mutex tracks nesting so the token is unused.
 *
 * BARE-METAL Cortex-M:
 *   uint32_t nros_platform_critical_section_acquire(void) {
 *       uint32_t primask = __get_PRIMASK();
 *       __disable_irq();
 *       return primask;                 // token = prior PRIMASK
 *   }
 *   void nros_platform_critical_section_release(uint32_t token) {
 *       if (!token) __enable_irq();      // only re-enable if we disabled
 *   }
 * ==========================================================================*/

static pthread_mutex_t s_cs_mutex;
static pthread_once_t s_cs_once = PTHREAD_ONCE_INIT;

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
    (void)token;
    pthread_mutex_unlock(&s_cs_mutex);
}

/* ============================================================================
 * Logging (Phase 88)
 *
 * `nros-log` formats the message body; this sink only prepends the severity +
 * logger name and appends a newline. The mutex keeps multi-thread writes to one
 * line at a time. A board with a UART/RTT sink writes there instead (and may
 * expose `nros_platform_register_log_writer` — omitted here, matching the
 * direct-writer POSIX port).
 * ==========================================================================*/

static const char* severity_label(uint8_t s) {
    switch (s) {
    case 0:
        return "TRACE";
    case 1:
        return "DEBUG";
    case 2:
        return "INFO";
    case 3:
        return "WARN";
    case 4:
        return "ERROR";
    case 5:
        return "FATAL";
    default:
        return "?";
    }
}

static pthread_mutex_t s_log_mutex = PTHREAD_MUTEX_INITIALIZER;

void nros_platform_log_write(uint8_t severity, const uint8_t* name_ptr, uintptr_t name_len,
                             const uint8_t* msg_ptr, uintptr_t msg_len) {
    if (msg_ptr == NULL && msg_len > 0) {
        return;
    }
    const char* label = severity_label(severity);
    pthread_mutex_lock(&s_log_mutex);
    if (name_ptr != NULL && name_len > 0) {
        fprintf(stderr, "[%s] %.*s: %.*s\n", label, (int)name_len, (const char*)name_ptr,
                (int)msg_len, (const char*)msg_ptr);
    } else {
        fprintf(stderr, "[%s] %.*s\n", label, (int)msg_len, (const char*)msg_ptr);
    }
    pthread_mutex_unlock(&s_log_mutex);
}

void nros_platform_log_flush(void) {
    fflush(stderr);
}

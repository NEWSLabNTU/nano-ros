/*
 * A HOST stand-in for <zephyr/kernel.h>, for one purpose only: compiling
 * `zephyr/nros_platform_zephyr_shims.c` on the build host so its thread-slot
 * table can be unit tested (phase-460 W6).
 *
 * This is NOT a Zephyr emulator and must never grow into one. The rule for
 * every name below is:
 *
 *   - a name the slot table genuinely USES is backed by the host equivalent
 *     (k_mutex -> pthread_mutex, the stack array -> an aligned char array
 *     pthread_attr_setstack can take);
 *   - a name the shims merely MENTION, so the translation unit compiles, is a
 *     stub that ABORTS when called. A silent no-op would let a future test
 *     "pass" against behaviour this file never implemented, which is the one
 *     way a stub tree can produce false evidence.
 *
 * The compile line, not this header, decides which of the shims' CONFIG_*
 * blocks exist. `tests/zephyr/run-thread-slots.sh` defines CONFIG_PTHREAD and
 * CONFIG_MAIN_STACK_SIZE and leaves CONFIG_NET_SOCKETS, CONFIG_SMP,
 * CONFIG_SCHED_CPU_MASK, CONFIG_SCHED_DEADLINE, CONFIG_TRACING_CTF and
 * CONFIG_NROS_SNTP_EPOCH unset, so the socket, SMP, cpu-pin, EDF and tracing
 * arms are preprocessed away and need nothing here.
 */
#ifndef NROS_HOST_SHIMS_ZEPHYR_KERNEL_H
#define NROS_HOST_SHIMS_ZEPHYR_KERNEL_H

#include <errno.h>  /* Zephyr's kernel.h reaches errno; the shims use ENOSYS */
#include <pthread.h>
#include <sched.h>
#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include <zephyr/sys/printk.h>

/* The shims call these only from arms this build compiles out; a call means
 * the host test has drifted into territory the stub tree does not model. */
#define NROS_HOST_SHIM_UNREACHABLE(name)                                        \
    do {                                                                        \
        fprintf(stderr,                                                          \
                "host-shims: %s() is a STUB and was called. The host thread-slot\n" \
                "            test does not model it; see tests/zephyr/host-shims.\n", \
                (name));                                                         \
        abort();                                                                 \
    } while (0)

/* ---- time, scheduling, entropy ------------------------------------------
 *
 * Real enough to run: the slot test sleeps and yields. */

typedef struct nros_host_k_thread* k_tid_t;
struct k_thread {
    int unused;
};

typedef struct {
    int64_t ticks;
} k_timeout_t;

#define K_NO_WAIT ((k_timeout_t){.ticks = 0})
#define K_FOREVER ((k_timeout_t){.ticks = -1})
#define K_USEC(us) ((k_timeout_t){.ticks = (int64_t)(us)})

static inline int64_t k_uptime_get(void) {
    NROS_HOST_SHIM_UNREACHABLE("k_uptime_get");
    return 0;
}

static inline int32_t k_msleep(int32_t ms) {
    NROS_HOST_SHIM_UNREACHABLE("k_msleep");
    return ms;
}

static inline void k_yield(void) {
    sched_yield();
}

static inline k_tid_t k_current_get(void) {
    NROS_HOST_SHIM_UNREACHABLE("k_current_get");
    return NULL;
}

static inline void k_thread_priority_set(k_tid_t tid, int prio) {
    (void)tid;
    (void)prio;
    NROS_HOST_SHIM_UNREACHABLE("k_thread_priority_set");
}

/* ---- the heap the timer shims use --------------------------------------- */

static inline void* k_malloc(size_t n) {
    return malloc(n);
}

static inline void k_free(void* p) {
    free(p);
}

/* ---- mutex: the slot table's ACTUAL serialisation ------------------------
 *
 * `nros_claim_thread_slot` / `nros_zephyr_task_slot_release` take this lock
 * around the table, so it has to be a real mutex or the test would prove
 * nothing about concurrent claims. K_MUTEX_DEFINE is a file-scope definition
 * in Zephyr, so it is one here too; PTHREAD_MUTEX_INITIALIZER makes that a
 * static initialiser with no init call to place. */

struct k_mutex {
    pthread_mutex_t m;
};

#define K_MUTEX_DEFINE(name) struct k_mutex name = {PTHREAD_MUTEX_INITIALIZER}

static inline int k_mutex_lock(struct k_mutex* mutex, k_timeout_t timeout) {
    (void)timeout; /* the shims pass K_FOREVER only */
    return pthread_mutex_lock(&mutex->m);
}

static inline int k_mutex_unlock(struct k_mutex* mutex) {
    return pthread_mutex_unlock(&mutex->m);
}

/* ---- stacks -------------------------------------------------------------
 *
 * `nros_zephyr_task_create_prio` hands `&nros_thread_stacks[slot]` to
 * `pthread_attr_setstack`, so on the host these must be memory glibc will
 * accept as a thread stack: page-aligned, and sized by the compile line
 * (NROS_ZEPHYR_STACK_SIZE, well above PTHREAD_STACK_MIN). Reusing one of these
 * arrays for a second thread after its first occupant was joined IS the
 * behaviour under test. */
#define K_THREAD_STACK_ARRAY_DEFINE(sym, nmemb, size) \
    static char __attribute__((aligned(4096))) sym[nmemb][size]

/* ---- threads ------------------------------------------------------------
 *
 * The tier pool is a different pool with a different lifetime rule and no part
 * of this wave. Stubbed loudly rather than emulated. */

typedef void (*k_thread_entry_t)(void*, void*, void*);

static inline k_tid_t k_thread_create(struct k_thread* new_thread, void* stack,
                                      size_t stack_size, k_thread_entry_t entry, void* p1,
                                      void* p2, void* p3, int prio, uint32_t options,
                                      k_timeout_t delay) {
    (void)new_thread;
    (void)stack;
    (void)stack_size;
    (void)entry;
    (void)p1;
    (void)p2;
    (void)p3;
    (void)prio;
    (void)options;
    (void)delay;
    NROS_HOST_SHIM_UNREACHABLE("k_thread_create");
    return NULL;
}

static inline int k_thread_name_set(k_tid_t tid, const char* name) {
    (void)tid;
    (void)name;
    NROS_HOST_SHIM_UNREACHABLE("k_thread_name_set");
    return -1;
}

static inline void k_thread_start(k_tid_t tid) {
    (void)tid;
    NROS_HOST_SHIM_UNREACHABLE("k_thread_start");
}

/* ---- timers -------------------------------------------------------------
 *
 * Mentioned by the shims' Sporadic-server budget wrappers, unreachable here. */

struct k_timer;
typedef void (*k_timer_expiry_t)(struct k_timer*);

struct k_timer {
    void* user_data;
};

static inline void k_timer_init(struct k_timer* t, k_timer_expiry_t expiry, void* stop) {
    (void)t;
    (void)expiry;
    (void)stop;
    NROS_HOST_SHIM_UNREACHABLE("k_timer_init");
}

static inline void k_timer_start(struct k_timer* t, k_timeout_t duration, k_timeout_t period) {
    (void)t;
    (void)duration;
    (void)period;
    NROS_HOST_SHIM_UNREACHABLE("k_timer_start");
}

static inline void k_timer_stop(struct k_timer* t) {
    (void)t;
    NROS_HOST_SHIM_UNREACHABLE("k_timer_stop");
}

static inline void k_timer_user_data_set(struct k_timer* t, void* user_data) {
    (void)t;
    (void)user_data;
    NROS_HOST_SHIM_UNREACHABLE("k_timer_user_data_set");
}

static inline void* k_timer_user_data_get(struct k_timer* t) {
    (void)t;
    NROS_HOST_SHIM_UNREACHABLE("k_timer_user_data_get");
    return NULL;
}

static inline uint32_t k_timer_remaining_get(struct k_timer* t) {
    (void)t;
    NROS_HOST_SHIM_UNREACHABLE("k_timer_remaining_get");
    return 0;
}

/* ---- interrupt lock -----------------------------------------------------
 *
 * There are no interrupts to mask on the host. Unlike the names above this one
 * is a NO-OP rather than an abort: it is a critical-section wrapper whose
 * contract on a uniprocessor kernel is "nothing else runs", and a host test
 * that called it would want it to return, not die. Nothing in the slot test
 * calls it today. */
static inline unsigned int irq_lock(void) {
    return 0u;
}

static inline void irq_unlock(unsigned int key) {
    (void)key;
}

#endif /* NROS_HOST_SHIMS_ZEPHYR_KERNEL_H */

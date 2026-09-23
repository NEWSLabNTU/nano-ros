/*
 * A HOST stand-in for <zephyr/kernel.h>, for one purpose only: compiling
 * `packages/platform/nros-platform-zephyr/src/platform.c` on the build host so
 * its HEAP EXHAUSTION path can be driven by a test (phase-460 W7, issue 1425).
 *
 * This is NOT a Zephyr emulator and must never grow into one. It follows the
 * same rule as its sibling `tests/zephyr/host-shims/include/zephyr/kernel.h`,
 * which exists for a different translation unit and a different wave:
 *
 *   - a name the path under test genuinely USES is backed by the host
 *     equivalent (the spinlock -> a pthread mutex, printk -> the test's own
 *     capture, k_panic -> a longjmp the test catches);
 *   - a name `platform.c` merely MENTIONS, so the translation unit compiles, is
 *     a stub that ABORTS when called. A silent no-op would let a future test
 *     "pass" against behaviour this file never implemented, which is the one
 *     way a stub tree can produce false evidence.
 *
 * A SEPARATE TREE from `host-shims/`, on purpose. That one is sized to exactly
 * what `nros_platform_zephyr_shims.c` touches and its header says so; growing
 * it to cover a second, much larger translation unit would make W6's gate
 * depend on names W6 has nothing to do with, and a stub tree that serves two
 * subjects is the first step to the emulator neither may become.
 *
 * The compile line, not this header, decides which of `platform.c`'s CONFIG_*
 * blocks exist. `tests/zephyr/run-heap-exhaustion.sh` defines
 * CONFIG_NROS_BOOT_REPORT and (per case) CONFIG_NROS_HEAP_EXHAUSTION_IS_FATAL,
 * and leaves CONFIG_POSIX_API, CONFIG_DYNAMIC_THREAD, CONFIG_SNTP, CONFIG_LOG,
 * CONFIG_ARCH_POSIX, CONFIG_INIT_STACKS and CONFIG_TEST_RANDOM_GENERATOR unset
 * -- so the pthread, dynamic-thread, SNTP, logging, native_sim-argv and
 * stack-space arms are preprocessed away and need nothing here.
 */
#ifndef NROS_HOST_PLATFORM_ZEPHYR_KERNEL_H
#define NROS_HOST_PLATFORM_ZEPHYR_KERNEL_H

#include <errno.h>
#include <pthread.h>
#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include <zephyr/sys/printk.h>

#define NROS_HOST_PLATFORM_UNREACHABLE(name)                                     \
    do {                                                                         \
        fprintf(stderr,                                                          \
                "host-platform: %s() is a STUB and was called. The host heap\n"  \
                "               test does not model it; see "                    \
                "tests/zephyr/host-platform.\n",                                 \
                (name));                                                         \
        abort();                                                                 \
    } while (0)

/* `IS_ENABLED` is the whole reason CONFIG_NROS_HEAP_EXHAUSTION_IS_FATAL can be
 * read with both arms compiled. Zephyr's version is a macro-expansion trick so
 * that an UNDEFINED symbol is 0 rather than a compile error; this is that
 * trick, transcribed, because the test needs the same "defined to 1 or absent"
 * semantics the real one has. */
#define NROS_Z_IS_ENABLED1(cfg) NROS_Z_IS_ENABLED2(NROS_Z_XXXX##cfg)
#define NROS_Z_XXXX1 0,
#define NROS_Z_IS_ENABLED2(one_or_two) NROS_Z_IS_ENABLED3(one_or_two 1, 0)
#define NROS_Z_IS_ENABLED3(ignore_this, val, ...) val
#define IS_ENABLED(cfg) NROS_Z_IS_ENABLED1(cfg)

/* ---- time ---------------------------------------------------------------
 *
 * The allocator path does not read a clock. Present because `platform.c` has
 * clock entry points at the top of the file. */

typedef struct {
    int64_t ticks;
} k_timeout_t;

#define K_NO_WAIT ((k_timeout_t){.ticks = 0})
#define K_FOREVER ((k_timeout_t){.ticks = -1})
#define K_MSEC(ms) ((k_timeout_t){.ticks = (int64_t)(ms)})
#define K_USEC(us) ((k_timeout_t){.ticks = (int64_t)(us)})
#define K_SECONDS(s) ((k_timeout_t){.ticks = (int64_t)(s) * 1000})

static inline int64_t k_uptime_ticks(void) {
    NROS_HOST_PLATFORM_UNREACHABLE("k_uptime_ticks");
    return 0;
}

static inline uint64_t k_cycle_get_64(void) {
    NROS_HOST_PLATFORM_UNREACHABLE("k_cycle_get_64");
    return 0;
}

static inline uint64_t k_ticks_to_ns_floor64(int64_t t) {
    (void)t;
    NROS_HOST_PLATFORM_UNREACHABLE("k_ticks_to_ns_floor64");
    return 0;
}

static inline uint64_t k_cyc_to_ns_floor64(uint64_t c) {
    (void)c;
    NROS_HOST_PLATFORM_UNREACHABLE("k_cyc_to_ns_floor64");
    return 0;
}

static inline uint64_t k_ticks_to_us_floor64(int64_t t) {
    (void)t;
    NROS_HOST_PLATFORM_UNREACHABLE("k_ticks_to_us_floor64");
    return 0;
}

static inline uint32_t sys_clock_hw_cycles_per_sec(void) {
    NROS_HOST_PLATFORM_UNREACHABLE("sys_clock_hw_cycles_per_sec");
    return 0;
}

#define CONFIG_SYS_CLOCK_TICKS_PER_SEC 1000

static inline int32_t k_usleep(int32_t us) {
    (void)us;
    NROS_HOST_PLATFORM_UNREACHABLE("k_usleep");
    return 0;
}

static inline int32_t k_msleep(int32_t ms) {
    (void)ms;
    NROS_HOST_PLATFORM_UNREACHABLE("k_msleep");
    return 0;
}

static inline int32_t k_sleep(k_timeout_t t) {
    (void)t;
    NROS_HOST_PLATFORM_UNREACHABLE("k_sleep");
    return 0;
}

static inline void k_yield(void) {
    NROS_HOST_PLATFORM_UNREACHABLE("k_yield");
}

static inline bool k_is_in_isr(void) {
    return false;
}

/* ---- the spinlock around the heap funnel --------------------------------
 *
 * REAL, not a stub, and it has to be: `nros_platform_alloc` takes it around
 * every call into the rlsf arena, and a lock that does not lock would make the
 * test prove nothing about the path it claims to drive. A pthread mutex is the
 * host's equivalent -- the contract Zephyr's spinlock has on a uniprocessor
 * kernel is mutual exclusion, and that is what this provides. */

struct k_spinlock {
    pthread_mutex_t m;
};

typedef int k_spinlock_key_t;

static inline k_spinlock_key_t k_spin_lock(struct k_spinlock* l) {
    static pthread_once_t once = PTHREAD_ONCE_INIT;
    (void)once;
    pthread_mutex_lock(&l->m);
    return 0;
}

static inline void k_spin_unlock(struct k_spinlock* l, k_spinlock_key_t key) {
    (void)key;
    pthread_mutex_unlock(&l->m);
}

/* `static struct k_spinlock x;` zero-initialises, and a zeroed
 * pthread_mutex_t IS PTHREAD_MUTEX_INITIALIZER on glibc. Asserted rather than
 * assumed -- see `heap_exhaustion_test.c`. */

/* ---- interrupt lock -----------------------------------------------------
 *
 * A NO-OP rather than an abort, unlike the names above: this is a
 * critical-section wrapper whose contract on a uniprocessor kernel is "nothing
 * else runs", and a host caller would want it to return, not die. */
static inline unsigned int irq_lock(void) {
    return 0u;
}

static inline void irq_unlock(unsigned int key) {
    (void)key;
}

/* ---- the fatal hook -----------------------------------------------------
 *
 * THE SUBJECT OF THIS TEST. `nros_platform_panic` printk()s and then calls
 * k_panic(), which on a board enters Zephyr's fatal path so the image's
 * `k_sys_fatal_error_handler` runs. On the host there is no such path, so the
 * test supplies the definition (in `heap_exhaustion_test.c`) and records that
 * it was reached -- which is the assertion the whole wave exists to make.
 *
 * Declared `noreturn` because `nros_platform_panic` is, and the compiler must
 * see the same thing the board's compiler does: the code after the call is
 * unreachable, and a k_panic() that returned would change what `platform.c`
 * means. */
void k_panic(void) __attribute__((noreturn));

/* ---- mutex / condvar / semaphore ----------------------------------------
 *
 * `platform.c`'s non-POSIX arm implements the ABI's mutex, condvar and wake
 * objects on these. None of them is on the allocation path, so all are stubs
 * -- but they must still be TYPES of the right shape, because the file takes
 * `sizeof` of two of them for its storage-size entry points. */

struct k_mutex {
    void* opaque;
};

struct k_condvar {
    void* opaque;
};

struct k_sem {
    void* opaque;
};

static inline int k_mutex_init(struct k_mutex* m) {
    (void)m;
    NROS_HOST_PLATFORM_UNREACHABLE("k_mutex_init");
    return -1;
}

static inline int k_mutex_lock(struct k_mutex* m, k_timeout_t t) {
    (void)m;
    (void)t;
    NROS_HOST_PLATFORM_UNREACHABLE("k_mutex_lock");
    return -1;
}

static inline int k_mutex_unlock(struct k_mutex* m) {
    (void)m;
    NROS_HOST_PLATFORM_UNREACHABLE("k_mutex_unlock");
    return -1;
}

static inline int k_condvar_init(struct k_condvar* cv) {
    (void)cv;
    NROS_HOST_PLATFORM_UNREACHABLE("k_condvar_init");
    return -1;
}

static inline int k_condvar_signal(struct k_condvar* cv) {
    (void)cv;
    NROS_HOST_PLATFORM_UNREACHABLE("k_condvar_signal");
    return -1;
}

static inline int k_condvar_broadcast(struct k_condvar* cv) {
    (void)cv;
    NROS_HOST_PLATFORM_UNREACHABLE("k_condvar_broadcast");
    return -1;
}

static inline int k_condvar_wait(struct k_condvar* cv, struct k_mutex* m, k_timeout_t t) {
    (void)cv;
    (void)m;
    (void)t;
    NROS_HOST_PLATFORM_UNREACHABLE("k_condvar_wait");
    return -1;
}

static inline void k_sem_init(struct k_sem* s, unsigned int initial, unsigned int limit) {
    (void)s;
    (void)initial;
    (void)limit;
    NROS_HOST_PLATFORM_UNREACHABLE("k_sem_init");
}

static inline void k_sem_reset(struct k_sem* s) {
    (void)s;
    NROS_HOST_PLATFORM_UNREACHABLE("k_sem_reset");
}

static inline int k_sem_take(struct k_sem* s, k_timeout_t t) {
    (void)s;
    (void)t;
    NROS_HOST_PLATFORM_UNREACHABLE("k_sem_take");
    return -1;
}

static inline void k_sem_give(struct k_sem* s) {
    (void)s;
    NROS_HOST_PLATFORM_UNREACHABLE("k_sem_give");
}

#endif /* NROS_HOST_PLATFORM_ZEPHYR_KERNEL_H */

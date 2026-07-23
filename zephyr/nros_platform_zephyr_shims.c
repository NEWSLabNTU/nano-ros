/**
 * @file nros_platform_zephyr_shims.c
 * @brief Real-symbol wrappers around Zephyr kernel inlines.
 *
 * Several Zephyr APIs are declared `static inline` in headers
 * (`k_msleep`, `k_uptime_get`, `sys_rand_get`, etc.) and have no exported
 * symbol. Rust FFI can only call real symbols, so we wrap them here —
 * this TU is compiled by the Zephyr module build and exports the real
 * functions that `nros-platform-zephyr` declares as `extern "C"`.
 *
 * Real-function Zephyr APIs (`k_malloc`, `k_free`, `k_usleep`,
 * `sys_rand32_get`, `pthread_*`) are called directly from Rust and do
 * not need wrappers.
 */

#include <stddef.h>
#include <stdint.h>

#include <zephyr/kernel.h>
#include <zephyr/random/random.h>

/* ── Clock / sleep / random (no POSIX dependency) ───────────────────── */

int64_t nros_zephyr_uptime_ms(void) {
    return k_uptime_get();
}

int32_t nros_zephyr_msleep(int32_t ms) {
    return k_msleep(ms);
}

void nros_zephyr_rand_fill(void* dst, size_t len) {
    sys_rand_get(dst, len);
}

/* Phase 77.22: cooperative yield. k_yield is declared `static inline`
 * in <zephyr/kernel.h>, so wrap it here to get a real callable symbol.
 */
void nros_zephyr_yield(void) {
    k_yield();
}

/* Phase 110.D — per-thread scheduling controls. `k_thread_priority_set`
 * and `k_current_get` are static inlines, so wrap each as a real
 * symbol that Rust FFI can link.
 */
void nros_zephyr_thread_priority_set(int prio) {
    k_thread_priority_set(k_current_get(), prio);
}

int nros_zephyr_thread_cpu_pin(int cpu) {
#ifdef CONFIG_SCHED_CPU_MASK_PIN_ONLY
    return k_thread_cpu_pin(k_current_get(), cpu);
#else
    (void)cpu;
    return -ENOSYS;
#endif
}

/* Phase 110.E.b — periodic timer for Sporadic-server budget refill.
 * Wraps `k_timer_*` (static inlines) plus a per-timer bridge struct
 * holding (callback, user_data) so the Rust side can pass an
 * `extern "C" fn(*mut c_void)` despite Zephyr's
 * `void(*)(struct k_timer *)` expiration signature.
 */
typedef struct {
    void (*cb)(void*);
    void* user_data;
} nros_zephyr_timer_bridge_t;

static void nros_zephyr_timer_expiry(struct k_timer* t) {
    nros_zephyr_timer_bridge_t* b = (nros_zephyr_timer_bridge_t*)k_timer_user_data_get(t);
    if (b && b->cb) {
        b->cb(b->user_data);
    }
}

void* nros_zephyr_timer_create_periodic(unsigned int period_us, void (*cb)(void*),
                                        void* user_data) {
    struct k_timer* t = k_malloc(sizeof(*t));
    if (!t) return NULL;
    nros_zephyr_timer_bridge_t* b = k_malloc(sizeof(*b));
    if (!b) {
        k_free(t);
        return NULL;
    }
    b->cb = cb;
    b->user_data = user_data;
    k_timer_init(t, nros_zephyr_timer_expiry, NULL);
    k_timer_user_data_set(t, b);
    k_timer_start(t, K_USEC(period_us), K_USEC(period_us));
    return t;
}

void nros_zephyr_timer_destroy(void* timer) {
    if (!timer) return;
    struct k_timer* t = (struct k_timer*)timer;
    nros_zephyr_timer_bridge_t* b = (nros_zephyr_timer_bridge_t*)k_timer_user_data_get(t);
    k_timer_stop(t);
    if (b) k_free(b);
    k_free(t);
}

/* Phase 110.E.b follow-up — oneshot variant (period = K_NO_WAIT
 * means fire once and stop).
 */
void* nros_zephyr_timer_create_oneshot(unsigned int timeout_us, void (*cb)(void*),
                                       void* user_data) {
    struct k_timer* t = k_malloc(sizeof(*t));
    if (!t) return NULL;
    nros_zephyr_timer_bridge_t* b = k_malloc(sizeof(*b));
    if (!b) {
        k_free(t);
        return NULL;
    }
    b->cb = cb;
    b->user_data = user_data;
    k_timer_init(t, nros_zephyr_timer_expiry, NULL);
    k_timer_user_data_set(t, b);
    /* Second arg = period; K_NO_WAIT (0) makes this a oneshot. */
    k_timer_start(t, K_USEC(timeout_us), K_NO_WAIT);
    return t;
}

/* Stop the timer without freeing. Returns 1 if the timer was running
 * and got stopped, 0 if it had already expired or was never started.
 */
int nros_zephyr_timer_cancel(void* timer) {
    if (!timer) return 0;
    struct k_timer* t = (struct k_timer*)timer;
    /* k_timer_status_get reports remaining time; 0 means already
     * fired. We use k_timer_remaining_get which returns 0 on fired. */
    unsigned int remaining = k_timer_remaining_get(t);
    k_timer_stop(t);
    return remaining > 0 ? 1 : 0;
}

/* ── BSD socket wrappers ────────────────────────────────────────────
 *
 * On native_sim, glibc's getaddrinfo/freeaddrinfo symbols override
 * Zephyr's POSIX wrappers. The glibc versions return POSIX addrinfo
 * layout (ai_flags first), but Zephyr's zsock_addrinfo has ai_next
 * first. Use Zephyr's zsock_* API directly to avoid the collision.
 */

#include <zephyr/net/socket.h>

int nros_zephyr_getaddrinfo(const char* node, const char* service,
                            const struct zsock_addrinfo* hints, struct zsock_addrinfo** res) {
    return zsock_getaddrinfo(node, service, hints, res);
}

void nros_zephyr_freeaddrinfo(struct zsock_addrinfo* res) {
    zsock_freeaddrinfo(res);
}

int nros_zephyr_socket(int family, int type, int proto) {
    return zsock_socket(family, type, proto);
}

int nros_zephyr_close(int fd) {
    return zsock_close(fd);
}

int nros_zephyr_connect(int fd, const struct sockaddr* addr, socklen_t addrlen) {
    return zsock_connect(fd, addr, addrlen);
}

int nros_zephyr_bind(int fd, const struct sockaddr* addr, socklen_t addrlen) {
    return zsock_bind(fd, addr, addrlen);
}

int nros_zephyr_listen(int fd, int backlog) {
    return zsock_listen(fd, backlog);
}

int nros_zephyr_accept(int fd, struct sockaddr* addr, socklen_t* addrlen) {
    return zsock_accept(fd, addr, addrlen);
}

int nros_zephyr_shutdown(int fd, int how) {
    return zsock_shutdown(fd, how);
}

int nros_zephyr_setsockopt(int fd, int level, int optname, const void* optval, socklen_t optlen) {
    return zsock_setsockopt(fd, level, optname, optval, optlen);
}

int nros_zephyr_fcntl(int fd, int cmd, int arg) {
    return zsock_fcntl(fd, cmd, arg);
}

ssize_t nros_zephyr_recv(int fd, void* buf, size_t len, int flags) {
    return zsock_recv(fd, buf, len, flags);
}

ssize_t nros_zephyr_recvfrom(int fd, void* buf, size_t len, int flags, struct sockaddr* src_addr,
                             socklen_t* addrlen) {
    return zsock_recvfrom(fd, buf, len, flags, src_addr, addrlen);
}

ssize_t nros_zephyr_send(int fd, const void* buf, size_t len, int flags) {
    return zsock_send(fd, buf, len, flags);
}

ssize_t nros_zephyr_sendto(int fd, const void* buf, size_t len, int flags,
                           const struct sockaddr* dest_addr, socklen_t addrlen) {
    return zsock_sendto(fd, buf, len, flags, dest_addr, addrlen);
}

/* ── Thread creation with Zephyr-managed stacks ─────────────────────
 *
 * Requires CONFIG_POSIX_API (or equivalent CONFIG_PTHREAD).
 * Only compiled when POSIX threads are available.
 */

#if defined(CONFIG_POSIX_API) || defined(CONFIG_PTHREAD)

#include <zephyr/posix/pthread.h>

#ifndef NROS_ZEPHYR_MAX_THREADS
#define NROS_ZEPHYR_MAX_THREADS 8
#endif

#ifndef NROS_ZEPHYR_STACK_SIZE
#define NROS_ZEPHYR_STACK_SIZE CONFIG_MAIN_STACK_SIZE
#endif

K_THREAD_STACK_ARRAY_DEFINE(nros_thread_stacks, NROS_ZEPHYR_MAX_THREADS, NROS_ZEPHYR_STACK_SIZE);
static int nros_thread_index;

int nros_zephyr_task_create(pthread_t* thread, void* (*entry)(void*), void* arg) {
    if (nros_thread_index >= NROS_ZEPHYR_MAX_THREADS) {
        return -1; /* no more stack slots */
    }

    pthread_attr_t attr;
    (void)pthread_attr_init(&attr);
    (void)pthread_attr_setstack(&attr, &nros_thread_stacks[nros_thread_index++],
                                NROS_ZEPHYR_STACK_SIZE);

    int ret = pthread_create(thread, &attr, entry, arg);
    (void)pthread_attr_destroy(&attr);
    return ret;
}

#endif /* CONFIG_POSIX_API || CONFIG_PTHREAD */

/* ── errno read helper (Phase 92.5 diagnostic) ──────────────────────
 *
 * Zephyr's `errno` is thread-local and lives behind a per-thread
 * pointer that's only accessible through the `errno` macro. Rust
 * callers can't expand the macro, so wrap it here.
 */
#include <errno.h>
int nros_zephyr_errno(void) {
    return errno;
}

/* ── critical-section wrappers (Phase 71.6) ────────────────────────
 *
 * Zephyr's `irq_lock()` / `irq_unlock()` are static inline macros with
 * no exported symbols. nros-c / nros-cpp's Rust-side critical-section
 * impl needs real linkable symbols to call, so wrap them here.
 *
 * Used by the C/C++ API path on platform-zephyr to satisfy
 * `_critical_section_1_0_acquire` / `_critical_section_1_0_release`
 * referenced from dust-dds + portable-atomic when the
 * zephyr-lang-rust crate (which provides its own impl) isn't linked.
 */
unsigned int nros_zephyr_irq_lock(void) {
    return irq_lock();
}

void nros_zephyr_irq_unlock(unsigned int key) {
    irq_unlock(key);
}

/* phase-243 — the nros_platform_time_ns / sleep_ns exported wrappers are retired.
 * nros-c's no_std path (platform-zephyr) now uses the canonical
 * nros_platform_clock_us() / sleep_us() (nros-platform-zephyr provides them), so
 * no Rust caller needs the ns symbols here anymore. */

/* ── Phase 97.4.zephyr-native_sim debug printk shims ─────────────────
 *
 * Rust extern "C" can't directly call variadic `printk`. Provide
 * non-variadic wrappers per shape. Always exported; Rust call sites
 * are cfg-gated behind feature flags.
 */
void nros_zephyr_log(const char* msg) {
    printk("[nros] %s\n", msg);
}

void nros_zephyr_log_int(const char* tag, int64_t v) {
    printk("[nros] %s=%lld\n", tag, (long long)v);
}

void nros_zephyr_log_2int(const char* tag, int64_t a, int64_t b) {
    printk("[nros] %s=%lld,%lld\n", tag, (long long)a, (long long)b);
}

/* ── RT-tier task spawn (issue #128 / RFC-0015 Model 1) ─────────────
 *
 * One `k_thread` per priority tier, RAW Zephyr priority (negatives =
 * cooperative — the `[tiers.<name>.zephyr].priority` value verbatim,
 * which the POSIX pthread shim above cannot express). Static pool:
 * tier count is a compile-time property of the baked system, so a
 * small fixed pool avoids CONFIG_DYNAMIC_THREAD. C-ABI-shaped so the
 * phase-274 W3 C/C++ zephyr `run_tiers` can reuse it.
 */

#ifndef NROS_ZEPHYR_MAX_TIERS
#define NROS_ZEPHYR_MAX_TIERS 4
#endif

#ifndef NROS_ZEPHYR_TIER_STACK_SIZE
#define NROS_ZEPHYR_TIER_STACK_SIZE 16384
#endif

K_THREAD_STACK_ARRAY_DEFINE(nros_tier_stacks, NROS_ZEPHYR_MAX_TIERS, NROS_ZEPHYR_TIER_STACK_SIZE);
static struct k_thread nros_tier_threads[NROS_ZEPHYR_MAX_TIERS];
static int nros_tier_index;

static void nros_zephyr_tier_trampoline(void* entry, void* arg, void* unused) {
    (void)unused;
    printk("[nros] tier task entered\n");
    void* (*fn)(void*) = (void* (*)(void*))entry;
    (void)fn(arg);
    printk("[nros] tier task RETURNED (unexpected)\n");
}

/**
 * Spawn one tier task. `entry(arg)` runs on a pool thread at the RAW
 * Zephyr `priority` (cooperative if negative). `name` is the thread's
 * debug name (may be NULL). Returns 0 on success, -1 when the pool is
 * exhausted (more than NROS_ZEPHYR_MAX_TIERS spawns).
 */
int nros_zephyr_tier_task_create(void* (*entry)(void*), void* arg, int32_t priority,
                                 const char* name) {
    if (entry == NULL || nros_tier_index >= NROS_ZEPHYR_MAX_TIERS) {
        return -1;
    }
    int idx = nros_tier_index++;
    k_tid_t tid = k_thread_create(&nros_tier_threads[idx], nros_tier_stacks[idx],
                                  NROS_ZEPHYR_TIER_STACK_SIZE, nros_zephyr_tier_trampoline,
                                  (void*)entry, arg, NULL, (int)priority, 0, K_NO_WAIT);
    if (tid == NULL) {
        return -1;
    }
    if (name != NULL) {
        (void)k_thread_name_set(tid, name);
    }
    return 0;
}

/**
 * Set the CALLING thread's priority to a raw Zephyr priority. The tier
 * boot thread (`rust_main`) runs `tiers[0]` itself, so it must adopt that
 * tier's declared priority instead of keeping the main-thread default.
 */
void nros_zephyr_set_current_priority(int32_t priority) {
    k_thread_priority_set(k_current_get(), (int)priority);
}

/**
 * phase-296 W5.5 — apply a per-thread earliest-deadline (µs) on the CALLING
 * thread. `k_thread_deadline_set` takes CYCLES; convert from µs. Compiled to a
 * no-op when the kernel lacks EDF (`CONFIG_SCHED_DEADLINE`) so the image still
 * links; the Rust caller (`entry_tiers::apply_tier_deadline`) additionally gates
 * the CALL behind the `zephyr-edf` feature, so a no-op here means an honest
 * fall-through to the executor's cooperative deadline monitor.
 *
 * NOTE: this lives here (the Zephyr-module C shims, linked by BOTH the pure-Rust
 * `ZephyrBoard::run_tiers` image and the C/C++ `nros_board_zephyr_run_tiers`
 * path) rather than in `c/zephyr_run_tiers.c` — that file is compiled only into
 * the C/C++ entry image, so a definition there is invisible to the Rust link.
 *
 * Returns 1 when the kernel actually applied the deadline (EDF present) and 0
 * when it was a no-op (`CONFIG_SCHED_DEADLINE` unset) — the Rust caller logs
 * its "EDF deadline set" marker ONLY on a 1, so the marker can never fire
 * from an image where the kernel never applied anything.
 */
int nros_zephyr_set_current_deadline(unsigned int deadline_us) {
#ifdef CONFIG_SCHED_DEADLINE
    k_thread_deadline_set(k_current_get(), (int)k_us_to_cyc_near32(deadline_us));
    return 1; /* applied — kernel EDF present */
#else
    (void)deadline_us;
    return 0; /* not applied — no kernel EDF; executor monitor is sole enforcement */
#endif
}

/*
 * Phase 121.3.threadx — native C implementation of the canonical
 * platform ABI for Azure RTOS ThreadX.
 *
 * Behavioural parity with `nros-platform-threadx`'s Rust impl:
 *
 *   - Clock    — tx_time_get() scaled by TX_TIMER_TICKS_PER_SECOND.
 *   - Alloc    — tx_byte_allocate / tx_byte_release against a
 *                caller-provided byte pool. The application sets the
 *                pool pointer once via `nros_platform_threadx_set_byte_pool`
 *                before the first allocation.
 *   - Sleep    — tx_thread_sleep(ms_to_ticks).
 *   - Yield    — tx_thread_relinquish() (ThreadX's native
 *                cooperative yield).
 *   - Random   — deterministic xorshift64; seedable via
 *                `nros_platform_threadx_seed_rng(u32)`.
 *   - Time     — wall clock unsupported; returns 0.
 *   - Tasks    — tx_thread_create + tx_thread_delete.
 *   - Mutexes  — tx_mutex_create with TX_INHERIT=1. ThreadX mutexes
 *                are recursive by design, so mutex_* and mutex_rec_*
 *                share the same primitive.
 *   - Condvars — tx_semaphore. tx_semaphore_get / tx_semaphore_put
 *                with the caller's mutex released around the wait
 *                (matches the Rust impl).
 *
 * Build verification requires ThreadX headers + a configured port;
 * CMakeLists.txt parametrises THREADX_KERNEL_TARGET. Integration
 * tests live at the application level (per-board).
 */

#include <nros/platform.h>

#include <tx_api.h>

#include <stddef.h>
#include <stdint.h>
#include <string.h>
#include <sys/types.h>
#include <errno.h>

#ifndef TX_TIMER_TICKS_PER_SECOND
#  define TX_TIMER_TICKS_PER_SECOND 100u
#endif

#define MS_PER_TICK ((uint64_t) (1000U / TX_TIMER_TICKS_PER_SECOND))

/* ---- Clock ---- */

/* RFC-0073 — the tick is all ThreadX portably offers (no sub-tick source:
 * TX_TRACE_TIME_SOURCE is a port-private macro), so nanoseconds here are a
 * constant multiply of a coarse counter. That is not a widening of what
 * this port knows: `clock_resolution_ns` states the tick, so a caller can
 * see that the low digits are always zero rather than infer precision the
 * signature seems to promise. The old `clock_us` DIVIDED; this multiplies. */
#define NS_PER_TICK ((uint64_t) (1000000000ULL / TX_TIMER_TICKS_PER_SECOND))

uint64_t nros_platform_clock_ns(void) {
    return (uint64_t) tx_time_get() * NS_PER_TICK;
}

uint64_t nros_platform_clock_resolution_ns(void) {
    return NS_PER_TICK;
}

/* issue 0758 — no wall-clock source on this platform, so `0` is the honest
 * answer, not a placeholder. The header makes `0` mean "no epoch here", which
 * lets a caller keep stamping boot-relative time knowingly instead of
 * publishing a confidently wrong absolute one.
 *
 * A port gains a real epoch by acquiring one (SNTP, an RTC handoff) and
 * returning it here; until then this is correct rather than unfinished.
 * Monotonic time is `nros_platform_clock_ns` and is unaffected. */
uint64_t nros_platform_epoch_us(void) {
    return 0;
}

/* ---- Byte-pool wiring ----
 *
 * ThreadX has no global heap; allocations come out of a caller-owned
 * `TX_BYTE_POOL`. The application initialises the pool, calls
 * `nros_platform_threadx_set_byte_pool` once, and from then on the
 * canonical alloc/realloc/dealloc symbols route through it.
 */

static TX_BYTE_POOL *s_byte_pool = NULL;

void nros_platform_threadx_set_byte_pool(void *pool) {
    s_byte_pool = (TX_BYTE_POOL *) pool;
}

/* ---- Alloc ---- */

void *nros_platform_alloc(size_t size) {
    if (size == 0 || s_byte_pool == NULL) {
        return NULL;
    }
    void *p = NULL;
    if (tx_byte_allocate(s_byte_pool, &p, (ULONG) size, TX_WAIT_FOREVER) != TX_SUCCESS) {
        return NULL;
    }
    return p;
}

void nros_platform_dealloc(void *ptr) {
    if (ptr != NULL) {
        (void) tx_byte_release(ptr);
    }
}

/* ---- Heap stats (phase-230 1b / RFC-0034 D7) ----
 * Query the byte pool: used = pool size − available. ThreadX is a Mode-A
 * platform (both zenoh-pico's z_malloc and nano-ros allocations funnel
 * through nros_platform_alloc → tx_byte_allocate), so this is the exact
 * unified figure. Returns 0 before the pool is registered. */
size_t nros_platform_heap_used_bytes(void) {
    if (s_byte_pool == NULL) {
        return 0u;
    }
    ULONG available = 0;
    if (tx_byte_pool_info_get(s_byte_pool, TX_NULL, &available, TX_NULL, TX_NULL, TX_NULL,
                              TX_NULL) != TX_SUCCESS) {
        return 0u;
    }
    ULONG total = s_byte_pool->tx_byte_pool_size;
    return (size_t) (total >= available ? total - available : 0u);
}

size_t nros_platform_heap_total_bytes(void) {
    if (s_byte_pool == NULL) {
        return 0u;
    }
    return (size_t) s_byte_pool->tx_byte_pool_size;
}

/* phase-230 1f (RFC-0034): the `z_malloc`/`z_free` funnel on ThreadX is owned
 * by zpico-sys's `platform_aliases.c` (the `platform-aliases` feature, on by
 * default for the ThreadX boards) — a STRONG `z_malloc`/`z_free` →
 * `nros_platform_alloc`/`_dealloc`. ThreadX uses zenoh-pico's generic
 * `system/common` platform, which defines NO `z_malloc`, so there is no
 * vendored bypass to guard (unlike FreeRTOS) and the alias is the sole
 * definition on the link. The earlier `__attribute__((weak)) z_malloc` here
 * (RFC-0034's "footgun") was silently shadowed by that alias and is removed:
 * a ThreadX zenoh build without `platform-aliases` should fail to link loudly
 * (no `z_malloc` provider) rather than fall back to a hidden weak forwarder. */

/*
 * Minimal POSIX/picolibc hooks for freestanding ThreadX links. Cyclone DDS
 * avoids file I/O here; its ThreadX socket waitset path still references a
 * few POSIX names, so provide weak stubs until the backend supplies native
 * waitset plumbing.
 *
 * HOSTED EXCEPTION (`__linux__`): the ThreadX *linux* port (threadx-linux)
 * runs as a real Linux process linked against glibc, which already provides
 * strong open/close/read/write/lseek/pipe and a real `stdin`. A *weak*
 * definition living in the main executable still shadows the glibc public
 * symbol for the dynamic lookup, so these stubs would hijack every public
 * `write(2)` etc. C/C++ stdio escapes this because glibc routes printf
 * through the internal `__write` alias, but Rust's `std::io::Stdout` calls
 * the public `write`, gets the stub's unconditional `-1`, and panics
 * ("failed printing to stdout"); with `panic = "abort"` that SIGABRTs the
 * whole node before it prints its readiness banner. So compile these only
 * for the freestanding (bare-metal) ThreadX targets — the riscv64 cross
 * toolchain does not define `__linux__`; the hosted linux port does.
 */
/* phase-386 W2 — these set `errno` before returning -1.
 *
 * Returning -1 is the CORRECT POSIX answer on a freestanding target: there is
 * no filesystem, so `open` genuinely cannot open. What was wrong is that a
 * caller doing the standard `if (rc < 0) perror(...)` read a STALE errno and
 * got a confident, unrelated diagnosis — silence would have been better.
 *
 * `ENOSYS` where the operation does not exist here at all (open, pipe);
 * `EBADF` where a descriptor was supplied that cannot be valid, since nothing
 * on this target can have produced one.
 *
 * NOTE these cannot fail loud by PRINTING. `write` is how printing reaches the
 * console, so a diagnostic inside it recurses — the same hazard as issue 0589
 * on Zephyr, where a Rust `println!` re-entered `zvfs_write` and exhausted the
 * stack with no message. errno is the only channel available to this group.
 */
#if !defined(__linux__)
__attribute__((weak)) void *stdin = NULL;

__attribute__((weak)) int open(const char *path, int flags, ...) {
    (void) path;
    (void) flags;
    errno = ENOSYS;
    return -1;
}

__attribute__((weak)) int close(int fd) {
    (void) fd;
    errno = EBADF;
    return -1;
}

__attribute__((weak)) ssize_t read(int fd, void *buf, size_t count) {
    (void) fd;
    (void) buf;
    (void) count;
    errno = EBADF;
    return -1;
}

__attribute__((weak)) ssize_t write(int fd, const void *buf, size_t count) {
    (void) fd;
    (void) buf;
    (void) count;
    errno = EBADF;
    return -1;
}

__attribute__((weak)) off_t lseek(int fd, off_t offset, int whence) {
    (void) fd;
    (void) offset;
    (void) whence;
    errno = EBADF;
    return (off_t) -1;
}

__attribute__((weak)) int pipe(int fds[2]) {
    if (fds != NULL) {
        fds[0] = -1;
        fds[1] = -1;
    }
    errno = ENOSYS;
    return -1;
}
#endif /* !__linux__ */

/*
 * tx_byte_allocate has no "remaining size" query; mirror the Rust
 * impl's strategy of malloc + memcpy + free with a best-effort copy
 * up to the new size.
 */
void *nros_platform_realloc(void *ptr, size_t size) {
    if (size == 0) {
        nros_platform_dealloc(ptr);
        return NULL;
    }
    if (ptr == NULL) {
        return nros_platform_alloc(size);
    }
    void *out = nros_platform_alloc(size);
    if (out == NULL) {
        return NULL;
    }
    memcpy(out, ptr, size);
    nros_platform_dealloc(ptr);
    return out;
}

/* ---- Sleep ---- */

static inline ULONG ms_to_ticks(size_t ms) {
    return (ULONG) ((ms * TX_TIMER_TICKS_PER_SECOND + 999U) / 1000U);
}

void nros_platform_sleep_us(size_t us) {
    if (us == 0) return;
    ULONG ticks = (ULONG) ((us + 9999U) / 10000U);  /* assumes 100Hz tick */
    if (ticks == 0) ticks = 1;
    tx_thread_sleep(ticks);
}

void nros_platform_sleep_ms(size_t ms) {
    tx_thread_sleep(ms_to_ticks(ms));
}

void nros_platform_sleep_s(size_t s) {
    tx_thread_sleep(ms_to_ticks(s * 1000U));
}

/* ---- Yield ---- */

void nros_platform_yield_now(void) {
    tx_thread_relinquish();
}

/* ---- Random — deterministic xorshift64 ---- */

static uint64_t s_rng_state = 0x9E3779B97F4A7C15ULL;

void nros_platform_threadx_seed_rng(uint32_t value) {
    s_rng_state = ((uint64_t) value) | (((uint64_t) value) << 32) | 1ULL;
}

static uint64_t rng_next(void) {
    uint64_t x = s_rng_state;
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    s_rng_state = x;
    return x;
}

uint8_t  nros_platform_random_u8(void)   { return (uint8_t)  rng_next(); }
uint16_t nros_platform_random_u16(void)  { return (uint16_t) rng_next(); }
uint32_t nros_platform_random_u32(void)  { return (uint32_t) rng_next(); }
uint64_t nros_platform_random_u64(void)  { return rng_next(); }

void nros_platform_random_fill(void *buf, size_t len) {
    uint8_t *p = (uint8_t *) buf;
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

/* ---- Wall clock — unsupported ---- */

/* No real-time clock on this port: 0 means "no wall clock", per the ABI. */
uint64_t nros_platform_time_now_ns(void)              { return 0; }

/* ---- Tasks ----
 *
 * Storage is a caller-allocated `nros_threadx_task_t` — the size to allocate
 * comes from `nros_platform_task_storage_size()`, so this port can carry a
 * pointer beside the control block without any caller knowing.
 */

/* phase-364 W3 — `nros_threadx_task_attr_t` deleted; the ABI defines
 * `nros_platform_task_attr_t` for every port. */

/* phase-364 W3 — task storage is a WRAPPER, not a bare `TX_THREAD`.
 *
 * ThreadX is the one port that cannot let the kernel find a stack: the caller
 * supplies the memory. That is why `attr == NULL` used to be a hard failure
 * here while posix and zephyr ignored `attr` entirely — the single largest
 * portability hole in the old ABI.
 *
 * With W2's storage probe the port can declare its own size, so it now carries
 * a pointer beside the control block and can OWN a stack it allocated when the
 * caller supplied none. `attr == NULL` therefore means the same here as
 * everywhere else. */
#ifdef NROS_TX_PORT_HAS_REENT
/* Defined in the port assembly (`tx_thread_schedule.S`): holds `&_impure_ptr`
 * once this port has a reent to install, 0 otherwise. */
extern struct _reent **nros_tx_impure_slot;
#endif

typedef struct {
    TX_THREAD thread;
    /** Non-NULL when this port allocated the stack and must release it. */
    void *owned_stack;
#ifdef NROS_TX_PORT_HAS_REENT
    /* issue 0680 — this task's newlib reentrancy block, owned here and
     * released with the stack. The PORT declares the capability (see its
     * `tx_port.h`), because whether per-thread libc state is needed is a
     * property of the C library that port links: threadx-linux compiles this
     * whole mechanism out, its host libc already giving every pthread its own
     * `errno`. */
    struct _reent *owned_reent;
#endif
} nros_threadx_task_t;

/** Default stack for a task spawned with no `stack_bytes`. Sized for the
 *  zenoh-pico / executor call depth, matching the other ports' defaults. */
#define NROS_THREADX_DEFAULT_STACK_BYTES 16384u

void nros_platform_task_attr_init(nros_platform_task_attr_t *attr) {
    if (attr == NULL) {
        return;
    }
    memset(attr, 0, sizeof(*attr));
    attr->priority = INT32_MIN;
    attr->core = -1;
}

int8_t nros_platform_task_init(void *task, void *attr,
                               void *(*entry)(void *), void *arg) {
    /* phase-364 W1/W3 — INVALID for a caller-side impossibility. `attr` is NO
     * LONGER among them: a NULL means every default, as on every other port. */
    if (task == NULL || entry == NULL) {
        return NROS_PLATFORM_RET_INVALID;
    }
    const nros_platform_task_attr_t *a = (const nros_platform_task_attr_t *) attr;

    nros_threadx_task_t *slot = (nros_threadx_task_t *) task;
    memset(slot, 0, sizeof(*slot));

    size_t stack_bytes = (a != NULL && a->stack_bytes > 0u)
                             ? a->stack_bytes
                             : NROS_THREADX_DEFAULT_STACK_BYTES;
    /* issue 0612 — raise a below-floor request to this port's minimum. Here the
     * clamp also decides how much memory the allocation below asks for, so it
     * has to happen before it: `tx_thread_create` answers TX_SIZE_ERROR for a
     * stack under TX_MINIMUM_STACK, which this port would then report as a
     * generic failure. */
    if (stack_bytes < (size_t) TX_MINIMUM_STACK) {
        stack_bytes = (size_t) TX_MINIMUM_STACK;
    }
    void *stack = (a != NULL) ? a->stack_mem : NULL;
    if (stack == NULL) {
        stack = nros_platform_alloc(stack_bytes);
        if (stack == NULL) {
            /* A shortage now, not a permanent property of the port. */
            return NROS_PLATFORM_RET_NOMEM;
        }
        slot->owned_stack = stack;
    }

#ifdef NROS_TX_PORT_HAS_REENT
    /* issue 0680 — give this task its own newlib reentrancy block BEFORE the
     * thread can run. The thread is created suspended and resumed only after
     * `nros_reent` is filled, because `tx_thread_schedule.S` reads that field
     * on the way in: a field filled afterwards would leave the first
     * scheduling of this thread pointing at the shared `_impure_data`.
     *
     * `_REENT_INIT_PTR` is newlib's own initialiser; zeroing is not
     * sufficient, the block carries non-zero fields (stdio pointers, the
     * locale pointer). A failure here is NOMEM like the stack's, not a silent
     * fallback to the shared block — sharing `errno` is the bug this exists to
     * fix, and doing it quietly is how it stayed invisible. */
    /* Point the port's scheduler slot at newlib's `_impure_ptr`. The swap in
     * `tx_thread_schedule.S` goes through this indirection rather than naming
     * `_impure_ptr` itself, because the same kernel archive links into
     * pure-Rust `no_std` images with no libc at all, where any reference to it
     * fails to link (and a weak one is out of PC-relative range). Assigning it
     * HERE means it is set by the only code path that also allocates a reent:
     * an image that never calls this has no per-thread reent to install, and
     * the slot stays 0 so the scheduler skips the store. Idempotent. */
    nros_tx_impure_slot = &_impure_ptr;

    slot->owned_reent = (struct _reent *) nros_platform_alloc(sizeof(struct _reent));
    if (slot->owned_reent == NULL) {
        if (slot->owned_stack != NULL) {
            nros_platform_dealloc(slot->owned_stack);
            slot->owned_stack = NULL;
        }
        return NROS_PLATFORM_RET_NOMEM;
    }
    _REENT_INIT_PTR(slot->owned_reent);
#endif

    /* ThreadX entry signature is `void(*)(ULONG)`. We forward our
     * pointer-shaped `arg` via reinterpretation; the double-cast via
     * `void *` defeats `-Werror=cast-function-type`; ABI parity is
     * the caller's responsibility (matches the Rust impl). */
    union { void *(*src)(void *); VOID (*dst)(ULONG); } _entry_cvt;
    _entry_cvt.src = entry;

    /* phase-364 W5 — map the normalised band onto ThreadX's range.
     *
     * ThreadX INVERTS: 0 is the HIGHEST priority, TX_MAX_PRIORITIES-1 the
     * lowest. The band is the other way round (larger = more urgent), so this
     * is the port where a shared number would otherwise mean the opposite of
     * what its author wrote — the reason W5 exists.
     *
     * `RAW` bypasses the map for a caller tuning ThreadX against its own
     * documentation. */
    UINT prio = (UINT) (TX_MAX_PRIORITIES / 2); /* default: mid-range */
    if (a != NULL) {
        if (NROS_PLATFORM_PRIORITY_IS_RAW(a->priority)) {
            prio = (UINT) NROS_PLATFORM_PRIORITY_RAW_VALUE(a->priority);
        } else if (a->priority != NROS_PLATFORM_PRIORITY_INHERIT && a->priority >= 0) {
            int32_t band = a->priority > NROS_PLATFORM_PRIORITY_MAX
                               ? NROS_PLATFORM_PRIORITY_MAX
                               : a->priority;
            /* Invert, then scale the band onto [0, TX_MAX_PRIORITIES-1]. */
            int32_t inverted = NROS_PLATFORM_PRIORITY_MAX - band;
            prio = (UINT) ((inverted * (TX_MAX_PRIORITIES - 1)) / NROS_PLATFORM_PRIORITY_MAX);
        }
    }
    if (prio >= (UINT) TX_MAX_PRIORITIES) {
        prio = (UINT) TX_MAX_PRIORITIES - 1u;
    }

    /* issue 0680 — a port with per-thread reentrancy creates SUSPENDED.
     * `tx_thread_create` does `TX_MEMSET(thread_ptr, 0, sizeof(TX_THREAD))`
     * (tx_thread_create.c:168), wiping the whole control block INCLUDING the
     * extension slot, so a `nros_reent` written BEFORE the call does not
     * survive it. Written AFTER, with the thread already running under
     * TX_AUTO_START, it would race the scheduler's execution-notify hook —
     * which reads the slot on the way in — and this thread's first scheduling
     * would take the shared `_impure_data`. Suspended-then-resume is the only
     * ordering with neither hole.
     *
     * Hoisted into a variable rather than an `#ifdef` inside the call:
     * `tx_thread_create` is a macro, and a directive among macro arguments is
     * not portable (gcc `-Werror` rejects it outright). */
#ifdef NROS_TX_PORT_HAS_REENT
    const UINT start_option = TX_DONT_START;
#else
    const UINT start_option = TX_AUTO_START;
#endif

    UINT rc = tx_thread_create(
        &slot->thread,
        (a != NULL && a->name != NULL) ? (char *) a->name : (char *) "nros",
        _entry_cvt.dst,
        (ULONG) (uintptr_t) arg,
        stack,
        (ULONG) stack_bytes,
        prio,
        prio,
        TX_NO_TIME_SLICE,
        start_option);
    if (rc != TX_SUCCESS) {
        if (slot->owned_stack != NULL) {
            /* `_dealloc`, not `_free`: the canonical `nros_platform_free` is a
             * `static inline` gated on `NROS_PLATFORM_HAS_MALLOC` (the 241.A
             * capability gate, so a heap container on a heap-less board is a
             * compile error). threadx-riscv64 does not define it, so the call
             * was an implicit declaration and `-Werror` took the family out.
             * This pairs with the `nros_platform_alloc` above, which is the
             * ungated funnel; a port implementing the ABI has no reason to
             * reach for the C++-facing alias. */
            nros_platform_dealloc(slot->owned_stack);
            slot->owned_stack = NULL;
        }
#ifdef NROS_TX_PORT_HAS_REENT
        if (slot->owned_reent != NULL) {
            nros_platform_dealloc(slot->owned_reent);
            slot->owned_reent = NULL;
        }
#endif
        /* phase-364 W1 — a refused create is a RESOURCE condition, not a
         * permanent one: ThreadX rejects when the priority is out of range (a
         * caller bug) or when the control block cannot be taken. `NOMEM` tells
         * the caller to retry rather than cache the refusal — the distinction
         * issue 0246 turns on. */
        return NROS_PLATFORM_RET_NOMEM;
    }
#ifdef NROS_TX_PORT_HAS_REENT
    /* Publish the reent BEFORE the thread can be scheduled, then start it.
     * This is the half `TX_DONT_START` above exists for. */
    slot->thread.nros_reent = slot->owned_reent;
    (void) tx_thread_resume(&slot->thread);
#endif
    return NROS_PLATFORM_RET_OK;
}

int8_t nros_platform_task_join(void *task) {
    if (task == NULL) return -1;
    /* ThreadX has no native join. Poll the thread state until it
     * reports completed/terminated. */
    UINT state = 0;
    while (1) {
        if (tx_thread_info_get(&((nros_threadx_task_t *) task)->thread,
                               TX_NULL, &state,
                               TX_NULL, TX_NULL, TX_NULL,
                               TX_NULL, TX_NULL, TX_NULL) != TX_SUCCESS) {
            return -1;
        }
        if (state == TX_COMPLETED || state == TX_TERMINATED) {
            return 0;
        }
        tx_thread_sleep(1);
    }
}

int8_t nros_platform_task_detach(void *task) {
    (void) task;
    return 0;  /* ThreadX threads don't need detach */
}

int8_t nros_platform_task_cancel(void *task) {
    if (task == NULL) return -1;
    return tx_thread_terminate(&((nros_threadx_task_t *) task)->thread) == TX_SUCCESS
               ? NROS_PLATFORM_RET_OK
               : NROS_PLATFORM_RET_ERROR;
}

void nros_platform_task_exit(void) {
    /* ThreadX threads exit by returning from their entry function.
     * A no-op here lets the caller's `return` propagate. */
}

void nros_platform_task_free(void **task) {
    if (task == NULL || *task == NULL) return;
    nros_threadx_task_t *slot = (nros_threadx_task_t *) *task;
    (void) tx_thread_delete(&slot->thread);
    /* phase-364 W3 — release a stack this port allocated because the caller
     * supplied none. A caller-provided `stack_mem` is the caller's to free. */
    if (slot->owned_stack != NULL) {
        /* `_dealloc` for the same reason as the create path above. */
        nros_platform_dealloc(slot->owned_stack);
        slot->owned_stack = NULL;
    }
#ifdef NROS_TX_PORT_HAS_REENT
    /* issue 0680 — the reent outlives the thread only until here. `_reclaim_reent`
     * is deliberately NOT called: it walks newlib's per-reent stdio and atexit
     * chains, and on this board nothing populates them (no `fopen`, no
     * `atexit`), while calling it would drag that machinery into every image.
     * The block is a plain allocation from this port's bump `_sbrk`, so
     * releasing it is releasing it. Cleared after `tx_thread_delete`, so no
     * scheduling of this thread can observe a freed pointer. */
    if (slot->owned_reent != NULL) {
        nros_platform_dealloc(slot->owned_reent);
        slot->owned_reent = NULL;
    }
#endif
}

/* ---- Mutex (non-recursive + recursive share the same primitive) ----
 *
 * ThreadX mutexes are recursive by design: the owner thread may
 * tx_mutex_get the same mutex multiple times and must tx_mutex_put
 * matching times. Both API families forward to the same code.
 */

int8_t nros_platform_mutex_init(void *m) {
    if (m == NULL) return -1;
    return tx_mutex_create((TX_MUTEX *) m, (char *) "nros", TX_INHERIT) == TX_SUCCESS
        ? 0 : -1;
}

int8_t nros_platform_mutex_drop(void *m) {
    if (m == NULL) return -1;
    return tx_mutex_delete((TX_MUTEX *) m) == TX_SUCCESS ? 0 : -1;
}

int8_t nros_platform_mutex_lock(void *m) {
    if (m == NULL) return -1;
    return tx_mutex_get((TX_MUTEX *) m, TX_WAIT_FOREVER) == TX_SUCCESS ? 0 : -1;
}

int8_t nros_platform_mutex_try_lock(void *m) {
    if (m == NULL) return -1;
    UINT rc = tx_mutex_get((TX_MUTEX *) m, TX_NO_WAIT);
    if (rc == TX_SUCCESS)         return 0;
    if (rc == TX_NOT_AVAILABLE)   return 1;
    return -1;
}

int8_t nros_platform_mutex_unlock(void *m) {
    if (m == NULL) return -1;
    return tx_mutex_put((TX_MUTEX *) m) == TX_SUCCESS ? 0 : -1;
}

int8_t nros_platform_mutex_rec_init(void *m)     { return nros_platform_mutex_init(m); }
int8_t nros_platform_mutex_rec_drop(void *m)     { return nros_platform_mutex_drop(m); }
int8_t nros_platform_mutex_rec_lock(void *m)     { return nros_platform_mutex_lock(m); }
int8_t nros_platform_mutex_rec_try_lock(void *m) { return nros_platform_mutex_try_lock(m); }
int8_t nros_platform_mutex_rec_unlock(void *m)   { return nros_platform_mutex_unlock(m); }

/* ---- Condvar — tx_semaphore-backed ----
 *
 * Storage is a `TX_SEMAPHORE`. Signal does tx_semaphore_put; wait
 * does tx_semaphore_get with the caller's mutex released around the
 * blocking call. Matches the Rust impl's behaviour.
 */

int8_t nros_platform_condvar_init(void *cv) {
    if (cv == NULL) return -1;
    return tx_semaphore_create((TX_SEMAPHORE *) cv, (char *) "nros_cv", 0) == TX_SUCCESS
        ? 0 : -1;
}

int8_t nros_platform_condvar_drop(void *cv) {
    if (cv == NULL) return -1;
    return tx_semaphore_delete((TX_SEMAPHORE *) cv) == TX_SUCCESS ? 0 : -1;
}

int8_t nros_platform_condvar_signal(void *cv) {
    if (cv == NULL) return -1;
    return tx_semaphore_put((TX_SEMAPHORE *) cv) == TX_SUCCESS ? 0 : -1;
}

int8_t nros_platform_condvar_signal_all(void *cv) {
    /* tx_semaphore has no broadcast; the Rust impl issues a single
     * put. Match that behaviour. Callers needing broadcast can
     * loop, but the semantic is "wake at least one". */
    return nros_platform_condvar_signal(cv);
}

/* Phase 124.B.7.a — ISR-safe signal.
 *
 * tx_semaphore_put is ISR-safe under ThreadX (callable from any
 * context, including ISRs). Same impl as the thread-context path. */
int8_t nros_platform_condvar_signal_from_isr(void *cv) {
    if (cv == NULL) return -1;
    return tx_semaphore_put((TX_SEMAPHORE *) cv) == TX_SUCCESS ? 0 : -1;
}

int8_t nros_platform_condvar_wait(void *cv, void *m) {
    if (cv == NULL || m == NULL) return -1;
    nros_platform_mutex_unlock(m);
    UINT rc = tx_semaphore_get((TX_SEMAPHORE *) cv, TX_WAIT_FOREVER);
    nros_platform_mutex_lock(m);
    return rc == TX_SUCCESS ? 0 : -1;
}

int8_t nros_platform_condvar_wait_until(void *cv, void *m, uint64_t abstime_ms) {
    if (cv == NULL || m == NULL) return -1;
    uint64_t now = (nros_platform_clock_ns() / 1000000ULL);
    ULONG timeout_ticks = abstime_ms > now
        ? (ULONG) ((abstime_ms - now) * TX_TIMER_TICKS_PER_SECOND / 1000U)
        : 0;
    nros_platform_mutex_unlock(m);
    UINT rc = tx_semaphore_get((TX_SEMAPHORE *) cv, timeout_ticks);
    nros_platform_mutex_lock(m);
    if (rc == TX_SUCCESS)       return 0;
    if (rc == TX_NO_INSTANCE)   return 1;  /* timeout */
    return -1;
}

/* ============================================================
 *   Wake primitive (Phase 130)
 *
 *   Binary semaphore backed by `tx_semaphore`. `tx_semaphore_put`
 *   is documented ISR-safe by ThreadX (callable from ISRs without
 *   a separate `_from_isr` variant).
 * ============================================================ */

/* TX_SEMAPHORE control block lives inline in caller storage. */
typedef TX_SEMAPHORE nros_wake_t;

int8_t nros_platform_wake_init(void *w) {
    if (w == NULL) return -1;
    /* Initial count 0 (waiter blocks until first put). */
    UINT rc = tx_semaphore_create((TX_SEMAPHORE *) w, (CHAR *) "nros_wake", 0u);
    return rc == TX_SUCCESS ? 0 : -1;
}

int8_t nros_platform_wake_drop(void *w) {
    if (w == NULL) return 0;
    (void) tx_semaphore_delete((TX_SEMAPHORE *) w);
    return 0;
}

int8_t nros_platform_wake_wait_ms(void *w, uint32_t timeout_ms) {
    if (w == NULL) return -1;
    /* ThreadX ticks come from `TX_TIMER_TICKS_PER_SECOND`; convert
     * ms via the same formula nros_platform_clock_ms uses. */
    ULONG ticks;
    if (timeout_ms == 0u) {
        ticks = TX_NO_WAIT;
    } else {
        ULONG tps = TX_TIMER_TICKS_PER_SECOND;
        if (tps == 0u) tps = 100u;  /* defensive fallback */
        ticks = (ULONG) (((uint64_t) timeout_ms * tps + 999u) / 1000u);
        if (ticks == 0u) ticks = 1u;
    }
    UINT rc = tx_semaphore_get((TX_SEMAPHORE *) w, ticks);
    if (rc == TX_SUCCESS)            return 0;
    if (rc == TX_NO_INSTANCE
        || rc == TX_WAIT_ABORTED)    return 1;
    return -1;
}

int8_t nros_platform_wake_signal(void *w) {
    if (w == NULL) return -1;
    UINT rc = tx_semaphore_ceiling_put((TX_SEMAPHORE *) w, 1u);
    /* Ceiling-put with limit 1 = binary semaphore semantics:
     * subsequent puts coalesce instead of stacking. */
    return rc == TX_SUCCESS ? 0 : -1;
}

int8_t nros_platform_wake_signal_from_isr(void *w) {
    /* tx_semaphore_put / _ceiling_put are ISR-safe per ThreadX spec. */
    return nros_platform_wake_signal(w);
}

size_t nros_platform_wake_storage_size(void) {
    return sizeof(nros_wake_t);
}

size_t nros_platform_wake_storage_align(void) {
    return __alignof__(nros_wake_t);
}

/* phase-359 W10 — opaque-storage sizing for `task`, the sibling of the wake
 * probes above. `task_init`'s contract says the implementor decides the size;
 * these let a caller ASK instead of hard-coding it (issue 0570's trap). */
size_t nros_platform_task_storage_size(void) {
    return sizeof(nros_threadx_task_t);
}

size_t nros_platform_task_storage_align(void) {
    return _Alignof(nros_threadx_task_t);
}
/* phase-364 W2 (RFC-0076 D1) — opaque-storage sizing for the lock family, the
 * siblings of the `wake` and `task` probes.
 *
 * Two forms because callers need two. A Rust or otherwise-dynamic caller asks
 * at RUNTIME and allocates; zenoh-pico embeds `_z_mutex_t` BY VALUE and needs
 * the number at COMPILE time, which a function call cannot provide. The
 * `_Static_assert`s below are what stop the two from drifting: the macro and
 * the type are checked against each other in the port that owns both, so a
 * wrong macro is a compile error here rather than a buffer overrun in a
 * consumer.
 *
 * This replaces a hand-computed table in `zpico-sys` that guessed OTHER
 * platforms' struct sizes with `≈` and a "2× safety margin". */
size_t nros_platform_mutex_storage_size(void) { return sizeof(TX_MUTEX); }
size_t nros_platform_mutex_storage_align(void) { return _Alignof(TX_MUTEX); }
size_t nros_platform_mutex_rec_storage_size(void) { return sizeof(TX_MUTEX); }
size_t nros_platform_mutex_rec_storage_align(void) { return _Alignof(TX_MUTEX); }
size_t nros_platform_condvar_storage_size(void) { return sizeof(TX_SEMAPHORE); }
size_t nros_platform_condvar_storage_align(void) { return _Alignof(TX_SEMAPHORE); }

_Static_assert(NROS_PLATFORM_MUTEX_STORAGE_SIZE >= sizeof(TX_MUTEX),
               "NROS_PLATFORM_MUTEX_STORAGE_SIZE too small for this port");
_Static_assert(NROS_PLATFORM_MUTEX_REC_STORAGE_SIZE >= sizeof(TX_MUTEX),
               "NROS_PLATFORM_MUTEX_REC_STORAGE_SIZE too small for this port");
_Static_assert(NROS_PLATFORM_CONDVAR_STORAGE_SIZE >= sizeof(TX_SEMAPHORE),
               "NROS_PLATFORM_CONDVAR_STORAGE_SIZE too small for this port");
_Static_assert(NROS_PLATFORM_TASK_STORAGE_SIZE >= sizeof(nros_threadx_task_t),
               "NROS_PLATFORM_TASK_STORAGE_SIZE too small for this port");


/* ============================================================
 *   Critical section (Phase 121.9)
 * ============================================================ */
/* `tx_interrupt_control(TX_INT_DISABLE)` returns the prior posture
 * (TX_INT_ENABLE or TX_INT_DISABLE); pass the same value back via
 * `tx_interrupt_control(token)` to restore. ThreadX's port already
 * stacks interrupt state across nested acquire/release pairs. */
uint32_t nros_platform_critical_section_acquire(void) {
    return (uint32_t) tx_interrupt_control(TX_INT_DISABLE);
}

void nros_platform_critical_section_release(uint32_t token) {
    (void) tx_interrupt_control((UINT) token);
}

/* ============================================================
 *   Logging (Phase 88)
 *
 *   ThreadX has no native text logger. Same fn-ptr pattern as
 *   FreeRTOS: board crate registers a writer at startup. Without
 *   one, the ABI is a no-op.
 * ============================================================ */
#include <string.h>

typedef void (*nros_platform_log_writer_fn)(
    uint8_t        severity,
    const uint8_t *name_ptr, uintptr_t name_len,
    const uint8_t *msg_ptr,  uintptr_t msg_len);

typedef void (*nros_platform_log_flush_fn)(void);

static nros_platform_log_writer_fn s_log_writer = NULL;
static nros_platform_log_flush_fn  s_log_flusher = NULL;

/* Board-crate hook. NULL flusher = writer is fully synchronous. */
void nros_platform_register_log_writer(nros_platform_log_writer_fn writer,
                                       nros_platform_log_flush_fn  flusher) {
    s_log_writer  = writer;
    s_log_flusher = flusher;
}

void nros_platform_log_write(uint8_t severity,
                             const uint8_t *name_ptr, uintptr_t name_len,
                             const uint8_t *msg_ptr,  uintptr_t msg_len) {
    nros_platform_log_writer_fn writer = s_log_writer;
    if (writer == NULL) {
        return;
    }
    writer(severity, name_ptr, name_len, msg_ptr, msg_len);
}

void nros_platform_log_flush(void) {
    nros_platform_log_flush_fn flusher = s_log_flusher;
    if (flusher != NULL) {
        flusher();
    }
}

/* ---- Fatal error (phase-366 / RFC-0077) ----
 *
 * ThreadX has no fatal primitive of its own — no `k_panic`, no `PANIC()`. The
 * kernel's model is that an application checks return codes, so the ending has
 * to be built here.
 *
 * Text goes through the REGISTERED WRITER rather than the log ABI's usual entry:
 * the writer is a raw function pointer the board installed (UART, semihosting,
 * stderr), so it needs no scheduler and is safe with interrupts disabled, which
 * is what this contract demands. Severity 5 = Fatal.
 *
 * The two supported ThreadX ports want different endings, and the discriminator
 * is whether the image is hosted:
 *
 *   - hosted (threadx-linux, ThreadX-over-pthreads): `exit(1)`. A test harness
 *     watching this process gets a status; spinning would make it hang until the
 *     harness's timeout, turning a clear failure into a slow one.
 *   - bare metal (threadx-riscv64): disable interrupts and halt, the only thing
 *     that is true everywhere. A board with a reset controller or a debugger
 *     probe should override this symbol strongly and use it.
 */
__attribute__((weak))
_Noreturn void nros_platform_panic(const char *msg, size_t len) {
    nros_platform_log_writer_fn writer = s_log_writer;
    if (writer != NULL) {
        static const uint8_t kName[] = "nros";
        writer(5, kName, sizeof(kName) - 1,
               (const uint8_t *) msg, (uintptr_t) (msg != NULL ? len : 0));
        nros_platform_log_flush_fn flusher = s_log_flusher;
        if (flusher != NULL) {
            flusher();
        }
    }
#if defined(__linux__)
    /* Declared locally rather than pulling <stdlib.h> into this TU: the
     * bare-metal port compiles the same file with a freestanding libc where the
     * header may not exist. */
    extern _Noreturn void exit(int status);
    exit(1);
#else
    TX_INTERRUPT_SAVE_AREA
    TX_DISABLE
    for (;;) {
    }
#endif
}

/* ThreadX keeps `tx_thread_stack_highest_ptr` only when built with
 * TX_ENABLE_STACK_CHECKING; without it the field is not maintained and any
 * number derived from it would be fiction. Headroom is the highest pointer
 * minus the stack start. */
size_t nros_platform_task_stack_unused_bytes(void) {
#ifdef TX_ENABLE_STACK_CHECKING
    TX_THREAD *self = tx_thread_identify();
    if (self == TX_NULL || self->tx_thread_stack_highest_ptr == TX_NULL ||
        self->tx_thread_stack_start == TX_NULL) {
        return 0;
    }
    return (size_t) ((char *) self->tx_thread_stack_highest_ptr -
                     (char *) self->tx_thread_stack_start);
#else
    return 0;
#endif
}

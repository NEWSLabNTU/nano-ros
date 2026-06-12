#ifndef NROS_PLATFORM_H
#define NROS_PLATFORM_H

#include <stdint.h>
#include <stddef.h>

/**
 * @file platform.h
 * @brief Canonical C ABI for the nros platform abstraction.
 *
 * RFC-0042 D1 / phase-241 wave B — this is THE single canonical platform header,
 * owned by `nros-platform-api` (the lowest crate, no deps). `nros-c` and
 * `nros-platform-cffi` re-export it, so neither package's consumers need the
 * other's include dir (it breaks the historical nros-c↔cffi header tangle, and
 * there is exactly one file named `<nros/platform.h>` — no include-order race).
 *
 * A platform implementor supplies the symbols declared here. Every nros binary
 * links exactly one platform implementation; resolution is at link time — no
 * runtime registration. Implementations may be any language with a C ABI; for
 * Rust platform crates, `nros-platform-cffi` re-exports the Rust impl as
 * `#[unsafe(no_mangle)] extern "C"` symbols matching the names below (its
 * `src/lib.rs` extern block is the hand-written mirror, guarded byte-for-byte by
 * `c_stub_platform.rs`).
 *
 * Companion to the canonical-C-ABI RMW vtable (`<nros/rmw_vtable.h>`); the
 * platform layer sits one tier below RMW.
 *
 * # Return-value conventions
 *
 *  - `int8_t` returns: `0` = success, non-zero = error.
 *  - Pointer returns: `NULL` = allocation failure or not-implemented;
 *    non-`NULL` is the resource handle.
 *  - `clock_*` / `time_*` returns are absolute / monotonic counters and
 *    never error. If the platform has no clock, return `0`.
 *
 * # Threading
 *
 * All symbols must be safe to invoke from any thread.
 * `mutex_*` / `condvar_*` must be safe under concurrent callers.
 * `mutex_rec_*` must support same-thread re-entry (zenoh-pico
 * re-enters the same mutex).
 *
 * RTOS yields (`yield_now`) are **not** ISR-safe. Bare-metal yields
 * built on `core::hint::spin_loop()` are.
 */

/* ---- Platform capability defaults (RFC-0042 D2 / phase-241 B + C) ----
 *
 * Platform-*constant* capabilities. The single source of truth for a board's
 * *variable* capabilities is its `nros-board.toml` `[board.capabilities]`, lowered
 * to `-DNROS_PLATFORM_HAS_MALLOC` (etc.) by the build (phase-241 C.2). These
 * blocks supply only the per-platform constants, and never override a `-D` the
 * board already set (every `#define` is `#ifndef`-guarded).
 *
 *  - `NROS_PLATFORM_HAS_MALLOC` gates the canonical `malloc`/`free` shim below.
 *    Absent → a TU using the nros-cpp heap containers fails to *compile* (not
 *    link) — the issue-0038 guard.
 *  - `NROS_PLATFORM_HAS_ATOMICS` is true on every supported target today.
 *
 * RTOS-config-derived capabilities are intentionally NOT defaulted here:
 *  - FreeRTOS heap ← `configSUPPORT_DYNAMIC_ALLOCATION` (FreeRTOSConfig.h),
 *  - Zephyr heap/mutex ← `CONFIG_HEAP_MEM_POOL_SIZE` / `CONFIG_MULTITHREADING`.
 *    Those boards declare `heap = true` in board.toml and the build lowers it to
 *    the Kconfig/FreeRTOSConfig knob, keeping the C view tied to the RTOS config.
 */
/* HAS_ATOMICS is a constant on every supported target. */
#ifndef NROS_PLATFORM_HAS_ATOMICS
#  define NROS_PLATFORM_HAS_ATOMICS
#endif

/* HAS_MALLOC by platform. POSIX (and the POSIX-mapped hosted platforms: NuttX,
 * ThreadX-linux, native) and the heap RTOSes (Zephyr, FreeRTOS) always run with
 * an allocator under nros — the generator always configures the RTOS heap
 * (`CONFIG_HEAP_MEM_POOL_SIZE`, `configSUPPORT_DYNAMIC_ALLOCATION`), and each
 * declares `heap = true` in its `nros-board.toml`. They get the canonical
 * malloc/free unconditionally here.
 *
 * Bare-metal / ThreadX-RV64 / ESP / custom do NOT default a heap: they opt in
 * via the board.toml-derived `-DNROS_PLATFORM_HAS_MALLOC` (phase-241 C.2), so a
 * genuinely heap-less board still fails to compile a heap container (the #38
 * compile-gate). */
#if defined(NROS_PLATFORM_POSIX) || defined(NROS_PLATFORM_ZEPHYR) \
    || defined(NROS_PLATFORM_FREERTOS)
#  ifndef NROS_PLATFORM_HAS_MALLOC
#    define NROS_PLATFORM_HAS_MALLOC
#  endif
#endif

#ifdef __cplusplus
extern "C" {
#endif

/* ---- Return codes ---- */

typedef int32_t nros_platform_ret_t;

/** Operation completed successfully. */
#define NROS_PLATFORM_RET_OK              ((nros_platform_ret_t) 0)
/** Generic failure not covered by a more specific code. */
#define NROS_PLATFORM_RET_ERROR           ((nros_platform_ret_t) -1)
/** The platform does not implement this operation. */
#define NROS_PLATFORM_RET_UNSUPPORTED     ((nros_platform_ret_t) -5)

/* ---- Clock (monotonic) ---- */

/** Monotonic milliseconds since a platform-defined epoch (boot, program
 *  start, …). Never decreases. Wraps after ~584 million years. */
uint64_t nros_platform_clock_ms(void);

/** Monotonic microseconds since the same epoch as `nros_platform_clock_ms`.
 *  Used for fine-grained spin / wait deadlines. */
uint64_t nros_platform_clock_us(void);

/* ---- Allocation ---- */

/** Allocate `size` bytes; return `NULL` on failure. May be called from
 *  any thread. */
void *nros_platform_alloc(size_t size);

/** Resize the block at `ptr` to `size` bytes. Equivalent to libc
 *  `realloc`: `NULL` ptr → fresh alloc; `0` size → free + return `NULL`.
 *  Preserves contents up to `min(old, new)`. */
void *nros_platform_realloc(void *ptr, size_t size);

/** Free a previously allocated block. `NULL` is a no-op. */
void nros_platform_dealloc(void *ptr);

/* Canonical `malloc`/`free` C-ABI surface (issue-0038). nros-cpp's heap
 * containers (`heap_string.hpp`, `heap_sequence.hpp`) allocate through
 * `nros_platform_malloc` / `nros_platform_free` so C and C++ share one
 * allocator. Defined ONCE here (replacing the former 5 per-header copies) as a
 * thin forward to the platform `alloc`/`dealloc` funnel (RFC-0034 D6), GATED on
 * the heap capability: a board without `NROS_PLATFORM_HAS_MALLOC` does not get
 * the canonical malloc/free, so using a heap container on a heap-less board is a
 * compile error (the 241.A gate), not a latent link failure. */
#ifdef NROS_PLATFORM_HAS_MALLOC
static inline void *nros_platform_malloc(size_t size) {
    return nros_platform_alloc(size);
}

static inline void nros_platform_free(void *ptr) {
    nros_platform_dealloc(ptr);
}
#endif /* NROS_PLATFORM_HAS_MALLOC */

/** Bytes currently allocated from the platform heap, or `0` if the port
 *  does not instrument it. Phase 230 / RFC-0034 D7: the true unified figure
 *  where the platform owns one kernel heap shared by the C side and the
 *  Rust `#[global_allocator]`. */
size_t nros_platform_heap_used_bytes(void);

/** Total managed heap size in bytes (used + free), or `0` if unknown. */
size_t nros_platform_heap_total_bytes(void);

/* ---- Sleep ---- */

/** Sleep at least `us` microseconds. Spin if the platform clock has no
 *  sub-millisecond timer. */
void nros_platform_sleep_us(size_t us);

/** Sleep at least `ms` milliseconds. */
void nros_platform_sleep_ms(size_t ms);

/** Sleep at least `s` seconds. */
void nros_platform_sleep_s(size_t s);

/* ---- Cooperative yield ---- */

/** Voluntarily yield the current task / thread. On bare-metal,
 *  `core::hint::spin_loop()` is acceptable; on RTOSes use the native
 *  cooperative-yield primitive (`k_yield`, `vPortYield`,
 *  `tx_thread_relinquish`, `sched_yield`, …). RTOS yields are **not**
 *  ISR-safe. */
void nros_platform_yield_now(void);

/* ---- Random ---- */

/** Random `u8`. Cryptographically random where the platform has an
 *  entropy source; otherwise a seeded PRNG. Must be deterministic
 *  within a single test session for reproducibility. */
uint8_t  nros_platform_random_u8(void);
/** Random `u16`. See `random_u8` notes. */
uint16_t nros_platform_random_u16(void);
/** Random `u32`. See `random_u8` notes. */
uint32_t nros_platform_random_u32(void);
/** Random `u64`. See `random_u8` notes. */
uint64_t nros_platform_random_u64(void);
/** Fill `len` bytes at `buf` with random data. */
void     nros_platform_random_fill(void *buf, size_t len);

/* ---- Wall clock ---- */

/** Wall-clock milliseconds since the Unix epoch, or `0` if the platform
 *  has no real-time clock. */
uint64_t nros_platform_time_now_ms(void);

/** Whole seconds since the Unix epoch (truncated `time_now_ms`). */
uint32_t nros_platform_time_since_epoch_secs(void);

/** Sub-second nanosecond component of the wall clock (`0..1e9`). */
uint32_t nros_platform_time_since_epoch_nanos(void);

/* ---- Threading: tasks ---- */

/** Spawn a new task. `task` is opaque caller-provided storage (size
 *  determined by the implementor); `attr` carries scheduling hints
 *  (priority, stack size, …) or is `NULL` for defaults; `entry` is the
 *  task entry point; `arg` is forwarded to `entry`. Returns `0` on
 *  success, non-zero on failure. */
int8_t nros_platform_task_init(void *task, void *attr,
                               void *(*entry)(void *), void *arg);

/** Block until `task` exits. Cleans up task storage on success. */
int8_t nros_platform_task_join(void *task);

/** Mark `task` as detached — its storage is reclaimed on exit without
 *  a join. */
int8_t nros_platform_task_detach(void *task);

/** Request `task` to terminate at the next cancellation point.
 *  Cooperative: a task that never reaches a cancel point will not stop. */
int8_t nros_platform_task_cancel(void *task);

/** Terminate the calling task immediately. Does not return. */
void nros_platform_task_exit(void);

/** Free task storage allocated by `task_init`. Called after `task_join`
 *  or `task_detach + exit`. */
void nros_platform_task_free(void **task);

/* ---- Threading: non-recursive mutex ---- */

int8_t nros_platform_mutex_init(void *m);
int8_t nros_platform_mutex_drop(void *m);
int8_t nros_platform_mutex_lock(void *m);
int8_t nros_platform_mutex_try_lock(void *m);
int8_t nros_platform_mutex_unlock(void *m);

/* ---- Threading: recursive mutex ---- */

/** Initialise a recursive mutex (same-thread re-entry permitted).
 *  Required by zenoh-pico. */
int8_t nros_platform_mutex_rec_init(void *m);
int8_t nros_platform_mutex_rec_drop(void *m);
int8_t nros_platform_mutex_rec_lock(void *m);
int8_t nros_platform_mutex_rec_try_lock(void *m);
int8_t nros_platform_mutex_rec_unlock(void *m);

/* ---- Threading: condition variables ---- */

int8_t nros_platform_condvar_init(void *cv);
int8_t nros_platform_condvar_drop(void *cv);
int8_t nros_platform_condvar_signal(void *cv);
int8_t nros_platform_condvar_signal_all(void *cv);

/** Phase 124.B.7.a — ISR-safe signal.
 *
 *  Callable from interrupt context. `nros_platform_condvar_signal`
 *  is NOT ISR-safe on every platform (POSIX `pthread_cond_signal`
 *  isn't on the async-signal-safe function list; RTOS condvar
 *  primitives often require thread context). Backends MUST use
 *  this variant when triggering from an ISR or POSIX signal handler.
 *
 *  Per-platform implementation:
 *  * POSIX: `eventfd`/`pipe` write — async-signal-safe; a runtime
 *    worker thread forwards to the underlying condvar.
 *  * Zephyr: `k_sem_give` on the wake semaphore (ISR-safe).
 *  * FreeRTOS: `xSemaphoreGiveFromISR` + `portYIELD_FROM_ISR` on
 *    the wake semaphore.
 *  * NuttX: `sem_post` (POSIX-safe under NuttX) on the wake sem.
 *  * ThreadX: `tx_event_flags_set` on the wake event flag group
 *    (ISR-safe).
 *  * Bare-metal: atomic flag store + `__SEV()` (Cortex-M).
 *
 *  Returns non-zero on error (e.g. ISR-unsafe call on a backend
 *  that mandates ISR-context-only via a separate primitive).
 *  Backends without an ISR-safe path return non-zero so callers
 *  can fall back to thread-context signal (with the obvious
 *  latency cost). */
int8_t nros_platform_condvar_signal_from_isr(void *cv);

/** Atomically release `m` and block on `cv`. The mutex is re-acquired
 *  before this function returns. */
int8_t nros_platform_condvar_wait(void *cv, void *m);

/** Like `condvar_wait`, but with an absolute monotonic deadline (in
 *  `clock_ms` units). Returns non-zero on timeout. */
int8_t nros_platform_condvar_wait_until(void *cv, void *m, uint64_t abstime);

/* ---- Threading: wake primitive (Phase 130) ----
 *
 * Binary-semaphore-shaped primitive used by the executor's wake_flag /
 * spin_once cv-wait pair. Separate from `nros_platform_condvar_*` so
 * the executor doesn't inherit zenoh-pico's pthread-shaped Zephyr
 * ABI (which on libc hangs past `pthread_cond_timedwait` deadlines).
 *
 * Per-platform impl:
 *  * POSIX:    `sem_t` with `sem_timedwait` (`CLOCK_MONOTONIC`).
 *  * Zephyr:   `k_sem` (kernel-native, libc-pthread-free).
 *  * FreeRTOS: `xSemaphoreCreateBinary` + `xSemaphoreGiveFromISR`.
 *  * NuttX:    POSIX `sem_t` via NuttX libc (`sem_timedwait`).
 *  * ThreadX:  `tx_semaphore` (ISR-safe `tx_semaphore_put`).
 *  * bare:     atomic flag + busy-spin against the platform clock.
 *
 * `wait_ms` returns 0 on signal, 1 on timeout, -1 on error.
 * `signal_from_isr` returns -1 if the backend has no ISR-safe path;
 * callers may fall back to `signal()` accepting the latency cost.
 * Storage is opaque to the caller; size/alignment via the probe
 * helpers below. */
int8_t  nros_platform_wake_init(void *w);
int8_t  nros_platform_wake_drop(void *w);
int8_t  nros_platform_wake_wait_ms(void *w, uint32_t timeout_ms);
int8_t  nros_platform_wake_signal(void *w);
int8_t  nros_platform_wake_signal_from_isr(void *w);

/** Opaque-storage sizing. Both helpers are pure functions (no global
 *  state) and may be called before `nros_platform_wake_init`. */
size_t  nros_platform_wake_storage_size(void);
size_t  nros_platform_wake_storage_align(void);

/* ---- Critical section ---- */
/* Phase 121.9 — global mutual exclusion against preemption + ISR
 * delivery. Backs the Rust `critical_section::Impl` registration used
 * by DDS, nros-rmw-{xrce,zenoh}, and any consumer of
 * `critical_section::with()`. Reentrant by contract: every acquire
 * must be paired with exactly one release, and the platform handles
 * nesting (PRIMASK already stacks; pthread side uses a recursive
 * mutex).
 *
 * The `uint32_t` token holds whatever the platform needs to restore
 * the prior posture — Cortex-M PRIMASK bit, Cortex-R CPSR I-bit,
 * RISC-V `mstatus.MIE` snapshot, pthread depth counter — and is
 * opaque to the caller. */
uint32_t nros_platform_critical_section_acquire(void);
void     nros_platform_critical_section_release(uint32_t token);

/* ---- Logging (Phase 88) ---- */
/* Per-platform leveled log delivery, matching the post-Phase-129
 * pattern: portable facade (`nros-log`) formats into a buffer and
 * hands the rendered text to whichever `nros-platform-<rtos>` is
 * linked in. POSIX writes to stderr; Zephyr routes through
 * `LOG_MODULE_DECLARE(nros)` (or `printk` fallback); ESP-IDF goes
 * through `esp_log_write`; NuttX through `syslog`; FreeRTOS /
 * ThreadX / bare-metal expose a `register_log_writer(...)` helper
 * in their platform crate so boards register the actual writer
 * (UART / semihosting / defmt / RTT) once at startup.
 *
 * `severity`: matches `nros_log::Severity::as_u8()`:
 *   0 = Trace, 1 = Debug, 2 = Info, 3 = Warn, 4 = Error, 5 = Fatal.
 * Implementors should map onto the platform's nearest level.
 *
 * `name_ptr` / `name_len`: logger name (NOT null-terminated; UTF-8).
 *   May be empty (the catch-all logger).
 * `msg_ptr`  / `msg_len`:  already-formatted message body (NOT null-
 *   terminated; UTF-8). Implementors that need a `CStr` must copy
 *   into their own buffer + append `'\0'`.
 *
 * No return value: log delivery never fails from the caller's POV.
 * Platforms drop silently on internal overflow (e.g. RTT ring full).
 *
 * `nros_platform_log_flush`: best-effort drain of any internal
 * buffer (RTT, syslog buffered chan, etc.). Default impl = no-op.
 *
 * Thread / ISR safety is documented per platform — see the table
 * in `docs/roadmap/phase-88-nros-log.md`. The ABI itself is
 * synchronous + reentrant-safe at the caller layer (a recursion
 * guard in `nros-log` prevents fan-out re-entry). */
void nros_platform_log_write(
    uint8_t        severity,
    const uint8_t *name_ptr, uintptr_t name_len,
    const uint8_t *msg_ptr,  uintptr_t msg_len);
void nros_platform_log_flush(void);

/* Board-supplied writer hook. ONLY meaningful on platforms whose
 * `nros_platform_log_write` impl is itself a thin dispatcher to a
 * board-registered fn (FreeRTOS, ThreadX, bare-metal). On platforms
 * with a native logger (POSIX, Zephyr, ESP-IDF, NuttX), the symbol
 * is absent and the board should not link against it.
 *
 * Board crates call this ONCE at startup, before any task / thread
 * begins logging. Re-calling replaces the registration.
 *
 * `writer` matches the signature of `nros_platform_log_write`.
 * `flusher` is optional (pass NULL for fully-synchronous writers). */
typedef void (*nros_platform_log_writer_fn_t)(
    uint8_t        severity,
    const uint8_t *name_ptr, uintptr_t name_len,
    const uint8_t *msg_ptr,  uintptr_t msg_len);

typedef void (*nros_platform_log_flush_fn_t)(void);

void nros_platform_register_log_writer(
    nros_platform_log_writer_fn_t writer,
    nros_platform_log_flush_fn_t  flusher);

#ifdef __cplusplus
}  /* extern "C" */
#endif

#endif /* NROS_PLATFORM_H */

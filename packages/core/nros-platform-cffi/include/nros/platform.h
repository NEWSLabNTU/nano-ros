#ifndef NROS_PLATFORM_H
#define NROS_PLATFORM_H

#include <stdint.h>
#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

/**
 * @file platform.h
 * @brief Canonical C ABI for the nros platform abstraction.
 *
 * A platform implementor supplies the symbols declared in this header.
 * Every nros binary links exactly one platform implementation; resolution
 * is performed at link time. There is no runtime registration step.
 *
 * Implementations may be written in any language with a C ABI. For Rust
 * platform crates, a sibling `-cffi` shim crate re-exports the Rust impl
 * as `#[unsafe(no_mangle)] extern "C"` symbols matching the names below.
 *
 * Companion to the Phase 117 canonical-C-ABI RMW vtable
 * (`<nros/rmw_vtable.h>`); the platform layer sits one tier below RMW.
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

/** Atomically release `m` and block on `cv`. The mutex is re-acquired
 *  before this function returns. */
int8_t nros_platform_condvar_wait(void *cv, void *m);

/** Like `condvar_wait`, but with an absolute monotonic deadline (in
 *  `clock_ms` units). Returns non-zero on timeout. */
int8_t nros_platform_condvar_wait_until(void *cv, void *m, uint64_t abstime);

#ifdef __cplusplus
}  /* extern "C" */
#endif

#endif /* NROS_PLATFORM_H */

#ifndef NROS_PLATFORM_TIMER_H
#define NROS_PLATFORM_TIMER_H

#include <stdint.h>
#include <stddef.h>

#include "nros/platform.h"

#ifdef __cplusplus
extern "C" {
#endif

/**
 * @file platform_timer.h
 * @brief Canonical C ABI for the nros platform timer surface.
 *
 * Companion to `<nros/platform.h>`; sits beside the core 39-symbol
 * ABI as the Phase 110.E platform-timer interface. Carved into a
 * separate header because bare-metal / single-shot consumers can
 * omit timer support without losing the canonical platform symbols.
 *
 * # Handle model
 *
 *  Platform timer handles are opaque `void *`. The implementation
 *  allocates whatever it needs to track the underlying primitive
 *  (`TimerHandle_t` on FreeRTOS, `TX_TIMER *` on ThreadX, `timer_t`
 *  on POSIX, `*mut k_timer` on Zephyr); the runtime never inspects
 *  the bytes pointed at.
 *
 * # Return-value conventions
 *
 *  - `create_periodic` / `create_oneshot` return non-NULL on success
 *    and `NULL` on failure (analogous to libc `malloc`).
 *  - `cancel` returns 1 when the cancellation prevented the callback
 *    from firing, 0 if the callback already fired (or the timer was
 *    already cancelled or destroyed), -1 on unrecoverable error.
 *  - `destroy` is best-effort; idempotent on already-destroyed handles.
 *
 * # Threading / context
 *
 *  Callbacks are invoked from a platform-defined timer context — a
 *  direct ISR on Zephyr / bare-metal, deferred to the FreeRTOS timer
 *  task on FreeRTOS, a real-time signal on POSIX. Callback bodies
 *  must be short, must not call back into nros_platform_* heap or
 *  blocking primitives, and must use atomic operations for shared
 *  state. `user_data` must outlive the timer handle.
 */

typedef void (*nros_platform_timer_callback_t)(void *user_data);

/** Register a periodic timer that invokes `callback(user_data)` every
 *  `period_us` microseconds. Returns the platform-native handle on
 *  success, NULL on failure. */
void *nros_platform_timer_create_periodic(uint32_t period_us,
                                          nros_platform_timer_callback_t callback,
                                          void *user_data);

/** Register a one-shot timer that invokes `callback(user_data)` once
 *  after `timeout_us` microseconds. Returns the platform-native
 *  handle on success, NULL on failure. */
void *nros_platform_timer_create_oneshot(uint32_t timeout_us,
                                         nros_platform_timer_callback_t callback,
                                         void *user_data);

/** Cancel + free the timer. Drains in-flight callback invocations
 *  before returning so `user_data` is no longer accessed. Idempotent
 *  on already-destroyed handles. */
void nros_platform_timer_destroy(void *handle);

/** Cancel a previously-armed timer. Returns:
 *
 *   - 1 if cancellation prevented the callback from firing,
 *   - 0 if the callback already fired (or the timer was already
 *     cancelled / destroyed),
 *   - -1 on unrecoverable error.
 *
 *  Distinct from `destroy` — the handle remains valid after
 *  `cancel` and may be re-armed by the application's own
 *  bookkeeping if the implementation supports it. */
int8_t nros_platform_timer_cancel(void *handle);

#ifdef __cplusplus
}  /* extern "C" */
#endif

#endif /* NROS_PLATFORM_TIMER_H */

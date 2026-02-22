/**
 * nros timer API
 *
 * Timer creation and management functions.
 *
 * Copyright 2024 nros contributors
 * Licensed under Apache-2.0
 */

#ifndef NROS_TIMER_H
#define NROS_TIMER_H

#include "nros/types.h"
#include "nros/visibility.h"
#include "nros/init.h"

#ifdef __cplusplus
extern "C" {
#endif

// ============================================================================
// Timer State
// ============================================================================

/** Timer state */
typedef enum nros_timer_state_t {
    /** Not initialized */
    NROS_TIMER_STATE_UNINITIALIZED = 0,
    /** Initialized and running */
    NROS_TIMER_STATE_RUNNING = 1,
    /** Initialized but canceled */
    NROS_TIMER_STATE_CANCELED = 2,
    /** Shutdown */
    NROS_TIMER_STATE_SHUTDOWN = 3,
} nros_timer_state_t;

// ============================================================================
// Timer Callback
// ============================================================================

// Forward declaration
struct nros_timer_t;

/**
 * Timer callback function type.
 *
 * @param timer Pointer to the timer that triggered
 * @param context User-provided context pointer
 */
typedef void (*nros_timer_callback_t)(
    struct nros_timer_t *timer,
    void *context);

// ============================================================================
// Timer Structure
// ============================================================================

/**
 * Timer structure.
 *
 * IMPORTANT: This struct layout must match the Rust `nros_timer_t` in
 * `packages/core/nros-c/src/timer.rs` exactly (field order, types, sizes).
 */
typedef struct nros_timer_t {
    /** Current state */
    nros_timer_state_t state;
    /** Period in nanoseconds */
    uint64_t period_ns;
    /** Last trigger time in nanoseconds */
    uint64_t last_call_time_ns;
    /** User callback function */
    nros_timer_callback_t callback;
    /** User context pointer */
    void *context;
    /** Pointer to parent support context */
    const nros_support_t *support;
    /** Handle ID from executor registration (internal, do not touch) */
    size_t _handle_id;
    /** Opaque pointer to internal executor (internal, do not touch) */
    void *_executor;
} nros_timer_t;

// ============================================================================
// Timer Functions
// ============================================================================

/**
 * Get a zero-initialized timer.
 *
 * @return Zero-initialized timer structure
 */
NROS_PUBLIC
nros_timer_t nros_timer_get_zero_initialized(void);

/**
 * Initialize a timer.
 *
 * @param timer Pointer to a zero-initialized timer
 * @param support Pointer to an initialized support context
 * @param period_ns Timer period in nanoseconds
 * @param callback Callback function to invoke when timer fires
 * @param context User context pointer passed to callback (can be NULL)
 *
 * @return NROS_RET_OK on success
 * @return NROS_RET_INVALID_ARGUMENT if any required pointer is NULL or period is 0
 * @return NROS_RET_NOT_INIT if support is not initialized
 */
NROS_PUBLIC NROS_WARN_UNUSED
nros_ret_t nros_timer_init(
    nros_timer_t *timer,
    const nros_support_t *support,
    uint64_t period_ns,
    nros_timer_callback_t callback,
    void *context);

/**
 * Cancel a timer.
 *
 * A canceled timer will not fire, but can be reset to start again.
 *
 * @param timer Pointer to an initialized timer
 *
 * @return NROS_RET_OK on success
 * @return NROS_RET_INVALID_ARGUMENT if timer is NULL
 * @return NROS_RET_NOT_INIT if not initialized
 */
NROS_PUBLIC NROS_WARN_UNUSED
nros_ret_t nros_timer_cancel(nros_timer_t *timer);

/**
 * Reset a timer.
 *
 * This resets the timer's last call time and starts it running again
 * if it was canceled.
 *
 * @param timer Pointer to an initialized timer
 *
 * @return NROS_RET_OK on success
 * @return NROS_RET_INVALID_ARGUMENT if timer is NULL
 * @return NROS_RET_NOT_INIT if not initialized
 */
NROS_PUBLIC NROS_WARN_UNUSED
nros_ret_t nros_timer_reset(nros_timer_t *timer);

/**
 * Finalize a timer.
 *
 * @param timer Pointer to an initialized timer
 *
 * @return NROS_RET_OK on success
 * @return NROS_RET_INVALID_ARGUMENT if timer is NULL
 * @return NROS_RET_NOT_INIT if not initialized
 */
NROS_PUBLIC NROS_WARN_UNUSED
nros_ret_t nros_timer_fini(nros_timer_t *timer);

/**
 * Check if timer is ready to fire.
 *
 * @param timer Pointer to an initialized timer
 * @param current_time_ns Current time in nanoseconds
 *
 * @return Non-zero if timer is ready, 0 otherwise
 */
NROS_PUBLIC
int nros_timer_is_ready(const nros_timer_t *timer, uint64_t current_time_ns);

/**
 * Call the timer callback and update last call time.
 *
 * This is called by the executor when the timer is ready.
 *
 * @param timer Pointer to an initialized timer
 * @param current_time_ns Current time in nanoseconds
 *
 * @return NROS_RET_OK on success
 * @return NROS_RET_INVALID_ARGUMENT if timer is NULL
 * @return NROS_RET_NOT_INIT if not initialized or not running
 */
NROS_PUBLIC NROS_WARN_UNUSED
nros_ret_t nros_timer_call(nros_timer_t *timer, uint64_t current_time_ns);

/**
 * Check if timer is valid (initialized and not shutdown).
 *
 * @param timer Pointer to a timer
 *
 * @return Non-zero if valid, 0 if invalid or NULL
 */
NROS_PUBLIC
int nros_timer_is_valid(const nros_timer_t *timer);

/**
 * Get the timer period in nanoseconds.
 *
 * @param timer Pointer to a timer
 *
 * @return Period in nanoseconds, or 0 if invalid
 */
NROS_PUBLIC
uint64_t nros_timer_get_period(const nros_timer_t *timer);

/**
 * Get the time until next timer firing.
 *
 * @param timer Pointer to a timer
 * @param current_time_ns Current time in nanoseconds
 *
 * @return Time until next firing in nanoseconds, or 0 if ready now or invalid
 */
NROS_PUBLIC
uint64_t nros_timer_get_time_until_next_call(
    const nros_timer_t *timer,
    uint64_t current_time_ns);

#ifdef __cplusplus
}
#endif

#endif // NROS_TIMER_H

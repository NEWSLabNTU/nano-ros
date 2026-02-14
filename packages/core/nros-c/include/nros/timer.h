/**
 * nros timer API
 *
 * Timer creation and management functions.
 *
 * Copyright 2024 nros contributors
 * Licensed under Apache-2.0
 */

#ifndef NANO_ROS_TIMER_H
#define NANO_ROS_TIMER_H

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
typedef enum nano_ros_timer_state_t {
    /** Not initialized */
    NANO_ROS_TIMER_STATE_UNINITIALIZED = 0,
    /** Initialized and running */
    NANO_ROS_TIMER_STATE_RUNNING = 1,
    /** Initialized but canceled */
    NANO_ROS_TIMER_STATE_CANCELED = 2,
    /** Shutdown */
    NANO_ROS_TIMER_STATE_SHUTDOWN = 3,
} nano_ros_timer_state_t;

// ============================================================================
// Timer Callback
// ============================================================================

// Forward declaration
struct nano_ros_timer_t;

/**
 * Timer callback function type.
 *
 * @param timer Pointer to the timer that triggered
 * @param context User-provided context pointer
 */
typedef void (*nano_ros_timer_callback_t)(
    struct nano_ros_timer_t *timer,
    void *context);

// ============================================================================
// Timer Structure
// ============================================================================

/** Timer structure */
typedef struct nano_ros_timer_t {
    /** Current state */
    nano_ros_timer_state_t state;
    /** Period in nanoseconds */
    uint64_t period_ns;
    /** Last trigger time in nanoseconds */
    uint64_t last_call_time_ns;
    /** User callback function */
    nano_ros_timer_callback_t callback;
    /** User context pointer */
    void *context;
    /** Pointer to parent support context */
    const nano_ros_support_t *support;
} nano_ros_timer_t;

// ============================================================================
// Timer Functions
// ============================================================================

/**
 * Get a zero-initialized timer.
 *
 * @return Zero-initialized timer structure
 */
NANO_ROS_PUBLIC
nano_ros_timer_t nano_ros_timer_get_zero_initialized(void);

/**
 * Initialize a timer.
 *
 * @param timer Pointer to a zero-initialized timer
 * @param support Pointer to an initialized support context
 * @param period_ns Timer period in nanoseconds
 * @param callback Callback function to invoke when timer fires
 * @param context User context pointer passed to callback (can be NULL)
 *
 * @return NANO_ROS_RET_OK on success
 * @return NANO_ROS_RET_INVALID_ARGUMENT if any required pointer is NULL or period is 0
 * @return NANO_ROS_RET_NOT_INIT if support is not initialized
 */
NANO_ROS_PUBLIC NANO_ROS_WARN_UNUSED
nano_ros_ret_t nano_ros_timer_init(
    nano_ros_timer_t *timer,
    const nano_ros_support_t *support,
    uint64_t period_ns,
    nano_ros_timer_callback_t callback,
    void *context);

/**
 * Cancel a timer.
 *
 * A canceled timer will not fire, but can be reset to start again.
 *
 * @param timer Pointer to an initialized timer
 *
 * @return NANO_ROS_RET_OK on success
 * @return NANO_ROS_RET_INVALID_ARGUMENT if timer is NULL
 * @return NANO_ROS_RET_NOT_INIT if not initialized
 */
NANO_ROS_PUBLIC NANO_ROS_WARN_UNUSED
nano_ros_ret_t nano_ros_timer_cancel(nano_ros_timer_t *timer);

/**
 * Reset a timer.
 *
 * This resets the timer's last call time and starts it running again
 * if it was canceled.
 *
 * @param timer Pointer to an initialized timer
 *
 * @return NANO_ROS_RET_OK on success
 * @return NANO_ROS_RET_INVALID_ARGUMENT if timer is NULL
 * @return NANO_ROS_RET_NOT_INIT if not initialized
 */
NANO_ROS_PUBLIC NANO_ROS_WARN_UNUSED
nano_ros_ret_t nano_ros_timer_reset(nano_ros_timer_t *timer);

/**
 * Finalize a timer.
 *
 * @param timer Pointer to an initialized timer
 *
 * @return NANO_ROS_RET_OK on success
 * @return NANO_ROS_RET_INVALID_ARGUMENT if timer is NULL
 * @return NANO_ROS_RET_NOT_INIT if not initialized
 */
NANO_ROS_PUBLIC NANO_ROS_WARN_UNUSED
nano_ros_ret_t nano_ros_timer_fini(nano_ros_timer_t *timer);

/**
 * Check if timer is ready to fire.
 *
 * @param timer Pointer to an initialized timer
 * @param current_time_ns Current time in nanoseconds
 *
 * @return Non-zero if timer is ready, 0 otherwise
 */
NANO_ROS_PUBLIC
int nano_ros_timer_is_ready(const nano_ros_timer_t *timer, uint64_t current_time_ns);

/**
 * Call the timer callback and update last call time.
 *
 * This is called by the executor when the timer is ready.
 *
 * @param timer Pointer to an initialized timer
 * @param current_time_ns Current time in nanoseconds
 *
 * @return NANO_ROS_RET_OK on success
 * @return NANO_ROS_RET_INVALID_ARGUMENT if timer is NULL
 * @return NANO_ROS_RET_NOT_INIT if not initialized or not running
 */
NANO_ROS_PUBLIC NANO_ROS_WARN_UNUSED
nano_ros_ret_t nano_ros_timer_call(nano_ros_timer_t *timer, uint64_t current_time_ns);

/**
 * Check if timer is valid (initialized and not shutdown).
 *
 * @param timer Pointer to a timer
 *
 * @return Non-zero if valid, 0 if invalid or NULL
 */
NANO_ROS_PUBLIC
int nano_ros_timer_is_valid(const nano_ros_timer_t *timer);

/**
 * Get the timer period in nanoseconds.
 *
 * @param timer Pointer to a timer
 *
 * @return Period in nanoseconds, or 0 if invalid
 */
NANO_ROS_PUBLIC
uint64_t nano_ros_timer_get_period(const nano_ros_timer_t *timer);

/**
 * Get the time until next timer firing.
 *
 * @param timer Pointer to a timer
 * @param current_time_ns Current time in nanoseconds
 *
 * @return Time until next firing in nanoseconds, or 0 if ready now or invalid
 */
NANO_ROS_PUBLIC
uint64_t nano_ros_timer_get_time_until_next_call(
    const nano_ros_timer_t *timer,
    uint64_t current_time_ns);

#ifdef __cplusplus
}
#endif

#endif // NANO_ROS_TIMER_H

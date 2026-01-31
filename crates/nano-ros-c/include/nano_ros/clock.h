/**
 * nano-ros clock API
 *
 * Provides time sources for ROS 2 compatible timing operations.
 *
 * Copyright 2024 nano-ros contributors
 * Licensed under Apache-2.0
 */

#ifndef NANO_ROS_CLOCK_H
#define NANO_ROS_CLOCK_H

#include <stdint.h>
#include <stdbool.h>

#include "nano_ros/types.h"
#include "nano_ros/visibility.h"

#ifdef __cplusplus
extern "C" {
#endif

// ============================================================================
// Time Types
// ============================================================================

/**
 * Time representation compatible with builtin_interfaces/msg/Time.
 *
 * The time is represented as seconds and nanoseconds since the Unix epoch
 * (for system/ROS time) or since an arbitrary point (for steady time).
 */
typedef struct nano_ros_time_t {
    /** Seconds component */
    int32_t sec;
    /** Nanoseconds component (0 to 999,999,999) */
    uint32_t nanosec;
} nano_ros_time_t;

/**
 * Duration representation compatible with builtin_interfaces/msg/Duration.
 */
typedef struct nano_ros_duration_t {
    /** Seconds component (can be negative) */
    int32_t sec;
    /** Nanoseconds component (0 to 999,999,999) */
    uint32_t nanosec;
} nano_ros_duration_t;

// ============================================================================
// Clock Types
// ============================================================================

/**
 * Clock type enumeration.
 *
 * Matches RCL clock types for compatibility.
 */
typedef enum nano_ros_clock_type_t {
    /** Uninitialized clock */
    NANO_ROS_CLOCK_UNINITIALIZED = 0,
    /** ROS time - follows /clock topic if available, otherwise system time */
    NANO_ROS_CLOCK_ROS_TIME = 1,
    /** System time - wall clock time from the operating system */
    NANO_ROS_CLOCK_SYSTEM_TIME = 2,
    /** Steady time - monotonic clock, not affected by system time changes */
    NANO_ROS_CLOCK_STEADY_TIME = 3,
} nano_ros_clock_type_t;

/**
 * Clock state enumeration.
 */
typedef enum nano_ros_clock_state_t {
    /** Not initialized */
    NANO_ROS_CLOCK_STATE_UNINITIALIZED = 0,
    /** Initialized and ready */
    NANO_ROS_CLOCK_STATE_READY = 1,
    /** Shutdown */
    NANO_ROS_CLOCK_STATE_SHUTDOWN = 2,
} nano_ros_clock_state_t;

/**
 * Clock structure.
 *
 * Provides access to different time sources.
 */
typedef struct nano_ros_clock_t {
    /** Clock type */
    nano_ros_clock_type_t type;
    /** Current state */
    nano_ros_clock_state_t state;
    /** Internal: steady clock epoch (nanoseconds) */
    uint64_t _steady_epoch_ns;
} nano_ros_clock_t;

// ============================================================================
// Clock Functions
// ============================================================================

/**
 * Get a zero-initialized clock.
 *
 * @return Zero-initialized clock structure
 */
NANO_ROS_PUBLIC
nano_ros_clock_t nano_ros_clock_get_zero_initialized(void);

/**
 * Initialize a clock.
 *
 * @param clock Pointer to a zero-initialized clock
 * @param type The type of clock to create
 * @return NANO_ROS_RET_OK on success
 * @return NANO_ROS_RET_INVALID_ARGUMENT if clock is NULL or type is invalid
 */
NANO_ROS_PUBLIC NANO_ROS_WARN_UNUSED
nano_ros_ret_t nano_ros_clock_init(
    nano_ros_clock_t *clock,
    nano_ros_clock_type_t type);

/**
 * Get the current time from a clock.
 *
 * @param clock Pointer to an initialized clock
 * @param time_out Pointer to store the current time
 * @return NANO_ROS_RET_OK on success
 * @return NANO_ROS_RET_INVALID_ARGUMENT if clock or time_out is NULL
 * @return NANO_ROS_RET_NOT_INIT if clock is not initialized
 */
NANO_ROS_PUBLIC NANO_ROS_WARN_UNUSED
nano_ros_ret_t nano_ros_clock_get_now(
    const nano_ros_clock_t *clock,
    nano_ros_time_t *time_out);

/**
 * Get the current time from a clock as nanoseconds.
 *
 * @param clock Pointer to an initialized clock
 * @param nanoseconds Pointer to store the current time in nanoseconds
 * @return NANO_ROS_RET_OK on success
 * @return NANO_ROS_RET_INVALID_ARGUMENT if clock or nanoseconds is NULL
 * @return NANO_ROS_RET_NOT_INIT if clock is not initialized
 */
NANO_ROS_PUBLIC NANO_ROS_WARN_UNUSED
nano_ros_ret_t nano_ros_clock_get_now_ns(
    const nano_ros_clock_t *clock,
    int64_t *nanoseconds);

/**
 * Check if a clock is valid (initialized and not shutdown).
 *
 * @param clock Pointer to a clock
 * @return true if the clock is valid, false otherwise
 */
NANO_ROS_PUBLIC
bool nano_ros_clock_is_valid(const nano_ros_clock_t *clock);

/**
 * Get the clock type.
 *
 * @param clock Pointer to a clock
 * @return The clock type, or NANO_ROS_CLOCK_UNINITIALIZED if clock is NULL
 */
NANO_ROS_PUBLIC
nano_ros_clock_type_t nano_ros_clock_get_type(const nano_ros_clock_t *clock);

/**
 * Finalize a clock and release resources.
 *
 * @param clock Pointer to an initialized clock
 * @return NANO_ROS_RET_OK on success
 * @return NANO_ROS_RET_INVALID_ARGUMENT if clock is NULL
 */
NANO_ROS_PUBLIC NANO_ROS_WARN_UNUSED
nano_ros_ret_t nano_ros_clock_fini(nano_ros_clock_t *clock);

// ============================================================================
// Time Utility Functions
// ============================================================================

/**
 * Convert nanoseconds to a nano_ros_time_t structure.
 *
 * @param nanoseconds Time in nanoseconds
 * @return Time structure
 */
NANO_ROS_PUBLIC
nano_ros_time_t nano_ros_time_from_nanoseconds(int64_t nanoseconds);

/**
 * Convert a nano_ros_time_t structure to nanoseconds.
 *
 * @param time Pointer to time structure
 * @return Time in nanoseconds, or 0 if time is NULL
 */
NANO_ROS_PUBLIC
int64_t nano_ros_time_to_nanoseconds(const nano_ros_time_t *time);

/**
 * Add a duration to a time.
 *
 * @param time Base time
 * @param duration Duration to add
 * @return Resulting time
 */
NANO_ROS_PUBLIC
nano_ros_time_t nano_ros_time_add(nano_ros_time_t time, nano_ros_duration_t duration);

/**
 * Subtract a duration from a time.
 *
 * @param time Base time
 * @param duration Duration to subtract
 * @return Resulting time
 */
NANO_ROS_PUBLIC
nano_ros_time_t nano_ros_time_sub(nano_ros_time_t time, nano_ros_duration_t duration);

/**
 * Compare two times.
 *
 * @param a First time
 * @param b Second time
 * @return Negative if a < b, zero if a == b, positive if a > b
 */
NANO_ROS_PUBLIC
int nano_ros_time_compare(nano_ros_time_t a, nano_ros_time_t b);

#ifdef __cplusplus
}
#endif

#endif // NANO_ROS_CLOCK_H

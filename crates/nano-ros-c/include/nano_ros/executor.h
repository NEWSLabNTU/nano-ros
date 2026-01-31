/**
 * nano-ros executor API
 *
 * Executor for managing and processing subscriptions, timers, and services.
 *
 * Copyright 2024 nano-ros contributors
 * Licensed under Apache-2.0
 */

#ifndef NANO_ROS_EXECUTOR_H
#define NANO_ROS_EXECUTOR_H

#include "nano_ros/types.h"
#include "nano_ros/visibility.h"
#include "nano_ros/init.h"
#include "nano_ros/subscription.h"
#include "nano_ros/timer.h"
#include "nano_ros/service.h"
#include "nano_ros/guard_condition.h"

#ifdef __cplusplus
extern "C" {
#endif

// ============================================================================
// Executor Constants
// ============================================================================

/** Maximum number of handles in an executor */
#define NANO_ROS_EXECUTOR_MAX_HANDLES 16

// ============================================================================
// Executor Types
// ============================================================================

/** Executor state */
typedef enum nano_ros_executor_state_t {
    /** Not initialized */
    NANO_ROS_EXECUTOR_STATE_UNINITIALIZED = 0,
    /** Initialized and ready */
    NANO_ROS_EXECUTOR_STATE_INITIALIZED = 1,
    /** Currently spinning */
    NANO_ROS_EXECUTOR_STATE_SPINNING = 2,
    /** Shutdown */
    NANO_ROS_EXECUTOR_STATE_SHUTDOWN = 3,
} nano_ros_executor_state_t;

/** Handle type for executor */
typedef enum nano_ros_executor_handle_type_t {
    /** No handle (empty slot) */
    NANO_ROS_EXECUTOR_HANDLE_NONE = 0,
    /** Subscription handle */
    NANO_ROS_EXECUTOR_HANDLE_SUBSCRIPTION = 1,
    /** Timer handle */
    NANO_ROS_EXECUTOR_HANDLE_TIMER = 2,
    /** Service handle */
    NANO_ROS_EXECUTOR_HANDLE_SERVICE = 3,
    /** Client handle */
    NANO_ROS_EXECUTOR_HANDLE_CLIENT = 4,
    /** Guard condition handle */
    NANO_ROS_EXECUTOR_HANDLE_GUARD_CONDITION = 5,
} nano_ros_executor_handle_type_t;

/** Callback invocation mode */
typedef enum nano_ros_executor_invocation_t {
    /** Only invoke callback when new data is available */
    NANO_ROS_EXECUTOR_ON_NEW_DATA = 0,
    /** Always invoke callback (even with NULL data) */
    NANO_ROS_EXECUTOR_ALWAYS = 1,
} nano_ros_executor_invocation_t;

// ============================================================================
// Executor Structures
// ============================================================================

/** Executor handle (union-like structure) */
typedef struct nano_ros_executor_handle_t {
    /** Handle type */
    nano_ros_executor_handle_type_t handle_type;
    /** Invocation mode (for subscriptions) */
    nano_ros_executor_invocation_t invocation;
    /** Handle pointer (type depends on handle_type) */
    void *handle;
    /** Flag indicating if handle has new data ready */
    bool data_ready;
} nano_ros_executor_handle_t;

/**
 * Executor structure.
 *
 * The executor manages a fixed array of handles and processes them
 * in the order they were added.
 */
typedef struct nano_ros_executor_t {
    /** Current state */
    nano_ros_executor_state_t state;
    /** Handle array */
    nano_ros_executor_handle_t handles[NANO_ROS_EXECUTOR_MAX_HANDLES];
    /** Number of handles in use */
    size_t handle_count;
    /** Maximum handles (configured at init) */
    size_t max_handles;
    /** Timeout in nanoseconds for spin_some */
    uint64_t timeout_ns;
    /** Pointer to support context */
    const nano_ros_support_t *support;
} nano_ros_executor_t;

// ============================================================================
// Executor Functions
// ============================================================================

/**
 * Get a zero-initialized executor.
 *
 * @return Zero-initialized executor structure
 */
NANO_ROS_PUBLIC
nano_ros_executor_t nano_ros_executor_get_zero_initialized(void);

/**
 * Initialize an executor.
 *
 * @param executor Pointer to a zero-initialized executor
 * @param support Pointer to an initialized support context
 * @param max_handles Maximum number of handles (capped at NANO_ROS_EXECUTOR_MAX_HANDLES)
 *
 * @return NANO_ROS_RET_OK on success
 * @return NANO_ROS_RET_INVALID_ARGUMENT if any pointer is NULL or max_handles is 0
 * @return NANO_ROS_RET_NOT_INIT if support is not initialized
 */
NANO_ROS_PUBLIC NANO_ROS_WARN_UNUSED
nano_ros_ret_t nano_ros_executor_init(
    nano_ros_executor_t *executor,
    const nano_ros_support_t *support,
    size_t max_handles);

/**
 * Set the executor timeout.
 *
 * @param executor Pointer to an initialized executor
 * @param timeout_ns Timeout in nanoseconds for spin_some
 *
 * @return NANO_ROS_RET_OK on success
 * @return NANO_ROS_RET_INVALID_ARGUMENT if executor is NULL
 * @return NANO_ROS_RET_NOT_INIT if not initialized
 */
NANO_ROS_PUBLIC NANO_ROS_WARN_UNUSED
nano_ros_ret_t nano_ros_executor_set_timeout(
    nano_ros_executor_t *executor,
    uint64_t timeout_ns);

/**
 * Add a subscription to the executor.
 *
 * @param executor Pointer to an initialized executor
 * @param subscription Pointer to an initialized subscription
 * @param invocation When to invoke the callback
 *
 * @return NANO_ROS_RET_OK on success
 * @return NANO_ROS_RET_INVALID_ARGUMENT if any pointer is NULL
 * @return NANO_ROS_RET_FULL if executor is full
 * @return NANO_ROS_RET_NOT_INIT if not initialized
 */
NANO_ROS_PUBLIC NANO_ROS_WARN_UNUSED
nano_ros_ret_t nano_ros_executor_add_subscription(
    nano_ros_executor_t *executor,
    nano_ros_subscription_t *subscription,
    nano_ros_executor_invocation_t invocation);

/**
 * Add a timer to the executor.
 *
 * @param executor Pointer to an initialized executor
 * @param timer Pointer to an initialized timer
 *
 * @return NANO_ROS_RET_OK on success
 * @return NANO_ROS_RET_INVALID_ARGUMENT if any pointer is NULL
 * @return NANO_ROS_RET_FULL if executor is full
 * @return NANO_ROS_RET_NOT_INIT if not initialized
 */
NANO_ROS_PUBLIC NANO_ROS_WARN_UNUSED
nano_ros_ret_t nano_ros_executor_add_timer(
    nano_ros_executor_t *executor,
    nano_ros_timer_t *timer);

/**
 * Add a service to the executor.
 *
 * @param executor Pointer to an initialized executor
 * @param service Pointer to an initialized service
 *
 * @return NANO_ROS_RET_OK on success
 * @return NANO_ROS_RET_INVALID_ARGUMENT if any pointer is NULL
 * @return NANO_ROS_RET_FULL if executor is full
 * @return NANO_ROS_RET_NOT_INIT if not initialized
 */
NANO_ROS_PUBLIC NANO_ROS_WARN_UNUSED
nano_ros_ret_t nano_ros_executor_add_service(
    nano_ros_executor_t *executor,
    nano_ros_service_t *service);

/**
 * Add a guard condition to the executor.
 *
 * Guard conditions allow other threads to wake up the executor.
 * When triggered, the callback (if set) will be executed.
 *
 * @param executor Pointer to an initialized executor
 * @param guard Pointer to an initialized guard condition
 *
 * @return NANO_ROS_RET_OK on success
 * @return NANO_ROS_RET_INVALID_ARGUMENT if any pointer is NULL
 * @return NANO_ROS_RET_FULL if executor is full
 * @return NANO_ROS_RET_NOT_INIT if not initialized
 */
NANO_ROS_PUBLIC NANO_ROS_WARN_UNUSED
nano_ros_ret_t nano_ros_executor_add_guard_condition(
    nano_ros_executor_t *executor,
    nano_ros_guard_condition_t *guard);

/**
 * Spin the executor once.
 *
 * This function checks for ready handles and processes them once.
 *
 * @param executor Pointer to an initialized executor
 * @param timeout_ns Timeout in nanoseconds (0 for non-blocking)
 *
 * @return NANO_ROS_RET_OK if callbacks were executed
 * @return NANO_ROS_RET_TIMEOUT if no callbacks were ready within timeout
 * @return NANO_ROS_RET_INVALID_ARGUMENT if executor is NULL
 * @return NANO_ROS_RET_NOT_INIT if not initialized
 */
NANO_ROS_PUBLIC NANO_ROS_WARN_UNUSED
nano_ros_ret_t nano_ros_executor_spin_some(
    nano_ros_executor_t *executor,
    uint64_t timeout_ns);

/**
 * Spin the executor forever.
 *
 * This function continuously processes callbacks until shutdown.
 *
 * @param executor Pointer to an initialized executor
 *
 * @return NANO_ROS_RET_OK if shutdown gracefully
 * @return NANO_ROS_RET_INVALID_ARGUMENT if executor is NULL
 * @return NANO_ROS_RET_NOT_INIT if not initialized
 */
NANO_ROS_PUBLIC NANO_ROS_WARN_UNUSED
nano_ros_ret_t nano_ros_executor_spin(nano_ros_executor_t *executor);

/**
 * Spin the executor with a fixed period.
 *
 * This function processes callbacks at a fixed rate.
 *
 * @param executor Pointer to an initialized executor
 * @param period_ns Period in nanoseconds
 *
 * @return NANO_ROS_RET_OK if shutdown gracefully
 * @return NANO_ROS_RET_INVALID_ARGUMENT if executor is NULL or period is 0
 * @return NANO_ROS_RET_NOT_INIT if not initialized
 */
NANO_ROS_PUBLIC NANO_ROS_WARN_UNUSED
nano_ros_ret_t nano_ros_executor_spin_period(
    nano_ros_executor_t *executor,
    uint64_t period_ns);

/**
 * Stop a spinning executor.
 *
 * @param executor Pointer to a spinning executor
 *
 * @return NANO_ROS_RET_OK on success
 * @return NANO_ROS_RET_INVALID_ARGUMENT if executor is NULL
 */
NANO_ROS_PUBLIC NANO_ROS_WARN_UNUSED
nano_ros_ret_t nano_ros_executor_stop(nano_ros_executor_t *executor);

/**
 * Finalize an executor.
 *
 * @param executor Pointer to an initialized executor
 *
 * @return NANO_ROS_RET_OK on success
 * @return NANO_ROS_RET_INVALID_ARGUMENT if executor is NULL
 * @return NANO_ROS_RET_NOT_INIT if not initialized
 */
NANO_ROS_PUBLIC NANO_ROS_WARN_UNUSED
nano_ros_ret_t nano_ros_executor_fini(nano_ros_executor_t *executor);

/**
 * Get the number of handles in the executor.
 *
 * @param executor Pointer to an executor
 *
 * @return Number of handles, or -1 if invalid
 */
NANO_ROS_PUBLIC
int nano_ros_executor_get_handle_count(const nano_ros_executor_t *executor);

/**
 * Check if executor is valid (initialized).
 *
 * @param executor Pointer to an executor
 *
 * @return Non-zero if valid, 0 if invalid or NULL
 */
NANO_ROS_PUBLIC
int nano_ros_executor_is_valid(const nano_ros_executor_t *executor);

#ifdef __cplusplus
}
#endif

#endif // NANO_ROS_EXECUTOR_H

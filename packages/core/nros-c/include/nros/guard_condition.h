/**
 * nros guard condition API
 *
 * Provides guard conditions for executor wake-up from other threads.
 * Guard conditions are used to signal events that should wake up a
 * spinning executor, such as shutdown requests or custom triggers.
 *
 * Copyright 2024 nros contributors
 * Licensed under Apache-2.0
 */

#ifndef NROS_GUARD_CONDITION_H
#define NROS_GUARD_CONDITION_H

#include <stdint.h>
#include <stdbool.h>
#include <stddef.h>

#include "nros/types.h"
#include "nros/visibility.h"

#ifdef __cplusplus
extern "C" {
#endif

// ============================================================================
// Guard Condition Types
// ============================================================================

/**
 * Guard condition state.
 */
typedef enum nano_ros_guard_condition_state_t {
    /** Not initialized */
    NROS_GUARD_CONDITION_STATE_UNINITIALIZED = 0,
    /** Initialized and ready */
    NROS_GUARD_CONDITION_STATE_INITIALIZED = 1,
    /** Shutdown */
    NROS_GUARD_CONDITION_STATE_SHUTDOWN = 2,
} nano_ros_guard_condition_state_t;

/**
 * Guard condition callback type.
 *
 * Called when the guard condition is triggered and processed by the executor.
 *
 * @param context User-provided context
 */
typedef void (*nano_ros_guard_condition_callback_t)(void *context);

/**
 * Guard condition structure.
 *
 * Guard conditions provide a mechanism for signaling the executor from
 * another thread. When triggered, the associated callback is executed
 * during the next executor spin cycle.
 *
 * Thread-safety:
 * - nano_ros_guard_condition_trigger() is safe to call from any thread
 * - Other functions should only be called from the executor thread
 */
typedef struct nano_ros_guard_condition_t {
    /** Current state */
    nano_ros_guard_condition_state_t state;
    /** Triggered flag (atomic in practice) */
    volatile bool triggered;
    /** Callback function */
    nano_ros_guard_condition_callback_t callback;
    /** User context pointer */
    void *context;
    /** Pointer to parent support context */
    void *_support;
} nano_ros_guard_condition_t;

// ============================================================================
// Guard Condition Functions
// ============================================================================

/**
 * Get a zero-initialized guard condition.
 *
 * @return Zero-initialized guard condition structure
 */
NROS_PUBLIC
nano_ros_guard_condition_t nano_ros_guard_condition_get_zero_initialized(void);

/**
 * Initialize a guard condition.
 *
 * @param guard Pointer to a zero-initialized guard condition
 * @param support Pointer to an initialized support context
 * @return NROS_RET_OK on success
 * @return NROS_RET_INVALID_ARGUMENT if guard or support is NULL
 * @return NROS_RET_NOT_INIT if support is not initialized
 * @return NROS_RET_BAD_SEQUENCE if already initialized
 */
NROS_PUBLIC NROS_WARN_UNUSED
nano_ros_ret_t nano_ros_guard_condition_init(
    nano_ros_guard_condition_t *guard,
    struct nano_ros_support_t *support);

/**
 * Set the guard condition callback.
 *
 * @param guard Pointer to an initialized guard condition
 * @param callback Callback function (can be NULL to disable)
 * @param context User context passed to callback
 * @return NROS_RET_OK on success
 * @return NROS_RET_INVALID_ARGUMENT if guard is NULL
 * @return NROS_RET_NOT_INIT if not initialized
 */
NROS_PUBLIC NROS_WARN_UNUSED
nano_ros_ret_t nano_ros_guard_condition_set_callback(
    nano_ros_guard_condition_t *guard,
    nano_ros_guard_condition_callback_t callback,
    void *context);

/**
 * Trigger a guard condition.
 *
 * This function is thread-safe and can be called from any thread.
 * The associated callback will be executed during the next executor
 * spin cycle.
 *
 * @param guard Pointer to an initialized guard condition
 * @return NROS_RET_OK on success
 * @return NROS_RET_INVALID_ARGUMENT if guard is NULL
 * @return NROS_RET_NOT_INIT if not initialized
 */
NROS_PUBLIC NROS_WARN_UNUSED
nano_ros_ret_t nano_ros_guard_condition_trigger(nano_ros_guard_condition_t *guard);

/**
 * Check if the guard condition is triggered.
 *
 * @param guard Pointer to a guard condition
 * @return true if triggered, false otherwise
 */
NROS_PUBLIC
bool nano_ros_guard_condition_is_triggered(const nano_ros_guard_condition_t *guard);

/**
 * Clear the triggered flag (called by executor after processing).
 *
 * This function should typically only be called by the executor
 * after processing the guard condition callback.
 *
 * @param guard Pointer to an initialized guard condition
 * @return NROS_RET_OK on success
 * @return NROS_RET_INVALID_ARGUMENT if guard is NULL
 */
NROS_PUBLIC NROS_WARN_UNUSED
nano_ros_ret_t nano_ros_guard_condition_clear(nano_ros_guard_condition_t *guard);

/**
 * Check if guard condition is valid (initialized).
 *
 * @param guard Pointer to a guard condition
 * @return Non-zero if valid, 0 if invalid or NULL
 */
NROS_PUBLIC
int nano_ros_guard_condition_is_valid(const nano_ros_guard_condition_t *guard);

/**
 * Finalize a guard condition.
 *
 * @param guard Pointer to an initialized guard condition
 * @return NROS_RET_OK on success
 * @return NROS_RET_INVALID_ARGUMENT if guard is NULL
 * @return NROS_RET_NOT_INIT if not initialized
 */
NROS_PUBLIC NROS_WARN_UNUSED
nano_ros_ret_t nano_ros_guard_condition_fini(nano_ros_guard_condition_t *guard);

#ifdef __cplusplus
}
#endif

#endif // NROS_GUARD_CONDITION_H

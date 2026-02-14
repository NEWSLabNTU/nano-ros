/**
 * nros lifecycle node API
 *
 * REP-2002 lifecycle state machine for managed nodes.
 * Provides states (Unconfigured, Inactive, Active, Finalized, ErrorProcessing)
 * and transitions with user-registered callbacks.
 *
 * Copyright 2024 nros contributors
 * Licensed under Apache-2.0
 */

#ifndef NANO_ROS_LIFECYCLE_H
#define NANO_ROS_LIFECYCLE_H

#include "nano_ros/types.h"
#include "nano_ros/node.h"

#ifdef __cplusplus
extern "C" {
#endif

/* ============================================================================
 * Lifecycle State Constants
 * ============================================================================ */

/** Lifecycle state: Unconfigured (initial state) */
#define NANO_ROS_LIFECYCLE_STATE_UNCONFIGURED 1

/** Lifecycle state: Inactive (configured but not processing) */
#define NANO_ROS_LIFECYCLE_STATE_INACTIVE 2

/** Lifecycle state: Active (fully operational) */
#define NANO_ROS_LIFECYCLE_STATE_ACTIVE 3

/** Lifecycle state: Finalized (terminal state) */
#define NANO_ROS_LIFECYCLE_STATE_FINALIZED 4

/** Lifecycle state: ErrorProcessing (error occurred during transition) */
#define NANO_ROS_LIFECYCLE_STATE_ERROR_PROCESSING 5

/* ============================================================================
 * Lifecycle Transition Constants
 * ============================================================================ */

/** Transition: Configure (Unconfigured -> Inactive) */
#define NANO_ROS_LIFECYCLE_TRANSITION_CONFIGURE 1

/** Transition: Activate (Inactive -> Active) */
#define NANO_ROS_LIFECYCLE_TRANSITION_ACTIVATE 2

/** Transition: Deactivate (Active -> Inactive) */
#define NANO_ROS_LIFECYCLE_TRANSITION_DEACTIVATE 3

/** Transition: Cleanup (Inactive -> Unconfigured) */
#define NANO_ROS_LIFECYCLE_TRANSITION_CLEANUP 4

/** Transition: Shutdown from Unconfigured (-> Finalized) */
#define NANO_ROS_LIFECYCLE_TRANSITION_SHUTDOWN_UNCONFIGURED 5

/** Transition: Shutdown from Inactive (-> Finalized) */
#define NANO_ROS_LIFECYCLE_TRANSITION_SHUTDOWN_INACTIVE 6

/** Transition: Shutdown from Active (-> Finalized) */
#define NANO_ROS_LIFECYCLE_TRANSITION_SHUTDOWN_ACTIVE 7

/** Transition: Error Recovery (ErrorProcessing -> Unconfigured) */
#define NANO_ROS_LIFECYCLE_TRANSITION_ERROR_RECOVERY 8

/* ============================================================================
 * Transition Result Constants
 * ============================================================================ */

/** Transition callback result: Success */
#define NANO_ROS_LIFECYCLE_RET_OK 0

/** Transition callback result: Failure (rollback to previous state) */
#define NANO_ROS_LIFECYCLE_RET_FAILURE 1

/** Transition callback result: Error (move to ErrorProcessing) */
#define NANO_ROS_LIFECYCLE_RET_ERROR 2

/* ============================================================================
 * Types
 * ============================================================================ */

/**
 * Lifecycle state machine structure.
 *
 * Manages the REP-2002 lifecycle state and transition callbacks for a node.
 * Created with nano_ros_lifecycle_get_zero_initialized() and initialized
 * with nano_ros_lifecycle_init().
 */
typedef struct nano_ros_lifecycle_state_machine_t {
    /** Current lifecycle state (NANO_ROS_LIFECYCLE_STATE_*) */
    uint8_t current_state;
    /** Configure callback (Unconfigured -> Inactive) */
    uint8_t (*on_configure)(void *context);
    /** Activate callback (Inactive -> Active) */
    uint8_t (*on_activate)(void *context);
    /** Deactivate callback (Active -> Inactive) */
    uint8_t (*on_deactivate)(void *context);
    /** Cleanup callback (Inactive -> Unconfigured) */
    uint8_t (*on_cleanup)(void *context);
    /** Shutdown callback (any -> Finalized) */
    uint8_t (*on_shutdown)(void *context);
    /** Error recovery callback (ErrorProcessing -> Unconfigured) */
    uint8_t (*on_error)(void *context);
    /** User context pointer passed to callbacks */
    void *context;
    /** Whether the state machine has been initialized */
    bool initialized;
} nano_ros_lifecycle_state_machine_t;

/* ============================================================================
 * Functions
 * ============================================================================ */

/**
 * Get a zero-initialized lifecycle state machine.
 *
 * @return Zero-initialized state machine structure
 */
NANO_ROS_PUBLIC
nano_ros_lifecycle_state_machine_t nano_ros_lifecycle_get_zero_initialized(void);

/**
 * Initialize a lifecycle state machine for a node.
 *
 * Sets the state to Unconfigured and marks the state machine as initialized.
 *
 * @param sm Pointer to a zero-initialized state machine
 * @param node Pointer to an initialized node
 *
 * @return NANO_ROS_RET_OK on success
 * @return NANO_ROS_RET_INVALID_ARGUMENT if any pointer is NULL
 * @return NANO_ROS_RET_BAD_SEQUENCE if already initialized
 */
NANO_ROS_PUBLIC NANO_ROS_WARN_UNUSED
nano_ros_ret_t nano_ros_lifecycle_init(
    nano_ros_lifecycle_state_machine_t *sm,
    const nros_node_t *node);

/**
 * Finalize a lifecycle state machine.
 *
 * @param sm Pointer to an initialized state machine
 *
 * @return NANO_ROS_RET_OK on success
 * @return NANO_ROS_RET_INVALID_ARGUMENT if sm is NULL
 * @return NANO_ROS_RET_NOT_INIT if not initialized
 */
NANO_ROS_PUBLIC NANO_ROS_WARN_UNUSED
nano_ros_ret_t nano_ros_lifecycle_fini(
    nano_ros_lifecycle_state_machine_t *sm);

/**
 * Trigger a lifecycle state transition.
 *
 * Validates the transition, invokes the registered callback (if any),
 * and applies the result per REP-2002.
 *
 * @param sm Pointer to an initialized state machine
 * @param transition_id One of the NANO_ROS_LIFECYCLE_TRANSITION_* constants
 *
 * @return NANO_ROS_RET_OK on success
 * @return NANO_ROS_RET_INVALID_ARGUMENT if sm is NULL or transition_id is invalid
 * @return NANO_ROS_RET_NOT_INIT if not initialized
 * @return NANO_ROS_RET_BAD_SEQUENCE if transition is not valid from current state
 * @return NANO_ROS_RET_ERROR if the callback returned failure or error
 */
NANO_ROS_PUBLIC NANO_ROS_WARN_UNUSED
nano_ros_ret_t nano_ros_lifecycle_change_state(
    nano_ros_lifecycle_state_machine_t *sm,
    uint8_t transition_id);

/**
 * Get the current lifecycle state.
 *
 * @param sm Pointer to an initialized state machine
 *
 * @return Current state as uint8_t, or 0 if sm is NULL or not initialized
 */
NANO_ROS_PUBLIC
uint8_t nano_ros_lifecycle_get_state(
    const nano_ros_lifecycle_state_machine_t *sm);

/**
 * Register a callback for the configure transition.
 *
 * @param sm Pointer to an initialized state machine
 * @param cb Callback function, or NULL to clear
 * @param context User context passed to the callback
 *
 * @return NANO_ROS_RET_OK on success
 * @return NANO_ROS_RET_INVALID_ARGUMENT if sm is NULL
 * @return NANO_ROS_RET_NOT_INIT if not initialized
 */
NANO_ROS_PUBLIC NANO_ROS_WARN_UNUSED
nano_ros_ret_t nano_ros_lifecycle_register_on_configure(
    nano_ros_lifecycle_state_machine_t *sm,
    uint8_t (*cb)(void *context),
    void *context);

/**
 * Register a callback for the activate transition.
 */
NANO_ROS_PUBLIC NANO_ROS_WARN_UNUSED
nano_ros_ret_t nano_ros_lifecycle_register_on_activate(
    nano_ros_lifecycle_state_machine_t *sm,
    uint8_t (*cb)(void *context),
    void *context);

/**
 * Register a callback for the deactivate transition.
 */
NANO_ROS_PUBLIC NANO_ROS_WARN_UNUSED
nano_ros_ret_t nano_ros_lifecycle_register_on_deactivate(
    nano_ros_lifecycle_state_machine_t *sm,
    uint8_t (*cb)(void *context),
    void *context);

/**
 * Register a callback for the cleanup transition.
 */
NANO_ROS_PUBLIC NANO_ROS_WARN_UNUSED
nano_ros_ret_t nano_ros_lifecycle_register_on_cleanup(
    nano_ros_lifecycle_state_machine_t *sm,
    uint8_t (*cb)(void *context),
    void *context);

/**
 * Register a callback for the shutdown transition.
 */
NANO_ROS_PUBLIC NANO_ROS_WARN_UNUSED
nano_ros_ret_t nano_ros_lifecycle_register_on_shutdown(
    nano_ros_lifecycle_state_machine_t *sm,
    uint8_t (*cb)(void *context),
    void *context);

/**
 * Register a callback for error recovery transition.
 */
NANO_ROS_PUBLIC NANO_ROS_WARN_UNUSED
nano_ros_ret_t nano_ros_lifecycle_register_on_error(
    nano_ros_lifecycle_state_machine_t *sm,
    uint8_t (*cb)(void *context),
    void *context);

/**
 * Convenience: initialize a lifecycle state machine for a node.
 *
 * Equivalent to nano_ros_lifecycle_init(sm, node).
 * Named to match rclc's rclc_make_node_a_lifecycle_node.
 *
 * @param sm Pointer to a zero-initialized state machine
 * @param node Pointer to an initialized node
 *
 * @return NANO_ROS_RET_OK on success
 */
NANO_ROS_PUBLIC NANO_ROS_WARN_UNUSED
nano_ros_ret_t nano_ros_make_node_a_lifecycle_node(
    nano_ros_lifecycle_state_machine_t *sm,
    const nros_node_t *node);

#ifdef __cplusplus
}
#endif

#endif /* NANO_ROS_LIFECYCLE_H */

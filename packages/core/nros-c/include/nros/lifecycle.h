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

#ifndef NROS_LIFECYCLE_H
#define NROS_LIFECYCLE_H

#include "nros/types.h"
#include "nros/node.h"

#ifdef __cplusplus
extern "C" {
#endif

/* ============================================================================
 * Lifecycle State Constants
 * ============================================================================ */

/** Lifecycle state: Unconfigured (initial state) */
#define NROS_LIFECYCLE_STATE_UNCONFIGURED 1

/** Lifecycle state: Inactive (configured but not processing) */
#define NROS_LIFECYCLE_STATE_INACTIVE 2

/** Lifecycle state: Active (fully operational) */
#define NROS_LIFECYCLE_STATE_ACTIVE 3

/** Lifecycle state: Finalized (terminal state) */
#define NROS_LIFECYCLE_STATE_FINALIZED 4

/** Lifecycle state: ErrorProcessing (error occurred during transition) */
#define NROS_LIFECYCLE_STATE_ERROR_PROCESSING 5

/* ============================================================================
 * Lifecycle Transition Constants
 * ============================================================================ */

/** Transition: Configure (Unconfigured -> Inactive) */
#define NROS_LIFECYCLE_TRANSITION_CONFIGURE 1

/** Transition: Activate (Inactive -> Active) */
#define NROS_LIFECYCLE_TRANSITION_ACTIVATE 2

/** Transition: Deactivate (Active -> Inactive) */
#define NROS_LIFECYCLE_TRANSITION_DEACTIVATE 3

/** Transition: Cleanup (Inactive -> Unconfigured) */
#define NROS_LIFECYCLE_TRANSITION_CLEANUP 4

/** Transition: Shutdown from Unconfigured (-> Finalized) */
#define NROS_LIFECYCLE_TRANSITION_SHUTDOWN_UNCONFIGURED 5

/** Transition: Shutdown from Inactive (-> Finalized) */
#define NROS_LIFECYCLE_TRANSITION_SHUTDOWN_INACTIVE 6

/** Transition: Shutdown from Active (-> Finalized) */
#define NROS_LIFECYCLE_TRANSITION_SHUTDOWN_ACTIVE 7

/** Transition: Error Recovery (ErrorProcessing -> Unconfigured) */
#define NROS_LIFECYCLE_TRANSITION_ERROR_RECOVERY 8

/* ============================================================================
 * Transition Result Constants
 * ============================================================================ */

/** Transition callback result: Success */
#define NROS_LIFECYCLE_RET_OK 0

/** Transition callback result: Failure (rollback to previous state) */
#define NROS_LIFECYCLE_RET_FAILURE 1

/** Transition callback result: Error (move to ErrorProcessing) */
#define NROS_LIFECYCLE_RET_ERROR 2

/* ============================================================================
 * Types
 * ============================================================================ */

/**
 * Lifecycle state machine structure.
 *
 * Manages the REP-2002 lifecycle state and transition callbacks for a node.
 * Created with nros_lifecycle_get_zero_initialized() and initialized
 * with nros_lifecycle_init().
 */
typedef struct nros_lifecycle_state_machine_t {
    /** Current lifecycle state (NROS_LIFECYCLE_STATE_*) */
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
} nros_lifecycle_state_machine_t;

/* ============================================================================
 * Functions
 * ============================================================================ */

/**
 * Get a zero-initialized lifecycle state machine.
 *
 * @return Zero-initialized state machine structure
 */
NROS_PUBLIC
nros_lifecycle_state_machine_t nros_lifecycle_get_zero_initialized(void);

/**
 * Initialize a lifecycle state machine for a node.
 *
 * Sets the state to Unconfigured and marks the state machine as initialized.
 *
 * @param sm Pointer to a zero-initialized state machine
 * @param node Pointer to an initialized node
 *
 * @return NROS_RET_OK on success
 * @return NROS_RET_INVALID_ARGUMENT if any pointer is NULL
 * @return NROS_RET_BAD_SEQUENCE if already initialized
 */
NROS_PUBLIC NROS_WARN_UNUSED
nros_ret_t nros_lifecycle_init(
    nros_lifecycle_state_machine_t *sm,
    const nros_node_t *node);

/**
 * Finalize a lifecycle state machine.
 *
 * @param sm Pointer to an initialized state machine
 *
 * @return NROS_RET_OK on success
 * @return NROS_RET_INVALID_ARGUMENT if sm is NULL
 * @return NROS_RET_NOT_INIT if not initialized
 */
NROS_PUBLIC NROS_WARN_UNUSED
nros_ret_t nros_lifecycle_fini(
    nros_lifecycle_state_machine_t *sm);

/**
 * Trigger a lifecycle state transition.
 *
 * Validates the transition, invokes the registered callback (if any),
 * and applies the result per REP-2002.
 *
 * @param sm Pointer to an initialized state machine
 * @param transition_id One of the NROS_LIFECYCLE_TRANSITION_* constants
 *
 * @return NROS_RET_OK on success
 * @return NROS_RET_INVALID_ARGUMENT if sm is NULL or transition_id is invalid
 * @return NROS_RET_NOT_INIT if not initialized
 * @return NROS_RET_BAD_SEQUENCE if transition is not valid from current state
 * @return NROS_RET_ERROR if the callback returned failure or error
 */
NROS_PUBLIC NROS_WARN_UNUSED
nros_ret_t nros_lifecycle_change_state(
    nros_lifecycle_state_machine_t *sm,
    uint8_t transition_id);

/**
 * Get the current lifecycle state.
 *
 * @param sm Pointer to an initialized state machine
 *
 * @return Current state as uint8_t, or 0 if sm is NULL or not initialized
 */
NROS_PUBLIC
uint8_t nros_lifecycle_get_state(
    const nros_lifecycle_state_machine_t *sm);

/**
 * Register a callback for the configure transition.
 *
 * @param sm Pointer to an initialized state machine
 * @param cb Callback function, or NULL to clear
 * @param context User context passed to the callback
 *
 * @return NROS_RET_OK on success
 * @return NROS_RET_INVALID_ARGUMENT if sm is NULL
 * @return NROS_RET_NOT_INIT if not initialized
 */
NROS_PUBLIC NROS_WARN_UNUSED
nros_ret_t nros_lifecycle_register_on_configure(
    nros_lifecycle_state_machine_t *sm,
    uint8_t (*cb)(void *context),
    void *context);

/**
 * Register a callback for the activate transition.
 */
NROS_PUBLIC NROS_WARN_UNUSED
nros_ret_t nros_lifecycle_register_on_activate(
    nros_lifecycle_state_machine_t *sm,
    uint8_t (*cb)(void *context),
    void *context);

/**
 * Register a callback for the deactivate transition.
 */
NROS_PUBLIC NROS_WARN_UNUSED
nros_ret_t nros_lifecycle_register_on_deactivate(
    nros_lifecycle_state_machine_t *sm,
    uint8_t (*cb)(void *context),
    void *context);

/**
 * Register a callback for the cleanup transition.
 */
NROS_PUBLIC NROS_WARN_UNUSED
nros_ret_t nros_lifecycle_register_on_cleanup(
    nros_lifecycle_state_machine_t *sm,
    uint8_t (*cb)(void *context),
    void *context);

/**
 * Register a callback for the shutdown transition.
 */
NROS_PUBLIC NROS_WARN_UNUSED
nros_ret_t nros_lifecycle_register_on_shutdown(
    nros_lifecycle_state_machine_t *sm,
    uint8_t (*cb)(void *context),
    void *context);

/**
 * Register a callback for error recovery transition.
 */
NROS_PUBLIC NROS_WARN_UNUSED
nros_ret_t nros_lifecycle_register_on_error(
    nros_lifecycle_state_machine_t *sm,
    uint8_t (*cb)(void *context),
    void *context);

/**
 * Convenience: initialize a lifecycle state machine for a node.
 *
 * Equivalent to nros_lifecycle_init(sm, node).
 * Named to match rclc's rclc_make_node_a_lifecycle_node.
 *
 * @param sm Pointer to a zero-initialized state machine
 * @param node Pointer to an initialized node
 *
 * @return NROS_RET_OK on success
 */
NROS_PUBLIC NROS_WARN_UNUSED
nros_ret_t nros_make_node_a_lifecycle_node(
    nros_lifecycle_state_machine_t *sm,
    const nros_node_t *node);

#ifdef __cplusplus
}
#endif

#endif /* NROS_LIFECYCLE_H */

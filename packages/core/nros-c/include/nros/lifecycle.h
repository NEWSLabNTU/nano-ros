/**
 * @file lifecycle.h
 * @brief Node lifecycle state machine (REP-2002).
 *
 * Provides a state machine with callbacks for lifecycle transitions:
 * Unconfigured -> Inactive -> Active -> Finalized, plus error recovery.
 */

#ifndef NROS_LIFECYCLE_H
#define NROS_LIFECYCLE_H

#include "nros/types.h"

#ifdef __cplusplus
extern "C" {
#endif

/* Forward declarations */
struct nros_node_t;

/* ===================================================================
 * Lifecycle State Constants
 * =================================================================== */

/** Lifecycle state: Unconfigured. */
#define NROS_LIFECYCLE_STATE_UNCONFIGURED    0
/** Lifecycle state: Inactive. */
#define NROS_LIFECYCLE_STATE_INACTIVE        1
/** Lifecycle state: Active. */
#define NROS_LIFECYCLE_STATE_ACTIVE          2
/** Lifecycle state: Finalized. */
#define NROS_LIFECYCLE_STATE_FINALIZED       3
/** Lifecycle state: ErrorProcessing. */
#define NROS_LIFECYCLE_STATE_ERROR_PROCESSING 4

/* ===================================================================
 * Lifecycle Transition Constants
 * =================================================================== */

/** Transition: Configure (Unconfigured -> Inactive). */
#define NROS_LIFECYCLE_TRANSITION_CONFIGURE               0
/** Transition: Activate (Inactive -> Active). */
#define NROS_LIFECYCLE_TRANSITION_ACTIVATE                1
/** Transition: Deactivate (Active -> Inactive). */
#define NROS_LIFECYCLE_TRANSITION_DEACTIVATE              2
/** Transition: Cleanup (Inactive -> Unconfigured). */
#define NROS_LIFECYCLE_TRANSITION_CLEANUP                 3
/** Transition: Shutdown from Unconfigured. */
#define NROS_LIFECYCLE_TRANSITION_SHUTDOWN_UNCONFIGURED   4
/** Transition: Shutdown from Inactive. */
#define NROS_LIFECYCLE_TRANSITION_SHUTDOWN_INACTIVE       5
/** Transition: Shutdown from Active. */
#define NROS_LIFECYCLE_TRANSITION_SHUTDOWN_ACTIVE         6
/** Transition: Error recovery (ErrorProcessing -> Unconfigured). */
#define NROS_LIFECYCLE_TRANSITION_ERROR_RECOVERY          7

/* ===================================================================
 * Transition Result Constants
 * =================================================================== */

/** Callback returned success. */
#define NROS_LIFECYCLE_RET_OK      0
/** Callback returned failure (rollback). */
#define NROS_LIFECYCLE_RET_FAILURE 1
/** Callback returned error (enter ErrorProcessing). */
#define NROS_LIFECYCLE_RET_ERROR   2

/* ===================================================================
 * Types
 * =================================================================== */

/**
 * Lifecycle state machine structure.
 *
 * Manages the REP-2002 lifecycle state and transition callbacks for a
 * node.  Created with nros_lifecycle_get_zero_initialized() and
 * initialised with nros_lifecycle_init().
 */
typedef struct nros_lifecycle_state_machine_t {
    /** Current lifecycle state (one of the @c NROS_LIFECYCLE_STATE_* constants). */
    uint8_t current_state;
    /** Configure callback: Unconfigured -> Inactive. */
    uint8_t (*on_configure)(void*);
    /** Activate callback: Inactive -> Active. */
    uint8_t (*on_activate)(void*);
    /** Deactivate callback: Active -> Inactive. */
    uint8_t (*on_deactivate)(void*);
    /** Cleanup callback: Inactive -> Unconfigured. */
    uint8_t (*on_cleanup)(void*);
    /** Shutdown callback: any state -> Finalized. */
    uint8_t (*on_shutdown)(void*);
    /** Error callback: ErrorProcessing -> Unconfigured. */
    uint8_t (*on_error)(void*);
    /** User context pointer passed to callbacks. */
    void *context;
    /** Whether the state machine has been initialized. */
    bool initialized;
} nros_lifecycle_state_machine_t;

/* ===================================================================
 * Functions
 * =================================================================== */

/**
 * @brief Get a zero-initialized lifecycle state machine.
 * @return Zero-initialized @ref nros_lifecycle_state_machine_t.
 */
NROS_PUBLIC struct nros_lifecycle_state_machine_t nros_lifecycle_get_zero_initialized(void);

/**
 * @brief Initialise a lifecycle state machine for a node.
 *
 * Sets the state to Unconfigured and marks the state machine as
 * initialized.
 *
 * @param sm   Pointer to a zero-initialized state machine.
 * @param node Pointer to an initialized node (must outlive the state
 *             machine).
 *
 * @retval NROS_RET_OK               on success.
 * @retval NROS_RET_INVALID_ARGUMENT  if any pointer is NULL.
 * @retval NROS_RET_BAD_SEQUENCE      if already initialized.
 */
NROS_PUBLIC
nros_ret_t nros_lifecycle_init(struct nros_lifecycle_state_machine_t *sm,
                               const struct nros_node_t *node);

/**
 * @brief Finalise a lifecycle state machine.
 *
 * @param sm  Pointer to an initialized state machine.
 *
 * @retval NROS_RET_OK               on success.
 * @retval NROS_RET_INVALID_ARGUMENT  if @p sm is NULL.
 * @retval NROS_RET_NOT_INIT          if not initialized.
 */
NROS_PUBLIC nros_ret_t nros_lifecycle_fini(struct nros_lifecycle_state_machine_t *sm);

/**
 * @brief Trigger a lifecycle state transition.
 *
 * Validates the transition, invokes the registered callback (if any),
 * and applies the result per REP-2002.
 *
 * @param sm            Pointer to an initialized state machine.
 * @param transition_id One of the @c NROS_LIFECYCLE_TRANSITION_*
 *                      constants.
 *
 * @retval NROS_RET_OK               on success.
 * @retval NROS_RET_INVALID_ARGUMENT  if @p sm is NULL or
 *                                    @p transition_id is invalid.
 * @retval NROS_RET_NOT_INIT          if not initialized.
 * @retval NROS_RET_BAD_SEQUENCE      if transition is not valid from
 *                                    current state.
 * @retval NROS_RET_ERROR             if the callback returned an error.
 */
NROS_PUBLIC
nros_ret_t nros_lifecycle_change_state(struct nros_lifecycle_state_machine_t *sm,
                                       uint8_t transition_id);

/**
 * @brief Get the current lifecycle state.
 *
 * @param sm  Pointer to an initialized state machine.
 * @return Current state as @c uint8_t, or 0 if @p sm is NULL or not
 *         initialized.
 */
NROS_PUBLIC uint8_t nros_lifecycle_get_state(const struct nros_lifecycle_state_machine_t *sm);

/**
 * @brief Register a callback for the @e configure transition.
 *
 * @param sm      Pointer to an initialized state machine.
 * @param cb      Callback function, or NULL to clear.
 * @param context User context passed to the callback.
 *
 * @retval NROS_RET_OK               on success.
 * @retval NROS_RET_INVALID_ARGUMENT  if @p sm is NULL.
 * @retval NROS_RET_NOT_INIT          if not initialized.
 */
NROS_PUBLIC
nros_ret_t nros_lifecycle_register_on_configure(struct nros_lifecycle_state_machine_t *sm,
                                                uint8_t (*cb)(void*),
                                                void *context);

/**
 * @brief Register a callback for the @e activate transition.
 *
 * @param sm      Pointer to an initialized state machine.
 * @param cb      Callback function, or NULL to clear.
 * @param context User context passed to the callback.
 *
 * @retval NROS_RET_OK               on success.
 * @retval NROS_RET_INVALID_ARGUMENT  if @p sm is NULL.
 * @retval NROS_RET_NOT_INIT          if not initialized.
 */
NROS_PUBLIC
nros_ret_t nros_lifecycle_register_on_activate(struct nros_lifecycle_state_machine_t *sm,
                                               uint8_t (*cb)(void*),
                                               void *context);

/**
 * @brief Register a callback for the @e deactivate transition.
 *
 * @param sm      Pointer to an initialized state machine.
 * @param cb      Callback function, or NULL to clear.
 * @param context User context passed to the callback.
 *
 * @retval NROS_RET_OK               on success.
 * @retval NROS_RET_INVALID_ARGUMENT  if @p sm is NULL.
 * @retval NROS_RET_NOT_INIT          if not initialized.
 */
NROS_PUBLIC
nros_ret_t nros_lifecycle_register_on_deactivate(struct nros_lifecycle_state_machine_t *sm,
                                                 uint8_t (*cb)(void*),
                                                 void *context);

/**
 * @brief Register a callback for the @e cleanup transition.
 *
 * @param sm      Pointer to an initialized state machine.
 * @param cb      Callback function, or NULL to clear.
 * @param context User context passed to the callback.
 *
 * @retval NROS_RET_OK               on success.
 * @retval NROS_RET_INVALID_ARGUMENT  if @p sm is NULL.
 * @retval NROS_RET_NOT_INIT          if not initialized.
 */
NROS_PUBLIC
nros_ret_t nros_lifecycle_register_on_cleanup(struct nros_lifecycle_state_machine_t *sm,
                                              uint8_t (*cb)(void*),
                                              void *context);

/**
 * @brief Register a callback for the @e shutdown transition.
 *
 * @param sm      Pointer to an initialized state machine.
 * @param cb      Callback function, or NULL to clear.
 * @param context User context passed to the callback.
 *
 * @retval NROS_RET_OK               on success.
 * @retval NROS_RET_INVALID_ARGUMENT  if @p sm is NULL.
 * @retval NROS_RET_NOT_INIT          if not initialized.
 */
NROS_PUBLIC
nros_ret_t nros_lifecycle_register_on_shutdown(struct nros_lifecycle_state_machine_t *sm,
                                               uint8_t (*cb)(void*),
                                               void *context);

/**
 * @brief Register a callback for the @e error transition (error recovery).
 *
 * @param sm      Pointer to an initialized state machine.
 * @param cb      Callback function, or NULL to clear.
 * @param context User context passed to the callback.
 *
 * @retval NROS_RET_OK               on success.
 * @retval NROS_RET_INVALID_ARGUMENT  if @p sm is NULL.
 * @retval NROS_RET_NOT_INIT          if not initialized.
 */
NROS_PUBLIC
nros_ret_t nros_lifecycle_register_on_error(struct nros_lifecycle_state_machine_t *sm,
                                            uint8_t (*cb)(void*),
                                            void *context);

/**
 * @brief Convenience: initialise a lifecycle state machine for a node.
 *
 * Equivalent to calling nros_lifecycle_init().  Named to match rclc's
 * @c rclc_make_node_a_lifecycle_node.
 *
 * @param sm   Pointer to a zero-initialized state machine.
 * @param node Pointer to an initialized node (must outlive the state
 *             machine).
 *
 * @retval NROS_RET_OK               on success.
 * @retval NROS_RET_INVALID_ARGUMENT  if any pointer is NULL.
 * @retval NROS_RET_BAD_SEQUENCE      if already initialized.
 */
NROS_PUBLIC
nros_ret_t nros_make_node_a_lifecycle_node(struct nros_lifecycle_state_machine_t *sm,
                                           const struct nros_node_t *node);

#ifdef __cplusplus
}
#endif

#endif /* NROS_LIFECYCLE_H */

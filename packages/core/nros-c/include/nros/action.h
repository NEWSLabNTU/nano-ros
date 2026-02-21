/**
 * nros action API
 *
 * Provides action servers and clients for long-running tasks with feedback.
 * Actions are used for tasks that take significant time and may need
 * progress updates or cancellation.
 *
 * Copyright 2024 nros contributors
 * Licensed under Apache-2.0
 */

#ifndef NROS_ACTION_H
#define NROS_ACTION_H

#include <stdint.h>
#include <stdbool.h>
#include <stddef.h>

#include "nros/types.h"
#include "nros/visibility.h"

#ifdef __cplusplus
extern "C" {
#endif

// ============================================================================
// Constants
// ============================================================================

/** Maximum length of an action name */
#define NROS_MAX_ACTION_NAME_LEN 256

/** Maximum number of concurrent goals per action server */
#define NROS_MAX_CONCURRENT_GOALS 4

// ============================================================================
// Goal Status (compatible with action_msgs/msg/GoalStatus)
// ============================================================================

/**
 * Goal status enumeration.
 *
 * Compatible with action_msgs/msg/GoalStatus values.
 */
typedef enum nros_goal_status_t {
    /** Goal state is unknown */
    NROS_GOAL_STATUS_UNKNOWN = 0,
    /** Goal was accepted and is pending execution */
    NROS_GOAL_STATUS_ACCEPTED = 1,
    /** Goal is currently being executed */
    NROS_GOAL_STATUS_EXECUTING = 2,
    /** Goal is being canceled */
    NROS_GOAL_STATUS_CANCELING = 3,
    /** Goal completed successfully */
    NROS_GOAL_STATUS_SUCCEEDED = 4,
    /** Goal was canceled */
    NROS_GOAL_STATUS_CANCELED = 5,
    /** Goal was aborted (failed) */
    NROS_GOAL_STATUS_ABORTED = 6,
} nros_goal_status_t;

/**
 * Goal response codes for goal request handling.
 */
typedef enum nros_goal_response_t {
    /** Reject the goal */
    NROS_GOAL_REJECT = 0,
    /** Accept the goal and start executing immediately */
    NROS_GOAL_ACCEPT_AND_EXECUTE = 1,
    /** Accept the goal but defer execution */
    NROS_GOAL_ACCEPT_AND_DEFER = 2,
} nros_goal_response_t;

/**
 * Cancel response codes.
 */
typedef enum nros_cancel_response_t {
    /** Reject the cancel request */
    NROS_CANCEL_REJECT = 0,
    /** Accept the cancel request */
    NROS_CANCEL_ACCEPT = 1,
} nros_cancel_response_t;

// nros_action_type_t is defined in nros/types.h (included above)

// ============================================================================
// Goal UUID
// ============================================================================

/**
 * Goal UUID structure.
 *
 * 16-byte UUID identifying a goal.
 */
typedef struct nros_goal_uuid_t {
    /** UUID bytes */
    uint8_t uuid[16];
} nros_goal_uuid_t;

// ============================================================================
// Goal Handle
// ============================================================================

/**
 * Goal handle structure.
 *
 * Represents a single goal on the action server.
 */
typedef struct nros_goal_handle_t {
    /** Goal UUID */
    nros_goal_uuid_t uuid;
    /** Current status */
    nros_goal_status_t status;
    /** Whether this goal slot is in use */
    bool active;
    /** User context pointer for this goal */
    void *context;
} nros_goal_handle_t;

// ============================================================================
// Action Server
// ============================================================================

/**
 * Action server state.
 */
typedef enum nros_action_server_state_t {
    /** Not initialized */
    NROS_ACTION_SERVER_STATE_UNINITIALIZED = 0,
    /** Initialized and ready */
    NROS_ACTION_SERVER_STATE_INITIALIZED = 1,
    /** Shutdown */
    NROS_ACTION_SERVER_STATE_SHUTDOWN = 2,
} nros_action_server_state_t;

/**
 * Goal request callback type.
 *
 * Called when a new goal request arrives. The callback should inspect
 * the goal and decide whether to accept or reject it.
 *
 * @param goal_uuid UUID of the incoming goal
 * @param goal_request Pointer to CDR-serialized goal request
 * @param goal_len Length of goal request in bytes
 * @param context User-provided context
 * @return Goal response (accept/reject)
 */
typedef nros_goal_response_t (*nros_goal_callback_t)(
    const nros_goal_uuid_t *goal_uuid,
    const uint8_t *goal_request,
    size_t goal_len,
    void *context);

/**
 * Cancel request callback type.
 *
 * Called when a cancel request arrives for an executing goal.
 *
 * @param goal Goal handle being canceled
 * @param context User-provided context
 * @return Cancel response (accept/reject)
 */
typedef nros_cancel_response_t (*nros_cancel_callback_t)(
    nros_goal_handle_t *goal,
    void *context);

/**
 * Goal accepted callback type.
 *
 * Called after a goal has been accepted. The server should begin
 * executing the goal and publishing feedback.
 *
 * @param goal Goal handle for the accepted goal
 * @param context User-provided context
 */
typedef void (*nros_accepted_callback_t)(
    nros_goal_handle_t *goal,
    void *context);

/**
 * Action server structure.
 */
typedef struct nros_action_server_t {
    /** Current state */
    nros_action_server_state_t state;
    /** Goal callback */
    nros_goal_callback_t goal_callback;
    /** Cancel callback */
    nros_cancel_callback_t cancel_callback;
    /** Accepted callback */
    nros_accepted_callback_t accepted_callback;
    /** User context pointer */
    void *context;
    /** Goal handles (static array for embedded use) */
    nros_goal_handle_t goals[NROS_MAX_CONCURRENT_GOALS];
    /** Number of active goals */
    size_t active_goal_count;
    /** Opaque pointer to internal implementation */
    void *_internal;
} nros_action_server_t;

// ============================================================================
// Action Client
// ============================================================================

/**
 * Action client state.
 */
typedef enum nros_action_client_state_t {
    /** Not initialized */
    NROS_ACTION_CLIENT_STATE_UNINITIALIZED = 0,
    /** Initialized and ready */
    NROS_ACTION_CLIENT_STATE_INITIALIZED = 1,
    /** Shutdown */
    NROS_ACTION_CLIENT_STATE_SHUTDOWN = 2,
} nros_action_client_state_t;

/**
 * Feedback callback type.
 *
 * Called when feedback is received for an active goal.
 *
 * @param goal_uuid UUID of the goal
 * @param feedback Pointer to CDR-serialized feedback
 * @param feedback_len Length of feedback in bytes
 * @param context User-provided context
 */
typedef void (*nros_feedback_callback_t)(
    const nros_goal_uuid_t *goal_uuid,
    const uint8_t *feedback,
    size_t feedback_len,
    void *context);

/**
 * Result callback type.
 *
 * Called when a goal completes (succeeded, canceled, or aborted).
 *
 * @param goal_uuid UUID of the goal
 * @param status Final goal status
 * @param result Pointer to CDR-serialized result (may be NULL on cancel/abort)
 * @param result_len Length of result in bytes
 * @param context User-provided context
 */
typedef void (*nros_result_callback_t)(
    const nros_goal_uuid_t *goal_uuid,
    nros_goal_status_t status,
    const uint8_t *result,
    size_t result_len,
    void *context);

/**
 * Action client structure.
 */
typedef struct nros_action_client_t {
    /** Current state */
    nros_action_client_state_t state;
    /** Feedback callback */
    nros_feedback_callback_t feedback_callback;
    /** Result callback */
    nros_result_callback_t result_callback;
    /** User context pointer */
    void *context;
    /** Opaque pointer to internal implementation */
    void *_internal;
} nros_action_client_t;

// ============================================================================
// Action Server Functions
// ============================================================================

/**
 * Get a zero-initialized action server.
 *
 * @return Zero-initialized action server structure
 */
NROS_PUBLIC
nros_action_server_t nros_action_server_get_zero_initialized(void);

/**
 * Initialize an action server.
 *
 * @param server Pointer to a zero-initialized action server
 * @param node Pointer to an initialized node
 * @param action_name Action name (null-terminated string)
 * @param type Pointer to action type information
 * @param goal_callback Callback for goal requests (required)
 * @param cancel_callback Callback for cancel requests (can be NULL)
 * @param accepted_callback Callback for accepted goals (can be NULL)
 * @param context User context pointer passed to callbacks (can be NULL)
 * @return NROS_RET_OK on success
 * @return NROS_RET_INVALID_ARGUMENT if required pointer is NULL
 * @return NROS_RET_NOT_INIT if node is not initialized
 * @return NROS_RET_ERROR on initialization failure
 */
NROS_PUBLIC NROS_WARN_UNUSED
nros_ret_t nros_action_server_init(
    nros_action_server_t *server,
    struct nros_node_t *node,
    const char *action_name,
    const nros_action_type_t *type,
    nros_goal_callback_t goal_callback,
    nros_cancel_callback_t cancel_callback,
    nros_accepted_callback_t accepted_callback,
    void *context);

/**
 * Publish feedback for an executing goal.
 *
 * @param goal Pointer to the goal handle
 * @param feedback CDR-serialized feedback data
 * @param feedback_len Length of feedback data in bytes
 * @return NROS_RET_OK on success
 * @return NROS_RET_INVALID_ARGUMENT if goal or feedback is NULL
 * @return NROS_RET_NOT_ALLOWED if goal is not in executing state
 * @return NROS_RET_ERROR on publish failure
 */
NROS_PUBLIC NROS_WARN_UNUSED
nros_ret_t nros_action_publish_feedback(
    nros_goal_handle_t *goal,
    const uint8_t *feedback,
    size_t feedback_len);

/**
 * Mark a goal as succeeded with a result.
 *
 * @param goal Pointer to the goal handle
 * @param result CDR-serialized result data
 * @param result_len Length of result data in bytes
 * @return NROS_RET_OK on success
 * @return NROS_RET_INVALID_ARGUMENT if goal is NULL
 * @return NROS_RET_NOT_ALLOWED if goal is not in executing state
 * @return NROS_RET_ERROR on publish failure
 */
NROS_PUBLIC NROS_WARN_UNUSED
nros_ret_t nros_action_succeed(
    nros_goal_handle_t *goal,
    const uint8_t *result,
    size_t result_len);

/**
 * Mark a goal as aborted with an optional result.
 *
 * @param goal Pointer to the goal handle
 * @param result CDR-serialized result data (can be NULL)
 * @param result_len Length of result data in bytes
 * @return NROS_RET_OK on success
 * @return NROS_RET_INVALID_ARGUMENT if goal is NULL
 * @return NROS_RET_NOT_ALLOWED if goal is not in executing/canceling state
 * @return NROS_RET_ERROR on publish failure
 */
NROS_PUBLIC NROS_WARN_UNUSED
nros_ret_t nros_action_abort(
    nros_goal_handle_t *goal,
    const uint8_t *result,
    size_t result_len);

/**
 * Mark a goal as canceled with an optional result.
 *
 * @param goal Pointer to the goal handle
 * @param result CDR-serialized result data (can be NULL)
 * @param result_len Length of result data in bytes
 * @return NROS_RET_OK on success
 * @return NROS_RET_INVALID_ARGUMENT if goal is NULL
 * @return NROS_RET_NOT_ALLOWED if goal is not in canceling state
 * @return NROS_RET_ERROR on publish failure
 */
NROS_PUBLIC NROS_WARN_UNUSED
nros_ret_t nros_action_canceled(
    nros_goal_handle_t *goal,
    const uint8_t *result,
    size_t result_len);

/**
 * Execute a goal (transition from accepted to executing).
 *
 * @param goal Pointer to the goal handle
 * @return NROS_RET_OK on success
 * @return NROS_RET_INVALID_ARGUMENT if goal is NULL
 * @return NROS_RET_NOT_ALLOWED if goal is not in accepted state
 */
NROS_PUBLIC NROS_WARN_UNUSED
nros_ret_t nros_action_execute(nros_goal_handle_t *goal);

/**
 * Get the number of active goals.
 *
 * @param server Pointer to an initialized action server
 * @return Number of active goals, or 0 if server is NULL
 */
NROS_PUBLIC
size_t nros_action_server_get_active_goal_count(
    const nros_action_server_t *server);

/**
 * Finalize an action server.
 *
 * @param server Pointer to an initialized action server
 * @return NROS_RET_OK on success
 * @return NROS_RET_INVALID_ARGUMENT if server is NULL
 * @return NROS_RET_NOT_INIT if not initialized
 */
NROS_PUBLIC NROS_WARN_UNUSED
nros_ret_t nros_action_server_fini(nros_action_server_t *server);

// ============================================================================
// Action Client Functions
// ============================================================================

/**
 * Get a zero-initialized action client.
 *
 * @return Zero-initialized action client structure
 */
NROS_PUBLIC
nros_action_client_t nros_action_client_get_zero_initialized(void);

/**
 * Initialize an action client.
 *
 * @param client Pointer to a zero-initialized action client
 * @param node Pointer to an initialized node
 * @param action_name Action name (null-terminated string)
 * @param type Pointer to action type information
 * @return NROS_RET_OK on success
 * @return NROS_RET_INVALID_ARGUMENT if required pointer is NULL
 * @return NROS_RET_NOT_INIT if node is not initialized
 * @return NROS_RET_ERROR on initialization failure
 */
NROS_PUBLIC NROS_WARN_UNUSED
nros_ret_t nros_action_client_init(
    nros_action_client_t *client,
    struct nros_node_t *node,
    const char *action_name,
    const nros_action_type_t *type);

/**
 * Set feedback callback.
 *
 * @param client Pointer to an initialized action client
 * @param callback Feedback callback function (can be NULL to disable)
 * @param context User context passed to callback
 * @return NROS_RET_OK on success
 * @return NROS_RET_INVALID_ARGUMENT if client is NULL
 */
NROS_PUBLIC NROS_WARN_UNUSED
nros_ret_t nros_action_client_set_feedback_callback(
    nros_action_client_t *client,
    nros_feedback_callback_t callback,
    void *context);

/**
 * Set result callback.
 *
 * @param client Pointer to an initialized action client
 * @param callback Result callback function (can be NULL to disable)
 * @param context User context passed to callback
 * @return NROS_RET_OK on success
 * @return NROS_RET_INVALID_ARGUMENT if client is NULL
 */
NROS_PUBLIC NROS_WARN_UNUSED
nros_ret_t nros_action_client_set_result_callback(
    nros_action_client_t *client,
    nros_result_callback_t callback,
    void *context);

/**
 * Send a goal request.
 *
 * @param client Pointer to an initialized action client
 * @param goal CDR-serialized goal data
 * @param goal_len Length of goal data in bytes
 * @param goal_uuid Output: UUID assigned to this goal
 * @return NROS_RET_OK on success (goal accepted)
 * @return NROS_RET_REJECTED if goal was rejected
 * @return NROS_RET_INVALID_ARGUMENT if required pointer is NULL
 * @return NROS_RET_NOT_INIT if not initialized
 * @return NROS_RET_ERROR on send failure
 */
NROS_PUBLIC NROS_WARN_UNUSED
nros_ret_t nros_action_send_goal(
    nros_action_client_t *client,
    const uint8_t *goal,
    size_t goal_len,
    nros_goal_uuid_t *goal_uuid);

/**
 * Request cancellation of a goal.
 *
 * @param client Pointer to an initialized action client
 * @param goal_uuid UUID of the goal to cancel
 * @return NROS_RET_OK on success (cancel request sent)
 * @return NROS_RET_INVALID_ARGUMENT if client is NULL
 * @return NROS_RET_NOT_INIT if not initialized
 * @return NROS_RET_ERROR on send failure
 */
NROS_PUBLIC NROS_WARN_UNUSED
nros_ret_t nros_action_cancel_goal(
    nros_action_client_t *client,
    const nros_goal_uuid_t *goal_uuid);

/**
 * Request result of a goal (blocking).
 *
 * @param client Pointer to an initialized action client
 * @param goal_uuid UUID of the goal
 * @param status Output: final goal status
 * @param result Buffer to receive CDR-serialized result
 * @param result_capacity Capacity of result buffer
 * @param result_len Output: actual length of result data
 * @return NROS_RET_OK on success
 * @return NROS_RET_INVALID_ARGUMENT if required pointer is NULL
 * @return NROS_RET_NOT_INIT if not initialized
 * @return NROS_RET_TIMEOUT if result not available within timeout
 * @return NROS_RET_ERROR on failure
 */
NROS_PUBLIC NROS_WARN_UNUSED
nros_ret_t nros_action_get_result(
    nros_action_client_t *client,
    const nros_goal_uuid_t *goal_uuid,
    nros_goal_status_t *status,
    uint8_t *result,
    size_t result_capacity,
    size_t *result_len);

/**
 * Finalize an action client.
 *
 * @param client Pointer to an initialized action client
 * @return NROS_RET_OK on success
 * @return NROS_RET_INVALID_ARGUMENT if client is NULL
 * @return NROS_RET_NOT_INIT if not initialized
 */
NROS_PUBLIC NROS_WARN_UNUSED
nros_ret_t nros_action_client_fini(nros_action_client_t *client);

// ============================================================================
// Utility Functions
// ============================================================================

/**
 * Generate a new random goal UUID.
 *
 * @param uuid Output: generated UUID
 * @return NROS_RET_OK on success
 * @return NROS_RET_INVALID_ARGUMENT if uuid is NULL
 */
NROS_PUBLIC NROS_WARN_UNUSED
nros_ret_t nros_goal_uuid_generate(nros_goal_uuid_t *uuid);

/**
 * Compare two goal UUIDs.
 *
 * @param a First UUID
 * @param b Second UUID
 * @return true if equal, false otherwise
 */
NROS_PUBLIC
bool nros_goal_uuid_equal(
    const nros_goal_uuid_t *a,
    const nros_goal_uuid_t *b);

/**
 * Get status name as string.
 *
 * @param status Goal status value
 * @return String representation of status
 */
NROS_PUBLIC
const char *nros_goal_status_to_string(nros_goal_status_t status);

#ifdef __cplusplus
}
#endif

#endif // NROS_ACTION_H

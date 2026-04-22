/**
 * @file action.h
 * @brief Action server and client API.
 *
 * Actions provide long-running goal-oriented communication with
 * feedback and cancellation support.
 */

#ifndef NROS_ACTION_H
#define NROS_ACTION_H

#include "nros/types.h"
#include "nros/nros_config_generated.h"

#ifdef __cplusplus
extern "C" {
#endif

/* Forward declarations */
struct nros_node_t;
struct nros_executor_t;
struct nros_action_server_t;
struct nros_goal_handle_t;
struct nros_goal_uuid_t;

/* ===================================================================
 * Types — Enums
 * =================================================================== */

/**
 * Goal status enumeration.
 *
 * Compatible with action_msgs/msg/GoalStatus values.
 */
typedef enum nros_goal_status_t {
    /** Goal state is unknown. */
    NROS_GOAL_STATUS_UNKNOWN = 0,
    /** Goal was accepted and is pending execution. */
    NROS_GOAL_STATUS_ACCEPTED = 1,
    /** Goal is currently being executed. */
    NROS_GOAL_STATUS_EXECUTING = 2,
    /** Goal is being canceled. */
    NROS_GOAL_STATUS_CANCELING = 3,
    /** Goal completed successfully. */
    NROS_GOAL_STATUS_SUCCEEDED = 4,
    /** Goal was canceled. */
    NROS_GOAL_STATUS_CANCELED = 5,
    /** Goal was aborted (failed). */
    NROS_GOAL_STATUS_ABORTED = 6,
} nros_goal_status_t;

/** Goal response codes for goal request handling. */
typedef enum nros_goal_response_t {
    /** Reject the goal. */
    NROS_GOAL_REJECT = 0,
    /** Accept the goal and start executing immediately. */
    NROS_GOAL_ACCEPT_AND_EXECUTE = 1,
    /** Accept the goal but defer execution. */
    NROS_GOAL_ACCEPT_AND_DEFER = 2,
} nros_goal_response_t;

/** Cancel response codes. */
typedef enum nros_cancel_response_t {
    /** Reject the cancel request. */
    NROS_CANCEL_REJECT = 0,
    /** Accept the cancel request. */
    NROS_CANCEL_ACCEPT = 1,
} nros_cancel_response_t;

/** Action client state. */
typedef enum nros_action_client_state_t {
    /** Not initialized. */
    NROS_ACTION_CLIENT_STATE_UNINITIALIZED = 0,
    /** Initialized and ready. */
    NROS_ACTION_CLIENT_STATE_INITIALIZED = 1,
    /** Shutdown. */
    NROS_ACTION_CLIENT_STATE_SHUTDOWN = 2,
} nros_action_client_state_t;

/** Action server state. */
typedef enum nros_action_server_state_t {
    /** Not initialized. */
    NROS_ACTION_SERVER_STATE_UNINITIALIZED = 0,
    /** Initialized and ready. */
    NROS_ACTION_SERVER_STATE_INITIALIZED = 1,
    /** Shutdown. */
    NROS_ACTION_SERVER_STATE_SHUTDOWN = 2,
} nros_action_server_state_t;

/* ===================================================================
 * Types — Structs and Callbacks
 * =================================================================== */

/** Goal UUID structure (16 bytes). */
typedef struct nros_goal_uuid_t {
    /** UUID bytes. */
    uint8_t uuid[16];
} nros_goal_uuid_t;

/* nros_action_type_t is defined in nros/types.h (included above). */

/* --- Client callbacks --- */

/**
 * Feedback callback type (for client).
 *
 * Called when feedback is received for an active goal.
 *
 * @param goal_uuid    UUID of the goal this feedback belongs to.
 * @param feedback     CDR-serialized feedback data.
 * @param feedback_len Length of @p feedback in bytes.
 * @param context      User-provided context pointer.
 */
typedef void (*nros_feedback_callback_t)(const struct nros_goal_uuid_t* goal_uuid,
                                         const uint8_t* feedback, size_t feedback_len,
                                         void* context);

/**
 * Result callback type (for client).
 *
 * Called when a goal completes (succeeded, canceled, or aborted).
 *
 * @param goal_uuid UUID of the completed goal.
 * @param status    Final goal status.
 * @param result    CDR-serialized result data.
 * @param result_len Length of @p result in bytes.
 * @param context   User-provided context pointer.
 */
typedef void (*nros_result_callback_t)(const struct nros_goal_uuid_t* goal_uuid,
                                       enum nros_goal_status_t status, const uint8_t* result,
                                       size_t result_len, void* context);

/**
 * Goal response callback type (for async client).
 *
 * Called when the action server accepts or rejects a goal.
 *
 * @param goal_uuid UUID of the goal.
 * @param accepted  true if accepted, false if rejected.
 * @param context   User-provided context pointer.
 */
typedef void (*nros_goal_response_callback_t)(const struct nros_goal_uuid_t* goal_uuid,
                                              bool accepted, void* context);

/* --- Goal handle --- */

/**
 * Goal handle — a pure UUID identity card.
 *
 * Carries just the goal UUID. All lifecycle state (accepted, executing,
 * cancelling, succeeded, ...) and per-goal user context are managed
 * outside the handle: status comes from the arena via
 * @ref nros_action_get_goal_status, per-goal user context is tracked in
 * caller-managed `{uuid → state}` storage. Handles are copyable by value;
 * trampolines hand user callbacks a `const nros_goal_handle_t *` that
 * points to a stack-local, and users copy it into their own data
 * structures to reference the goal past the callback.
 */
typedef struct nros_goal_handle_t {
    /** Goal UUID. */
    struct nros_goal_uuid_t uuid;
} nros_goal_handle_t;

/* --- Server callbacks --- */

/**
 * Goal request callback type.
 *
 * Called when a client sends a new goal request.  Return a
 * @ref nros_goal_response_t value to accept or reject the goal.
 *
 * @param server       Pointer to the owning action server.
 * @param goal         Pointer to the new goal's identity-only handle.
 *                     The pointer is valid only for the duration of this
 *                     callback; copy the handle by value if needed later.
 * @param goal_request CDR-serialized goal request data.
 * @param goal_len     Length of @p goal_request in bytes.
 * @param context      User-provided context pointer.
 * @return @ref NROS_GOAL_ACCEPT_AND_EXECUTE, @ref NROS_GOAL_ACCEPT_AND_DEFER,
 *         or @ref NROS_GOAL_REJECT.
 */
typedef enum nros_goal_response_t (*nros_goal_callback_t)(struct nros_action_server_t* server,
                                                          const struct nros_goal_handle_t* goal,
                                                          const uint8_t* goal_request,
                                                          size_t goal_len, void* context);

/**
 * Cancel request callback type.
 *
 * Called when a client requests cancellation of an active goal.
 *
 * @param server  Pointer to the owning action server.
 * @param goal    Pointer to the goal handle being canceled.
 * @param context User-provided context pointer.
 * @return @ref NROS_CANCEL_ACCEPT or @ref NROS_CANCEL_REJECT.
 */
typedef enum nros_cancel_response_t (*nros_cancel_callback_t)(struct nros_action_server_t* server,
                                                              const struct nros_goal_handle_t* goal,
                                                              void* context);

/**
 * Goal accepted callback type.
 *
 * Called after a goal has been accepted (response was
 * @ref NROS_GOAL_ACCEPT_AND_EXECUTE or @ref NROS_GOAL_ACCEPT_AND_DEFER).
 *
 * @param server  Pointer to the owning action server.
 * @param goal    Pointer to the newly created goal handle.
 * @param context User-provided context pointer.
 */
typedef void (*nros_accepted_callback_t)(struct nros_action_server_t* server,
                                         const struct nros_goal_handle_t* goal, void* context);

/* --- Client struct --- */

/**
 * Internal state for an action client (Phase 87.5: typed inline field).
 *
 * Mirrors the Rust `ActionClientInternal` struct. Layout is fixed by
 * `#[repr(C)]` on the Rust side; the executor sets these fields during
 * registration.
 */
typedef struct nros_action_client_internal_t {
    /** Arena entry index. -1 means not registered with any executor yet. */
    int32_t arena_entry_index;
    /** Pointer to the parent @ref nros_executor_t that owns the arena entry. */
    void* executor_ptr;
} nros_action_client_internal_t;

/** Action client structure. */
typedef struct nros_action_client_t {
    /** Current state. */
    enum nros_action_client_state_t state;
    /** Action name storage. */
    uint8_t action_name[NROS_MAX_ACTION_NAME_LEN];
    /** Action name length. */
    size_t action_name_len;
    /** Type name storage. */
    uint8_t type_name[NROS_MAX_TYPE_NAME_LEN];
    /** Type name length. */
    size_t type_name_len;
    /** Type hash storage. */
    uint8_t type_hash[NROS_MAX_TYPE_HASH_LEN];
    /** Type hash length. */
    size_t type_hash_len;
    /** Goal response callback (for async send_goal). */
    nros_goal_response_callback_t goal_response_callback;
    /** Feedback callback. */
    nros_feedback_callback_t feedback_callback;
    /** Result callback. */
    nros_result_callback_t result_callback;
    /** User context pointer. */
    void* context;
    /** Pointer to parent node. */
    const struct nros_node_t* node;
    /** Internal state — set by @ref nros_executor_add_action_client. */
    nros_action_client_internal_t _internal;
} nros_action_client_t;

/* --- Server struct --- */

/** Action server structure. */
typedef struct nros_action_server_t {
    /** Current state. */
    enum nros_action_server_state_t state;
    /** Action name storage. */
    uint8_t action_name[NROS_MAX_ACTION_NAME_LEN];
    /** Action name length. */
    size_t action_name_len;
    /** Type name storage. */
    uint8_t type_name[NROS_MAX_TYPE_NAME_LEN];
    /** Type name length. */
    size_t type_name_len;
    /** Type hash storage. */
    uint8_t type_hash[NROS_MAX_TYPE_HASH_LEN];
    /** Type hash length. */
    size_t type_hash_len;
    /** Goal callback. */
    nros_goal_callback_t goal_callback;
    /** Cancel callback. */
    nros_cancel_callback_t cancel_callback;
    /** Accepted callback. */
    nros_accepted_callback_t accepted_callback;
    /** User context pointer. */
    void* context;
    /** Pointer to parent node. */
    const struct nros_node_t* node;
    /** Inline opaque storage for internal implementation. */
    _Alignas(8) uint8_t _internal[NROS_ACTION_SERVER_STORAGE_SIZE];
} nros_action_server_t;

/* ===================================================================
 * Functions — Client
 * =================================================================== */

/**
 * @brief Get a zero-initialized action client.
 * @return Zero-initialized @ref nros_action_client_t.
 */
NROS_PUBLIC struct nros_action_client_t nros_action_client_get_zero_initialized(void);

/**
 * @brief Initialise an action client.
 *
 * @param client      Pointer to a zero-initialized action client.
 * @param node        Pointer to an initialized node.
 * @param action_name Action name (null-terminated).
 * @param type_info   Pointer to action type information.
 *
 * @retval NROS_RET_OK on success.
 */
NROS_PUBLIC
nros_ret_t nros_action_client_init(struct nros_action_client_t* client,
                                   const struct nros_node_t* node, const char* action_name,
                                   const struct nros_action_type_t* type_info);

/**
 * @brief Set feedback callback.
 *
 * @param client   Pointer to an initialized action client.
 * @param callback Feedback callback function.
 * @param context  User context.
 *
 * @retval NROS_RET_OK on success.
 */
NROS_PUBLIC
nros_ret_t nros_action_client_set_feedback_callback(struct nros_action_client_t* client,
                                                    nros_feedback_callback_t callback,
                                                    void* context);

/**
 * @brief Set result callback.
 *
 * @param client   Pointer to an initialized action client.
 * @param callback Result callback function.
 * @param context  User context.
 *
 * @retval NROS_RET_OK on success.
 */
NROS_PUBLIC
nros_ret_t nros_action_client_set_result_callback(struct nros_action_client_t* client,
                                                  nros_result_callback_t callback, void* context);

/**
 * @brief Set goal response callback (for async send_goal).
 *
 * Called during nros_executor_spin_some() when the server accepts or rejects
 * a goal sent via nros_action_send_goal_async().
 *
 * @param client   Pointer to an initialized action client.
 * @param callback Goal response callback function.
 * @param context  User context.
 *
 * @retval NROS_RET_OK on success.
 */
NROS_PUBLIC
nros_ret_t nros_action_client_set_goal_response_callback(struct nros_action_client_t* client,
                                                         nros_goal_response_callback_t callback,
                                                         void* context);

/**
 * @brief Register an action client with the executor.
 *
 * Creates transport handles (service clients + feedback subscriber) in the
 * executor's arena. Must be called after nros_action_client_init() and before
 * any send_goal/get_result calls.
 *
 * Callbacks registered via set_*_callback are invoked during
 * nros_executor_spin_some().
 *
 * @param executor Pointer to an initialized executor.
 * @param client   Pointer to an initialized action client.
 *
 * @retval NROS_RET_OK on success.
 */
NROS_PUBLIC
nros_ret_t nros_executor_add_action_client(struct nros_executor_t* executor,
                                           struct nros_action_client_t* client);

/**
 * @brief Send a goal request (blocking convenience).
 *
 * Spins the executor internally until the server accepts/rejects or timeout.
 * Never calls zpico_get directly — all I/O is driven by the executor.
 *
 * @param client    Pointer to an initialized action client (registered with executor).
 * @param executor  Pointer to an initialized executor.
 * @param goal      CDR-serialized goal data.
 * @param goal_len  Length of goal data.
 * @param goal_uuid Output: generated goal UUID.
 *
 * @retval NROS_RET_OK      on success (goal accepted).
 * @retval NROS_RET_TIMEOUT if no response within timeout.
 */
NROS_PUBLIC
nros_ret_t nros_action_send_goal(struct nros_action_client_t* client,
                                 struct nros_executor_t* executor, const uint8_t* goal,
                                 size_t goal_len, struct nros_goal_uuid_t* goal_uuid);

/**
 * @brief Send a goal request asynchronously (non-blocking).
 *
 * Returns immediately after sending. The goal response arrives via the
 * goal_response_callback during nros_executor_spin_some().
 *
 * @param client    Pointer to an initialized action client (registered with executor).
 * @param goal      CDR-serialized goal data.
 * @param goal_len  Length of goal data.
 * @param goal_uuid Output: generated goal UUID.
 *
 * @retval NROS_RET_OK on success.
 */
NROS_PUBLIC
nros_ret_t nros_action_send_goal_async(struct nros_action_client_t* client, const uint8_t* goal,
                                       size_t goal_len, struct nros_goal_uuid_t* goal_uuid);

/**
 * @brief Request cancellation of a goal.
 *
 * @param client    Pointer to an initialized action client.
 * @param goal_uuid UUID of the goal to cancel.
 *
 * @retval NROS_RET_OK on success.
 */
NROS_PUBLIC
nros_ret_t nros_action_cancel_goal(struct nros_action_client_t* client,
                                   const struct nros_goal_uuid_t* goal_uuid);

/**
 * @brief Request result of a goal (blocking convenience).
 *
 * Spins the executor internally until the result arrives or timeout.
 *
 * @param client          Pointer to an initialized action client (registered with executor).
 * @param executor        Pointer to an initialized executor.
 * @param goal_uuid       UUID of the goal.
 * @param status          Output: goal status.
 * @param result          Buffer for CDR-serialized result.
 * @param result_capacity Capacity of result buffer.
 * @param result_len      Output: actual result length.
 *
 * @retval NROS_RET_OK      on success.
 * @retval NROS_RET_TIMEOUT  if no result within timeout.
 */
NROS_PUBLIC
nros_ret_t nros_action_get_result(struct nros_action_client_t* client,
                                  struct nros_executor_t* executor,
                                  const struct nros_goal_uuid_t* goal_uuid,
                                  enum nros_goal_status_t* status, uint8_t* result,
                                  size_t result_capacity, size_t* result_len);

/**
 * @brief Request result of a goal asynchronously (non-blocking).
 *
 * Returns immediately after sending. The result arrives via the
 * result_callback during nros_executor_spin_some().
 *
 * @param client    Pointer to an initialized action client (registered with executor).
 * @param goal_uuid UUID of the goal.
 *
 * @retval NROS_RET_OK on success.
 */
NROS_PUBLIC
nros_ret_t nros_action_get_result_async(struct nros_action_client_t* client,
                                        const struct nros_goal_uuid_t* goal_uuid);

/**
 * @brief Try to receive feedback for an active goal (non-blocking).
 *
 * If feedback is available, invokes the feedback callback (if set).
 *
 * @param client  Pointer to an initialized action client.
 *
 * @retval NROS_RET_OK      if feedback was received and dispatched.
 * @retval NROS_RET_TIMEOUT  if no feedback is available.
 */
NROS_PUBLIC nros_ret_t nros_action_try_recv_feedback(struct nros_action_client_t* client);

/**
 * @brief Finalise an action client.
 *
 * @param client  Pointer to an initialized action client.
 * @retval NROS_RET_OK on success.
 */
NROS_PUBLIC nros_ret_t nros_action_client_fini(struct nros_action_client_t* client);

/* ===================================================================
 * Functions — Goal UUID Utilities
 * =================================================================== */

/**
 * @brief Generate a new random goal UUID.
 *
 * @param uuid  Output: generated UUID.
 * @retval NROS_RET_OK on success.
 */
NROS_PUBLIC nros_ret_t nros_goal_uuid_generate(struct nros_goal_uuid_t* uuid);

/**
 * @brief Compare two goal UUIDs.
 *
 * @param a  First UUID.
 * @param b  Second UUID.
 * @return @c true if equal, @c false otherwise.
 */
NROS_PUBLIC
bool nros_goal_uuid_equal(const struct nros_goal_uuid_t* a, const struct nros_goal_uuid_t* b);

/**
 * @brief Get status name as string.
 *
 * @param status  Goal status value.
 * @return Null-terminated status name string.
 */
NROS_PUBLIC const char* nros_goal_status_to_string(enum nros_goal_status_t status);

/* ===================================================================
 * Functions — Server
 * =================================================================== */

/**
 * @brief Get a zero-initialized action server.
 * @return Zero-initialized @ref nros_action_server_t.
 */
NROS_PUBLIC struct nros_action_server_t nros_action_server_get_zero_initialized(void);

/**
 * @brief Initialise an action server.
 *
 * Stores metadata (name, type, callbacks).  RMW entity creation is
 * deferred to nros_executor_add_action_server().
 *
 * @param server            Pointer to a zero-initialized action server.
 * @param node              Pointer to an initialized node.
 * @param action_name       Action name (null-terminated).
 * @param type_info         Pointer to action type information.
 * @param goal_callback     Callback for incoming goal requests.
 * @param cancel_callback   Callback for cancel requests.
 * @param accepted_callback Callback when a goal is accepted.
 * @param context           User context pointer.
 *
 * @retval NROS_RET_OK on success.
 */
NROS_PUBLIC
nros_ret_t nros_action_server_init(struct nros_action_server_t* server,
                                   const struct nros_node_t* node, const char* action_name,
                                   const struct nros_action_type_t* type_info,
                                   nros_goal_callback_t goal_callback,
                                   nros_cancel_callback_t cancel_callback,
                                   nros_accepted_callback_t accepted_callback, void* context);

/**
 * @brief Publish feedback for an active goal.
 *
 * @param server       Pointer to the owning action server.
 * @param goal         Pointer to an active goal handle.
 * @param feedback     CDR-serialized feedback data.
 * @param feedback_len Length of feedback data.
 *
 * @retval NROS_RET_OK on success.
 */
NROS_PUBLIC
nros_ret_t nros_action_publish_feedback(struct nros_action_server_t* server,
                                        const struct nros_goal_handle_t* goal,
                                        const uint8_t* feedback, size_t feedback_len);

/**
 * @brief Mark a goal as succeeded with a result.
 *
 * @param server     Pointer to the owning action server.
 * @param goal       Pointer to an active goal handle.
 * @param result     CDR-serialized result data.
 * @param result_len Length of result data.
 *
 * @retval NROS_RET_OK on success.
 */
NROS_PUBLIC
nros_ret_t nros_action_succeed(struct nros_action_server_t* server,
                               const struct nros_goal_handle_t* goal, const uint8_t* result,
                               size_t result_len);

/**
 * @brief Mark a goal as aborted with an optional result.
 *
 * @param server     Pointer to the owning action server.
 * @param goal       Pointer to an active goal handle.
 * @param result     CDR-serialized result data (can be NULL).
 * @param result_len Length of result data (0 if no result).
 *
 * @retval NROS_RET_OK on success.
 */
NROS_PUBLIC
nros_ret_t nros_action_abort(struct nros_action_server_t* server,
                             const struct nros_goal_handle_t* goal, const uint8_t* result,
                             size_t result_len);

/**
 * @brief Mark a goal as canceled with an optional result.
 *
 * @param server     Pointer to the owning action server.
 * @param goal       Pointer to an active goal handle.
 * @param result     CDR-serialized result data (can be NULL).
 * @param result_len Length of result data (0 if no result).
 *
 * @retval NROS_RET_OK on success.
 */
NROS_PUBLIC
nros_ret_t nros_action_canceled(struct nros_action_server_t* server,
                                const struct nros_goal_handle_t* goal, const uint8_t* result,
                                size_t result_len);

/**
 * @brief Execute a goal (transition to `Executing`).
 *
 * Idempotent: a no-op if the goal isn't in the arena's active-goals vector.
 *
 * @param server  Pointer to the owning action server.
 * @param goal    Pointer to an accepted goal handle.
 * @retval NROS_RET_OK on success.
 */
NROS_PUBLIC nros_ret_t nros_action_execute(struct nros_action_server_t* server,
                                           const struct nros_goal_handle_t* goal);

/**
 * @brief Get the number of currently active goals.
 *
 * Reads from the arena via `ActionServerRawHandle::active_goal_count`.
 * Returns 0 if the server isn't registered or has been finalised.
 *
 * @param server  Pointer to an action server.
 * @return Number of active goals.
 */
NROS_PUBLIC
size_t nros_action_server_get_active_goal_count(const struct nros_action_server_t* server);

/**
 * @brief Look up a goal's current status in the arena by UUID.
 *
 * Returns `NROS_RET_OK` and writes the arena-sourced status on success.
 * Returns `NROS_RET_NOT_FOUND` if the arena has already retired the goal
 * (completed + result delivered, or cancelled + acknowledged).
 *
 * @param server   Pointer to the owning action server.
 * @param goal     Pointer to a goal handle.
 * @param status   Output: the arena-sourced goal status.
 * @retval NROS_RET_OK on success.
 * @retval NROS_RET_NOT_FOUND if the arena has no record of this goal.
 */
NROS_PUBLIC
nros_ret_t nros_action_get_goal_status(const struct nros_action_server_t* server,
                                       const struct nros_goal_handle_t* goal,
                                       enum nros_goal_status_t* status);

/**
 * @brief Finalise an action server.
 *
 * @param server  Pointer to an initialized action server.
 * @retval NROS_RET_OK on success.
 */
NROS_PUBLIC nros_ret_t nros_action_server_fini(struct nros_action_server_t* server);

#ifdef __cplusplus
}
#endif

#endif /* NROS_ACTION_H */

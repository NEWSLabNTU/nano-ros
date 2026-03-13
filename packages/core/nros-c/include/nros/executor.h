/**
 * @file executor.h
 * @brief Callback executor (polling) API.
 *
 * The executor drives middleware I/O and dispatches ready callbacks for
 * subscriptions, timers, services, guard conditions, and action servers.
 */

#ifndef NROS_EXECUTOR_H
#define NROS_EXECUTOR_H

#include "nros/types.h"
#include "nros/nros_config_generated.h"

#ifdef __cplusplus
extern "C" {
#endif

/* Forward declarations for types owned by other modules */
struct nros_support_t;
struct nros_subscription_t;
struct nros_timer_t;
struct nros_service_t;
struct nros_guard_condition_t;
struct nros_action_server_t;

/* ===================================================================
 * Types
 * =================================================================== */

/** Executor state. */
typedef enum nros_executor_state_t {
    /** Not initialized. */
    NROS_EXECUTOR_STATE_UNINITIALIZED = 0,
    /** Initialized and ready. */
    NROS_EXECUTOR_STATE_INITIALIZED = 1,
    /** Currently spinning. */
    NROS_EXECUTOR_STATE_SPINNING = 2,
    /** Shutdown. */
    NROS_EXECUTOR_STATE_SHUTDOWN = 3,
} nros_executor_state_t;

/**
 * Executor data communication semantics.
 *
 * Defines when data is taken from DDS during spin operations.
 */
typedef enum nros_executor_semantics_t {
    /**
     * RCLCPP executor semantics: data is taken from DDS just before
     * the corresponding callback is called.
     */
    NROS_SEMANTICS_RCLCPP_EXECUTOR = 0,
    /**
     * Logical Execution Time (LET) semantics: at one sampling point,
     * new data of all ready subscriptions are taken from DDS.  During
     * sequential processing, the data from that sampling point is used.
     */
    NROS_SEMANTICS_LOGICAL_EXECUTION_TIME = 1,
} nros_executor_semantics_t;

/**
 * Trigger function type for executor.
 *
 * A trigger function receives a boolean array indicating which handles
 * have data ready, along with the count of handles.  It returns @c true
 * if the executor should process callbacks.
 *
 * @param ready   Pointer to boolean array (one per handle).
 * @param count   Number of elements in the array.
 * @param context User-provided context pointer.
 * @return @c true if the executor should process callbacks.
 */
typedef bool (*nros_executor_trigger_t)(const bool* ready, size_t count, void* context);

/**
 * Executor structure.
 *
 * The executor delegates all dispatch logic to an internal Rust
 * executor.  The C struct retains state, timeout, and per-type counters
 * for API compatibility.
 */
typedef struct nros_executor_t {
    /** Current state. */
    enum nros_executor_state_t state;
    /** Timeout in nanoseconds for spin_some. */
    uint64_t timeout_ns;
    /** Data communication semantics. */
    enum nros_executor_semantics_t semantics;
    /** Pointer to support context. */
    const struct nros_support_t* support;
    /** Trigger function (NULL = default "any" trigger). */
    nros_executor_trigger_t trigger;
    /** User context for trigger function. */
    void* trigger_context;
    /** Number of handles registered. */
    size_t handle_count;
    /** Maximum handles (configured at init). */
    size_t max_handles;
    /** Number of subscription handles. */
    size_t subscription_count;
    /** Number of timer handles. */
    size_t timer_count;
    /** Number of service handles. */
    size_t service_count;
    /** Next invocation time in nanoseconds for drift-compensated spin_period. */
    uint64_t invocation_time_ns;
    /** Inline opaque storage for the Rust executor.
     *  Size is computed at build time from NROS_EXECUTOR_MAX_CBS and
     *  NROS_EXECUTOR_ARENA_SIZE — no heap allocation needed. */
    _Alignas(8) uint8_t _opaque[NROS_EXECUTOR_STORAGE_SIZE];
} nros_executor_t;

/* ===================================================================
 * Functions
 * =================================================================== */

/**
 * @brief Get a zero-initialized executor.
 * @return Zero-initialized @ref nros_executor_t.
 */
NROS_PUBLIC struct nros_executor_t nros_executor_get_zero_initialized(void);

/**
 * @brief Initialise an executor.
 *
 * @param executor    Pointer to a zero-initialized executor.
 * @param support     Pointer to an initialized support context.
 * @param max_handles Maximum number of handles (capped at
 *                    NROS_EXECUTOR_MAX_HANDLES).
 *
 * @retval NROS_RET_OK               on success.
 * @retval NROS_RET_INVALID_ARGUMENT  if any pointer is NULL or
 *                                    @p max_handles is 0.
 * @retval NROS_RET_NOT_INIT          if @p support is not initialized.
 *
 * @pre All pointers must be valid.
 */
NROS_PUBLIC
nros_ret_t nros_executor_init(struct nros_executor_t* executor,
                              const struct nros_support_t* support, size_t max_handles);

/**
 * @brief Set the executor timeout.
 *
 * @param executor   Pointer to an initialized executor.
 * @param timeout_ns Timeout in nanoseconds.
 *
 * @retval NROS_RET_OK on success.
 *
 * @pre @p executor must point to an initialized executor.
 */
NROS_PUBLIC
nros_ret_t nros_executor_set_timeout(struct nros_executor_t* executor, uint64_t timeout_ns);

/**
 * @brief Set data communication semantics.
 *
 * @param executor  Pointer to an initialized executor.
 * @param semantics Data communication semantics.
 *
 * @retval NROS_RET_OK on success.
 *
 * @pre @p executor must point to an initialized executor.
 */
NROS_PUBLIC
nros_ret_t nros_executor_set_semantics(struct nros_executor_t* executor,
                                       enum nros_executor_semantics_t semantics);

/**
 * @brief Set the trigger condition for the executor.
 *
 * @param executor Pointer to an initialized executor.
 * @param trigger  Trigger function, or NULL for default "any" trigger.
 * @param context  User context passed to the trigger function.
 *
 * @retval NROS_RET_OK on success.
 *
 * @pre @p executor must point to an initialized executor.
 */
NROS_PUBLIC
nros_ret_t nros_executor_set_trigger(struct nros_executor_t* executor,
                                     nros_executor_trigger_t trigger, void* context);

/**
 * @brief Built-in trigger: fire when ANY handle has data ready.
 *
 * @param ready   Array of boolean ready-flags, one per registered handle.
 * @param count   Number of elements in @p ready.
 * @param context User-provided context pointer (unused).
 * @return @c true if at least one handle is ready.
 *
 * @pre @p ready must point to a valid array of at least @p count booleans.
 */
NROS_PUBLIC bool nros_executor_trigger_any(const bool* ready, size_t count, void* context);

/**
 * @brief Built-in trigger: fire when ALL handles have data ready.
 *
 * @param ready   Array of boolean ready-flags, one per registered handle.
 * @param count   Number of elements in @p ready.
 * @param context User-provided context pointer (unused).
 * @return @c true if all handles are ready.
 *
 * @pre @p ready must point to a valid array of at least @p count booleans.
 */
NROS_PUBLIC bool nros_executor_trigger_all(const bool* ready, size_t count, void* context);

/**
 * @brief Built-in trigger: always fire (unconditionally).
 *
 * @param ready   Array of boolean ready-flags (ignored).
 * @param count   Number of elements in @p ready (ignored).
 * @param context User-provided context pointer (unused).
 * @return Always @c true.
 */
NROS_PUBLIC bool nros_executor_trigger_always(const bool* ready, size_t count, void* context);

/**
 * @brief Built-in trigger: fire when the handle at a specific index
 *        has data.
 *
 * Pass the handle index (cast to @c void*) as the @p context parameter.
 *
 * @param ready   Array of boolean ready-flags, one per registered handle.
 * @param count   Number of elements in @p ready.
 * @param context Handle index cast to @c void* (the first registered
 *                handle when 0).
 * @return @c true if the handle at the given index is ready.
 *
 * @pre @p ready must point to a valid array of at least @p count booleans.
 * @pre @p context is interpreted as a @c size_t index.
 */
NROS_PUBLIC bool nros_executor_trigger_one(const bool* ready, size_t count, void* context);

/**
 * @brief Add a subscription to the executor.
 *
 * Extracts metadata from the subscription struct and registers a
 * raw-bytes callback with the internal executor.  The RMW subscriber
 * handle is created here.
 *
 * @param executor     Pointer to an initialized executor.
 * @param subscription Pointer to an initialized subscription.
 * @param invocation   Callback invocation mode.
 *
 * @retval NROS_RET_OK   on success.
 * @retval NROS_RET_FULL  if handle limit reached.
 *
 * @pre All pointers must be valid and point to initialized objects.
 */
NROS_PUBLIC
nros_ret_t nros_executor_add_subscription(struct nros_executor_t* executor,
                                          struct nros_subscription_t* subscription,
                                          enum nros_executor_invocation_t invocation);

/**
 * @brief Add a timer to the executor.
 *
 * @param executor Pointer to an initialized executor.
 * @param timer    Pointer to an initialized timer.
 *
 * @retval NROS_RET_OK   on success.
 * @retval NROS_RET_FULL  if handle limit reached.
 *
 * @pre All pointers must be valid and point to initialized objects.
 */
NROS_PUBLIC
nros_ret_t nros_executor_add_timer(struct nros_executor_t* executor, struct nros_timer_t* timer);

/**
 * @brief Add a service to the executor.
 *
 * @param executor Pointer to an initialized executor.
 * @param service  Pointer to an initialized service.
 *
 * @retval NROS_RET_OK   on success.
 * @retval NROS_RET_FULL  if handle limit reached.
 *
 * @pre All pointers must be valid and point to initialized objects.
 */
NROS_PUBLIC
nros_ret_t nros_executor_add_service(struct nros_executor_t* executor,
                                     struct nros_service_t* service);

/**
 * @brief Add a guard condition to the executor.
 *
 * @param executor Pointer to an initialized executor.
 * @param guard    Pointer to an initialized guard condition.
 *
 * @retval NROS_RET_OK   on success.
 * @retval NROS_RET_FULL  if handle limit reached.
 *
 * @pre All pointers must be valid and point to initialized objects.
 */
NROS_PUBLIC
nros_ret_t nros_executor_add_guard_condition(struct nros_executor_t* executor,
                                             struct nros_guard_condition_t* guard);

/**
 * @brief Add an action server to the executor.
 *
 * Extracts metadata from the action server struct, creates callback
 * trampolines, and registers with the internal executor.
 *
 * @param executor Pointer to an initialized executor.
 * @param server   Pointer to an initialized action server.
 *
 * @retval NROS_RET_OK   on success.
 * @retval NROS_RET_FULL  if handle limit reached.
 *
 * @pre All pointers must be valid and point to initialized objects.
 */
NROS_PUBLIC
nros_ret_t nros_executor_add_action_server(struct nros_executor_t* executor,
                                           struct nros_action_server_t* server);

/**
 * @brief Spin the executor once.
 *
 * Drives middleware I/O, then dispatches ready callbacks.
 *
 * @param executor   Pointer to an initialized executor.
 * @param timeout_ns Timeout in nanoseconds.
 *
 * @retval NROS_RET_OK on success.
 *
 * @pre @p executor must point to an initialized executor.
 */
NROS_PUBLIC
nros_ret_t nros_executor_spin_some(struct nros_executor_t* executor, uint64_t timeout_ns);

/**
 * @brief Spin the executor forever.
 *
 * @param executor  Pointer to an initialized executor.
 * @retval NROS_RET_OK on success.
 *
 * @pre @p executor must point to an initialized executor.
 */
NROS_PUBLIC nros_ret_t nros_executor_spin(struct nros_executor_t* executor);

/**
 * @brief Spin the executor with a fixed period.
 *
 * @param executor  Pointer to an initialized executor.
 * @param period_ns Period in nanoseconds.
 *
 * @retval NROS_RET_OK on success.
 *
 * @pre @p executor must point to an initialized executor.
 */
NROS_PUBLIC
nros_ret_t nros_executor_spin_period(struct nros_executor_t* executor, uint64_t period_ns);

/**
 * @brief Spin the executor for one period.
 *
 * @param executor  Pointer to an initialized executor.
 * @param period_ns Period in nanoseconds.
 *
 * @retval NROS_RET_OK on success.
 *
 * @pre @p executor must point to an initialized executor.
 */
NROS_PUBLIC
nros_ret_t nros_executor_spin_one_period(struct nros_executor_t* executor, uint64_t period_ns);

/**
 * @brief Stop a spinning executor.
 *
 * @param executor  Pointer to an initialized executor.
 * @retval NROS_RET_OK on success.
 */
NROS_PUBLIC nros_ret_t nros_executor_stop(struct nros_executor_t* executor);

/**
 * @brief Finalise an executor.
 *
 * @param executor  Pointer to an initialized executor.
 * @retval NROS_RET_OK on success.
 */
NROS_PUBLIC nros_ret_t nros_executor_fini(struct nros_executor_t* executor);

/**
 * @brief Get the number of handles in the executor.
 * @param executor  Pointer to an executor.
 * @return Number of registered handles, or 0 if invalid.
 */
NROS_PUBLIC int nros_executor_get_handle_count(const struct nros_executor_t* executor);

/**
 * @brief Check if executor is valid (initialized).
 * @param executor  Pointer to an executor.
 * @return Non-zero if valid, 0 if invalid or NULL.
 */
NROS_PUBLIC int nros_executor_is_valid(const struct nros_executor_t* executor);

/**
 * @brief Get remaining total handle capacity.
 * @param executor  Pointer to an executor.
 * @return Remaining capacity, or 0 if invalid.
 */
NROS_PUBLIC int nros_executor_get_remaining_handles(const struct nros_executor_t* executor);

/**
 * @brief Get remaining subscription capacity.
 * @param executor  Pointer to an executor.
 * @return Remaining subscription slots, or 0 if invalid.
 */
NROS_PUBLIC int nros_executor_get_remaining_subscriptions(const struct nros_executor_t* executor);

/**
 * @brief Get remaining timer capacity.
 * @param executor  Pointer to an executor.
 * @return Remaining timer slots, or 0 if invalid.
 */
NROS_PUBLIC int nros_executor_get_remaining_timers(const struct nros_executor_t* executor);

/**
 * @brief Get remaining service capacity.
 * @param executor  Pointer to an executor.
 * @return Remaining service slots, or 0 if invalid.
 */
NROS_PUBLIC int nros_executor_get_remaining_services(const struct nros_executor_t* executor);

#ifdef __cplusplus
}
#endif

#endif /* NROS_EXECUTOR_H */

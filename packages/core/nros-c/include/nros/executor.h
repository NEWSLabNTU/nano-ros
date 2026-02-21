/**
 * nros executor API
 *
 * Executor for managing and processing subscriptions, timers, and services.
 *
 * Copyright 2024 nros contributors
 * Licensed under Apache-2.0
 */

#ifndef NROS_EXECUTOR_H
#define NROS_EXECUTOR_H

#include "nros/types.h"
#include "nros/visibility.h"
#include "nros/init.h"
#include "nros/subscription.h"
#include "nros/timer.h"
#include "nros/service.h"
#include "nros/guard_condition.h"

#ifdef __cplusplus
extern "C" {
#endif

// ============================================================================
// Executor Constants
// ============================================================================

/** Maximum number of handles in an executor */
#define NROS_EXECUTOR_MAX_HANDLES 16

/** Maximum number of subscriptions in an executor */
#define NROS_MAX_SUBSCRIPTIONS 8

/** Maximum number of timers in an executor */
#define NROS_MAX_TIMERS 8

/** Maximum number of services in an executor */
#define NROS_MAX_SERVICES 4

// ============================================================================
// Executor Types
// ============================================================================

/** Executor state */
typedef enum nros_executor_state_t {
    /** Not initialized */
    NROS_EXECUTOR_STATE_UNINITIALIZED = 0,
    /** Initialized and ready */
    NROS_EXECUTOR_STATE_INITIALIZED = 1,
    /** Currently spinning */
    NROS_EXECUTOR_STATE_SPINNING = 2,
    /** Shutdown */
    NROS_EXECUTOR_STATE_SHUTDOWN = 3,
} nros_executor_state_t;

/** Handle type for executor */
typedef enum nros_executor_handle_type_t {
    /** No handle (empty slot) */
    NROS_EXECUTOR_HANDLE_NONE = 0,
    /** Subscription handle */
    NROS_EXECUTOR_HANDLE_SUBSCRIPTION = 1,
    /** Timer handle */
    NROS_EXECUTOR_HANDLE_TIMER = 2,
    /** Service handle */
    NROS_EXECUTOR_HANDLE_SERVICE = 3,
    /** Client handle */
    NROS_EXECUTOR_HANDLE_CLIENT = 4,
    /** Guard condition handle */
    NROS_EXECUTOR_HANDLE_GUARD_CONDITION = 5,
} nros_executor_handle_type_t;

/** Callback invocation mode */
typedef enum nros_executor_invocation_t {
    /** Only invoke callback when new data is available */
    NROS_EXECUTOR_ON_NEW_DATA = 0,
    /** Always invoke callback (even with NULL data) */
    NROS_EXECUTOR_ALWAYS = 1,
} nros_executor_invocation_t;

// ============================================================================
// Trigger Types
// ============================================================================

/**
 * Trigger function type for executor.
 *
 * A trigger function receives a boolean array indicating which handles have
 * data ready, along with the count. Returns true if the executor should process.
 *
 * @param ready Pointer to boolean array (one per handle)
 * @param count Number of elements in the array
 * @param context User-provided context pointer
 * @return true if executor should process callbacks
 */
typedef bool (*nros_executor_trigger_t)(
    const bool *ready,
    size_t count,
    void *context);

// ============================================================================
// Executor Structures
// ============================================================================

/** Executor handle (union-like structure) */
typedef struct nros_executor_handle_t {
    /** Handle type */
    nros_executor_handle_type_t handle_type;
    /** Invocation mode (for subscriptions) */
    nros_executor_invocation_t invocation;
    /** Handle pointer (type depends on handle_type) */
    void *handle;
    /** Flag indicating if handle has new data ready */
    bool data_ready;
} nros_executor_handle_t;

/**
 * Executor structure.
 *
 * The executor manages a fixed array of handles and processes them
 * in the order they were added.
 */
typedef struct nros_executor_t {
    /** Current state */
    nros_executor_state_t state;
    /** Handle array */
    nros_executor_handle_t handles[NROS_EXECUTOR_MAX_HANDLES];
    /** Number of handles in use */
    size_t handle_count;
    /** Maximum handles (configured at init) */
    size_t max_handles;
    /** Timeout in nanoseconds for spin_some */
    uint64_t timeout_ns;
    /** Data communication semantics */
    int semantics;
    /** Pointer to support context */
    const nros_support_t *support;
    /** Trigger function (NULL = default "any" trigger) */
    nros_executor_trigger_t trigger;
    /** User context for trigger function */
    void *trigger_context;
    /** LET buffers (internal) */
    uint8_t _let_buffers[NROS_EXECUTOR_MAX_HANDLES][512];
    /** LET buffer lengths (internal) */
    size_t _let_buffer_lens[NROS_EXECUTOR_MAX_HANDLES];
    /** LET data availability flags (internal) */
    bool _let_data_available[NROS_EXECUTOR_MAX_HANDLES];
    /** Next invocation time in nanoseconds (internal) */
    uint64_t _invocation_time_ns;
    /** Number of subscription handles */
    size_t subscription_count;
    /** Number of timer handles */
    size_t timer_count;
    /** Number of service handles */
    size_t service_count;
} nros_executor_t;

// ============================================================================
// Executor Functions
// ============================================================================

/**
 * Get a zero-initialized executor.
 *
 * @return Zero-initialized executor structure
 */
NROS_PUBLIC
nros_executor_t nros_executor_get_zero_initialized(void);

/**
 * Initialize an executor.
 *
 * @param executor Pointer to a zero-initialized executor
 * @param support Pointer to an initialized support context
 * @param max_handles Maximum number of handles (capped at NROS_EXECUTOR_MAX_HANDLES)
 *
 * @return NROS_RET_OK on success
 * @return NROS_RET_INVALID_ARGUMENT if any pointer is NULL or max_handles is 0
 * @return NROS_RET_NOT_INIT if support is not initialized
 */
NROS_PUBLIC NROS_WARN_UNUSED
nros_ret_t nros_executor_init(
    nros_executor_t *executor,
    const nros_support_t *support,
    size_t max_handles);

/**
 * Set the executor timeout.
 *
 * @param executor Pointer to an initialized executor
 * @param timeout_ns Timeout in nanoseconds for spin_some
 *
 * @return NROS_RET_OK on success
 * @return NROS_RET_INVALID_ARGUMENT if executor is NULL
 * @return NROS_RET_NOT_INIT if not initialized
 */
NROS_PUBLIC NROS_WARN_UNUSED
nros_ret_t nros_executor_set_timeout(
    nros_executor_t *executor,
    uint64_t timeout_ns);

/**
 * Add a subscription to the executor.
 *
 * @param executor Pointer to an initialized executor
 * @param subscription Pointer to an initialized subscription
 * @param invocation When to invoke the callback
 *
 * @return NROS_RET_OK on success
 * @return NROS_RET_INVALID_ARGUMENT if any pointer is NULL
 * @return NROS_RET_FULL if executor is full
 * @return NROS_RET_NOT_INIT if not initialized
 */
NROS_PUBLIC NROS_WARN_UNUSED
nros_ret_t nros_executor_add_subscription(
    nros_executor_t *executor,
    nros_subscription_t *subscription,
    nros_executor_invocation_t invocation);

/**
 * Add a timer to the executor.
 *
 * @param executor Pointer to an initialized executor
 * @param timer Pointer to an initialized timer
 *
 * @return NROS_RET_OK on success
 * @return NROS_RET_INVALID_ARGUMENT if any pointer is NULL
 * @return NROS_RET_FULL if executor is full
 * @return NROS_RET_NOT_INIT if not initialized
 */
NROS_PUBLIC NROS_WARN_UNUSED
nros_ret_t nros_executor_add_timer(
    nros_executor_t *executor,
    nros_timer_t *timer);

/**
 * Add a service to the executor.
 *
 * @param executor Pointer to an initialized executor
 * @param service Pointer to an initialized service
 *
 * @return NROS_RET_OK on success
 * @return NROS_RET_INVALID_ARGUMENT if any pointer is NULL
 * @return NROS_RET_FULL if executor is full
 * @return NROS_RET_NOT_INIT if not initialized
 */
NROS_PUBLIC NROS_WARN_UNUSED
nros_ret_t nros_executor_add_service(
    nros_executor_t *executor,
    nros_service_t *service);

/**
 * Add a guard condition to the executor.
 *
 * Guard conditions allow other threads to wake up the executor.
 * When triggered, the callback (if set) will be executed.
 *
 * @param executor Pointer to an initialized executor
 * @param guard Pointer to an initialized guard condition
 *
 * @return NROS_RET_OK on success
 * @return NROS_RET_INVALID_ARGUMENT if any pointer is NULL
 * @return NROS_RET_FULL if executor is full
 * @return NROS_RET_NOT_INIT if not initialized
 */
NROS_PUBLIC NROS_WARN_UNUSED
nros_ret_t nros_executor_add_guard_condition(
    nros_executor_t *executor,
    nros_guard_condition_t *guard);

/**
 * Spin the executor once.
 *
 * This function checks for ready handles and processes them once.
 *
 * @param executor Pointer to an initialized executor
 * @param timeout_ns Timeout in nanoseconds (0 for non-blocking)
 *
 * @return NROS_RET_OK if callbacks were executed
 * @return NROS_RET_TIMEOUT if no callbacks were ready within timeout
 * @return NROS_RET_INVALID_ARGUMENT if executor is NULL
 * @return NROS_RET_NOT_INIT if not initialized
 */
NROS_PUBLIC NROS_WARN_UNUSED
nros_ret_t nros_executor_spin_some(
    nros_executor_t *executor,
    uint64_t timeout_ns);

/**
 * Spin the executor forever.
 *
 * This function continuously processes callbacks until shutdown.
 *
 * @param executor Pointer to an initialized executor
 *
 * @return NROS_RET_OK if shutdown gracefully
 * @return NROS_RET_INVALID_ARGUMENT if executor is NULL
 * @return NROS_RET_NOT_INIT if not initialized
 */
NROS_PUBLIC NROS_WARN_UNUSED
nros_ret_t nros_executor_spin(nros_executor_t *executor);

/**
 * Spin the executor with a fixed period.
 *
 * This function processes callbacks at a fixed rate.
 *
 * @param executor Pointer to an initialized executor
 * @param period_ns Period in nanoseconds
 *
 * @return NROS_RET_OK if shutdown gracefully
 * @return NROS_RET_INVALID_ARGUMENT if executor is NULL or period is 0
 * @return NROS_RET_NOT_INIT if not initialized
 */
NROS_PUBLIC NROS_WARN_UNUSED
nros_ret_t nros_executor_spin_period(
    nros_executor_t *executor,
    uint64_t period_ns);

/**
 * Spin the executor for one period.
 *
 * This function processes callbacks once and sleeps for the remainder
 * of the period. Matches rclc's rclc_executor_spin_one_period().
 *
 * @param executor Pointer to an initialized executor
 * @param period_ns Period in nanoseconds
 *
 * @return NROS_RET_OK on success
 * @return NROS_RET_INVALID_ARGUMENT if executor is NULL or period is 0
 * @return NROS_RET_NOT_INIT if not initialized
 */
NROS_PUBLIC NROS_WARN_UNUSED
nros_ret_t nros_executor_spin_one_period(
    nros_executor_t *executor,
    uint64_t period_ns);

/**
 * Stop a spinning executor.
 *
 * @param executor Pointer to a spinning executor
 *
 * @return NROS_RET_OK on success
 * @return NROS_RET_INVALID_ARGUMENT if executor is NULL
 */
NROS_PUBLIC NROS_WARN_UNUSED
nros_ret_t nros_executor_stop(nros_executor_t *executor);

/**
 * Finalize an executor.
 *
 * @param executor Pointer to an initialized executor
 *
 * @return NROS_RET_OK on success
 * @return NROS_RET_INVALID_ARGUMENT if executor is NULL
 * @return NROS_RET_NOT_INIT if not initialized
 */
NROS_PUBLIC NROS_WARN_UNUSED
nros_ret_t nros_executor_fini(nros_executor_t *executor);

/**
 * Get the number of handles in the executor.
 *
 * @param executor Pointer to an executor
 *
 * @return Number of handles, or -1 if invalid
 */
NROS_PUBLIC
int nros_executor_get_handle_count(const nros_executor_t *executor);

/**
 * Check if executor is valid (initialized).
 *
 * @param executor Pointer to an executor
 *
 * @return Non-zero if valid, 0 if invalid or NULL
 */
NROS_PUBLIC
int nros_executor_is_valid(const nros_executor_t *executor);

// ============================================================================
// Capacity Queries
// ============================================================================

/**
 * Get remaining total handle capacity.
 *
 * @param executor Pointer to an executor
 *
 * @return Remaining capacity, or -1 if NULL
 */
NROS_PUBLIC
int nros_executor_get_remaining_handles(const nros_executor_t *executor);

/**
 * Get remaining subscription capacity.
 *
 * @param executor Pointer to an executor
 *
 * @return Remaining subscription capacity, or -1 if NULL
 */
NROS_PUBLIC
int nros_executor_get_remaining_subscriptions(const nros_executor_t *executor);

/**
 * Get remaining timer capacity.
 *
 * @param executor Pointer to an executor
 *
 * @return Remaining timer capacity, or -1 if NULL
 */
NROS_PUBLIC
int nros_executor_get_remaining_timers(const nros_executor_t *executor);

/**
 * Get remaining service capacity.
 *
 * @param executor Pointer to an executor
 *
 * @return Remaining service capacity, or -1 if NULL
 */
NROS_PUBLIC
int nros_executor_get_remaining_services(const nros_executor_t *executor);

// ============================================================================
// Trigger Functions
// ============================================================================

/**
 * Set the trigger condition for the executor.
 *
 * The trigger controls when spin_some processes callbacks.
 * Pass NULL for the trigger function to use the default "any" behavior.
 *
 * @param executor Pointer to an initialized executor
 * @param trigger Trigger function (NULL for default "any" behavior)
 * @param context User context passed to trigger function (may be NULL)
 *
 * @return NROS_RET_OK on success
 * @return NROS_RET_INVALID_ARGUMENT if executor is NULL
 * @return NROS_RET_NOT_INIT if not initialized
 */
NROS_PUBLIC NROS_WARN_UNUSED
nros_ret_t nros_executor_set_trigger(
    nros_executor_t *executor,
    nros_executor_trigger_t trigger,
    void *context);

/** Built-in trigger: fire when ANY handle has data ready (default). */
NROS_PUBLIC
bool nros_executor_trigger_any(const bool *ready, size_t count, void *context);

/** Built-in trigger: fire when ALL handles have data ready. */
NROS_PUBLIC
bool nros_executor_trigger_all(const bool *ready, size_t count, void *context);

/** Built-in trigger: always fire (unconditionally). */
NROS_PUBLIC
bool nros_executor_trigger_always(const bool *ready, size_t count, void *context);

/**
 * Built-in trigger: fire when handle at a specific index has data.
 * Pass the handle index (cast to void*) as the context parameter.
 */
NROS_PUBLIC
bool nros_executor_trigger_one(const bool *ready, size_t count, void *context);

#ifdef __cplusplus
}
#endif

#endif // NROS_EXECUTOR_H

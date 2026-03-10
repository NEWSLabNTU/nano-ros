/**
 * @file timer.h
 * @brief Periodic timer API.
 *
 * Create timers with nros_timer_init() and register them with an
 * executor via nros_executor_add_timer().
 */

#ifndef NROS_TIMER_H
#define NROS_TIMER_H

#include "nros/types.h"

#ifdef __cplusplus
extern "C" {
#endif

/* Forward declarations */
struct nros_support_t;
struct nros_timer_t;

/* ===================================================================
 * Types
 * =================================================================== */

/** Timer state. */
typedef enum nros_timer_state_t {
    /** Not initialized. */
    NROS_TIMER_STATE_UNINITIALIZED = 0,
    /** Initialized and running. */
    NROS_TIMER_STATE_RUNNING = 1,
    /** Initialized but canceled. */
    NROS_TIMER_STATE_CANCELED = 2,
    /** Shutdown. */
    NROS_TIMER_STATE_SHUTDOWN = 3,
} nros_timer_state_t;

/**
 * Timer callback function type.
 *
 * @param timer   Pointer to the timer that triggered.
 * @param context User-provided context pointer.
 */
typedef void (*nros_timer_callback_t)(struct nros_timer_t* timer, void* context);

/** Timer structure. */
typedef struct nros_timer_t {
    /** Current state. */
    enum nros_timer_state_t state;
    /** Period in nanoseconds. */
    uint64_t period_ns;
    /** Last trigger time in nanoseconds. */
    uint64_t last_call_time_ns;
    /** User callback function. */
    nros_timer_callback_t callback;
    /** User context pointer. */
    void* context;
    /** Pointer to parent support context. */
    const struct nros_support_t* support;
    /** Handle ID from executor registration (SIZE_MAX = not registered). */
    size_t handle_id;
    /** Opaque pointer to internal executor (set by nros_executor_add_timer). */
    void* _executor;
} nros_timer_t;

/* ===================================================================
 * Functions
 * =================================================================== */

/**
 * @brief Get a zero-initialized timer.
 * @return Zero-initialized @ref nros_timer_t.
 */
NROS_PUBLIC struct nros_timer_t nros_timer_get_zero_initialized(void);

/**
 * @brief Initialise a timer.
 *
 * @param timer     Pointer to a zero-initialized timer.
 * @param support   Pointer to an initialized support context.
 * @param period_ns Timer period in nanoseconds.
 * @param callback  Callback function to invoke when timer fires.
 * @param context   User context pointer (can be NULL).
 *
 * @retval NROS_RET_OK               on success.
 * @retval NROS_RET_INVALID_ARGUMENT  if any required pointer is NULL
 *                                    or @p period_ns is 0.
 * @retval NROS_RET_NOT_INIT          if @p support is not initialized.
 *
 * @pre All required pointers must be valid.
 * @pre @p callback must be a valid function pointer.
 */
NROS_PUBLIC
nros_ret_t nros_timer_init(struct nros_timer_t* timer, const struct nros_support_t* support,
                           uint64_t period_ns, nros_timer_callback_t callback, void* context);

/**
 * @brief Cancel a timer.
 *
 * A canceled timer will not fire, but can be reset to start again.
 *
 * @param timer  Pointer to an initialized timer.
 *
 * @retval NROS_RET_OK               on success.
 * @retval NROS_RET_INVALID_ARGUMENT  if @p timer is NULL.
 * @retval NROS_RET_NOT_INIT          if not initialized.
 */
NROS_PUBLIC nros_ret_t nros_timer_cancel(struct nros_timer_t* timer);

/**
 * @brief Reset a timer.
 *
 * Resets the timer's last call time and starts it running again if it
 * was canceled.
 *
 * @param timer  Pointer to an initialized timer.
 *
 * @retval NROS_RET_OK               on success.
 * @retval NROS_RET_INVALID_ARGUMENT  if @p timer is NULL.
 * @retval NROS_RET_NOT_INIT          if not initialized.
 */
NROS_PUBLIC nros_ret_t nros_timer_reset(struct nros_timer_t* timer);

/**
 * @brief Finalise a timer.
 *
 * @param timer  Pointer to an initialized timer.
 *
 * @retval NROS_RET_OK               on success.
 * @retval NROS_RET_INVALID_ARGUMENT  if @p timer is NULL.
 * @retval NROS_RET_NOT_INIT          if not initialized.
 */
NROS_PUBLIC nros_ret_t nros_timer_fini(struct nros_timer_t* timer);

/**
 * @brief Check if timer is ready to fire.
 *
 * @param timer           Pointer to an initialized timer.
 * @param current_time_ns Current time in nanoseconds.
 * @return Non-zero if timer is ready, 0 otherwise.
 */
NROS_PUBLIC int nros_timer_is_ready(const struct nros_timer_t* timer, uint64_t current_time_ns);

/**
 * @brief Call the timer callback and update last call time.
 *
 * This is called by the executor when the timer is ready.
 *
 * @param timer           Pointer to an initialized timer.
 * @param current_time_ns Current time in nanoseconds.
 *
 * @retval NROS_RET_OK               on success.
 * @retval NROS_RET_INVALID_ARGUMENT  if @p timer is NULL.
 * @retval NROS_RET_NOT_INIT          if not initialized or not running.
 */
NROS_PUBLIC nros_ret_t nros_timer_call(struct nros_timer_t* timer, uint64_t current_time_ns);

/**
 * @brief Check if timer is valid (initialized and not shutdown).
 *
 * @param timer  Pointer to a timer.
 * @return Non-zero if valid, 0 if invalid or NULL.
 */
NROS_PUBLIC int nros_timer_is_valid(const struct nros_timer_t* timer);

/**
 * @brief Get the timer period in nanoseconds.
 *
 * @param timer  Pointer to a timer.
 * @return Period in nanoseconds, or 0 if invalid.
 */
NROS_PUBLIC uint64_t nros_timer_get_period(const struct nros_timer_t* timer);

/**
 * @brief Get the time until next timer firing.
 *
 * @param timer           Pointer to a timer.
 * @param current_time_ns Current time in nanoseconds.
 * @return Time until next firing in nanoseconds, or 0 if ready now or invalid.
 */
NROS_PUBLIC
uint64_t nros_timer_get_time_until_next_call(const struct nros_timer_t* timer,
                                             uint64_t current_time_ns);

#ifdef __cplusplus
}
#endif

#endif /* NROS_TIMER_H */

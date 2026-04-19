/**
 * @file guard_condition.h
 * @brief Manual wake-up trigger API.
 *
 * Guard conditions provide a mechanism for signalling an executor from
 * outside the middleware (e.g., from an ISR or another thread).
 */

#ifndef NROS_GUARD_CONDITION_H
#define NROS_GUARD_CONDITION_H

#include "nros/types.h"

#ifdef __cplusplus
extern "C" {
#endif

/* Forward declarations */
struct nros_support_t;

/* ===================================================================
 * Types
 * =================================================================== */

/** Guard condition state. */
typedef enum nros_guard_condition_state_t {
    /** Not initialized. */
    NROS_GUARD_CONDITION_STATE_UNINITIALIZED = 0,
    /** Initialized and ready. */
    NROS_GUARD_CONDITION_STATE_INITIALIZED = 1,
    /** Shutdown. */
    NROS_GUARD_CONDITION_STATE_SHUTDOWN = 2,
} nros_guard_condition_state_t;

/**
 * Guard condition callback type.
 *
 * Called by the executor when the guard condition is triggered.
 *
 * @param context User-provided context pointer.
 */
typedef void (*nros_guard_condition_callback_t)(void* context);

/** Guard condition structure. */
typedef struct nros_guard_condition_t {
    /** Current state. */
    enum nros_guard_condition_state_t state;
    /** Triggered flag (volatile for cross-thread visibility). */
    bool triggered;
    /** Callback function. */
    nros_guard_condition_callback_t callback;
    /** User context pointer. */
    void* context;
    /** Pointer to parent support context. */
    const struct nros_support_t* _support;
    /** Handle ID from executor registration (SIZE_MAX = not registered). */
    size_t handle_id;
    /** Inline opaque storage for the guard condition handle. */
    uint64_t _guard_opaque[NROS_GUARD_HANDLE_OPAQUE_U64S];
} nros_guard_condition_t;

/* ===================================================================
 * Functions
 * =================================================================== */

/**
 * @brief Get a zero-initialized guard condition.
 * @return Zero-initialized @ref nros_guard_condition_t.
 */
NROS_PUBLIC struct nros_guard_condition_t nros_guard_condition_get_zero_initialized(void);

/**
 * @brief Initialise a guard condition.
 *
 * @param guard   Pointer to a zero-initialized guard condition.
 * @param support Pointer to an initialized support context.
 *
 * @retval NROS_RET_OK on success.
 */
NROS_PUBLIC
nros_ret_t nros_guard_condition_init(struct nros_guard_condition_t* guard,
                                     const struct nros_support_t* support);

/**
 * @brief Set the guard condition callback.
 *
 * @param guard    Pointer to an initialized guard condition.
 * @param callback Callback function, or NULL to clear.
 * @param context  User context passed to the callback.
 *
 * @retval NROS_RET_OK on success.
 */
NROS_PUBLIC
nros_ret_t nros_guard_condition_set_callback(struct nros_guard_condition_t* guard,
                                             nros_guard_condition_callback_t callback,
                                             void* context);

/**
 * @brief Trigger a guard condition.
 *
 * This function is designed to be thread-safe.  When registered with an
 * executor, it triggers via the executor's guard handle (atomic flag in
 * the arena).  Otherwise falls back to the local triggered flag.
 *
 * @param guard  Pointer to an initialized guard condition.
 * @retval NROS_RET_OK on success.
 */
NROS_PUBLIC nros_ret_t nros_guard_condition_trigger(struct nros_guard_condition_t* guard);

/**
 * @brief Check if the guard condition is triggered.
 *
 * @param guard  Pointer to a guard condition.
 * @return @c true if triggered, @c false otherwise.
 */
NROS_PUBLIC bool nros_guard_condition_is_triggered(const struct nros_guard_condition_t* guard);

/**
 * @brief Clear the triggered flag.
 *
 * @param guard  Pointer to an initialized guard condition.
 * @retval NROS_RET_OK on success.
 */
NROS_PUBLIC nros_ret_t nros_guard_condition_clear(struct nros_guard_condition_t* guard);

/**
 * @brief Check if guard condition is valid (initialized).
 *
 * @param guard  Pointer to a guard condition.
 * @return @c true if valid, @c false if invalid or NULL.
 */
NROS_PUBLIC bool nros_guard_condition_is_valid(const struct nros_guard_condition_t* guard);

/**
 * @brief Finalise a guard condition.
 *
 * @param guard  Pointer to an initialized guard condition.
 * @retval NROS_RET_OK on success.
 */
NROS_PUBLIC nros_ret_t nros_guard_condition_fini(struct nros_guard_condition_t* guard);

#ifdef __cplusplus
}
#endif

#endif /* NROS_GUARD_CONDITION_H */

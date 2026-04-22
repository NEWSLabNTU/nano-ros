/**
 * @file init.h
 * @brief Library initialisation and support context.
 *
 * The support context (@ref nros_support_t) is the entry point for all
 * nros operations.  It manages the middleware session (zenoh-pico) and
 * must be initialised before any nodes, publishers, or subscriptions
 * are created.
 *
 * Typical usage:
 * @code
 * nros_support_t support = nros_support_get_zero_initialized();
 * nros_support_init(&support, NULL, 0);
 * // ... create nodes, publishers, etc.
 * nros_support_fini(&support);
 * @endcode
 */

#ifndef NROS_INIT_H
#define NROS_INIT_H

#include "nros/types.h"

#ifdef __cplusplus
extern "C" {
#endif

/* ===================================================================
 * Types
 * =================================================================== */

/** Support context state. */
typedef enum nros_support_state_t {
    /** Not initialized. */
    NROS_SUPPORT_STATE_UNINITIALIZED = 0,
    /** Initialized and ready. */
    NROS_SUPPORT_STATE_INITIALIZED = 1,
    /** Shutdown. */
    NROS_SUPPORT_STATE_SHUTDOWN = 2,
} nros_support_state_t;

/**
 * Support context structure.
 *
 * This is the main context for nros, similar to @c rclc_support_t.
 * It manages the middleware session and provides shared resources.
 */
typedef struct nros_support_t {
    /** Current state. */
    enum nros_support_state_t state;
    /** Domain ID (ROS_DOMAIN_ID). */
    uint8_t domain_id;
    /** Locator string storage. */
    uint8_t locator[NROS_MAX_LOCATOR_LEN];
    /** Locator string length. */
    size_t locator_len;
    /** Inline opaque storage for the Rust middleware session.
     *  Sized from `size_of::<RmwSession>()` via the Phase 87 probe.
     *  Avoids heap allocation — managed by nros_support_init/fini. */
    _Alignas(8) uint8_t _opaque[NROS_SESSION_SIZE];
} nros_support_t;

/* ===================================================================
 * Functions
 * =================================================================== */

/**
 * @brief Get a zero-initialized support context.
 *
 * Returns a stack-allocated struct that must be passed to
 * nros_support_init() before use.
 *
 * @return Zero-initialized @ref nros_support_t.
 */
NROS_PUBLIC struct nros_support_t nros_support_get_zero_initialized(void);

/**
 * @brief Initialise the support context.
 *
 * Opens a middleware session and prepares the context for creating
 * nodes, publishers, and subscribers.
 *
 * @param support   Pointer to a zero-initialized support context.
 * @param locator   Middleware locator string (e.g., "tcp/127.0.0.1:7447"),
 *                  or NULL for the default.
 * @param domain_id ROS domain ID (0–232).
 *
 * @retval NROS_RET_OK               on success.
 * @retval NROS_RET_INVALID_ARGUMENT  if @p support is NULL.
 * @retval NROS_RET_ERROR             on initialisation failure.
 *
 * @pre @p support must point to a zero-initialized @ref nros_support_t.
 * @pre @p locator must be a valid null-terminated string or NULL.
 */
NROS_PUBLIC
nros_ret_t nros_support_init(struct nros_support_t* support, const char* locator,
                             uint8_t domain_id);

/**
 * @brief Initialise the support context with a session name.
 *
 * Like nros_support_init(), but allows specifying a session name for
 * XRCE-DDS key derivation. Different XRCE clients on the same agent
 * MUST use different session names; otherwise the agent treats them
 * as the same client and won't relay data between them.
 *
 * @param support       Pointer to a zero-initialized support context.
 * @param locator       Middleware locator string, or NULL for default.
 * @param domain_id     ROS domain ID (0–232).
 * @param session_name  Session name for XRCE key derivation, or NULL for default.
 */
NROS_PUBLIC
nros_ret_t nros_support_init_named(struct nros_support_t* support, const char* locator,
                                   uint8_t domain_id, const char* session_name);

/**
 * @brief Finalise the support context.
 *
 * Closes the middleware session and releases all resources.
 *
 * @param support  Pointer to an initialized support context.
 *
 * @retval NROS_RET_OK               on success.
 * @retval NROS_RET_INVALID_ARGUMENT  if @p support is NULL.
 * @retval NROS_RET_NOT_INIT          if not initialized.
 *
 * @pre @p support must point to an initialized @ref nros_support_t.
 */
NROS_PUBLIC nros_ret_t nros_support_fini(struct nros_support_t* support);

/**
 * @brief Check if the support context is valid (initialized).
 *
 * @param support  Pointer to a support context.
 * @return @c true if valid, @c false if invalid or NULL.
 */
NROS_PUBLIC bool nros_support_is_valid(const struct nros_support_t* support);

#ifdef __cplusplus
}
#endif

#endif /* NROS_INIT_H */

/**
 * nros initialization functions
 *
 * Support context initialization and finalization.
 *
 * Copyright 2024 nros contributors
 * Licensed under Apache-2.0
 */

#ifndef NROS_INIT_H
#define NROS_INIT_H

#include "nros/types.h"
#include "nros/visibility.h"

#ifdef __cplusplus
extern "C" {
#endif

// ============================================================================
// Support Context State
// ============================================================================

/** Support context state */
typedef enum nros_support_state_t {
    /** Not initialized */
    NROS_SUPPORT_STATE_UNINITIALIZED = 0,
    /** Initialized and ready */
    NROS_SUPPORT_STATE_INITIALIZED = 1,
    /** Shutdown */
    NROS_SUPPORT_STATE_SHUTDOWN = 2,
} nros_support_state_t;

// ============================================================================
// Support Context Structure
// ============================================================================

/**
 * Support context structure.
 *
 * This is the main context for nros, similar to rclc_support_t.
 * It manages the middleware session and provides shared resources.
 */
typedef struct nros_support_t {
    /** Current state */
    nros_support_state_t state;
    /** Domain ID (ROS_DOMAIN_ID) */
    uint8_t domain_id;
    /** Locator string storage */
    uint8_t locator[NROS_MAX_LOCATOR_LEN];
    /** Locator string length */
    size_t locator_len;
    /** Opaque pointer to internal Rust context */
    void *internal;
} nros_support_t;

// ============================================================================
// Support Context Functions
// ============================================================================

/**
 * Get a zero-initialized support context.
 *
 * @return Zero-initialized support context
 */
NROS_PUBLIC
nros_support_t nros_support_get_zero_initialized(void);

/**
 * Initialize the support context.
 *
 * This function initializes the middleware session and prepares the context
 * for creating nodes, publishers, and subscribers.
 *
 * @param support Pointer to a zero-initialized support context
 * @param locator Middleware locator string, or NULL for default.
 *                Zenoh: "tcp/127.0.0.1:7447"; XRCE-DDS: "127.0.0.1:2019"
 * @param domain_id ROS domain ID (0-232)
 *
 * @return NROS_RET_OK on success
 * @return NROS_RET_INVALID_ARGUMENT if support is NULL
 * @return NROS_RET_ERROR on initialization failure
 */
NROS_PUBLIC NROS_WARN_UNUSED
nros_ret_t nros_support_init(
    nros_support_t *support,
    const char *locator,
    uint8_t domain_id);

/**
 * Finalize the support context.
 *
 * This function closes the middleware session and releases all resources.
 *
 * @param support Pointer to an initialized support context
 *
 * @return NROS_RET_OK on success
 * @return NROS_RET_INVALID_ARGUMENT if support is NULL
 * @return NROS_RET_NOT_INIT if not initialized
 */
NROS_PUBLIC NROS_WARN_UNUSED
nros_ret_t nros_support_fini(nros_support_t *support);

/**
 * Check if support context is valid (initialized).
 *
 * @param support Pointer to a support context
 *
 * @return Non-zero if valid, 0 if invalid or NULL
 */
NROS_PUBLIC
int nros_support_is_valid(const nros_support_t *support);

#ifdef __cplusplus
}
#endif

#endif // NROS_INIT_H

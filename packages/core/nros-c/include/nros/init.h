/**
 * nros initialization functions
 *
 * Support context initialization and finalization.
 *
 * Copyright 2024 nros contributors
 * Licensed under Apache-2.0
 */

#ifndef NANO_ROS_INIT_H
#define NANO_ROS_INIT_H

#include "nros/types.h"
#include "nros/visibility.h"

#ifdef __cplusplus
extern "C" {
#endif

// ============================================================================
// Support Context State
// ============================================================================

/** Support context state */
typedef enum nano_ros_support_state_t {
    /** Not initialized */
    NANO_ROS_SUPPORT_STATE_UNINITIALIZED = 0,
    /** Initialized and ready */
    NANO_ROS_SUPPORT_STATE_INITIALIZED = 1,
    /** Shutdown */
    NANO_ROS_SUPPORT_STATE_SHUTDOWN = 2,
} nano_ros_support_state_t;

// ============================================================================
// Support Context Structure
// ============================================================================

/**
 * Support context structure.
 *
 * This is the main context for nros, similar to rclc_support_t.
 * It manages the zenoh session and provides shared resources.
 */
typedef struct nano_ros_support_t {
    /** Current state */
    nano_ros_support_state_t state;
    /** Domain ID (ROS_DOMAIN_ID) */
    uint8_t domain_id;
    /** Locator string storage */
    uint8_t locator[NANO_ROS_MAX_LOCATOR_LEN];
    /** Locator string length */
    size_t locator_len;
    /** Opaque pointer to internal Rust context */
    void *internal;
} nano_ros_support_t;

// ============================================================================
// Support Context Functions
// ============================================================================

/**
 * Get a zero-initialized support context.
 *
 * @return Zero-initialized support context
 */
NANO_ROS_PUBLIC
nano_ros_support_t nano_ros_support_get_zero_initialized(void);

/**
 * Initialize the support context.
 *
 * This function initializes the zenoh session and prepares the context
 * for creating nodes, publishers, and subscribers.
 *
 * @param support Pointer to a zero-initialized support context
 * @param locator Zenoh locator string (e.g., "tcp/127.0.0.1:7447"), or NULL for default
 * @param domain_id ROS domain ID (0-232)
 *
 * @return NANO_ROS_RET_OK on success
 * @return NANO_ROS_RET_INVALID_ARGUMENT if support is NULL
 * @return NANO_ROS_RET_ERROR on initialization failure
 */
NANO_ROS_PUBLIC NANO_ROS_WARN_UNUSED
nano_ros_ret_t nano_ros_support_init(
    nano_ros_support_t *support,
    const char *locator,
    uint8_t domain_id);

/**
 * Finalize the support context.
 *
 * This function closes the zenoh session and releases all resources.
 *
 * @param support Pointer to an initialized support context
 *
 * @return NANO_ROS_RET_OK on success
 * @return NANO_ROS_RET_INVALID_ARGUMENT if support is NULL
 * @return NANO_ROS_RET_NOT_INIT if not initialized
 */
NANO_ROS_PUBLIC NANO_ROS_WARN_UNUSED
nano_ros_ret_t nano_ros_support_fini(nano_ros_support_t *support);

/**
 * Check if support context is valid (initialized).
 *
 * @param support Pointer to a support context
 *
 * @return Non-zero if valid, 0 if invalid or NULL
 */
NANO_ROS_PUBLIC
int nano_ros_support_is_valid(const nano_ros_support_t *support);

#ifdef __cplusplus
}
#endif

#endif // NANO_ROS_INIT_H

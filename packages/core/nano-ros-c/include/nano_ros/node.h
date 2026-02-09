/**
 * nano-ros node API
 *
 * Node creation and management functions.
 *
 * Copyright 2024 nano-ros contributors
 * Licensed under Apache-2.0
 */

#ifndef NANO_ROS_NODE_H
#define NANO_ROS_NODE_H

#include "nano_ros/types.h"
#include "nano_ros/visibility.h"
#include "nano_ros/init.h"

#ifdef __cplusplus
extern "C" {
#endif

// ============================================================================
// Node State
// ============================================================================

/** Node state */
typedef enum nano_ros_node_state_t {
    /** Not initialized */
    NANO_ROS_NODE_STATE_UNINITIALIZED = 0,
    /** Initialized and ready */
    NANO_ROS_NODE_STATE_INITIALIZED = 1,
    /** Shutdown */
    NANO_ROS_NODE_STATE_SHUTDOWN = 2,
} nano_ros_node_state_t;

// ============================================================================
// Node Structure
// ============================================================================

/**
 * Node structure.
 *
 * Represents a ROS 2 node with a name and namespace.
 */
typedef struct nano_ros_node_t {
    /** Current state */
    nano_ros_node_state_t state;
    /** Node name storage */
    uint8_t name[NANO_ROS_MAX_NAME_LEN];
    /** Node name length */
    size_t name_len;
    /** Namespace storage */
    uint8_t namespace_[NANO_ROS_MAX_NAMESPACE_LEN];
    /** Namespace length */
    size_t namespace_len;
    /** Pointer to parent support context */
    const nano_ros_support_t *support;
    /** Opaque pointer to internal Rust node */
    void *internal;
} nano_ros_node_t;

// ============================================================================
// Node Functions
// ============================================================================

/**
 * Get a zero-initialized node.
 *
 * @return Zero-initialized node structure
 */
NANO_ROS_PUBLIC
nano_ros_node_t nano_ros_node_get_zero_initialized(void);

/**
 * Initialize a node with default options.
 *
 * @param node Pointer to a zero-initialized node
 * @param support Pointer to an initialized support context
 * @param name Node name (null-terminated string)
 * @param namespace_ Node namespace (null-terminated string, use "/" for root)
 *
 * @return NANO_ROS_RET_OK on success
 * @return NANO_ROS_RET_INVALID_ARGUMENT if any pointer is NULL or strings are invalid
 * @return NANO_ROS_RET_NOT_INIT if support is not initialized
 * @return NANO_ROS_RET_ERROR on initialization failure
 */
NANO_ROS_PUBLIC NANO_ROS_WARN_UNUSED
nano_ros_ret_t nano_ros_node_init(
    nano_ros_node_t *node,
    const nano_ros_support_t *support,
    const char *name,
    const char *namespace_);

/**
 * Finalize a node.
 *
 * @param node Pointer to an initialized node
 *
 * @return NANO_ROS_RET_OK on success
 * @return NANO_ROS_RET_INVALID_ARGUMENT if node is NULL
 * @return NANO_ROS_RET_NOT_INIT if not initialized
 */
NANO_ROS_PUBLIC NANO_ROS_WARN_UNUSED
nano_ros_ret_t nano_ros_node_fini(nano_ros_node_t *node);

/**
 * Get the node name.
 *
 * @param node Pointer to an initialized node
 *
 * @return Pointer to the node name (null-terminated), or NULL if invalid
 */
NANO_ROS_PUBLIC
const char *nano_ros_node_get_name(const nano_ros_node_t *node);

/**
 * Get the node namespace.
 *
 * @param node Pointer to an initialized node
 *
 * @return Pointer to the node namespace (null-terminated), or NULL if invalid
 */
NANO_ROS_PUBLIC
const char *nano_ros_node_get_namespace(const nano_ros_node_t *node);

#ifdef __cplusplus
}
#endif

#endif // NANO_ROS_NODE_H

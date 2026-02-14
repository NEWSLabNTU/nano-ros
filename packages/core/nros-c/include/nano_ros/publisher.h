/**
 * nros publisher API
 *
 * Publisher creation and message publishing functions.
 *
 * Copyright 2024 nros contributors
 * Licensed under Apache-2.0
 */

#ifndef NANO_ROS_PUBLISHER_H
#define NANO_ROS_PUBLISHER_H

#include "nano_ros/types.h"
#include "nano_ros/visibility.h"
#include "nano_ros/node.h"

#ifdef __cplusplus
extern "C" {
#endif

// ============================================================================
// Publisher State
// ============================================================================

/** Publisher state */
typedef enum nano_ros_publisher_state_t {
    /** Not initialized */
    NANO_ROS_PUBLISHER_STATE_UNINITIALIZED = 0,
    /** Initialized and ready */
    NANO_ROS_PUBLISHER_STATE_INITIALIZED = 1,
    /** Shutdown */
    NANO_ROS_PUBLISHER_STATE_SHUTDOWN = 2,
} nano_ros_publisher_state_t;

// ============================================================================
// Publisher Structure
// ============================================================================

/** Publisher structure */
typedef struct nano_ros_publisher_t {
    /** Current state */
    nano_ros_publisher_state_t state;
    /** Topic name storage */
    uint8_t topic_name[NANO_ROS_MAX_TOPIC_LEN];
    /** Topic name length */
    size_t topic_name_len;
    /** Type name storage */
    uint8_t type_name[NANO_ROS_MAX_TYPE_NAME_LEN];
    /** Type name length */
    size_t type_name_len;
    /** Type hash storage */
    uint8_t type_hash[NANO_ROS_MAX_TYPE_HASH_LEN];
    /** Type hash length */
    size_t type_hash_len;
    /** Pointer to parent node */
    const nros_node_t *node;
    /** Opaque pointer to internal Rust publisher */
    void *internal;
} nano_ros_publisher_t;

// ============================================================================
// Publisher Functions
// ============================================================================

/**
 * Get a zero-initialized publisher.
 *
 * @return Zero-initialized publisher structure
 */
NANO_ROS_PUBLIC
nano_ros_publisher_t nano_ros_publisher_get_zero_initialized(void);

/**
 * Initialize a publisher with default QoS.
 *
 * @param publisher Pointer to a zero-initialized publisher
 * @param node Pointer to an initialized node
 * @param type_info Pointer to message type information
 * @param topic_name Topic name (null-terminated string)
 *
 * @return NANO_ROS_RET_OK on success
 * @return NANO_ROS_RET_INVALID_ARGUMENT if any pointer is NULL
 * @return NANO_ROS_RET_NOT_INIT if node is not initialized
 * @return NANO_ROS_RET_ERROR on initialization failure
 */
NANO_ROS_PUBLIC NANO_ROS_WARN_UNUSED
nano_ros_ret_t nano_ros_publisher_init(
    nano_ros_publisher_t *publisher,
    const nros_node_t *node,
    const nano_ros_message_type_t *type_info,
    const char *topic_name);

/**
 * Initialize a publisher with custom QoS.
 *
 * @param publisher Pointer to a zero-initialized publisher
 * @param node Pointer to an initialized node
 * @param type_info Pointer to message type information
 * @param topic_name Topic name (null-terminated string)
 * @param qos Pointer to QoS settings (NULL for default)
 *
 * @return NANO_ROS_RET_OK on success
 * @return NANO_ROS_RET_INVALID_ARGUMENT if any required pointer is NULL
 * @return NANO_ROS_RET_NOT_INIT if node is not initialized
 * @return NANO_ROS_RET_ERROR on initialization failure
 */
NANO_ROS_PUBLIC NANO_ROS_WARN_UNUSED
nano_ros_ret_t nano_ros_publisher_init_with_qos(
    nano_ros_publisher_t *publisher,
    const nros_node_t *node,
    const nano_ros_message_type_t *type_info,
    const char *topic_name,
    const nano_ros_qos_t *qos);

/**
 * Publish raw CDR-serialized data.
 *
 * @param publisher Pointer to an initialized publisher
 * @param data Pointer to CDR-serialized message data
 * @param len Length of data in bytes
 *
 * @return NANO_ROS_RET_OK on success
 * @return NANO_ROS_RET_INVALID_ARGUMENT if any pointer is NULL or len is 0
 * @return NANO_ROS_RET_NOT_INIT if publisher is not initialized
 * @return NANO_ROS_RET_PUBLISH_FAILED on publish failure
 */
NANO_ROS_PUBLIC NANO_ROS_WARN_UNUSED
nano_ros_ret_t nano_ros_publish_raw(
    const nano_ros_publisher_t *publisher,
    const uint8_t *data,
    size_t len);

/**
 * Finalize a publisher.
 *
 * @param publisher Pointer to an initialized publisher
 *
 * @return NANO_ROS_RET_OK on success
 * @return NANO_ROS_RET_INVALID_ARGUMENT if publisher is NULL
 * @return NANO_ROS_RET_NOT_INIT if not initialized
 */
NANO_ROS_PUBLIC NANO_ROS_WARN_UNUSED
nano_ros_ret_t nano_ros_publisher_fini(nano_ros_publisher_t *publisher);

/**
 * Get the topic name of a publisher.
 *
 * @param publisher Pointer to a publisher
 *
 * @return Pointer to topic name (null-terminated), or NULL if invalid
 */
NANO_ROS_PUBLIC
const char *nano_ros_publisher_get_topic_name(const nano_ros_publisher_t *publisher);

/**
 * Check if publisher is valid (initialized).
 *
 * @param publisher Pointer to a publisher
 *
 * @return Non-zero if valid, 0 if invalid or NULL
 */
NANO_ROS_PUBLIC
int nano_ros_publisher_is_valid(const nano_ros_publisher_t *publisher);

#ifdef __cplusplus
}
#endif

#endif // NANO_ROS_PUBLISHER_H

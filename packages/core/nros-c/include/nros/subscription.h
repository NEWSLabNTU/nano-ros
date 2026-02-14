/**
 * nros subscription API
 *
 * Subscription creation and message receiving functions.
 *
 * Copyright 2024 nros contributors
 * Licensed under Apache-2.0
 */

#ifndef NANO_ROS_SUBSCRIPTION_H
#define NANO_ROS_SUBSCRIPTION_H

#include "nros/types.h"
#include "nros/visibility.h"
#include "nros/node.h"

#ifdef __cplusplus
extern "C" {
#endif

// ============================================================================
// Subscription State
// ============================================================================

/** Subscription state */
typedef enum nano_ros_subscription_state_t {
    /** Not initialized */
    NANO_ROS_SUBSCRIPTION_STATE_UNINITIALIZED = 0,
    /** Initialized and ready */
    NANO_ROS_SUBSCRIPTION_STATE_INITIALIZED = 1,
    /** Shutdown */
    NANO_ROS_SUBSCRIPTION_STATE_SHUTDOWN = 2,
} nano_ros_subscription_state_t;

// ============================================================================
// Subscription Callback
// ============================================================================

/**
 * Subscription callback function type.
 *
 * @param data Pointer to received CDR-serialized message data
 * @param len Length of data in bytes
 * @param context User-provided context pointer
 */
typedef void (*nano_ros_subscription_callback_t)(
    const uint8_t *data,
    size_t len,
    void *context);

// ============================================================================
// Subscription Structure
// ============================================================================

/** Subscription structure */
typedef struct nano_ros_subscription_t {
    /** Current state */
    nano_ros_subscription_state_t state;
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
    /** User callback function */
    nano_ros_subscription_callback_t callback;
    /** User context pointer */
    void *context;
    /** Pointer to parent node */
    const nros_node_t *node;
    /** Opaque pointer to internal Rust subscriber */
    void *internal;
} nano_ros_subscription_t;

// ============================================================================
// Subscription Functions
// ============================================================================

/**
 * Get a zero-initialized subscription.
 *
 * @return Zero-initialized subscription structure
 */
NANO_ROS_PUBLIC
nano_ros_subscription_t nano_ros_subscription_get_zero_initialized(void);

/**
 * Initialize a subscription with default QoS.
 *
 * @param subscription Pointer to a zero-initialized subscription
 * @param node Pointer to an initialized node
 * @param type_info Pointer to message type information
 * @param topic_name Topic name (null-terminated string)
 * @param callback Callback function to invoke when messages arrive
 * @param context User context pointer passed to callback (can be NULL)
 *
 * @return NANO_ROS_RET_OK on success
 * @return NANO_ROS_RET_INVALID_ARGUMENT if any required pointer is NULL
 * @return NANO_ROS_RET_NOT_INIT if node is not initialized
 * @return NANO_ROS_RET_ERROR on initialization failure
 */
NANO_ROS_PUBLIC NANO_ROS_WARN_UNUSED
nano_ros_ret_t nano_ros_subscription_init(
    nano_ros_subscription_t *subscription,
    const nros_node_t *node,
    const nano_ros_message_type_t *type_info,
    const char *topic_name,
    nano_ros_subscription_callback_t callback,
    void *context);

/**
 * Initialize a subscription with custom QoS.
 *
 * @param subscription Pointer to a zero-initialized subscription
 * @param node Pointer to an initialized node
 * @param type_info Pointer to message type information
 * @param topic_name Topic name (null-terminated string)
 * @param callback Callback function to invoke when messages arrive
 * @param context User context pointer passed to callback (can be NULL)
 * @param qos Pointer to QoS settings
 *
 * @return NANO_ROS_RET_OK on success
 * @return NANO_ROS_RET_INVALID_ARGUMENT if any required pointer is NULL
 * @return NANO_ROS_RET_NOT_INIT if node is not initialized
 * @return NANO_ROS_RET_ERROR on initialization failure
 */
NANO_ROS_PUBLIC NANO_ROS_WARN_UNUSED
nano_ros_ret_t nano_ros_subscription_init_with_qos(
    nano_ros_subscription_t *subscription,
    const nros_node_t *node,
    const nano_ros_message_type_t *type_info,
    const char *topic_name,
    nano_ros_subscription_callback_t callback,
    void *context,
    const nano_ros_qos_t *qos);

/**
 * Finalize a subscription.
 *
 * @param subscription Pointer to an initialized subscription
 *
 * @return NANO_ROS_RET_OK on success
 * @return NANO_ROS_RET_INVALID_ARGUMENT if subscription is NULL
 * @return NANO_ROS_RET_NOT_INIT if not initialized
 */
NANO_ROS_PUBLIC NANO_ROS_WARN_UNUSED
nano_ros_ret_t nano_ros_subscription_fini(nano_ros_subscription_t *subscription);

/**
 * Get the topic name of a subscription.
 *
 * @param subscription Pointer to a subscription
 *
 * @return Pointer to topic name (null-terminated), or NULL if invalid
 */
NANO_ROS_PUBLIC
const char *nano_ros_subscription_get_topic_name(const nano_ros_subscription_t *subscription);

/**
 * Check if subscription is valid (initialized).
 *
 * @param subscription Pointer to a subscription
 *
 * @return Non-zero if valid, 0 if invalid or NULL
 */
NANO_ROS_PUBLIC
int nano_ros_subscription_is_valid(const nano_ros_subscription_t *subscription);

#ifdef __cplusplus
}
#endif

#endif // NANO_ROS_SUBSCRIPTION_H

/**
 * nros service client API
 *
 * Service client creation and service calling functions.
 *
 * Copyright 2024 nros contributors
 * Licensed under Apache-2.0
 */

#ifndef NANO_ROS_CLIENT_H
#define NANO_ROS_CLIENT_H

#include "nano_ros/types.h"
#include "nano_ros/visibility.h"
#include "nano_ros/node.h"

#ifdef __cplusplus
extern "C" {
#endif

// ============================================================================
// Client State
// ============================================================================

/** Client state */
typedef enum nano_ros_client_state_t {
    /** Not initialized */
    NANO_ROS_CLIENT_STATE_UNINITIALIZED = 0,
    /** Initialized and ready */
    NANO_ROS_CLIENT_STATE_INITIALIZED = 1,
    /** Shutdown */
    NANO_ROS_CLIENT_STATE_SHUTDOWN = 2,
} nano_ros_client_state_t;

// ============================================================================
// Client Structure
// ============================================================================

/** Service client structure */
typedef struct nano_ros_client_t {
    /** Current state */
    nano_ros_client_state_t state;
    /** Service name storage */
    uint8_t service_name[NANO_ROS_MAX_SERVICE_NAME_LEN];
    /** Service name length */
    size_t service_name_len;
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
    /** Opaque pointer to internal Rust service client */
    void *internal;
} nano_ros_client_t;

// ============================================================================
// Client Functions
// ============================================================================

/**
 * Get a zero-initialized client.
 *
 * @return Zero-initialized client structure
 */
NANO_ROS_PUBLIC
nano_ros_client_t nano_ros_client_get_zero_initialized(void);

/**
 * Initialize a service client.
 *
 * @param client Pointer to a zero-initialized client
 * @param node Pointer to an initialized node
 * @param type_info Pointer to service type information
 * @param service_name Service name (null-terminated string)
 *
 * @return NANO_ROS_RET_OK on success
 * @return NANO_ROS_RET_INVALID_ARGUMENT if any required pointer is NULL
 * @return NANO_ROS_RET_NOT_INIT if node is not initialized
 * @return NANO_ROS_RET_ERROR on initialization failure
 */
NANO_ROS_PUBLIC NANO_ROS_WARN_UNUSED
nano_ros_ret_t nano_ros_client_init(
    nano_ros_client_t *client,
    const nros_node_t *node,
    const nano_ros_message_type_t *type_info,
    const char *service_name);

/**
 * Finalize a service client.
 *
 * @param client Pointer to an initialized client
 *
 * @return NANO_ROS_RET_OK on success
 * @return NANO_ROS_RET_INVALID_ARGUMENT if client is NULL
 * @return NANO_ROS_RET_NOT_INIT if not initialized
 */
NANO_ROS_PUBLIC NANO_ROS_WARN_UNUSED
nano_ros_ret_t nano_ros_client_fini(nano_ros_client_t *client);

/**
 * Call a service (blocking).
 *
 * This function sends a request and blocks until a response is received
 * or a timeout occurs.
 *
 * @param client Pointer to an initialized client
 * @param request_data CDR-serialized request data
 * @param request_len Length of request data
 * @param response_data Buffer to receive CDR-serialized response
 * @param response_capacity Capacity of response buffer
 * @param response_len Output: actual length of response data
 *
 * @return NANO_ROS_RET_OK on success
 * @return NANO_ROS_RET_INVALID_ARGUMENT if any pointer is NULL
 * @return NANO_ROS_RET_NOT_INIT if not initialized
 * @return NANO_ROS_RET_TIMEOUT if no response within timeout
 * @return NANO_ROS_RET_ERROR on call failure
 */
NANO_ROS_PUBLIC NANO_ROS_WARN_UNUSED
nano_ros_ret_t nano_ros_client_call(
    nano_ros_client_t *client,
    const uint8_t *request_data,
    size_t request_len,
    uint8_t *response_data,
    size_t response_capacity,
    size_t *response_len);

/**
 * Get the service name of a client.
 *
 * @param client Pointer to a client
 *
 * @return Pointer to service name (null-terminated), or NULL if invalid
 */
NANO_ROS_PUBLIC
const char *nano_ros_client_get_service_name(const nano_ros_client_t *client);

/**
 * Check if client is valid (initialized).
 *
 * @param client Pointer to a client
 *
 * @return Non-zero if valid, 0 if invalid or NULL
 */
NANO_ROS_PUBLIC
int nano_ros_client_is_valid(const nano_ros_client_t *client);

#ifdef __cplusplus
}
#endif

#endif // NANO_ROS_CLIENT_H

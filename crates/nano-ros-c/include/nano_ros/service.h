/**
 * nano-ros service server API
 *
 * Service server creation and request handling functions.
 *
 * Copyright 2024 nano-ros contributors
 * Licensed under Apache-2.0
 */

#ifndef NANO_ROS_SERVICE_H
#define NANO_ROS_SERVICE_H

#include "nano_ros/types.h"
#include "nano_ros/visibility.h"
#include "nano_ros/node.h"

#ifdef __cplusplus
extern "C" {
#endif

// ============================================================================
// Service State
// ============================================================================

/** Service server state */
typedef enum nano_ros_service_state_t {
    /** Not initialized */
    NANO_ROS_SERVICE_STATE_UNINITIALIZED = 0,
    /** Initialized and ready */
    NANO_ROS_SERVICE_STATE_INITIALIZED = 1,
    /** Shutdown */
    NANO_ROS_SERVICE_STATE_SHUTDOWN = 2,
} nano_ros_service_state_t;

// ============================================================================
// Service Callback
// ============================================================================

/**
 * Service server callback function type.
 *
 * @param request_data Pointer to CDR-serialized request data
 * @param request_len Length of request data in bytes
 * @param response_data Pointer to buffer for CDR-serialized response
 * @param response_capacity Capacity of response buffer
 * @param response_len Output: actual length of response data written
 * @param context User-provided context pointer
 *
 * @return true if the request was handled successfully
 * @return false if there was an error handling the request
 */
typedef bool (*nano_ros_service_callback_t)(
    const uint8_t *request_data,
    size_t request_len,
    uint8_t *response_data,
    size_t response_capacity,
    size_t *response_len,
    void *context);

// ============================================================================
// Service Structure
// ============================================================================

/** Service server structure */
typedef struct nano_ros_service_t {
    /** Current state */
    nano_ros_service_state_t state;
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
    /** User callback function */
    nano_ros_service_callback_t callback;
    /** User context pointer */
    void *context;
    /** Pointer to parent node */
    const nano_ros_node_t *node;
    /** Opaque pointer to internal Rust service server */
    void *internal;
} nano_ros_service_t;

// ============================================================================
// Service Functions
// ============================================================================

/**
 * Get a zero-initialized service server.
 *
 * @return Zero-initialized service structure
 */
NANO_ROS_PUBLIC
nano_ros_service_t nano_ros_service_get_zero_initialized(void);

/**
 * Initialize a service server.
 *
 * @param service Pointer to a zero-initialized service
 * @param node Pointer to an initialized node
 * @param type_info Pointer to service type information
 * @param service_name Service name (null-terminated string)
 * @param callback Callback function to invoke when requests arrive
 * @param context User context pointer passed to callback (can be NULL)
 *
 * @return NANO_ROS_RET_OK on success
 * @return NANO_ROS_RET_INVALID_ARGUMENT if any required pointer is NULL
 * @return NANO_ROS_RET_NOT_INIT if node is not initialized
 * @return NANO_ROS_RET_ERROR on initialization failure
 */
NANO_ROS_PUBLIC NANO_ROS_WARN_UNUSED
nano_ros_ret_t nano_ros_service_init(
    nano_ros_service_t *service,
    const nano_ros_node_t *node,
    const nano_ros_message_type_t *type_info,
    const char *service_name,
    nano_ros_service_callback_t callback,
    void *context);

/**
 * Finalize a service server.
 *
 * @param service Pointer to an initialized service
 *
 * @return NANO_ROS_RET_OK on success
 * @return NANO_ROS_RET_INVALID_ARGUMENT if service is NULL
 * @return NANO_ROS_RET_NOT_INIT if not initialized
 */
NANO_ROS_PUBLIC NANO_ROS_WARN_UNUSED
nano_ros_ret_t nano_ros_service_fini(nano_ros_service_t *service);

/**
 * Take a service request (non-blocking).
 *
 * @param service Pointer to an initialized service
 * @param request_data Buffer to receive CDR-serialized request data
 * @param request_capacity Capacity of request buffer
 * @param request_len Output: actual length of request data
 * @param sequence_number Output: sequence number for response matching
 *
 * @return NANO_ROS_RET_OK if a request was received
 * @return NANO_ROS_RET_TIMEOUT if no request is available
 * @return NANO_ROS_RET_INVALID_ARGUMENT if any pointer is NULL
 * @return NANO_ROS_RET_NOT_INIT if not initialized
 */
NANO_ROS_PUBLIC NANO_ROS_WARN_UNUSED
nano_ros_ret_t nano_ros_service_take_request(
    nano_ros_service_t *service,
    uint8_t *request_data,
    size_t request_capacity,
    size_t *request_len,
    int64_t *sequence_number);

/**
 * Send a service response.
 *
 * @param service Pointer to an initialized service
 * @param sequence_number Sequence number from the request
 * @param response_data CDR-serialized response data
 * @param response_len Length of response data
 *
 * @return NANO_ROS_RET_OK on success
 * @return NANO_ROS_RET_INVALID_ARGUMENT if any pointer is NULL
 * @return NANO_ROS_RET_NOT_INIT if not initialized
 * @return NANO_ROS_RET_ERROR on send failure
 */
NANO_ROS_PUBLIC NANO_ROS_WARN_UNUSED
nano_ros_ret_t nano_ros_service_send_response(
    nano_ros_service_t *service,
    int64_t sequence_number,
    const uint8_t *response_data,
    size_t response_len);

/**
 * Get the service name.
 *
 * @param service Pointer to a service
 *
 * @return Pointer to service name (null-terminated), or NULL if invalid
 */
NANO_ROS_PUBLIC
const char *nano_ros_service_get_service_name(const nano_ros_service_t *service);

/**
 * Check if service is valid (initialized).
 *
 * @param service Pointer to a service
 *
 * @return Non-zero if valid, 0 if invalid or NULL
 */
NANO_ROS_PUBLIC
int nano_ros_service_is_valid(const nano_ros_service_t *service);

#ifdef __cplusplus
}
#endif

#endif // NANO_ROS_SERVICE_H

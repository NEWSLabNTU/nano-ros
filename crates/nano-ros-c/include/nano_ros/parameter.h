/**
 * nano-ros parameter API
 *
 * Provides node parameters for configuration and runtime tuning.
 * Designed for embedded systems with static allocation.
 *
 * Copyright 2024 nano-ros contributors
 * Licensed under Apache-2.0
 */

#ifndef NANO_ROS_PARAMETER_H
#define NANO_ROS_PARAMETER_H

#include <stdint.h>
#include <stdbool.h>
#include <stddef.h>

#include "nano_ros/types.h"
#include "nano_ros/visibility.h"

#ifdef __cplusplus
extern "C" {
#endif

// ============================================================================
// Constants
// ============================================================================

/** Maximum length of a parameter name */
#define NANO_ROS_MAX_PARAM_NAME_LEN 64

/** Maximum length of a string parameter value */
#define NANO_ROS_MAX_PARAM_STRING_LEN 128

/** Default maximum number of parameters per node */
#define NANO_ROS_DEFAULT_MAX_PARAMS 16

// ============================================================================
// Parameter Types
// ============================================================================

/**
 * Parameter type enumeration.
 *
 * Compatible with rcl_interfaces/msg/ParameterType.
 */
typedef enum nano_ros_parameter_type_t {
    /** Parameter not set */
    NANO_ROS_PARAMETER_NOT_SET = 0,
    /** Boolean parameter */
    NANO_ROS_PARAMETER_BOOL = 1,
    /** 64-bit signed integer parameter */
    NANO_ROS_PARAMETER_INTEGER = 2,
    /** 64-bit floating point parameter */
    NANO_ROS_PARAMETER_DOUBLE = 3,
    /** String parameter */
    NANO_ROS_PARAMETER_STRING = 4,
    /** Byte array parameter (not yet supported) */
    NANO_ROS_PARAMETER_BYTE_ARRAY = 5,
    /** Boolean array parameter (not yet supported) */
    NANO_ROS_PARAMETER_BOOL_ARRAY = 6,
    /** Integer array parameter (not yet supported) */
    NANO_ROS_PARAMETER_INTEGER_ARRAY = 7,
    /** Double array parameter (not yet supported) */
    NANO_ROS_PARAMETER_DOUBLE_ARRAY = 8,
    /** String array parameter (not yet supported) */
    NANO_ROS_PARAMETER_STRING_ARRAY = 9,
} nano_ros_parameter_type_t;

/**
 * Parameter value union.
 *
 * Stores the actual parameter value based on type.
 */
typedef union nano_ros_parameter_value_t {
    /** Boolean value */
    bool bool_value;
    /** Integer value (64-bit) */
    int64_t integer_value;
    /** Double value */
    double double_value;
    /** String value (fixed-size buffer) */
    char string_value[NANO_ROS_MAX_PARAM_STRING_LEN];
} nano_ros_parameter_value_t;

/**
 * Parameter structure.
 *
 * Represents a single named parameter with its type and value.
 */
typedef struct nano_ros_parameter_t {
    /** Parameter name (null-terminated) */
    char name[NANO_ROS_MAX_PARAM_NAME_LEN];
    /** Parameter type */
    nano_ros_parameter_type_t type;
    /** Parameter value */
    nano_ros_parameter_value_t value;
} nano_ros_parameter_t;

// ============================================================================
// Parameter Server
// ============================================================================

/**
 * Parameter server state.
 */
typedef enum nano_ros_param_server_state_t {
    /** Not initialized */
    NANO_ROS_PARAM_SERVER_STATE_UNINITIALIZED = 0,
    /** Initialized and ready */
    NANO_ROS_PARAM_SERVER_STATE_READY = 1,
    /** Shutdown */
    NANO_ROS_PARAM_SERVER_STATE_SHUTDOWN = 2,
} nano_ros_param_server_state_t;

/**
 * Parameter change callback type.
 *
 * Called when a parameter value changes.
 *
 * @param name Parameter name
 * @param param Pointer to the new parameter value
 * @param context User-provided context
 * @return true to accept the change, false to reject it
 */
typedef bool (*nano_ros_param_callback_t)(
    const char *name,
    const nano_ros_parameter_t *param,
    void *context);

/**
 * Parameter server structure.
 *
 * Stores parameters for a node. Uses static allocation with fixed capacity.
 */
typedef struct nano_ros_param_server_t {
    /** Current state */
    nano_ros_param_server_state_t state;
    /** Maximum number of parameters */
    size_t capacity;
    /** Current number of parameters */
    size_t count;
    /** Parameter storage (pointer to user-provided array) */
    nano_ros_parameter_t *parameters;
    /** Parameter change callback */
    nano_ros_param_callback_t callback;
    /** Callback context */
    void *callback_context;
} nano_ros_param_server_t;

// ============================================================================
// Parameter Server Functions
// ============================================================================

/**
 * Get a zero-initialized parameter server.
 *
 * @return Zero-initialized parameter server structure
 */
NANO_ROS_PUBLIC
nano_ros_param_server_t nano_ros_param_server_get_zero_initialized(void);

/**
 * Initialize a parameter server with user-provided storage.
 *
 * @param server Pointer to a zero-initialized parameter server
 * @param storage Pointer to parameter storage array
 * @param capacity Number of parameters the storage can hold
 * @return NANO_ROS_RET_OK on success
 * @return NANO_ROS_RET_INVALID_ARGUMENT if server or storage is NULL, or capacity is 0
 */
NANO_ROS_PUBLIC NANO_ROS_WARN_UNUSED
nano_ros_ret_t nano_ros_param_server_init(
    nano_ros_param_server_t *server,
    nano_ros_parameter_t *storage,
    size_t capacity);

/**
 * Set a parameter change callback.
 *
 * The callback is invoked before a parameter change is applied.
 * Return false from the callback to reject the change.
 *
 * @param server Pointer to an initialized parameter server
 * @param callback Callback function (can be NULL to disable)
 * @param context User context passed to callback
 * @return NANO_ROS_RET_OK on success
 * @return NANO_ROS_RET_INVALID_ARGUMENT if server is NULL
 */
NANO_ROS_PUBLIC NANO_ROS_WARN_UNUSED
nano_ros_ret_t nano_ros_param_server_set_callback(
    nano_ros_param_server_t *server,
    nano_ros_param_callback_t callback,
    void *context);

/**
 * Declare a boolean parameter with a default value.
 *
 * @param server Pointer to an initialized parameter server
 * @param name Parameter name
 * @param default_value Default value
 * @return NANO_ROS_RET_OK on success
 * @return NANO_ROS_RET_INVALID_ARGUMENT if server or name is NULL
 * @return NANO_ROS_RET_FULL if parameter storage is full
 * @return NANO_ROS_RET_ALREADY_EXISTS if parameter already exists
 */
NANO_ROS_PUBLIC NANO_ROS_WARN_UNUSED
nano_ros_ret_t nano_ros_param_declare_bool(
    nano_ros_param_server_t *server,
    const char *name,
    bool default_value);

/**
 * Declare an integer parameter with a default value.
 *
 * @param server Pointer to an initialized parameter server
 * @param name Parameter name
 * @param default_value Default value
 * @return NANO_ROS_RET_OK on success
 */
NANO_ROS_PUBLIC NANO_ROS_WARN_UNUSED
nano_ros_ret_t nano_ros_param_declare_integer(
    nano_ros_param_server_t *server,
    const char *name,
    int64_t default_value);

/**
 * Declare a double parameter with a default value.
 *
 * @param server Pointer to an initialized parameter server
 * @param name Parameter name
 * @param default_value Default value
 * @return NANO_ROS_RET_OK on success
 */
NANO_ROS_PUBLIC NANO_ROS_WARN_UNUSED
nano_ros_ret_t nano_ros_param_declare_double(
    nano_ros_param_server_t *server,
    const char *name,
    double default_value);

/**
 * Declare a string parameter with a default value.
 *
 * @param server Pointer to an initialized parameter server
 * @param name Parameter name
 * @param default_value Default value (will be truncated if too long)
 * @return NANO_ROS_RET_OK on success
 */
NANO_ROS_PUBLIC NANO_ROS_WARN_UNUSED
nano_ros_ret_t nano_ros_param_declare_string(
    nano_ros_param_server_t *server,
    const char *name,
    const char *default_value);

/**
 * Get a boolean parameter value.
 *
 * @param server Pointer to an initialized parameter server
 * @param name Parameter name
 * @param value Pointer to store the value
 * @return NANO_ROS_RET_OK on success
 * @return NANO_ROS_RET_NOT_FOUND if parameter doesn't exist
 * @return NANO_ROS_RET_INVALID_ARGUMENT if types don't match
 */
NANO_ROS_PUBLIC NANO_ROS_WARN_UNUSED
nano_ros_ret_t nano_ros_param_get_bool(
    const nano_ros_param_server_t *server,
    const char *name,
    bool *value);

/**
 * Get an integer parameter value.
 *
 * @param server Pointer to an initialized parameter server
 * @param name Parameter name
 * @param value Pointer to store the value
 * @return NANO_ROS_RET_OK on success
 */
NANO_ROS_PUBLIC NANO_ROS_WARN_UNUSED
nano_ros_ret_t nano_ros_param_get_integer(
    const nano_ros_param_server_t *server,
    const char *name,
    int64_t *value);

/**
 * Get a double parameter value.
 *
 * @param server Pointer to an initialized parameter server
 * @param name Parameter name
 * @param value Pointer to store the value
 * @return NANO_ROS_RET_OK on success
 */
NANO_ROS_PUBLIC NANO_ROS_WARN_UNUSED
nano_ros_ret_t nano_ros_param_get_double(
    const nano_ros_param_server_t *server,
    const char *name,
    double *value);

/**
 * Get a string parameter value.
 *
 * @param server Pointer to an initialized parameter server
 * @param name Parameter name
 * @param value Buffer to store the value
 * @param max_len Maximum buffer length
 * @return NANO_ROS_RET_OK on success
 */
NANO_ROS_PUBLIC NANO_ROS_WARN_UNUSED
nano_ros_ret_t nano_ros_param_get_string(
    const nano_ros_param_server_t *server,
    const char *name,
    char *value,
    size_t max_len);

/**
 * Set a boolean parameter value.
 *
 * @param server Pointer to an initialized parameter server
 * @param name Parameter name
 * @param value New value
 * @return NANO_ROS_RET_OK on success
 * @return NANO_ROS_RET_NOT_FOUND if parameter doesn't exist
 * @return NANO_ROS_RET_INVALID_ARGUMENT if types don't match
 * @return NANO_ROS_RET_ERROR if change was rejected by callback
 */
NANO_ROS_PUBLIC NANO_ROS_WARN_UNUSED
nano_ros_ret_t nano_ros_param_set_bool(
    nano_ros_param_server_t *server,
    const char *name,
    bool value);

/**
 * Set an integer parameter value.
 *
 * @param server Pointer to an initialized parameter server
 * @param name Parameter name
 * @param value New value
 * @return NANO_ROS_RET_OK on success
 */
NANO_ROS_PUBLIC NANO_ROS_WARN_UNUSED
nano_ros_ret_t nano_ros_param_set_integer(
    nano_ros_param_server_t *server,
    const char *name,
    int64_t value);

/**
 * Set a double parameter value.
 *
 * @param server Pointer to an initialized parameter server
 * @param name Parameter name
 * @param value New value
 * @return NANO_ROS_RET_OK on success
 */
NANO_ROS_PUBLIC NANO_ROS_WARN_UNUSED
nano_ros_ret_t nano_ros_param_set_double(
    nano_ros_param_server_t *server,
    const char *name,
    double value);

/**
 * Set a string parameter value.
 *
 * @param server Pointer to an initialized parameter server
 * @param name Parameter name
 * @param value New value
 * @return NANO_ROS_RET_OK on success
 */
NANO_ROS_PUBLIC NANO_ROS_WARN_UNUSED
nano_ros_ret_t nano_ros_param_set_string(
    nano_ros_param_server_t *server,
    const char *name,
    const char *value);

/**
 * Check if a parameter exists.
 *
 * @param server Pointer to an initialized parameter server
 * @param name Parameter name
 * @return true if parameter exists, false otherwise
 */
NANO_ROS_PUBLIC
bool nano_ros_param_has(
    const nano_ros_param_server_t *server,
    const char *name);

/**
 * Get the type of a parameter.
 *
 * @param server Pointer to an initialized parameter server
 * @param name Parameter name
 * @return Parameter type, or NANO_ROS_PARAMETER_NOT_SET if not found
 */
NANO_ROS_PUBLIC
nano_ros_parameter_type_t nano_ros_param_get_type(
    const nano_ros_param_server_t *server,
    const char *name);

/**
 * Get the number of declared parameters.
 *
 * @param server Pointer to an initialized parameter server
 * @return Number of parameters, or 0 if server is NULL
 */
NANO_ROS_PUBLIC
size_t nano_ros_param_server_get_count(const nano_ros_param_server_t *server);

/**
 * Finalize a parameter server.
 *
 * @param server Pointer to an initialized parameter server
 * @return NANO_ROS_RET_OK on success
 */
NANO_ROS_PUBLIC NANO_ROS_WARN_UNUSED
nano_ros_ret_t nano_ros_param_server_fini(nano_ros_param_server_t *server);

#ifdef __cplusplus
}
#endif

#endif // NANO_ROS_PARAMETER_H

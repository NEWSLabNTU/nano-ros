/**
 * nros parameter API
 *
 * Provides node parameters for configuration and runtime tuning.
 * Designed for embedded systems with static allocation.
 *
 * Copyright 2024 nros contributors
 * Licensed under Apache-2.0
 */

#ifndef NROS_PARAMETER_H
#define NROS_PARAMETER_H

#include <stdint.h>
#include <stdbool.h>
#include <stddef.h>

#include "nros/types.h"
#include "nros/visibility.h"

#ifdef __cplusplus
extern "C" {
#endif

// ============================================================================
// Constants
// ============================================================================

/** Maximum length of a parameter name */
#define NROS_MAX_PARAM_NAME_LEN 64

/** Maximum length of a string parameter value */
#define NROS_MAX_PARAM_STRING_LEN 128

/** Default maximum number of parameters per node */
#define NROS_DEFAULT_MAX_PARAMS 16

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
    NROS_PARAMETER_NOT_SET = 0,
    /** Boolean parameter */
    NROS_PARAMETER_BOOL = 1,
    /** 64-bit signed integer parameter */
    NROS_PARAMETER_INTEGER = 2,
    /** 64-bit floating point parameter */
    NROS_PARAMETER_DOUBLE = 3,
    /** String parameter */
    NROS_PARAMETER_STRING = 4,
    /** Byte array parameter */
    NROS_PARAMETER_BYTE_ARRAY = 5,
    /** Boolean array parameter */
    NROS_PARAMETER_BOOL_ARRAY = 6,
    /** Integer array parameter */
    NROS_PARAMETER_INTEGER_ARRAY = 7,
    /** Double array parameter */
    NROS_PARAMETER_DOUBLE_ARRAY = 8,
    /** String array parameter */
    NROS_PARAMETER_STRING_ARRAY = 9,
} nano_ros_parameter_type_t;

/**
 * Array parameter value (pointer + length to caller-owned data).
 *
 * The caller must keep the array data valid for the lifetime of the parameter.
 * For string arrays, `data` points to an array of `const char *` pointers.
 */
typedef struct nano_ros_param_array_t {
    /** Pointer to caller-owned array data */
    const void *data;
    /** Number of elements */
    size_t len;
} nano_ros_param_array_t;

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
    char string_value[NROS_MAX_PARAM_STRING_LEN];
    /** Array value (pointer + length) */
    nano_ros_param_array_t array_value;
} nano_ros_parameter_value_t;

/**
 * Parameter structure.
 *
 * Represents a single named parameter with its type and value.
 */
typedef struct nano_ros_parameter_t {
    /** Parameter name (null-terminated) */
    char name[NROS_MAX_PARAM_NAME_LEN];
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
    NROS_PARAM_SERVER_STATE_UNINITIALIZED = 0,
    /** Initialized and ready */
    NROS_PARAM_SERVER_STATE_READY = 1,
    /** Shutdown */
    NROS_PARAM_SERVER_STATE_SHUTDOWN = 2,
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
NROS_PUBLIC
nano_ros_param_server_t nano_ros_param_server_get_zero_initialized(void);

/**
 * Initialize a parameter server with user-provided storage.
 *
 * @param server Pointer to a zero-initialized parameter server
 * @param storage Pointer to parameter storage array
 * @param capacity Number of parameters the storage can hold
 * @return NROS_RET_OK on success
 * @return NROS_RET_INVALID_ARGUMENT if server or storage is NULL, or capacity is 0
 */
NROS_PUBLIC NROS_WARN_UNUSED
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
 * @return NROS_RET_OK on success
 * @return NROS_RET_INVALID_ARGUMENT if server is NULL
 */
NROS_PUBLIC NROS_WARN_UNUSED
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
 * @return NROS_RET_OK on success
 * @return NROS_RET_INVALID_ARGUMENT if server or name is NULL
 * @return NROS_RET_FULL if parameter storage is full
 * @return NROS_RET_ALREADY_EXISTS if parameter already exists
 */
NROS_PUBLIC NROS_WARN_UNUSED
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
 * @return NROS_RET_OK on success
 */
NROS_PUBLIC NROS_WARN_UNUSED
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
 * @return NROS_RET_OK on success
 */
NROS_PUBLIC NROS_WARN_UNUSED
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
 * @return NROS_RET_OK on success
 */
NROS_PUBLIC NROS_WARN_UNUSED
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
 * @return NROS_RET_OK on success
 * @return NROS_RET_NOT_FOUND if parameter doesn't exist
 * @return NROS_RET_INVALID_ARGUMENT if types don't match
 */
NROS_PUBLIC NROS_WARN_UNUSED
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
 * @return NROS_RET_OK on success
 */
NROS_PUBLIC NROS_WARN_UNUSED
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
 * @return NROS_RET_OK on success
 */
NROS_PUBLIC NROS_WARN_UNUSED
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
 * @return NROS_RET_OK on success
 */
NROS_PUBLIC NROS_WARN_UNUSED
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
 * @return NROS_RET_OK on success
 * @return NROS_RET_NOT_FOUND if parameter doesn't exist
 * @return NROS_RET_INVALID_ARGUMENT if types don't match
 * @return NROS_RET_ERROR if change was rejected by callback
 */
NROS_PUBLIC NROS_WARN_UNUSED
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
 * @return NROS_RET_OK on success
 */
NROS_PUBLIC NROS_WARN_UNUSED
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
 * @return NROS_RET_OK on success
 */
NROS_PUBLIC NROS_WARN_UNUSED
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
 * @return NROS_RET_OK on success
 */
NROS_PUBLIC NROS_WARN_UNUSED
nano_ros_ret_t nano_ros_param_set_string(
    nano_ros_param_server_t *server,
    const char *name,
    const char *value);

// ============================================================================
// Array Parameter Functions
// ============================================================================
//
// Array parameters use caller-owned memory via pointer + length.
// The caller must keep the array data valid for the lifetime of the parameter.

/**
 * Declare a byte array parameter.
 *
 * @param server Pointer to an initialized parameter server
 * @param name Parameter name
 * @param data Pointer to byte array (NULL allowed if len is 0)
 * @param len Number of elements
 * @return NROS_RET_OK on success
 */
NROS_PUBLIC NROS_WARN_UNUSED
nano_ros_ret_t nano_ros_param_declare_byte_array(
    nano_ros_param_server_t *server,
    const char *name,
    const uint8_t *data,
    size_t len);

/**
 * Get a byte array parameter value.
 *
 * @param server Pointer to an initialized parameter server
 * @param name Parameter name
 * @param data Pointer to store the data pointer
 * @param len Pointer to store the element count
 * @return NROS_RET_OK on success
 */
NROS_PUBLIC NROS_WARN_UNUSED
nano_ros_ret_t nano_ros_param_get_byte_array(
    const nano_ros_param_server_t *server,
    const char *name,
    const uint8_t **data,
    size_t *len);

/**
 * Set a byte array parameter value.
 *
 * @param server Pointer to an initialized parameter server
 * @param name Parameter name
 * @param data Pointer to byte array (NULL allowed if len is 0)
 * @param len Number of elements
 * @return NROS_RET_OK on success
 */
NROS_PUBLIC NROS_WARN_UNUSED
nano_ros_ret_t nano_ros_param_set_byte_array(
    nano_ros_param_server_t *server,
    const char *name,
    const uint8_t *data,
    size_t len);

/**
 * Declare a boolean array parameter.
 *
 * @param server Pointer to an initialized parameter server
 * @param name Parameter name
 * @param data Pointer to boolean array (NULL allowed if len is 0)
 * @param len Number of elements
 * @return NROS_RET_OK on success
 */
NROS_PUBLIC NROS_WARN_UNUSED
nano_ros_ret_t nano_ros_param_declare_bool_array(
    nano_ros_param_server_t *server,
    const char *name,
    const bool *data,
    size_t len);

/**
 * Get a boolean array parameter value.
 *
 * @param server Pointer to an initialized parameter server
 * @param name Parameter name
 * @param data Pointer to store the data pointer
 * @param len Pointer to store the element count
 * @return NROS_RET_OK on success
 */
NROS_PUBLIC NROS_WARN_UNUSED
nano_ros_ret_t nano_ros_param_get_bool_array(
    const nano_ros_param_server_t *server,
    const char *name,
    const bool **data,
    size_t *len);

/**
 * Set a boolean array parameter value.
 *
 * @param server Pointer to an initialized parameter server
 * @param name Parameter name
 * @param data Pointer to boolean array (NULL allowed if len is 0)
 * @param len Number of elements
 * @return NROS_RET_OK on success
 */
NROS_PUBLIC NROS_WARN_UNUSED
nano_ros_ret_t nano_ros_param_set_bool_array(
    nano_ros_param_server_t *server,
    const char *name,
    const bool *data,
    size_t len);

/**
 * Declare an integer array parameter.
 *
 * @param server Pointer to an initialized parameter server
 * @param name Parameter name
 * @param data Pointer to integer array (NULL allowed if len is 0)
 * @param len Number of elements
 * @return NROS_RET_OK on success
 */
NROS_PUBLIC NROS_WARN_UNUSED
nano_ros_ret_t nano_ros_param_declare_integer_array(
    nano_ros_param_server_t *server,
    const char *name,
    const int64_t *data,
    size_t len);

/**
 * Get an integer array parameter value.
 *
 * @param server Pointer to an initialized parameter server
 * @param name Parameter name
 * @param data Pointer to store the data pointer
 * @param len Pointer to store the element count
 * @return NROS_RET_OK on success
 */
NROS_PUBLIC NROS_WARN_UNUSED
nano_ros_ret_t nano_ros_param_get_integer_array(
    const nano_ros_param_server_t *server,
    const char *name,
    const int64_t **data,
    size_t *len);

/**
 * Set an integer array parameter value.
 *
 * @param server Pointer to an initialized parameter server
 * @param name Parameter name
 * @param data Pointer to integer array (NULL allowed if len is 0)
 * @param len Number of elements
 * @return NROS_RET_OK on success
 */
NROS_PUBLIC NROS_WARN_UNUSED
nano_ros_ret_t nano_ros_param_set_integer_array(
    nano_ros_param_server_t *server,
    const char *name,
    const int64_t *data,
    size_t len);

/**
 * Declare a double array parameter.
 *
 * @param server Pointer to an initialized parameter server
 * @param name Parameter name
 * @param data Pointer to double array (NULL allowed if len is 0)
 * @param len Number of elements
 * @return NROS_RET_OK on success
 */
NROS_PUBLIC NROS_WARN_UNUSED
nano_ros_ret_t nano_ros_param_declare_double_array(
    nano_ros_param_server_t *server,
    const char *name,
    const double *data,
    size_t len);

/**
 * Get a double array parameter value.
 *
 * @param server Pointer to an initialized parameter server
 * @param name Parameter name
 * @param data Pointer to store the data pointer
 * @param len Pointer to store the element count
 * @return NROS_RET_OK on success
 */
NROS_PUBLIC NROS_WARN_UNUSED
nano_ros_ret_t nano_ros_param_get_double_array(
    const nano_ros_param_server_t *server,
    const char *name,
    const double **data,
    size_t *len);

/**
 * Set a double array parameter value.
 *
 * @param server Pointer to an initialized parameter server
 * @param name Parameter name
 * @param data Pointer to double array (NULL allowed if len is 0)
 * @param len Number of elements
 * @return NROS_RET_OK on success
 */
NROS_PUBLIC NROS_WARN_UNUSED
nano_ros_ret_t nano_ros_param_set_double_array(
    nano_ros_param_server_t *server,
    const char *name,
    const double *data,
    size_t len);

/**
 * Declare a string array parameter.
 *
 * @param server Pointer to an initialized parameter server
 * @param name Parameter name
 * @param data Pointer to string pointer array (NULL allowed if len is 0)
 * @param len Number of elements
 * @return NROS_RET_OK on success
 */
NROS_PUBLIC NROS_WARN_UNUSED
nano_ros_ret_t nano_ros_param_declare_string_array(
    nano_ros_param_server_t *server,
    const char *name,
    const char *const *data,
    size_t len);

/**
 * Get a string array parameter value.
 *
 * @param server Pointer to an initialized parameter server
 * @param name Parameter name
 * @param data Pointer to store the string pointer array
 * @param len Pointer to store the element count
 * @return NROS_RET_OK on success
 */
NROS_PUBLIC NROS_WARN_UNUSED
nano_ros_ret_t nano_ros_param_get_string_array(
    const nano_ros_param_server_t *server,
    const char *name,
    const char *const **data,
    size_t *len);

/**
 * Set a string array parameter value.
 *
 * @param server Pointer to an initialized parameter server
 * @param name Parameter name
 * @param data Pointer to string pointer array (NULL allowed if len is 0)
 * @param len Number of elements
 * @return NROS_RET_OK on success
 */
NROS_PUBLIC NROS_WARN_UNUSED
nano_ros_ret_t nano_ros_param_set_string_array(
    nano_ros_param_server_t *server,
    const char *name,
    const char *const *data,
    size_t len);

/**
 * Check if a parameter exists.
 *
 * @param server Pointer to an initialized parameter server
 * @param name Parameter name
 * @return true if parameter exists, false otherwise
 */
NROS_PUBLIC
bool nano_ros_param_has(
    const nano_ros_param_server_t *server,
    const char *name);

/**
 * Get the type of a parameter.
 *
 * @param server Pointer to an initialized parameter server
 * @param name Parameter name
 * @return Parameter type, or NROS_PARAMETER_NOT_SET if not found
 */
NROS_PUBLIC
nano_ros_parameter_type_t nano_ros_param_get_type(
    const nano_ros_param_server_t *server,
    const char *name);

/**
 * Get the number of declared parameters.
 *
 * @param server Pointer to an initialized parameter server
 * @return Number of parameters, or 0 if server is NULL
 */
NROS_PUBLIC
size_t nano_ros_param_server_get_count(const nano_ros_param_server_t *server);

/**
 * Finalize a parameter server.
 *
 * @param server Pointer to an initialized parameter server
 * @return NROS_RET_OK on success
 */
NROS_PUBLIC NROS_WARN_UNUSED
nano_ros_ret_t nano_ros_param_server_fini(nano_ros_param_server_t *server);

#ifdef __cplusplus
}
#endif

#endif // NROS_PARAMETER_H

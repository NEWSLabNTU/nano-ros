/**
 * @file parameter.h
 * @brief Parameter server API.
 *
 * Declare, get, and set typed parameters on a local parameter server.
 */

#ifndef NROS_PARAMETER_H
#define NROS_PARAMETER_H

#include "nros/types.h"

#ifdef __cplusplus
extern "C" {
#endif

/* ===================================================================
 * Types
 * =================================================================== */

/** Parameter server state. */
typedef enum nros_param_server_state_t {
    /** Not initialized. */
    NROS_PARAM_SERVER_STATE_UNINITIALIZED = 0,
    /** Initialized and ready. */
    NROS_PARAM_SERVER_STATE_READY = 1,
    /** Shutdown. */
    NROS_PARAM_SERVER_STATE_SHUTDOWN = 2,
} nros_param_server_state_t;

/** Parameter type enumeration. */
typedef enum nros_parameter_type_t {
    /** Parameter not set. */
    NROS_PARAMETER_NOT_SET = 0,
    /** Boolean parameter. */
    NROS_PARAMETER_BOOL = 1,
    /** 64-bit signed integer parameter. */
    NROS_PARAMETER_INTEGER = 2,
    /** 64-bit floating point parameter. */
    NROS_PARAMETER_DOUBLE = 3,
    /** String parameter. */
    NROS_PARAMETER_STRING = 4,
    /** Byte array parameter. */
    NROS_PARAMETER_BYTE_ARRAY = 5,
    /** Boolean array parameter. */
    NROS_PARAMETER_BOOL_ARRAY = 6,
    /** Integer array parameter. */
    NROS_PARAMETER_INTEGER_ARRAY = 7,
    /** Double array parameter. */
    NROS_PARAMETER_DOUBLE_ARRAY = 8,
    /** String array parameter. */
    NROS_PARAMETER_STRING_ARRAY = 9,
} nros_parameter_type_t;

/**
 * Array parameter value (pointer + length to caller-owned data).
 *
 * The caller must keep the array data valid for the lifetime of the
 * parameter.  For string arrays, @c data points to an array of
 * @c const @c char* pointers.
 */
typedef struct nros_param_array_t {
    /** Pointer to caller-owned array data. */
    const void *data;
    /** Number of elements. */
    size_t len;
} nros_param_array_t;

/** Parameter value union. */
typedef union nros_parameter_value_t {
    /** Boolean value. */
    bool bool_value;
    /** Integer value (64-bit). */
    int64_t integer_value;
    /** Double value. */
    double double_value;
    /** String value (fixed-size buffer). */
    uint8_t string_value[NROS_MAX_PARAM_STRING_LEN];
    /** Array value (pointer + length). */
    struct nros_param_array_t array_value;
} nros_parameter_value_t;

/** Parameter structure. */
typedef struct nros_parameter_t {
    /** Parameter name (null-terminated). */
    uint8_t name[NROS_MAX_PARAM_NAME_LEN];
    /** Parameter type. */
    enum nros_parameter_type_t type;
    /** Parameter value. */
    union nros_parameter_value_t value;
} nros_parameter_t;

/** Parameter change callback type. */
typedef bool (*nros_param_callback_t)(const char *name,
                                      const struct nros_parameter_t *param,
                                      void *context);

/** Parameter server structure. */
typedef struct nros_param_server_t {
    /** Current state. */
    enum nros_param_server_state_t state;
    /** Maximum number of parameters. */
    size_t capacity;
    /** Current number of parameters. */
    size_t count;
    /** Parameter storage (pointer to user-provided array). */
    struct nros_parameter_t *parameters;
    /** Parameter change callback. */
    nros_param_callback_t callback;
    /** Callback context. */
    void *callback_context;
} nros_param_server_t;

/* ===================================================================
 * Functions
 * =================================================================== */

/**
 * @brief Get a zero-initialized parameter server.
 * @return Zero-initialized @ref nros_param_server_t.
 */
NROS_PUBLIC struct nros_param_server_t nros_param_server_get_zero_initialized(void);

/**
 * @brief Initialise a parameter server with user-provided storage.
 *
 * @param server   Pointer to a zero-initialized parameter server.
 * @param storage  Pointer to a user-provided parameter array.
 * @param capacity Maximum number of parameters the array can hold.
 *
 * @retval NROS_RET_OK on success.
 */
NROS_PUBLIC
nros_ret_t nros_param_server_init(struct nros_param_server_t *server,
                                  struct nros_parameter_t *storage,
                                  size_t capacity);

/**
 * @brief Set a parameter change callback.
 *
 * @param server   Pointer to an initialized parameter server.
 * @param callback Callback function, or NULL to clear.
 * @param context  User context.
 *
 * @retval NROS_RET_OK on success.
 */
NROS_PUBLIC
nros_ret_t nros_param_server_set_callback(struct nros_param_server_t *server,
                                          nros_param_callback_t callback,
                                          void *context);

/**
 * @brief Declare a boolean parameter.
 *
 * @param server        Pointer to an initialized parameter server.
 * @param name          Parameter name (null-terminated).
 * @param default_value Default boolean value.
 *
 * @retval NROS_RET_OK on success.
 */
NROS_PUBLIC
nros_ret_t nros_param_declare_bool(struct nros_param_server_t *server,
                                   const char *name,
                                   bool default_value);

/**
 * @brief Declare an integer parameter.
 *
 * @param server        Pointer to an initialized parameter server.
 * @param name          Parameter name (null-terminated).
 * @param default_value Default integer value.
 *
 * @retval NROS_RET_OK on success.
 */
NROS_PUBLIC
nros_ret_t nros_param_declare_integer(struct nros_param_server_t *server,
                                      const char *name,
                                      int64_t default_value);

/**
 * @brief Declare a double parameter.
 *
 * @param server        Pointer to an initialized parameter server.
 * @param name          Parameter name (null-terminated).
 * @param default_value Default double value.
 *
 * @retval NROS_RET_OK on success.
 */
NROS_PUBLIC
nros_ret_t nros_param_declare_double(struct nros_param_server_t *server,
                                     const char *name,
                                     double default_value);

/**
 * @brief Declare a string parameter.
 *
 * @param server        Pointer to an initialized parameter server.
 * @param name          Parameter name (null-terminated).
 * @param default_value Default string value (null-terminated).
 *
 * @retval NROS_RET_OK on success.
 */
NROS_PUBLIC
nros_ret_t nros_param_declare_string(struct nros_param_server_t *server,
                                     const char *name,
                                     const char *default_value);

/**
 * @brief Get a boolean parameter value.
 *
 * @param server Pointer to an initialized parameter server.
 * @param name   Parameter name.
 * @param value  Output: parameter value.
 *
 * @retval NROS_RET_OK        on success.
 * @retval NROS_RET_NOT_FOUND  if the parameter does not exist.
 */
NROS_PUBLIC
nros_ret_t nros_param_get_bool(const struct nros_param_server_t *server,
                               const char *name,
                               bool *value);

/**
 * @brief Get an integer parameter value.
 *
 * @param server Pointer to an initialized parameter server.
 * @param name   Parameter name.
 * @param value  Output: parameter value.
 *
 * @retval NROS_RET_OK        on success.
 * @retval NROS_RET_NOT_FOUND  if the parameter does not exist.
 */
NROS_PUBLIC
nros_ret_t nros_param_get_integer(const struct nros_param_server_t *server,
                                  const char *name,
                                  int64_t *value);

/**
 * @brief Get a double parameter value.
 *
 * @param server Pointer to an initialized parameter server.
 * @param name   Parameter name.
 * @param value  Output: parameter value.
 *
 * @retval NROS_RET_OK        on success.
 * @retval NROS_RET_NOT_FOUND  if the parameter does not exist.
 */
NROS_PUBLIC
nros_ret_t nros_param_get_double(const struct nros_param_server_t *server,
                                 const char *name,
                                 double *value);

/**
 * @brief Get a string parameter value.
 *
 * @param server  Pointer to an initialized parameter server.
 * @param name    Parameter name.
 * @param value   Output buffer for the string.
 * @param max_len Maximum length of the output buffer.
 *
 * @retval NROS_RET_OK        on success.
 * @retval NROS_RET_NOT_FOUND  if the parameter does not exist.
 */
NROS_PUBLIC
nros_ret_t nros_param_get_string(const struct nros_param_server_t *server,
                                 const char *name,
                                 char *value,
                                 size_t max_len);

/**
 * @brief Set a boolean parameter value.
 *
 * @param server Pointer to an initialized parameter server.
 * @param name   Parameter name.
 * @param value  New boolean value.
 *
 * @retval NROS_RET_OK        on success.
 * @retval NROS_RET_NOT_FOUND  if the parameter does not exist.
 */
NROS_PUBLIC
nros_ret_t nros_param_set_bool(struct nros_param_server_t *server,
                               const char *name,
                               bool value);

/**
 * @brief Set an integer parameter value.
 *
 * @param server Pointer to an initialized parameter server.
 * @param name   Parameter name.
 * @param value  New integer value.
 *
 * @retval NROS_RET_OK        on success.
 * @retval NROS_RET_NOT_FOUND  if the parameter does not exist.
 */
NROS_PUBLIC
nros_ret_t nros_param_set_integer(struct nros_param_server_t *server,
                                  const char *name,
                                  int64_t value);

/**
 * @brief Set a double parameter value.
 *
 * @param server Pointer to an initialized parameter server.
 * @param name   Parameter name.
 * @param value  New double value.
 *
 * @retval NROS_RET_OK        on success.
 * @retval NROS_RET_NOT_FOUND  if the parameter does not exist.
 */
NROS_PUBLIC
nros_ret_t nros_param_set_double(struct nros_param_server_t *server,
                                 const char *name,
                                 double value);

/**
 * @brief Set a string parameter value.
 *
 * @param server Pointer to an initialized parameter server.
 * @param name   Parameter name.
 * @param value  New string value (null-terminated).
 *
 * @retval NROS_RET_OK        on success.
 * @retval NROS_RET_NOT_FOUND  if the parameter does not exist.
 */
NROS_PUBLIC
nros_ret_t nros_param_set_string(struct nros_param_server_t *server,
                                 const char *name,
                                 const char *value);

/**
 * @brief Check if a parameter exists.
 *
 * @param server Pointer to an initialized parameter server.
 * @param name   Parameter name.
 * @return true if the parameter exists, false otherwise.
 */
NROS_PUBLIC bool nros_param_has(const struct nros_param_server_t *server,
                                const char *name);

/**
 * @brief Get the type of a parameter.
 *
 * @param server Pointer to an initialized parameter server.
 * @param name   Parameter name.
 * @return Parameter type, or @ref NROS_PARAMETER_NOT_SET if not found.
 */
NROS_PUBLIC
enum nros_parameter_type_t nros_param_get_type(const struct nros_param_server_t *server,
                                               const char *name);

/**
 * @brief Get the number of declared parameters.
 *
 * @param server Pointer to an initialized parameter server.
 * @return Number of parameters.
 */
NROS_PUBLIC size_t nros_param_server_get_count(const struct nros_param_server_t *server);

/**
 * @brief Finalise a parameter server.
 *
 * @param server Pointer to an initialized parameter server.
 * @retval NROS_RET_OK on success.
 */
NROS_PUBLIC nros_ret_t nros_param_server_fini(struct nros_param_server_t *server);

#ifdef __cplusplus
}
#endif

#endif /* NROS_PARAMETER_H */

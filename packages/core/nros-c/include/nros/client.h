/**
 * @file client.h
 * @brief Service client API.
 *
 * Create service clients with nros_client_init() and call services
 * with nros_client_call() (blocking).
 */

#ifndef NROS_CLIENT_H
#define NROS_CLIENT_H

#include "nros/types.h"
#include "nros/nros_config_generated.h"

#ifdef __cplusplus
extern "C" {
#endif

/* Forward declarations */
struct nros_node_t;
struct nros_executor_t;

/* ===================================================================
 * Types
 * =================================================================== */

/** Response callback type for async service client (Phase 82). */
typedef void (*nros_response_callback_t)(const uint8_t* data, size_t len, void* context);

/** Client state. */
typedef enum nros_client_state_t {
    /** Not initialized. */
    NROS_CLIENT_STATE_UNINITIALIZED = 0,
    /** Initialized (metadata only, not yet registered with executor). */
    NROS_CLIENT_STATE_INITIALIZED = 1,
    /** Registered with an executor and ready for use. */
    NROS_CLIENT_STATE_REGISTERED = 2,
    /** Shutdown. */
    NROS_CLIENT_STATE_SHUTDOWN = 3,
} nros_client_state_t;

/** Service client structure. */
typedef struct nros_client_t {
    /** Current state. */
    enum nros_client_state_t state;
    /** Service name storage. */
    uint8_t service_name[NROS_MAX_SERVICE_NAME_LEN];
    /** Service name length. */
    size_t service_name_len;
    /** Type name storage. */
    uint8_t type_name[NROS_MAX_TYPE_NAME_LEN];
    /** Type name length. */
    size_t type_name_len;
    /** Type hash storage. */
    uint8_t type_hash[NROS_MAX_TYPE_HASH_LEN];
    /** Type hash length. */
    size_t type_hash_len;
    /** Response callback for async requests (Phase 82). */
    nros_response_callback_t response_callback;
    /** User context pointer for @ref response_callback. */
    void* context;
    /** Pointer to parent node. */
    const struct nros_node_t* node;
    /** Opaque inline storage for @c ServiceClientInternal.
     *  Filled by nros_executor_add_client(). */
    _Alignas(8) uint8_t _internal[NROS_SERVICE_CLIENT_INTERNAL_STORAGE_SIZE];
} nros_client_t;

/* ===================================================================
 * Functions
 * =================================================================== */

/**
 * @brief Get a zero-initialized client.
 * @return Zero-initialized @ref nros_client_t.
 */
NROS_PUBLIC struct nros_client_t nros_client_get_zero_initialized(void);

/**
 * @brief Initialise a service client.
 *
 * @param client       Pointer to a zero-initialized client.
 * @param node         Pointer to an initialized node.
 * @param type_info    Pointer to service type information.
 * @param service_name Service name (null-terminated).
 *
 * @retval NROS_RET_OK               on success.
 * @retval NROS_RET_INVALID_ARGUMENT  if any required pointer is NULL.
 * @retval NROS_RET_NOT_INIT          if @p node is not initialized.
 * @retval NROS_RET_ERROR             on initialisation failure.
 */
NROS_PUBLIC
nros_ret_t nros_client_init(struct nros_client_t* client, const struct nros_node_t* node,
                            const struct nros_service_type_t* type_info, const char* service_name);

/**
 * @brief Finalise a service client.
 *
 * @param client  Pointer to an initialized client.
 *
 * @retval NROS_RET_OK               on success.
 * @retval NROS_RET_INVALID_ARGUMENT  if @p client is NULL.
 * @retval NROS_RET_NOT_INIT          if not initialized.
 */
NROS_PUBLIC nros_ret_t nros_client_fini(struct nros_client_t* client);

/**
 * @brief Call a service (blocking).
 *
 * Sends a request and blocks until a response is received or a timeout
 * occurs.
 *
 * @param client            Pointer to an initialized client.
 * @param request_data      CDR-serialized request data.
 * @param request_len       Length of request data.
 * @param response_data     Buffer to receive CDR-serialized response.
 * @param response_capacity Capacity of response buffer.
 * @param response_len      Output: actual length of response data.
 *
 * @retval NROS_RET_OK               on success.
 * @retval NROS_RET_INVALID_ARGUMENT  if any pointer is NULL.
 * @retval NROS_RET_NOT_INIT          if not initialized.
 * @retval NROS_RET_TIMEOUT           if no response within timeout.
 * @retval NROS_RET_ERROR             on call failure.
 */
NROS_PUBLIC
nros_ret_t nros_client_call(struct nros_client_t* client, const uint8_t* request_data,
                            size_t request_len, uint8_t* response_data, size_t response_capacity,
                            size_t* response_len);

/**
 * @brief Send a service request asynchronously (non-blocking, Phase 82).
 *
 * The reply is delivered via the registered response callback during
 * nros_executor_spin_some(). The client must have been registered
 * with nros_executor_add_client() first.
 *
 * @param client       Pointer to a registered client.
 * @param request_data CDR-serialized request data.
 * @param request_len  Length of request data.
 *
 * @retval NROS_RET_OK           on success.
 * @retval NROS_RET_NOT_INIT     if not registered with an executor.
 * @retval NROS_RET_BAD_SEQUENCE if a previous request is still pending.
 */
NROS_PUBLIC
nros_ret_t nros_client_send_request_async(struct nros_client_t* client, const uint8_t* request_data,
                                          size_t request_len);

/**
 * @brief Set the response callback for async requests.
 *
 * @param client   Pointer to a client.
 * @param callback Callback invoked when a response arrives.
 * @param context  User context pointer passed to @p callback.
 */
NROS_PUBLIC
nros_ret_t nros_client_set_response_callback(struct nros_client_t* client,
                                             nros_response_callback_t callback, void* context);

/**
 * @brief Set the default timeout for nros_client_call().
 *
 * @param client     Pointer to a client.
 * @param timeout_ms Timeout in milliseconds.
 */
NROS_PUBLIC nros_ret_t nros_client_set_timeout(struct nros_client_t* client, uint32_t timeout_ms);

/**
 * @brief Get the service name of a client.
 *
 * @param client  Pointer to a client.
 * @return Null-terminated service name, or NULL if invalid.
 */
NROS_PUBLIC const char* nros_client_get_service_name(const struct nros_client_t* client);

/**
 * @brief Check if client is valid (initialized).
 *
 * @param client  Pointer to a client.
 * @return @c true if valid, @c false if invalid or NULL.
 */
NROS_PUBLIC bool nros_client_is_valid(const struct nros_client_t* client);

/**
 * @brief Register a service client with an executor.
 *
 * Required before nros_client_send_request_async() or
 * nros_client_call() can be used. The executor drives the response
 * callback during nros_executor_spin_some().
 *
 * @param executor Pointer to an initialized executor.
 * @param client   Pointer to an initialized service client.
 *
 * @retval NROS_RET_OK on success.
 */
NROS_PUBLIC
nros_ret_t nros_executor_add_client(struct nros_executor_t* executor, struct nros_client_t* client);

#ifdef __cplusplus
}
#endif

#endif /* NROS_CLIENT_H */

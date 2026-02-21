/**
 * nros service client API
 *
 * Service client creation and service calling functions.
 *
 * Copyright 2024 nros contributors
 * Licensed under Apache-2.0
 */

#ifndef NROS_CLIENT_H
#define NROS_CLIENT_H

#include "nros/types.h"
#include "nros/visibility.h"
#include "nros/node.h"

#ifdef __cplusplus
extern "C" {
#endif

// ============================================================================
// Client State
// ============================================================================

/** Client state */
typedef enum nros_client_state_t {
    /** Not initialized */
    NROS_CLIENT_STATE_UNINITIALIZED = 0,
    /** Initialized and ready */
    NROS_CLIENT_STATE_INITIALIZED = 1,
    /** Shutdown */
    NROS_CLIENT_STATE_SHUTDOWN = 2,
} nros_client_state_t;

// ============================================================================
// Client Structure
// ============================================================================

/** Service client structure */
typedef struct nros_client_t {
    /** Current state */
    nros_client_state_t state;
    /** Service name storage */
    uint8_t service_name[NROS_MAX_SERVICE_NAME_LEN];
    /** Service name length */
    size_t service_name_len;
    /** Type name storage */
    uint8_t type_name[NROS_MAX_TYPE_NAME_LEN];
    /** Type name length */
    size_t type_name_len;
    /** Type hash storage */
    uint8_t type_hash[NROS_MAX_TYPE_HASH_LEN];
    /** Type hash length */
    size_t type_hash_len;
    /** Pointer to parent node */
    const nros_node_t *node;
    /** Opaque pointer to internal Rust service client */
    void *internal;
} nros_client_t;

// ============================================================================
// Client Functions
// ============================================================================

/**
 * Get a zero-initialized client.
 *
 * @return Zero-initialized client structure
 */
NROS_PUBLIC
nros_client_t nros_client_get_zero_initialized(void);

/**
 * Initialize a service client.
 *
 * @param client Pointer to a zero-initialized client
 * @param node Pointer to an initialized node
 * @param type_info Pointer to service type information
 * @param service_name Service name (null-terminated string)
 *
 * @return NROS_RET_OK on success
 * @return NROS_RET_INVALID_ARGUMENT if any required pointer is NULL
 * @return NROS_RET_NOT_INIT if node is not initialized
 * @return NROS_RET_ERROR on initialization failure
 */
NROS_PUBLIC NROS_WARN_UNUSED
nros_ret_t nros_client_init(
    nros_client_t *client,
    const nros_node_t *node,
    const nros_message_type_t *type_info,
    const char *service_name);

/**
 * Finalize a service client.
 *
 * @param client Pointer to an initialized client
 *
 * @return NROS_RET_OK on success
 * @return NROS_RET_INVALID_ARGUMENT if client is NULL
 * @return NROS_RET_NOT_INIT if not initialized
 */
NROS_PUBLIC NROS_WARN_UNUSED
nros_ret_t nros_client_fini(nros_client_t *client);

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
 * @return NROS_RET_OK on success
 * @return NROS_RET_INVALID_ARGUMENT if any pointer is NULL
 * @return NROS_RET_NOT_INIT if not initialized
 * @return NROS_RET_TIMEOUT if no response within timeout
 * @return NROS_RET_ERROR on call failure
 */
NROS_PUBLIC NROS_WARN_UNUSED
nros_ret_t nros_client_call(
    nros_client_t *client,
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
NROS_PUBLIC
const char *nros_client_get_service_name(const nros_client_t *client);

/**
 * Check if client is valid (initialized).
 *
 * @param client Pointer to a client
 *
 * @return Non-zero if valid, 0 if invalid or NULL
 */
NROS_PUBLIC
int nros_client_is_valid(const nros_client_t *client);

#ifdef __cplusplus
}
#endif

#endif // NROS_CLIENT_H

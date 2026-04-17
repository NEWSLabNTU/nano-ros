/**
 * @file service.h
 * @brief Service server API.
 *
 * Create service servers with nros_service_init(), take incoming
 * requests with nros_service_take_request(), and send responses with
 * nros_service_send_response().  For executor-driven dispatch, register
 * a @ref nros_service_callback_t at init time.
 */

#ifndef NROS_SERVICE_H
#define NROS_SERVICE_H

#include "nros/types.h"
#include "nros/nros_config_generated.h"

#ifdef __cplusplus
extern "C" {
#endif

/* Forward declarations */
struct nros_node_t;

/* ===================================================================
 * Types
 * =================================================================== */

/** Service server state. */
typedef enum nros_service_state_t {
    /** Not initialized. */
    NROS_SERVICE_STATE_UNINITIALIZED = 0,
    /** Initialized and ready. */
    NROS_SERVICE_STATE_INITIALIZED = 1,
    /** Shutdown. */
    NROS_SERVICE_STATE_SHUTDOWN = 2,
} nros_service_state_t;

/**
 * Service server callback function type.
 *
 * Invoked by the executor when a request arrives.  The callback must
 * deserialise the request from @p request_data, compute the response,
 * serialise it into @p response_data, and set @p *response_len.
 *
 * @param request_data      Pointer to CDR-serialized request data.
 * @param request_len       Length of request data in bytes.
 * @param response_data     Pointer to buffer for CDR-serialized response.
 * @param response_capacity Capacity of @p response_data buffer in bytes.
 * @param response_len      Output: actual length of response data written.
 * @param context           User-provided context pointer.
 *
 * @return @c true if the request was handled successfully.
 * @return @c false if there was an error handling the request.
 */
typedef bool (*nros_service_callback_t)(const uint8_t* request_data, size_t request_len,
                                        uint8_t* response_data, size_t response_capacity,
                                        size_t* response_len, void* context);

/** Service server structure. */
typedef struct nros_service_t {
    /** Current state. */
    enum nros_service_state_t state;
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
    /** User callback function. */
    nros_service_callback_t callback;
    /** User context pointer passed to @ref callback. */
    void* context;
    /** Pointer to parent node. */
    const struct nros_node_t* node;
    /** Opaque inline storage for @c ServiceServerInternal. */
    _Alignas(8) uint8_t _internal[NROS_SERVICE_SERVER_INTERNAL_STORAGE_SIZE];
} nros_service_t;

/* ===================================================================
 * Functions
 * =================================================================== */

/**
 * @brief Get a zero-initialized service server.
 * @return Zero-initialized @ref nros_service_t.
 */
NROS_PUBLIC struct nros_service_t nros_service_get_zero_initialized(void);

/**
 * @brief Initialise a service server.
 *
 * @param service       Pointer to a zero-initialized service.
 * @param node          Pointer to an initialized node.
 * @param type_info     Pointer to service type information.
 * @param service_name  Service name (null-terminated).
 * @param callback      Callback function to invoke when requests arrive.
 * @param context       User context pointer passed to @p callback (can be NULL).
 *
 * @retval NROS_RET_OK               on success.
 * @retval NROS_RET_INVALID_ARGUMENT  if any required pointer is NULL.
 * @retval NROS_RET_NOT_INIT          if @p node is not initialized.
 * @retval NROS_RET_ERROR             on initialisation failure.
 *
 * @pre All required pointers must be valid.
 * @pre @p service_name must be a valid null-terminated string.
 */
NROS_PUBLIC
nros_ret_t nros_service_init(struct nros_service_t* service, const struct nros_node_t* node,
                             const struct nros_message_type_t* type_info, const char* service_name,
                             nros_service_callback_t callback, void* context);

/**
 * @brief Finalise a service server.
 *
 * @param service  Pointer to an initialized service.
 *
 * @retval NROS_RET_OK               on success.
 * @retval NROS_RET_INVALID_ARGUMENT  if @p service is NULL.
 * @retval NROS_RET_NOT_INIT          if not initialized.
 */
NROS_PUBLIC nros_ret_t nros_service_fini(struct nros_service_t* service);

/**
 * @brief Take a service request (non-blocking).
 *
 * Copies the next pending CDR-serialized request into @p request_data.
 * If no request is available, returns @c NROS_RET_TIMEOUT.
 *
 * @param service          Pointer to an initialized service.
 * @param request_data     Buffer to receive CDR-serialized request data.
 * @param request_capacity Capacity of @p request_data buffer in bytes.
 * @param request_len      Output: actual length of request data.
 * @param sequence_number  Output: sequence number for response matching.
 *
 * @retval NROS_RET_OK               if a request was received.
 * @retval NROS_RET_TIMEOUT           if no request is available.
 * @retval NROS_RET_INVALID_ARGUMENT  if any pointer is NULL.
 * @retval NROS_RET_NOT_INIT          if not initialized.
 *
 * @pre @p service must point to an initialized service.
 * @pre @p request_data must point to at least @p request_capacity bytes.
 */
NROS_PUBLIC
nros_ret_t nros_service_take_request(struct nros_service_t* service, uint8_t* request_data,
                                     size_t request_capacity, size_t* request_len,
                                     int64_t* sequence_number);

/**
 * @brief Send a service response.
 *
 * Sends a CDR-serialized response for a previously received request
 * identified by @p sequence_number.
 *
 * @param service         Pointer to an initialized service.
 * @param sequence_number Sequence number from the corresponding request.
 * @param response_data   CDR-serialized response data.
 * @param response_len    Length of @p response_data in bytes.
 *
 * @retval NROS_RET_OK               on success.
 * @retval NROS_RET_INVALID_ARGUMENT  if any pointer is NULL.
 * @retval NROS_RET_NOT_INIT          if not initialized.
 * @retval NROS_RET_ERROR             on send failure.
 *
 * @pre @p service must point to an initialized service.
 * @pre @p response_data must point to @p response_len valid bytes.
 */
NROS_PUBLIC
nros_ret_t nros_service_send_response(struct nros_service_t* service, int64_t sequence_number,
                                      const uint8_t* response_data, size_t response_len);

/**
 * @brief Get the service name.
 *
 * @param service  Pointer to a service.
 * @return Null-terminated service name, or NULL if invalid.
 */
NROS_PUBLIC const char* nros_service_get_service_name(const struct nros_service_t* service);

/**
 * @brief Check if service is valid (initialized).
 *
 * @param service  Pointer to a service.
 * @return Non-zero if valid, 0 if invalid or NULL.
 */
NROS_PUBLIC int nros_service_is_valid(const struct nros_service_t* service);

#ifdef __cplusplus
}
#endif

#endif /* NROS_SERVICE_H */

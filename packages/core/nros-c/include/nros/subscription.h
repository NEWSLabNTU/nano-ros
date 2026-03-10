/**
 * @file subscription.h
 * @brief Topic subscription API.
 *
 * Create subscriptions with nros_subscription_init() and receive
 * deserialised messages via a user-provided callback.
 */

#ifndef NROS_SUBSCRIPTION_H
#define NROS_SUBSCRIPTION_H

#include "nros/types.h"

#ifdef __cplusplus
extern "C" {
#endif

/* Forward declarations */
struct nros_node_t;

/* ===================================================================
 * Types
 * =================================================================== */

/** Subscription state. */
typedef enum nros_subscription_state_t {
    /** Not initialized. */
    NROS_SUBSCRIPTION_STATE_UNINITIALIZED = 0,
    /** Initialized and ready. */
    NROS_SUBSCRIPTION_STATE_INITIALIZED = 1,
    /** Shutdown. */
    NROS_SUBSCRIPTION_STATE_SHUTDOWN = 2,
} nros_subscription_state_t;

/**
 * @brief Subscription message callback.
 *
 * Called by the executor when a message arrives on the subscribed topic.
 *
 * @param data    Pointer to CDR-serialized message data.
 * @param len     Length of data in bytes.
 * @param context User-provided context pointer.
 */
typedef void (*nros_subscription_callback_t)(const uint8_t* data, size_t len, void* context);

/** Subscription structure. */
typedef struct nros_subscription_t {
    /** Current state. */
    enum nros_subscription_state_t state;
    /** Topic name storage. */
    uint8_t topic_name[NROS_MAX_TOPIC_LEN];
    /** Topic name length. */
    size_t topic_name_len;
    /** Type name storage. */
    uint8_t type_name[NROS_MAX_TYPE_NAME_LEN];
    /** Type name length. */
    size_t type_name_len;
    /** Type hash storage. */
    uint8_t type_hash[NROS_MAX_TYPE_HASH_LEN];
    /** Type hash length. */
    size_t type_hash_len;
    /** Message callback. */
    nros_subscription_callback_t callback;
    /** User-provided context pointer passed to @ref callback. */
    void* context;
    /** Pointer to parent node. */
    const struct nros_node_t* node;
    /** QoS settings. */
    struct nros_qos_t qos;
    /** Handle ID from executor registration (SIZE_MAX = not registered). */
    size_t handle_id;
    /** Opaque pointer to internal Rust subscription. */
    void* _internal;
} nros_subscription_t;

/* ===================================================================
 * Functions
 * =================================================================== */

/**
 * @brief Get a zero-initialized subscription.
 * @return Zero-initialized @ref nros_subscription_t.
 */
NROS_PUBLIC struct nros_subscription_t nros_subscription_get_zero_initialized(void);

/**
 * @brief Initialise a subscription with default QoS (RELIABLE, KEEP_LAST(10)).
 *
 * This is the recommended initialisation function for most use cases.
 *
 * @param subscription Pointer to a zero-initialized subscription.
 * @param node         Pointer to an initialized node.
 * @param type_info    Pointer to message type information.
 * @param topic_name   Topic name (null-terminated).
 * @param callback     Function called when a message is received.
 * @param context      User pointer passed to @p callback (may be NULL).
 *
 * @retval NROS_RET_OK               on success.
 * @retval NROS_RET_INVALID_ARGUMENT  if any required pointer is NULL.
 * @retval NROS_RET_NOT_INIT          if @p node is not initialized.
 * @retval NROS_RET_ERROR             on initialisation failure.
 *
 * @pre All required pointers must be valid.
 * @pre @p topic_name must be a valid null-terminated string.
 */
NROS_PUBLIC
nros_ret_t nros_subscription_init(struct nros_subscription_t* subscription,
                                  const struct nros_node_t* node,
                                  const struct nros_message_type_t* type_info,
                                  const char* topic_name, nros_subscription_callback_t callback,
                                  void* context);

/**
 * @brief Initialise a subscription with default QoS.
 *
 * Alias for nros_subscription_init() for rclc API compatibility.
 *
 * @param subscription Pointer to a zero-initialized subscription.
 * @param node         Pointer to an initialized node.
 * @param type_info    Pointer to message type information.
 * @param topic_name   Topic name (null-terminated).
 * @param callback     Function called when a message is received.
 * @param context      User pointer passed to @p callback (may be NULL).
 *
 * @retval NROS_RET_OK               on success.
 * @retval NROS_RET_INVALID_ARGUMENT  if any required pointer is NULL.
 * @retval NROS_RET_NOT_INIT          if @p node is not initialized.
 * @retval NROS_RET_ERROR             on initialisation failure.
 *
 * @pre All required pointers must be valid.
 * @pre @p topic_name must be a valid null-terminated string.
 */
NROS_PUBLIC
nros_ret_t nros_subscription_init_default(struct nros_subscription_t* subscription,
                                          const struct nros_node_t* node,
                                          const struct nros_message_type_t* type_info,
                                          const char* topic_name,
                                          nros_subscription_callback_t callback, void* context);

/**
 * @brief Initialise a subscription with best-effort QoS.
 *
 * Use this for sensor data or high-frequency topics where occasional
 * message loss is acceptable but low latency is preferred.
 *
 * @param subscription Pointer to a zero-initialized subscription.
 * @param node         Pointer to an initialized node.
 * @param type_info    Pointer to message type information.
 * @param topic_name   Topic name (null-terminated).
 * @param callback     Function called when a message is received.
 * @param context      User pointer passed to @p callback (may be NULL).
 *
 * @retval NROS_RET_OK               on success.
 * @retval NROS_RET_INVALID_ARGUMENT  if any required pointer is NULL.
 * @retval NROS_RET_NOT_INIT          if @p node is not initialized.
 * @retval NROS_RET_ERROR             on initialisation failure.
 *
 * @pre All required pointers must be valid.
 * @pre @p topic_name must be a valid null-terminated string.
 */
NROS_PUBLIC
nros_ret_t nros_subscription_init_best_effort(struct nros_subscription_t* subscription,
                                              const struct nros_node_t* node,
                                              const struct nros_message_type_t* type_info,
                                              const char* topic_name,
                                              nros_subscription_callback_t callback, void* context);

/**
 * @brief Initialise a subscription with custom QoS.
 *
 * @param subscription Pointer to a zero-initialized subscription.
 * @param node         Pointer to an initialized node.
 * @param type_info    Pointer to message type information.
 * @param topic_name   Topic name (null-terminated).
 * @param callback     Function called when a message is received.
 * @param context      User pointer passed to @p callback (may be NULL).
 * @param qos          Pointer to QoS settings (NULL for default).
 *
 * @retval NROS_RET_OK               on success.
 * @retval NROS_RET_INVALID_ARGUMENT  if any required pointer is NULL.
 * @retval NROS_RET_NOT_INIT          if @p node is not initialized.
 * @retval NROS_RET_ERROR             on initialisation failure.
 *
 * @pre All required pointers must be valid.
 * @pre @p topic_name must be a valid null-terminated string.
 */
NROS_PUBLIC
nros_ret_t nros_subscription_init_with_qos(struct nros_subscription_t* subscription,
                                           const struct nros_node_t* node,
                                           const struct nros_message_type_t* type_info,
                                           const char* topic_name,
                                           nros_subscription_callback_t callback, void* context,
                                           const struct nros_qos_t* qos);

/**
 * @brief Finalise a subscription.
 *
 * @param subscription Pointer to an initialized subscription.
 *
 * @retval NROS_RET_OK               on success.
 * @retval NROS_RET_INVALID_ARGUMENT  if @p subscription is NULL.
 * @retval NROS_RET_NOT_INIT          if not initialized.
 */
NROS_PUBLIC nros_ret_t nros_subscription_fini(struct nros_subscription_t* subscription);

/**
 * @brief Get the topic name of a subscription.
 *
 * @param subscription Pointer to a subscription.
 * @return Null-terminated topic name, or NULL if invalid.
 */
NROS_PUBLIC const char*
nros_subscription_get_topic_name(const struct nros_subscription_t* subscription);

/**
 * @brief Check if subscription is valid (initialized).
 *
 * @param subscription Pointer to a subscription.
 * @return Non-zero if valid, 0 if invalid or NULL.
 */
NROS_PUBLIC int nros_subscription_is_valid(const struct nros_subscription_t* subscription);

#ifdef __cplusplus
}
#endif

#endif /* NROS_SUBSCRIPTION_H */

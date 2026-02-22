/**
 * nros subscription API
 *
 * Subscription creation and message receiving functions.
 *
 * Copyright 2024 nros contributors
 * Licensed under Apache-2.0
 */

#ifndef NROS_SUBSCRIPTION_H
#define NROS_SUBSCRIPTION_H

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
typedef enum nros_subscription_state_t {
    /** Not initialized */
    NROS_SUBSCRIPTION_STATE_UNINITIALIZED = 0,
    /** Initialized and ready */
    NROS_SUBSCRIPTION_STATE_INITIALIZED = 1,
    /** Shutdown */
    NROS_SUBSCRIPTION_STATE_SHUTDOWN = 2,
} nros_subscription_state_t;

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
typedef void (*nros_subscription_callback_t)(
    const uint8_t *data,
    size_t len,
    void *context);

// ============================================================================
// Subscription Structure
// ============================================================================

/**
 * Subscription structure.
 *
 * IMPORTANT: This struct layout must match the Rust `nros_subscription_t` in
 * `packages/core/nros-c/src/subscription.rs` exactly (field order, types, sizes).
 */
typedef struct nros_subscription_t {
    /** Current state */
    nros_subscription_state_t state;
    /** Topic name storage */
    uint8_t topic_name[NROS_MAX_TOPIC_LEN];
    /** Topic name length */
    size_t topic_name_len;
    /** Type name storage */
    uint8_t type_name[NROS_MAX_TYPE_NAME_LEN];
    /** Type name length */
    size_t type_name_len;
    /** Type hash storage */
    uint8_t type_hash[NROS_MAX_TYPE_HASH_LEN];
    /** Type hash length */
    size_t type_hash_len;
    /** User callback function */
    nros_subscription_callback_t callback;
    /** User context pointer */
    void *context;
    /** Pointer to parent node */
    const nros_node_t *node;
    /** QoS settings (internal, do not touch) */
    nros_qos_t _qos;
    /** Handle ID from executor registration (internal, do not touch) */
    size_t _handle_id;
    /** Opaque pointer to internal Rust subscriber (internal, do not touch) */
    void *_internal;
} nros_subscription_t;

// ============================================================================
// Subscription Functions
// ============================================================================

/**
 * Get a zero-initialized subscription.
 *
 * @return Zero-initialized subscription structure
 */
NROS_PUBLIC
nros_subscription_t nros_subscription_get_zero_initialized(void);

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
 * @return NROS_RET_OK on success
 * @return NROS_RET_INVALID_ARGUMENT if any required pointer is NULL
 * @return NROS_RET_NOT_INIT if node is not initialized
 * @return NROS_RET_ERROR on initialization failure
 */
NROS_PUBLIC NROS_WARN_UNUSED
nros_ret_t nros_subscription_init(
    nros_subscription_t *subscription,
    const nros_node_t *node,
    const nros_message_type_t *type_info,
    const char *topic_name,
    nros_subscription_callback_t callback,
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
 * @return NROS_RET_OK on success
 * @return NROS_RET_INVALID_ARGUMENT if any required pointer is NULL
 * @return NROS_RET_NOT_INIT if node is not initialized
 * @return NROS_RET_ERROR on initialization failure
 */
NROS_PUBLIC NROS_WARN_UNUSED
nros_ret_t nros_subscription_init_with_qos(
    nros_subscription_t *subscription,
    const nros_node_t *node,
    const nros_message_type_t *type_info,
    const char *topic_name,
    nros_subscription_callback_t callback,
    void *context,
    const nros_qos_t *qos);

/**
 * Finalize a subscription.
 *
 * @param subscription Pointer to an initialized subscription
 *
 * @return NROS_RET_OK on success
 * @return NROS_RET_INVALID_ARGUMENT if subscription is NULL
 * @return NROS_RET_NOT_INIT if not initialized
 */
NROS_PUBLIC NROS_WARN_UNUSED
nros_ret_t nros_subscription_fini(nros_subscription_t *subscription);

/**
 * Get the topic name of a subscription.
 *
 * @param subscription Pointer to a subscription
 *
 * @return Pointer to topic name (null-terminated), or NULL if invalid
 */
NROS_PUBLIC
const char *nros_subscription_get_topic_name(const nros_subscription_t *subscription);

/**
 * Check if subscription is valid (initialized).
 *
 * @param subscription Pointer to a subscription
 *
 * @return Non-zero if valid, 0 if invalid or NULL
 */
NROS_PUBLIC
int nros_subscription_is_valid(const nros_subscription_t *subscription);

#ifdef __cplusplus
}
#endif

#endif // NROS_SUBSCRIPTION_H

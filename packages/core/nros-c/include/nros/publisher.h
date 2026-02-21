/**
 * nros publisher API
 *
 * Publisher creation and message publishing functions.
 *
 * Copyright 2024 nros contributors
 * Licensed under Apache-2.0
 */

#ifndef NROS_PUBLISHER_H
#define NROS_PUBLISHER_H

#include "nros/types.h"
#include "nros/visibility.h"
#include "nros/node.h"

#ifdef __cplusplus
extern "C" {
#endif

// ============================================================================
// Publisher State
// ============================================================================

/** Publisher state */
typedef enum nros_publisher_state_t {
    /** Not initialized */
    NROS_PUBLISHER_STATE_UNINITIALIZED = 0,
    /** Initialized and ready */
    NROS_PUBLISHER_STATE_INITIALIZED = 1,
    /** Shutdown */
    NROS_PUBLISHER_STATE_SHUTDOWN = 2,
} nros_publisher_state_t;

// ============================================================================
// Publisher Structure
// ============================================================================

/** Publisher structure */
typedef struct nros_publisher_t {
    /** Current state */
    nros_publisher_state_t state;
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
    /** Pointer to parent node */
    const nros_node_t *node;
    /** Opaque pointer to internal Rust publisher */
    void *internal;
} nros_publisher_t;

// ============================================================================
// Publisher Functions
// ============================================================================

/**
 * Get a zero-initialized publisher.
 *
 * @return Zero-initialized publisher structure
 */
NROS_PUBLIC
nros_publisher_t nros_publisher_get_zero_initialized(void);

/**
 * Initialize a publisher with default QoS.
 *
 * @param publisher Pointer to a zero-initialized publisher
 * @param node Pointer to an initialized node
 * @param type_info Pointer to message type information
 * @param topic_name Topic name (null-terminated string)
 *
 * @return NROS_RET_OK on success
 * @return NROS_RET_INVALID_ARGUMENT if any pointer is NULL
 * @return NROS_RET_NOT_INIT if node is not initialized
 * @return NROS_RET_ERROR on initialization failure
 */
NROS_PUBLIC NROS_WARN_UNUSED
nros_ret_t nros_publisher_init(
    nros_publisher_t *publisher,
    const nros_node_t *node,
    const nros_message_type_t *type_info,
    const char *topic_name);

/**
 * Initialize a publisher with custom QoS.
 *
 * @param publisher Pointer to a zero-initialized publisher
 * @param node Pointer to an initialized node
 * @param type_info Pointer to message type information
 * @param topic_name Topic name (null-terminated string)
 * @param qos Pointer to QoS settings (NULL for default)
 *
 * @return NROS_RET_OK on success
 * @return NROS_RET_INVALID_ARGUMENT if any required pointer is NULL
 * @return NROS_RET_NOT_INIT if node is not initialized
 * @return NROS_RET_ERROR on initialization failure
 */
NROS_PUBLIC NROS_WARN_UNUSED
nros_ret_t nros_publisher_init_with_qos(
    nros_publisher_t *publisher,
    const nros_node_t *node,
    const nros_message_type_t *type_info,
    const char *topic_name,
    const nros_qos_t *qos);

/**
 * Publish raw CDR-serialized data.
 *
 * @param publisher Pointer to an initialized publisher
 * @param data Pointer to CDR-serialized message data
 * @param len Length of data in bytes
 *
 * @return NROS_RET_OK on success
 * @return NROS_RET_INVALID_ARGUMENT if any pointer is NULL or len is 0
 * @return NROS_RET_NOT_INIT if publisher is not initialized
 * @return NROS_RET_PUBLISH_FAILED on publish failure
 */
NROS_PUBLIC NROS_WARN_UNUSED
nros_ret_t nros_publish_raw(
    const nros_publisher_t *publisher,
    const uint8_t *data,
    size_t len);

/**
 * Finalize a publisher.
 *
 * @param publisher Pointer to an initialized publisher
 *
 * @return NROS_RET_OK on success
 * @return NROS_RET_INVALID_ARGUMENT if publisher is NULL
 * @return NROS_RET_NOT_INIT if not initialized
 */
NROS_PUBLIC NROS_WARN_UNUSED
nros_ret_t nros_publisher_fini(nros_publisher_t *publisher);

/**
 * Get the topic name of a publisher.
 *
 * @param publisher Pointer to a publisher
 *
 * @return Pointer to topic name (null-terminated), or NULL if invalid
 */
NROS_PUBLIC
const char *nros_publisher_get_topic_name(const nros_publisher_t *publisher);

/**
 * Check if publisher is valid (initialized).
 *
 * @param publisher Pointer to a publisher
 *
 * @return Non-zero if valid, 0 if invalid or NULL
 */
NROS_PUBLIC
int nros_publisher_is_valid(const nros_publisher_t *publisher);

#ifdef __cplusplus
}
#endif

#endif // NROS_PUBLISHER_H

/**
 * @file publisher.h
 * @brief Topic publisher API.
 *
 * Create publishers with nros_publisher_init() and publish serialised
 * messages with nros_publish_raw().
 */

#ifndef NROS_PUBLISHER_H
#define NROS_PUBLISHER_H

#include "nros/types.h"

#ifdef __cplusplus
extern "C" {
#endif

/* Forward declarations */
struct nros_node_t;

/* ===================================================================
 * Types
 * =================================================================== */

/** Publisher state. */
typedef enum nros_publisher_state_t {
    /** Not initialized. */
    NROS_PUBLISHER_STATE_UNINITIALIZED = 0,
    /** Initialized and ready. */
    NROS_PUBLISHER_STATE_INITIALIZED = 1,
    /** Shutdown. */
    NROS_PUBLISHER_STATE_SHUTDOWN = 2,
} nros_publisher_state_t;

/** Publisher structure. */
typedef struct nros_publisher_t {
    /** Current state. */
    enum nros_publisher_state_t state;
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
    /** Pointer to parent node. */
    const struct nros_node_t *node;
    /** Opaque pointer to internal Rust publisher. */
    void *_internal;
} nros_publisher_t;

/* ===================================================================
 * Functions
 * =================================================================== */

/**
 * @brief Get a zero-initialized publisher.
 * @return Zero-initialized @ref nros_publisher_t.
 */
NROS_PUBLIC struct nros_publisher_t nros_publisher_get_zero_initialized(void);

/**
 * @brief Initialise a publisher with default QoS (RELIABLE, KEEP_LAST(10)).
 *
 * This is the recommended initialisation function for most use cases.
 *
 * @param publisher  Pointer to a zero-initialized publisher.
 * @param node       Pointer to an initialized node.
 * @param type_info  Pointer to message type information.
 * @param topic_name Topic name (null-terminated).
 *
 * @retval NROS_RET_OK               on success.
 * @retval NROS_RET_INVALID_ARGUMENT  if any pointer is NULL.
 * @retval NROS_RET_NOT_INIT          if @p node is not initialized.
 * @retval NROS_RET_ERROR             on initialisation failure.
 *
 * @pre All pointers must be valid.
 * @pre @p topic_name must be a valid null-terminated string.
 */
NROS_PUBLIC
nros_ret_t nros_publisher_init(struct nros_publisher_t *publisher,
                               const struct nros_node_t *node,
                               const struct nros_message_type_t *type_info,
                               const char *topic_name);

/**
 * @brief Initialise a publisher with default QoS.
 *
 * Alias for nros_publisher_init() for rclc API compatibility.
 *
 * @param publisher  Pointer to a zero-initialized publisher.
 * @param node       Pointer to an initialized node.
 * @param type_info  Pointer to message type information.
 * @param topic_name Topic name (null-terminated).
 *
 * @retval NROS_RET_OK               on success.
 * @retval NROS_RET_INVALID_ARGUMENT  if any pointer is NULL.
 * @retval NROS_RET_NOT_INIT          if @p node is not initialized.
 * @retval NROS_RET_ERROR             on initialisation failure.
 *
 * @pre All pointers must be valid.
 * @pre @p topic_name must be a valid null-terminated string.
 */
NROS_PUBLIC
nros_ret_t nros_publisher_init_default(struct nros_publisher_t *publisher,
                                       const struct nros_node_t *node,
                                       const struct nros_message_type_t *type_info,
                                       const char *topic_name);

/**
 * @brief Initialise a publisher with best-effort QoS.
 *
 * Use this for sensor data or high-frequency topics where occasional
 * message loss is acceptable but low latency is preferred.
 *
 * @param publisher  Pointer to a zero-initialized publisher.
 * @param node       Pointer to an initialized node.
 * @param type_info  Pointer to message type information.
 * @param topic_name Topic name (null-terminated).
 *
 * @retval NROS_RET_OK               on success.
 * @retval NROS_RET_INVALID_ARGUMENT  if any pointer is NULL.
 * @retval NROS_RET_NOT_INIT          if @p node is not initialized.
 * @retval NROS_RET_ERROR             on initialisation failure.
 *
 * @pre All pointers must be valid.
 * @pre @p topic_name must be a valid null-terminated string.
 */
NROS_PUBLIC
nros_ret_t nros_publisher_init_best_effort(struct nros_publisher_t *publisher,
                                           const struct nros_node_t *node,
                                           const struct nros_message_type_t *type_info,
                                           const char *topic_name);

/**
 * @brief Initialise a publisher with custom QoS.
 *
 * @param publisher  Pointer to a zero-initialized publisher.
 * @param node       Pointer to an initialized node.
 * @param type_info  Pointer to message type information.
 * @param topic_name Topic name (null-terminated).
 * @param qos        Pointer to QoS settings (NULL for default).
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
nros_ret_t nros_publisher_init_with_qos(struct nros_publisher_t *publisher,
                                        const struct nros_node_t *node,
                                        const struct nros_message_type_t *type_info,
                                        const char *topic_name,
                                        const struct nros_qos_t *qos);

/**
 * @brief Publish raw CDR-serialized data.
 *
 * @param publisher  Pointer to an initialized publisher.
 * @param data       Pointer to CDR-serialized message data.
 * @param len        Length of data in bytes.
 *
 * @retval NROS_RET_OK               on success.
 * @retval NROS_RET_INVALID_ARGUMENT  if any pointer is NULL or @p len is 0.
 * @retval NROS_RET_NOT_INIT          if publisher is not initialized.
 * @retval NROS_RET_PUBLISH_FAILED    on publish failure.
 *
 * @pre @p publisher must point to an initialized publisher.
 * @pre @p data must point to @p len valid bytes.
 */
NROS_PUBLIC
nros_ret_t nros_publish_raw(const struct nros_publisher_t *publisher,
                            const uint8_t *data,
                            size_t len);

/**
 * @brief Finalise a publisher.
 *
 * @param publisher  Pointer to an initialized publisher.
 *
 * @retval NROS_RET_OK               on success.
 * @retval NROS_RET_INVALID_ARGUMENT  if @p publisher is NULL.
 * @retval NROS_RET_NOT_INIT          if not initialized.
 */
NROS_PUBLIC nros_ret_t nros_publisher_fini(struct nros_publisher_t *publisher);

/**
 * @brief Get the topic name of a publisher.
 *
 * @param publisher  Pointer to a publisher.
 * @return Null-terminated topic name, or NULL if invalid.
 */
NROS_PUBLIC const char *nros_publisher_get_topic_name(const struct nros_publisher_t *publisher);

/**
 * @brief Check if publisher is valid (initialized).
 *
 * @param publisher  Pointer to a publisher.
 * @return Non-zero if valid, 0 if invalid or NULL.
 */
NROS_PUBLIC int nros_publisher_is_valid(const struct nros_publisher_t *publisher);

#ifdef __cplusplus
}
#endif

#endif /* NROS_PUBLISHER_H */

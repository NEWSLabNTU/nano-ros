/**
 * @file node.h
 * @brief ROS 2 node creation and management.
 *
 * A node represents a participant in the ROS 2 graph.  Create one with
 * nros_node_init() after initialising a support context.
 */

#ifndef NROS_NODE_H
#define NROS_NODE_H

#include "nros/types.h"

#ifdef __cplusplus
extern "C" {
#endif

/* Forward declaration */
struct nros_support_t;

/* ===================================================================
 * Types
 * =================================================================== */

/** Node state. */
typedef enum nros_node_state_t {
    /** Not initialized. */
    NROS_NODE_STATE_UNINITIALIZED = 0,
    /** Initialized and ready. */
    NROS_NODE_STATE_INITIALIZED = 1,
    /** Shutdown. */
    NROS_NODE_STATE_SHUTDOWN = 2,
} nros_node_state_t;

/**
 * Node structure.
 *
 * Represents a ROS 2 node with a name and namespace.
 */
typedef struct nros_node_t {
    /** Current state. */
    enum nros_node_state_t state;
    /** Node name storage. */
    uint8_t name[NROS_MAX_NAME_LEN];
    /** Node name length. */
    size_t name_len;
    /** Namespace storage. */
    uint8_t namespace_[NROS_MAX_NAMESPACE_LEN];
    /** Namespace length. */
    size_t namespace_len;
    /** Pointer to parent support context. */
    const struct nros_support_t *support;
    /** Opaque pointer to internal Rust node. */
    void *_internal;
} nros_node_t;

/* ===================================================================
 * Functions
 * =================================================================== */

/**
 * @brief Get a zero-initialized node.
 * @return Zero-initialized @ref nros_node_t.
 */
NROS_PUBLIC struct nros_node_t nros_node_get_zero_initialized(void);

/**
 * @brief Initialise a node with default options.
 *
 * @param node       Pointer to a zero-initialized node.
 * @param support    Pointer to an initialized support context.
 * @param name       Node name (null-terminated).
 * @param namespace_ Node namespace (null-terminated, use "/" for root).
 *
 * @retval NROS_RET_OK               on success.
 * @retval NROS_RET_INVALID_ARGUMENT  if any pointer is NULL or strings are invalid.
 * @retval NROS_RET_NOT_INIT          if @p support is not initialized.
 * @retval NROS_RET_ERROR             on initialisation failure.
 *
 * @pre All pointers must be valid.
 * @pre @p name and @p namespace_ must be valid null-terminated strings.
 */
NROS_PUBLIC
nros_ret_t nros_node_init(struct nros_node_t *node,
                          const struct nros_support_t *support,
                          const char *name,
                          const char *namespace_);

/**
 * @brief Finalise a node.
 *
 * @param node  Pointer to an initialized node.
 *
 * @retval NROS_RET_OK               on success.
 * @retval NROS_RET_INVALID_ARGUMENT  if @p node is NULL.
 * @retval NROS_RET_NOT_INIT          if not initialized.
 *
 * @pre @p node must point to an initialized @ref nros_node_t.
 */
NROS_PUBLIC nros_ret_t nros_node_fini(struct nros_node_t *node);

/**
 * @brief Get the node name.
 *
 * @param node  Pointer to an initialized node.
 * @return Null-terminated node name, or NULL if invalid.
 */
NROS_PUBLIC const char *nros_node_get_name(const struct nros_node_t *node);

/**
 * @brief Get the node namespace.
 *
 * @param node  Pointer to an initialized node.
 * @return Null-terminated node namespace, or NULL if invalid.
 */
NROS_PUBLIC const char *nros_node_get_namespace(const struct nros_node_t *node);

#ifdef __cplusplus
}
#endif

#endif /* NROS_NODE_H */

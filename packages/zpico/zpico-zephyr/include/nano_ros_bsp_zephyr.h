/**
 * @file nano_ros_bsp_zephyr.h
 * @brief nros Board Support Package for Zephyr RTOS
 *
 * This library provides a simplified API for creating ROS 2 nodes on Zephyr.
 * It abstracts away zenoh-pico configuration and provides ROS 2 compatible
 * topic naming.
 *
 * @example
 * @code
 * #include <nano_ros_bsp_zephyr.h>
 *
 * void main(void) {
 *     // Initialize BSP (uses Kconfig for zenoh locator)
 *     nano_ros_bsp_context_t ctx;
 *     nano_ros_bsp_init(&ctx);
 *
 *     // Create node
 *     nros_node_t node;
 *     nano_ros_bsp_create_node(&ctx, &node, "my_node");
 *
 *     // Create publisher
 *     nano_ros_publisher_t pub;
 *     nano_ros_bsp_create_publisher(&node, &pub, "/chatter", "std_msgs::msg::dds_::Int32_");
 *
 *     // Publish messages
 *     uint8_t buffer[64];
 *     while (1) {
 *         // ... serialize message to buffer ...
 *         nano_ros_bsp_publish(&pub, buffer, len);
 *         nano_ros_bsp_spin_once(&ctx, K_SECONDS(1));
 *     }
 * }
 * @endcode
 *
 * @copyright Copyright (c) 2024 nros contributors
 * @license MIT OR Apache-2.0
 */

#ifndef NANO_ROS_BSP_ZEPHYR_H
#define NANO_ROS_BSP_ZEPHYR_H

#include <stdint.h>
#include <stddef.h>
#include <stdbool.h>
#include <zephyr/kernel.h>

#ifdef __cplusplus
extern "C" {
#endif

/* ============================================================================
 * Error Codes
 * ============================================================================ */

/** Operation successful */
#define NANO_ROS_BSP_OK              0
/** Generic error */
#define NANO_ROS_BSP_ERR            -1
/** Not initialized */
#define NANO_ROS_BSP_ERR_NOT_INIT  -2
/** Resource limit reached */
#define NANO_ROS_BSP_ERR_LIMIT     -3
/** Invalid argument */
#define NANO_ROS_BSP_ERR_INVALID   -4
/** Connection failed */
#define NANO_ROS_BSP_ERR_CONNECT   -5
/** Timeout */
#define NANO_ROS_BSP_ERR_TIMEOUT   -6

/* ============================================================================
 * Types
 * ============================================================================ */

/** BSP context - manages zenoh session */
typedef struct nano_ros_bsp_context {
    bool initialized;
    bool session_open;
} nano_ros_bsp_context_t;

/** Node handle */
typedef struct nros_node {
    nano_ros_bsp_context_t *ctx;
    const char *name;
    int32_t domain_id;
} nros_node_t;

/** Publisher handle */
typedef struct nano_ros_publisher {
    nros_node_t *node;
    int32_t handle;
    char keyexpr[256];
} nano_ros_publisher_t;

/** Subscriber callback type */
typedef void (*nano_ros_subscriber_callback_t)(
    const uint8_t *data,
    size_t len,
    void *user_data
);

/** Subscriber handle */
typedef struct nano_ros_subscriber {
    nros_node_t *node;
    int32_t handle;
    char keyexpr[256];
    nano_ros_subscriber_callback_t callback;
    void *user_data;
} nano_ros_subscriber_t;

/* ============================================================================
 * Initialization
 * ============================================================================ */

/**
 * @brief Initialize the nros BSP
 *
 * This initializes the zenoh-pico transport using configuration from Kconfig:
 * - CONFIG_NANO_ROS_ZENOH_LOCATOR: Router address
 * - CONFIG_NANO_ROS_INIT_DELAY_MS: Startup delay
 *
 * @param ctx Context to initialize
 * @return NANO_ROS_BSP_OK on success, error code otherwise
 */
int32_t nano_ros_bsp_init(nano_ros_bsp_context_t *ctx);

/**
 * @brief Initialize with custom locator
 *
 * @param ctx Context to initialize
 * @param locator Zenoh router locator (e.g., "tcp/192.168.1.1:7447")
 * @return NANO_ROS_BSP_OK on success, error code otherwise
 */
int32_t nano_ros_bsp_init_with_locator(nano_ros_bsp_context_t *ctx, const char *locator);

/**
 * @brief Shutdown the BSP and release resources
 *
 * @param ctx Context to shutdown
 */
void nano_ros_bsp_shutdown(nano_ros_bsp_context_t *ctx);

/**
 * @brief Check if BSP is initialized
 *
 * @param ctx Context to check
 * @return true if initialized and session open
 */
bool nano_ros_bsp_is_ready(const nano_ros_bsp_context_t *ctx);

/* ============================================================================
 * Node Management
 * ============================================================================ */

/**
 * @brief Create a ROS 2 node
 *
 * @param ctx Initialized BSP context
 * @param node Node handle to initialize
 * @param name Node name (e.g., "my_node")
 * @return NANO_ROS_BSP_OK on success, error code otherwise
 */
int32_t nano_ros_bsp_create_node(
    nano_ros_bsp_context_t *ctx,
    nros_node_t *node,
    const char *name
);

/**
 * @brief Create a node with custom domain ID
 *
 * @param ctx Initialized BSP context
 * @param node Node handle to initialize
 * @param name Node name
 * @param domain_id ROS 2 domain ID
 * @return NANO_ROS_BSP_OK on success, error code otherwise
 */
int32_t nano_ros_bsp_create_node_with_domain(
    nano_ros_bsp_context_t *ctx,
    nros_node_t *node,
    const char *name,
    int32_t domain_id
);

/* ============================================================================
 * Publisher
 * ============================================================================ */

/**
 * @brief Create a publisher
 *
 * @param node Node that owns the publisher
 * @param pub Publisher handle to initialize
 * @param topic Topic name (e.g., "/chatter")
 * @param type_name ROS 2 type name (e.g., "std_msgs::msg::dds_::Int32_")
 * @return NANO_ROS_BSP_OK on success, error code otherwise
 */
int32_t nano_ros_bsp_create_publisher(
    nros_node_t *node,
    nano_ros_publisher_t *pub,
    const char *topic,
    const char *type_name
);

/**
 * @brief Publish a message
 *
 * @param pub Publisher handle
 * @param data Serialized message data (CDR format with header)
 * @param len Data length
 * @return NANO_ROS_BSP_OK on success, error code otherwise
 */
int32_t nano_ros_bsp_publish(
    nano_ros_publisher_t *pub,
    const uint8_t *data,
    size_t len
);

/**
 * @brief Destroy a publisher
 *
 * @param pub Publisher to destroy
 */
void nano_ros_bsp_destroy_publisher(nano_ros_publisher_t *pub);

/* ============================================================================
 * Subscriber
 * ============================================================================ */

/**
 * @brief Create a subscriber
 *
 * @param node Node that owns the subscriber
 * @param sub Subscriber handle to initialize
 * @param topic Topic name (e.g., "/chatter")
 * @param type_name ROS 2 type name
 * @param callback Function called when messages arrive
 * @param user_data User data passed to callback
 * @return NANO_ROS_BSP_OK on success, error code otherwise
 */
int32_t nano_ros_bsp_create_subscriber(
    nros_node_t *node,
    nano_ros_subscriber_t *sub,
    const char *topic,
    const char *type_name,
    nano_ros_subscriber_callback_t callback,
    void *user_data
);

/**
 * @brief Destroy a subscriber
 *
 * @param sub Subscriber to destroy
 */
void nano_ros_bsp_destroy_subscriber(nano_ros_subscriber_t *sub);

/* ============================================================================
 * Spinning
 * ============================================================================ */

/**
 * @brief Process pending callbacks and network events
 *
 * Call this periodically to handle incoming messages.
 *
 * @param ctx BSP context
 * @param timeout Maximum time to wait for events
 * @return NANO_ROS_BSP_OK on success, error code otherwise
 */
int32_t nano_ros_bsp_spin_once(nano_ros_bsp_context_t *ctx, k_timeout_t timeout);

/**
 * @brief Spin forever processing events
 *
 * This function does not return unless an error occurs.
 *
 * @param ctx BSP context
 * @return Error code (only returns on error)
 */
int32_t nano_ros_bsp_spin(nano_ros_bsp_context_t *ctx);

/* ============================================================================
 * Utility
 * ============================================================================ */

/**
 * @brief Build ROS 2 keyexpr from topic and type (for publishers)
 *
 * Constructs the zenoh keyexpr in ROS 2 format:
 * `<domain_id><topic>/<type_name>/TypeHashNotSupported`
 *
 * @param buffer Output buffer
 * @param buffer_size Buffer size
 * @param domain_id ROS 2 domain ID
 * @param topic Topic name
 * @param type_name ROS 2 type name
 * @return Length of keyexpr, or -1 on error
 */
int32_t nano_ros_bsp_build_keyexpr(
    char *buffer,
    size_t buffer_size,
    int32_t domain_id,
    const char *topic,
    const char *type_name
);

/**
 * @brief Build ROS 2 keyexpr with wildcard (for subscribers)
 *
 * Constructs the zenoh keyexpr with wildcard for type hash:
 * `<domain_id><topic>/<type_name>/*`
 *
 * This allows receiving from any publisher regardless of type hash.
 *
 * @param buffer Output buffer
 * @param buffer_size Buffer size
 * @param domain_id ROS 2 domain ID
 * @param topic Topic name
 * @param type_name ROS 2 type name
 * @return Length of keyexpr, or -1 on error
 */
int32_t nano_ros_bsp_build_keyexpr_wildcard(
    char *buffer,
    size_t buffer_size,
    int32_t domain_id,
    const char *topic,
    const char *type_name
);

#ifdef __cplusplus
}
#endif

#endif /* NANO_ROS_BSP_ZEPHYR_H */

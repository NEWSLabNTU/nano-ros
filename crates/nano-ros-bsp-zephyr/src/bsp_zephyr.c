/**
 * @file bsp_zephyr.c
 * @brief nano-ros BSP implementation for Zephyr RTOS
 *
 * @copyright Copyright (c) 2024 nano-ros contributors
 * @license MIT OR Apache-2.0
 */

#include "nano_ros_bsp_zephyr.h"
#include "zenoh_shim.h"

#include <zephyr/kernel.h>
#include <zephyr/logging/log.h>
#include <stdio.h>
#include <string.h>

LOG_MODULE_REGISTER(nano_ros_bsp, LOG_LEVEL_INF);

/* ============================================================================
 * Internal subscriber callback adapter
 * ============================================================================ */

/**
 * @brief Internal callback that routes to user callback
 */
static void internal_subscriber_callback(const uint8_t *data, size_t len, void *ctx)
{
    nano_ros_subscriber_t *sub = (nano_ros_subscriber_t *)ctx;
    if (sub && sub->callback) {
        sub->callback(data, len, sub->user_data);
    }
}

/* ============================================================================
 * Initialization
 * ============================================================================ */

int32_t nano_ros_bsp_init(nano_ros_bsp_context_t *ctx)
{
#ifdef CONFIG_NANO_ROS_ZENOH_LOCATOR
    return nano_ros_bsp_init_with_locator(ctx, CONFIG_NANO_ROS_ZENOH_LOCATOR);
#else
    return nano_ros_bsp_init_with_locator(ctx, "tcp/192.0.2.2:7447");
#endif
}

int32_t nano_ros_bsp_init_with_locator(nano_ros_bsp_context_t *ctx, const char *locator)
{
    if (ctx == NULL || locator == NULL) {
        return NANO_ROS_BSP_ERR_INVALID;
    }

    memset(ctx, 0, sizeof(*ctx));

    /* Wait for network to initialize */
#ifdef CONFIG_NANO_ROS_INIT_DELAY_MS
    k_sleep(K_MSEC(CONFIG_NANO_ROS_INIT_DELAY_MS));
#else
    k_sleep(K_MSEC(2000));
#endif

    LOG_INF("Initializing nano-ros BSP");
    LOG_INF("  Locator: %s", locator);

    /* Initialize zenoh shim */
    int32_t ret = zenoh_shim_init(locator);
    if (ret != ZENOH_SHIM_OK) {
        LOG_ERR("Failed to initialize zenoh: %d", ret);
        return NANO_ROS_BSP_ERR_CONNECT;
    }

    ctx->initialized = true;
    LOG_INF("  Zenoh initialized");

    /* Open zenoh session */
    ret = zenoh_shim_open();
    if (ret != ZENOH_SHIM_OK) {
        LOG_ERR("Failed to open zenoh session: %d", ret);
        ctx->initialized = false;
        return NANO_ROS_BSP_ERR_CONNECT;
    }

    ctx->session_open = true;
    LOG_INF("  Session opened");

    return NANO_ROS_BSP_OK;
}

void nano_ros_bsp_shutdown(nano_ros_bsp_context_t *ctx)
{
    if (ctx == NULL) {
        return;
    }

    if (ctx->session_open) {
        zenoh_shim_close();
        ctx->session_open = false;
    }

    ctx->initialized = false;
    LOG_INF("nano-ros BSP shutdown");
}

bool nano_ros_bsp_is_ready(const nano_ros_bsp_context_t *ctx)
{
    return ctx != NULL && ctx->initialized && ctx->session_open;
}

/* ============================================================================
 * Node Management
 * ============================================================================ */

int32_t nano_ros_bsp_create_node(
    nano_ros_bsp_context_t *ctx,
    nano_ros_node_t *node,
    const char *name)
{
#ifdef CONFIG_NANO_ROS_DOMAIN_ID
    return nano_ros_bsp_create_node_with_domain(ctx, node, name, CONFIG_NANO_ROS_DOMAIN_ID);
#else
    return nano_ros_bsp_create_node_with_domain(ctx, node, name, 0);
#endif
}

int32_t nano_ros_bsp_create_node_with_domain(
    nano_ros_bsp_context_t *ctx,
    nano_ros_node_t *node,
    const char *name,
    int32_t domain_id)
{
    if (!nano_ros_bsp_is_ready(ctx) || node == NULL || name == NULL) {
        return NANO_ROS_BSP_ERR_INVALID;
    }

    node->ctx = ctx;
    node->name = name;
    node->domain_id = domain_id;

    LOG_INF("Created node '%s' (domain=%d)", name, domain_id);

    return NANO_ROS_BSP_OK;
}

/* ============================================================================
 * Publisher
 * ============================================================================ */

int32_t nano_ros_bsp_create_publisher(
    nano_ros_node_t *node,
    nano_ros_publisher_t *pub,
    const char *topic,
    const char *type_name)
{
    if (node == NULL || pub == NULL || topic == NULL || type_name == NULL) {
        return NANO_ROS_BSP_ERR_INVALID;
    }

    if (!nano_ros_bsp_is_ready(node->ctx)) {
        return NANO_ROS_BSP_ERR_NOT_INIT;
    }

    /* Build keyexpr */
    int32_t keyexpr_len = nano_ros_bsp_build_keyexpr(
        pub->keyexpr, sizeof(pub->keyexpr),
        node->domain_id, topic, type_name
    );

    if (keyexpr_len < 0) {
        return NANO_ROS_BSP_ERR_INVALID;
    }

    /* Declare publisher */
    int32_t handle = zenoh_shim_declare_publisher(pub->keyexpr);
    if (handle < 0) {
        LOG_ERR("Failed to declare publisher: %d", handle);
        return NANO_ROS_BSP_ERR;
    }

    pub->node = node;
    pub->handle = handle;

    LOG_INF("Publisher created (handle=%d): %s", handle, pub->keyexpr);

    return NANO_ROS_BSP_OK;
}

int32_t nano_ros_bsp_publish(
    nano_ros_publisher_t *pub,
    const uint8_t *data,
    size_t len)
{
    if (pub == NULL || data == NULL || len == 0) {
        return NANO_ROS_BSP_ERR_INVALID;
    }

    int32_t ret = zenoh_shim_publish(pub->handle, data, len);
    if (ret != ZENOH_SHIM_OK) {
        return NANO_ROS_BSP_ERR;
    }

    return NANO_ROS_BSP_OK;
}

void nano_ros_bsp_destroy_publisher(nano_ros_publisher_t *pub)
{
    if (pub == NULL || pub->handle < 0) {
        return;
    }

    zenoh_shim_undeclare_publisher(pub->handle);
    pub->handle = -1;
    pub->node = NULL;

    LOG_INF("Publisher destroyed");
}

/* ============================================================================
 * Subscriber
 * ============================================================================ */

int32_t nano_ros_bsp_create_subscriber(
    nano_ros_node_t *node,
    nano_ros_subscriber_t *sub,
    const char *topic,
    const char *type_name,
    nano_ros_subscriber_callback_t callback,
    void *user_data)
{
    if (node == NULL || sub == NULL || topic == NULL || type_name == NULL || callback == NULL) {
        return NANO_ROS_BSP_ERR_INVALID;
    }

    if (!nano_ros_bsp_is_ready(node->ctx)) {
        return NANO_ROS_BSP_ERR_NOT_INIT;
    }

    /* Build keyexpr with wildcard for subscribers (to receive any type hash) */
    int32_t keyexpr_len = nano_ros_bsp_build_keyexpr_wildcard(
        sub->keyexpr, sizeof(sub->keyexpr),
        node->domain_id, topic, type_name
    );

    if (keyexpr_len < 0) {
        return NANO_ROS_BSP_ERR_INVALID;
    }

    /* Store callback info before declaring (for use in internal callback) */
    sub->node = node;
    sub->callback = callback;
    sub->user_data = user_data;

    /* Declare subscriber with internal callback adapter */
    int32_t handle = zenoh_shim_declare_subscriber(
        sub->keyexpr,
        internal_subscriber_callback,
        sub
    );

    if (handle < 0) {
        LOG_ERR("Failed to declare subscriber: %d", handle);
        return NANO_ROS_BSP_ERR;
    }

    sub->handle = handle;

    LOG_INF("Subscriber created (handle=%d): %s", handle, sub->keyexpr);

    return NANO_ROS_BSP_OK;
}

void nano_ros_bsp_destroy_subscriber(nano_ros_subscriber_t *sub)
{
    if (sub == NULL || sub->handle < 0) {
        return;
    }

    zenoh_shim_undeclare_subscriber(sub->handle);
    sub->handle = -1;
    sub->node = NULL;
    sub->callback = NULL;
    sub->user_data = NULL;

    LOG_INF("Subscriber destroyed");
}

/* ============================================================================
 * Spinning
 * ============================================================================ */

int32_t nano_ros_bsp_spin_once(nano_ros_bsp_context_t *ctx, k_timeout_t timeout)
{
    if (!nano_ros_bsp_is_ready(ctx)) {
        return NANO_ROS_BSP_ERR_NOT_INIT;
    }

    /* Convert Zephyr timeout to milliseconds */
    uint32_t timeout_ms = 0;
    if (K_TIMEOUT_EQ(timeout, K_FOREVER)) {
        timeout_ms = UINT32_MAX;
    } else if (!K_TIMEOUT_EQ(timeout, K_NO_WAIT)) {
        timeout_ms = (uint32_t)k_ticks_to_ms_floor64(timeout.ticks);
    }

    /* Poll zenoh for events */
    int32_t ret = zenoh_shim_spin_once(timeout_ms);
    if (ret < 0 && ret != ZENOH_SHIM_ERR_TIMEOUT) {
        return NANO_ROS_BSP_ERR;
    }

    return NANO_ROS_BSP_OK;
}

int32_t nano_ros_bsp_spin(nano_ros_bsp_context_t *ctx)
{
    if (!nano_ros_bsp_is_ready(ctx)) {
        return NANO_ROS_BSP_ERR_NOT_INIT;
    }

    while (1) {
        int32_t ret = nano_ros_bsp_spin_once(ctx, K_MSEC(100));
        if (ret != NANO_ROS_BSP_OK && ret != NANO_ROS_BSP_ERR_TIMEOUT) {
            return ret;
        }
    }

    /* Unreachable */
    return NANO_ROS_BSP_OK;
}

/* ============================================================================
 * Utility
 * ============================================================================ */

int32_t nano_ros_bsp_build_keyexpr(
    char *buffer,
    size_t buffer_size,
    int32_t domain_id,
    const char *topic,
    const char *type_name)
{
    if (buffer == NULL || buffer_size == 0 || topic == NULL || type_name == NULL) {
        return -1;
    }

    /* ROS 2 keyexpr format: <domain_id><topic>/<type_name>/TypeHashNotSupported */
    int len = snprintf(buffer, buffer_size, "%d%s/%s/TypeHashNotSupported",
                       domain_id, topic, type_name);

    if (len < 0 || (size_t)len >= buffer_size) {
        return -1;
    }

    return len;
}

int32_t nano_ros_bsp_build_keyexpr_wildcard(
    char *buffer,
    size_t buffer_size,
    int32_t domain_id,
    const char *topic,
    const char *type_name)
{
    if (buffer == NULL || buffer_size == 0 || topic == NULL || type_name == NULL) {
        return -1;
    }

    /* ROS 2 keyexpr format with wildcard: <domain_id><topic>/<type_name>/* */
    int len = snprintf(buffer, buffer_size, "%d%s/%s/*",
                       domain_id, topic, type_name);

    if (len < 0 || (size_t)len >= buffer_size) {
        return -1;
    }

    return len;
}

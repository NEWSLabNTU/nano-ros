/**
 * @file main.c
 * @brief Zephyr C listener example using nros BSP
 *
 * This example demonstrates using nros BSP for subscriptions on Zephyr.
 * The BSP handles zenoh initialization and ROS 2 keyexpr formatting.
 */

#include <zephyr/kernel.h>
#include <zephyr/logging/log.h>

#include "nano_ros_bsp_zephyr.h"

LOG_MODULE_REGISTER(nano_ros_listener, LOG_LEVEL_INF);

/* ============================================================================
 * std_msgs/Int32 message support
 * ============================================================================ */

typedef struct std_msgs_Int32 {
    int32_t data;
} std_msgs_Int32;

/**
 * Deserialize from CDR format
 * CDR format for Int32: 4-byte header + 4-byte int32
 */
static int32_t std_msgs_Int32_deserialize(std_msgs_Int32 *msg, const uint8_t *buffer, size_t buffer_size)
{
    if (buffer_size < 8) {
        return -1;
    }
    /* Skip CDR header (4 bytes), read little-endian int32 */
    msg->data = (int32_t)(
        buffer[4] |
        ((uint32_t)buffer[5] << 8) |
        ((uint32_t)buffer[6] << 16) |
        ((uint32_t)buffer[7] << 24)
    );
    return 0;
}

/* ============================================================================
 * Subscription Callback
 * ============================================================================ */

static int message_count = 0;

static void on_message(const uint8_t *data, size_t len, void *user_data)
{
    (void)user_data;

    std_msgs_Int32 msg;
    if (std_msgs_Int32_deserialize(&msg, data, len) == 0) {
        message_count++;
        LOG_INF("Received [%d]: %d", message_count, msg.data);
    } else {
        LOG_ERR("Failed to deserialize message (len=%zu)", len);
    }
}

/* ============================================================================
 * Application
 * ============================================================================ */

int main(void)
{
    LOG_INF("nros Zephyr C Listener (BSP)");
    LOG_INF("================================");

    /* Initialize BSP (uses Kconfig for zenoh locator) */
    nano_ros_bsp_context_t ctx;
    int32_t ret = nano_ros_bsp_init(&ctx);
    if (ret != NROS_BSP_OK) {
        LOG_ERR("BSP init failed: %d", ret);
        return 1;
    }

    /* Create node */
    nros_node_t node;
    ret = nano_ros_bsp_create_node(&ctx, &node, "zephyr_listener");
    if (ret != NROS_BSP_OK) {
        LOG_ERR("Node creation failed: %d", ret);
        return 1;
    }

    /* Create subscriber */
    nano_ros_subscriber_t sub;
    ret = nano_ros_bsp_create_subscriber(
        &node, &sub,
        "/chatter", "std_msgs::msg::dds_::Int32_",
        on_message, NULL
    );
    if (ret != NROS_BSP_OK) {
        LOG_ERR("Subscriber creation failed: %d", ret);
        return 1;
    }

    LOG_INF("Waiting for messages...");

    /* Main loop - messages delivered via callback */
    while (1) {
        nano_ros_bsp_spin_once(&ctx, K_MSEC(100));
    }

    /* Cleanup (unreachable in this example) */
    nano_ros_bsp_destroy_subscriber(&sub);
    nano_ros_bsp_shutdown(&ctx);

    return 0;
}

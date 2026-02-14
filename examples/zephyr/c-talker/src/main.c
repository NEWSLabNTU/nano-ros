/**
 * @file main.c
 * @brief Zephyr C talker example using nros BSP
 *
 * This example demonstrates using nros BSP on Zephyr RTOS.
 * The BSP handles zenoh initialization and ROS 2 keyexpr formatting.
 */

#include <zephyr/kernel.h>
#include <zephyr/logging/log.h>

#include "nano_ros_bsp_zephyr.h"

LOG_MODULE_REGISTER(nano_ros_talker, LOG_LEVEL_INF);

/* ============================================================================
 * std_msgs/Int32 message support
 * ============================================================================ */

typedef struct std_msgs_Int32 {
    int32_t data;
} std_msgs_Int32;

/**
 * Serialize to CDR format
 * CDR format for Int32: 4-byte header + 4-byte int32
 */
static int32_t std_msgs_Int32_serialize(const std_msgs_Int32 *msg, uint8_t *buffer, size_t buffer_size)
{
    if (buffer_size < 8) {
        return -1;
    }
    /* CDR header (little-endian) */
    buffer[0] = 0x00;
    buffer[1] = 0x01;
    buffer[2] = 0x00;
    buffer[3] = 0x00;
    /* Little-endian int32 */
    buffer[4] = (uint8_t)(msg->data & 0xFF);
    buffer[5] = (uint8_t)((msg->data >> 8) & 0xFF);
    buffer[6] = (uint8_t)((msg->data >> 16) & 0xFF);
    buffer[7] = (uint8_t)((msg->data >> 24) & 0xFF);
    return 8;
}

/* ============================================================================
 * Application
 * ============================================================================ */

int main(void)
{
    LOG_INF("nros Zephyr C Talker (BSP)");
    LOG_INF("==============================");

    /* Initialize BSP (uses Kconfig for zenoh locator) */
    nano_ros_bsp_context_t ctx;
    int32_t ret = nano_ros_bsp_init(&ctx);
    if (ret != NANO_ROS_BSP_OK) {
        LOG_ERR("BSP init failed: %d", ret);
        return 1;
    }

    /* Create node */
    nros_node_t node;
    ret = nano_ros_bsp_create_node(&ctx, &node, "zephyr_talker");
    if (ret != NANO_ROS_BSP_OK) {
        LOG_ERR("Node creation failed: %d", ret);
        return 1;
    }

    /* Create publisher */
    nano_ros_publisher_t pub;
    ret = nano_ros_bsp_create_publisher(&node, &pub, "/chatter", "std_msgs::msg::dds_::Int32_");
    if (ret != NANO_ROS_BSP_OK) {
        LOG_ERR("Publisher creation failed: %d", ret);
        return 1;
    }

    /* Publish messages */
    std_msgs_Int32 msg = { .data = 0 };
    uint8_t buffer[64];

    LOG_INF("Publishing messages...");

    while (1) {
        msg.data++;

        int32_t len = std_msgs_Int32_serialize(&msg, buffer, sizeof(buffer));
        if (len > 0) {
            ret = nano_ros_bsp_publish(&pub, buffer, (size_t)len);
            if (ret == NANO_ROS_BSP_OK) {
                LOG_INF("Published: %d", msg.data);
            } else {
                LOG_ERR("Publish failed: %d", ret);
            }
        }

        nano_ros_bsp_spin_once(&ctx, K_SECONDS(1));
    }

    /* Cleanup (unreachable in this example) */
    nano_ros_bsp_destroy_publisher(&pub);
    nano_ros_bsp_shutdown(&ctx);

    return 0;
}

/**
 * @file main.c
 * @brief Zephyr C talker example - publishes std_msgs/Int32 messages
 *
 * This example demonstrates using nano-ros C API on Zephyr RTOS.
 * It uses the zenoh shim singleton API for pub/sub communication.
 */

#include <zephyr/kernel.h>
#include <zephyr/logging/log.h>

#include "zenoh_shim.h"

LOG_MODULE_REGISTER(nano_ros_talker, LOG_LEVEL_INF);

// ----------------------------------------------------------------------------
// std_msgs/Int32 message support
// ----------------------------------------------------------------------------

typedef struct std_msgs_Int32 {
    int32_t data;
} std_msgs_Int32;

// Serialize to CDR format
// CDR format for Int32: 4-byte header + 4-byte int32
static int32_t std_msgs_Int32_serialize(const std_msgs_Int32* msg, uint8_t* buffer, size_t buffer_size) {
    if (buffer_size < 8) {
        return -1;
    }
    // CDR header (little-endian)
    buffer[0] = 0x00;
    buffer[1] = 0x01;
    buffer[2] = 0x00;
    buffer[3] = 0x00;
    // Little-endian int32
    buffer[4] = (uint8_t)(msg->data & 0xFF);
    buffer[5] = (uint8_t)((msg->data >> 8) & 0xFF);
    buffer[6] = (uint8_t)((msg->data >> 16) & 0xFF);
    buffer[7] = (uint8_t)((msg->data >> 24) & 0xFF);
    return 8;
}

// ----------------------------------------------------------------------------
// ROS 2 Topic Configuration
// ----------------------------------------------------------------------------

#define DOMAIN_ID 0
#define TOPIC_NAME "/chatter"
#define TYPE_NAME "std_msgs::msg::dds_::Int32_"

// ROS 2 keyexpr format: domain_id/topic_name/type_name/TypeHashNotSupported
static char topic_keyexpr[256];

// Zenoh locator (router address)
#define ZENOH_LOCATOR "tcp/192.0.2.2:7447"

// ----------------------------------------------------------------------------
// Application
// ----------------------------------------------------------------------------

static int32_t pub_handle = -1;

int main(void)
{
    LOG_INF("nano-ros Zephyr C Talker");
    LOG_INF("========================");

    // Give network time to initialize
    k_sleep(K_SECONDS(2));

    // Build ROS 2 compatible keyexpr
    snprintf(topic_keyexpr, sizeof(topic_keyexpr),
             "%d%s/%s/TypeHashNotSupported",
             DOMAIN_ID, TOPIC_NAME, TYPE_NAME);

    LOG_INF("Locator: %s", ZENOH_LOCATOR);
    LOG_INF("Topic keyexpr: %s", topic_keyexpr);

    // Initialize zenoh shim
    int32_t ret = zenoh_shim_init(ZENOH_LOCATOR);
    if (ret != ZENOH_SHIM_OK) {
        LOG_ERR("Failed to initialize zenoh: %d", ret);
        return 1;
    }
    LOG_INF("Zenoh initialized");

    // Open zenoh session
    ret = zenoh_shim_open();
    if (ret != ZENOH_SHIM_OK) {
        LOG_ERR("Failed to open zenoh session: %d", ret);
        return 1;
    }
    LOG_INF("Zenoh session opened");

    // Declare publisher (returns handle)
    pub_handle = zenoh_shim_declare_publisher(topic_keyexpr);
    if (pub_handle < 0) {
        LOG_ERR("Failed to declare publisher: %d", pub_handle);
        zenoh_shim_close();
        return 1;
    }
    LOG_INF("Publisher declared (handle=%d) for: %s", pub_handle, topic_keyexpr);

    // Publish messages
    std_msgs_Int32 msg = { .data = 0 };
    uint8_t buffer[64];

    LOG_INF("Publishing messages...");

    while (1) {
        msg.data++;

        int32_t len = std_msgs_Int32_serialize(&msg, buffer, sizeof(buffer));
        if (len > 0) {
            ret = zenoh_shim_publish(pub_handle, buffer, (size_t)len);
            if (ret == ZENOH_SHIM_OK) {
                LOG_INF("Published: %d", msg.data);
            } else {
                LOG_ERR("Publish failed: %d", ret);
            }
        }

        k_sleep(K_SECONDS(1));
    }

    // Cleanup (unreachable in this example)
    zenoh_shim_undeclare_publisher(pub_handle);
    zenoh_shim_close();

    return 0;
}

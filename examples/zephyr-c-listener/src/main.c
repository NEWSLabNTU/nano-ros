/**
 * @file main.c
 * @brief Zephyr C listener example - subscribes to std_msgs/Int32 messages
 *
 * This example demonstrates using nano-ros C API on Zephyr RTOS.
 * It uses the zenoh shim singleton API for pub/sub communication.
 */

#include <zephyr/kernel.h>
#include <zephyr/logging/log.h>

#include "zenoh_shim.h"

LOG_MODULE_REGISTER(nano_ros_listener, LOG_LEVEL_INF);

// ----------------------------------------------------------------------------
// std_msgs/Int32 message support
// ----------------------------------------------------------------------------

typedef struct std_msgs_Int32 {
    int32_t data;
} std_msgs_Int32;

// Deserialize from CDR format
// CDR format for Int32: 4-byte header + 4-byte int32
static int32_t std_msgs_Int32_deserialize(std_msgs_Int32* msg, const uint8_t* buffer, size_t buffer_size) {
    if (buffer_size < 8) {
        return -1;
    }
    // Skip CDR header (4 bytes), read little-endian int32
    msg->data = (int32_t)(
        buffer[4] |
        ((uint32_t)buffer[5] << 8) |
        ((uint32_t)buffer[6] << 16) |
        ((uint32_t)buffer[7] << 24)
    );
    return 0;
}

// ----------------------------------------------------------------------------
// ROS 2 Topic Configuration
// ----------------------------------------------------------------------------

#define DOMAIN_ID 0
#define TOPIC_NAME "/chatter"
#define TYPE_NAME "std_msgs::msg::dds_::Int32_"

// ROS 2 keyexpr format with wildcard for type hash
static char topic_keyexpr[256];

// Zenoh locator (router address)
#define ZENOH_LOCATOR "tcp/192.0.2.2:7447"

// ----------------------------------------------------------------------------
// Subscription Callback
// ----------------------------------------------------------------------------

static int message_count = 0;

// Callback signature: (const uint8_t *data, uintptr_t len, void *ctx)
static void subscription_callback(const uint8_t* data, uintptr_t len, void* context)
{
    (void)context;

    std_msgs_Int32 msg;
    if (std_msgs_Int32_deserialize(&msg, data, (size_t)len) == 0) {
        message_count++;
        LOG_INF("Received [%d]: %d", message_count, msg.data);
    } else {
        LOG_ERR("Failed to deserialize message (len=%zu)", (size_t)len);
    }
}

// ----------------------------------------------------------------------------
// Application
// ----------------------------------------------------------------------------

static int32_t sub_handle = -1;

int main(void)
{
    LOG_INF("nano-ros Zephyr C Listener");
    LOG_INF("==========================");

    // Give network time to initialize
    k_sleep(K_SECONDS(2));

    // Build ROS 2 compatible keyexpr with wildcard for type hash
    // This allows receiving from publishers with any type hash
    snprintf(topic_keyexpr, sizeof(topic_keyexpr),
             "%d%s/%s/*",
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

    // Declare subscriber (returns handle)
    sub_handle = zenoh_shim_declare_subscriber(topic_keyexpr, subscription_callback, NULL);
    if (sub_handle < 0) {
        LOG_ERR("Failed to declare subscriber: %d", sub_handle);
        zenoh_shim_close();
        return 1;
    }
    LOG_INF("Subscriber declared (handle=%d) for: %s", sub_handle, topic_keyexpr);

    LOG_INF("Waiting for messages...");

    // Main loop - zenoh tasks handle message delivery
    while (1) {
        // Messages are delivered via callback from zenoh tasks
        k_sleep(K_MSEC(100));
    }

    // Cleanup (unreachable in this example)
    zenoh_shim_undeclare_subscriber(sub_handle);
    zenoh_shim_close();

    return 0;
}

/**
 * @file main.c
 * @brief Zephyr C listener example using nros-c API
 *
 * This example demonstrates subscribing to Int32 messages on Zephyr RTOS
 * using the nros C API (nros/init.h, nros/node.h, nros/subscription.h,
 * nros/executor.h). The nros module handles zenoh initialization and
 * platform support.
 */

#include <zephyr/kernel.h>
#include <zephyr/logging/log.h>

#include <nros/init.h>
#include <nros/node.h>
#include <nros/subscription.h>
#include <nros/executor.h>
#include <nros/types.h>
#include <zpico_zephyr.h>

LOG_MODULE_REGISTER(nros_listener, LOG_LEVEL_INF);

/* ============================================================================
 * std_msgs/Int32 message support (hand-deserialized CDR)
 * ============================================================================ */

/** Message type info for std_msgs/Int32 */
static const nano_ros_message_type_t INT32_TYPE = {
    .type_name = "std_msgs::msg::dds_::Int32_",
    .type_hash = "TypeHashNotSupported",
    .serialized_size_max = 8,
};

/**
 * Deserialize Int32 from CDR format (4-byte header + 4-byte int32)
 */
static int32_t deserialize_int32(const uint8_t *buffer, size_t buffer_size)
{
    if (buffer_size < 8) {
        return 0;
    }
    /* Skip CDR header (4 bytes), read little-endian int32 */
    return (int32_t)(
        buffer[4] |
        ((uint32_t)buffer[5] << 8) |
        ((uint32_t)buffer[6] << 16) |
        ((uint32_t)buffer[7] << 24)
    );
}

/* ============================================================================
 * Subscription Callback
 * ============================================================================ */

static int message_count = 0;

static void on_message(const uint8_t *data, size_t len, void *context)
{
    (void)context;

    int32_t value = deserialize_int32(data, len);
    message_count++;
    LOG_INF("Received [%d]: %d", message_count, value);
}

/* ============================================================================
 * Application
 * ============================================================================ */

int main(void)
{
    LOG_INF("nros Zephyr C Listener");
    LOG_INF("=======================");

    /* Wait for network interface */
    if (zpico_zephyr_wait_network(CONFIG_NROS_INIT_DELAY_MS) != 0) {
        LOG_ERR("Network not ready");
        return 1;
    }

    /* Initialize support context */
    nano_ros_support_t support = nano_ros_support_get_zero_initialized();
    nano_ros_ret_t ret = nano_ros_support_init(
        &support,
        CONFIG_NROS_ZENOH_LOCATOR,
        CONFIG_NROS_DOMAIN_ID);
    if (ret != NROS_RET_OK) {
        LOG_ERR("Support init failed: %d", ret);
        return 1;
    }

    /* Create node */
    nros_node_t node = nros_node_get_zero_initialized();
    ret = nros_node_init(&node, &support, "zephyr_listener", "/");
    if (ret != NROS_RET_OK) {
        LOG_ERR("Node init failed: %d", ret);
        return 1;
    }

    /* Create subscription */
    nano_ros_subscription_t sub = nano_ros_subscription_get_zero_initialized();
    ret = nano_ros_subscription_init(
        &sub, &node, &INT32_TYPE, "/chatter",
        on_message, NULL);
    if (ret != NROS_RET_OK) {
        LOG_ERR("Subscription init failed: %d", ret);
        return 1;
    }

    /* Create executor and add subscription */
    nano_ros_executor_t executor = nano_ros_executor_get_zero_initialized();
    ret = nano_ros_executor_init(&executor, &support, 1);
    if (ret != NROS_RET_OK) {
        LOG_ERR("Executor init failed: %d", ret);
        return 1;
    }

    ret = nano_ros_executor_add_subscription(
        &executor, &sub, NROS_EXECUTOR_ON_NEW_DATA);
    if (ret != NROS_RET_OK) {
        LOG_ERR("Failed to add subscription to executor: %d", ret);
        return 1;
    }

    LOG_INF("Waiting for messages...");

    /* Spin forever — executor dispatches callbacks */
    nano_ros_executor_spin(&executor);

    /* Cleanup (unreachable in this example) */
    nano_ros_executor_fini(&executor);
    nano_ros_subscription_fini(&sub);
    nros_node_fini(&node);
    nano_ros_support_fini(&support);

    return 0;
}

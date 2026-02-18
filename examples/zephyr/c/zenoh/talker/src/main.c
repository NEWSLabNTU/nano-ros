/**
 * @file main.c
 * @brief Zephyr C talker example using nros-c API
 *
 * This example demonstrates publishing Int32 messages on Zephyr RTOS
 * using the nros C API (nros/init.h, nros/node.h, nros/publisher.h).
 * The nros module handles zenoh initialization and platform support.
 */

#include <zephyr/kernel.h>
#include <zephyr/logging/log.h>

#include <nros/init.h>
#include <nros/node.h>
#include <nros/publisher.h>
#include <nros/types.h>
#include <zpico_zephyr.h>

LOG_MODULE_REGISTER(nros_talker, LOG_LEVEL_INF);

/* ============================================================================
 * std_msgs/Int32 message support (hand-serialized CDR)
 * ============================================================================ */

/** Message type info for std_msgs/Int32 */
static const nano_ros_message_type_t INT32_TYPE = {
    .type_name = "std_msgs::msg::dds_::Int32_",
    .type_hash = "TypeHashNotSupported",
    .serialized_size_max = 8,
};

/**
 * Serialize Int32 to CDR format (4-byte header + 4-byte int32)
 */
static int32_t serialize_int32(int32_t value, uint8_t *buffer, size_t buffer_size)
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
    buffer[4] = (uint8_t)(value & 0xFF);
    buffer[5] = (uint8_t)((value >> 8) & 0xFF);
    buffer[6] = (uint8_t)((value >> 16) & 0xFF);
    buffer[7] = (uint8_t)((value >> 24) & 0xFF);
    return 8;
}

/* ============================================================================
 * Application
 * ============================================================================ */

int main(void)
{
    LOG_INF("nros Zephyr C Talker");
    LOG_INF("=====================");

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
    ret = nros_node_init(&node, &support, "zephyr_talker", "/");
    if (ret != NROS_RET_OK) {
        LOG_ERR("Node init failed: %d", ret);
        return 1;
    }

    /* Create publisher */
    nano_ros_publisher_t pub = nano_ros_publisher_get_zero_initialized();
    ret = nano_ros_publisher_init(&pub, &node, &INT32_TYPE, "/chatter");
    if (ret != NROS_RET_OK) {
        LOG_ERR("Publisher init failed: %d", ret);
        return 1;
    }

    /* Publish messages */
    int32_t count = 0;
    uint8_t buffer[64];

    LOG_INF("Publishing messages...");

    while (1) {
        count++;

        int32_t len = serialize_int32(count, buffer, sizeof(buffer));
        if (len > 0) {
            ret = nano_ros_publish_raw(&pub, buffer, (size_t)len);
            if (ret == NROS_RET_OK) {
                LOG_INF("Published: %d", count);
            } else {
                LOG_ERR("Publish failed: %d", ret);
            }
        }

        k_sleep(K_SECONDS(1));
    }

    /* Cleanup (unreachable in this example) */
    nano_ros_publisher_fini(&pub);
    nros_node_fini(&node);
    nano_ros_support_fini(&support);

    return 0;
}

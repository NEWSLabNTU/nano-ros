/**
 * @file main.c
 * @brief Zephyr C talker example using nros-c API with XRCE-DDS backend
 *
 * This example demonstrates publishing Int32 messages on Zephyr RTOS
 * using the nros C API with Micro-XRCE-DDS transport.
 * The nros module handles XRCE transport setup and platform support.
 */

#include <zephyr/kernel.h>
#include <zephyr/logging/log.h>

#include <nros/init.h>
#include <nros/node.h>
#include <nros/publisher.h>

/* Generated message bindings */
#include "std_msgs.h"

LOG_MODULE_REGISTER(nros_xrce_talker, LOG_LEVEL_INF);

/* ============================================================================
 * Application
 * ============================================================================ */

int main(void)
{
    LOG_INF("nros Zephyr XRCE C Talker");
    LOG_INF("==========================");

    /* Initialize support context (handles network wait + transport setup) */
    nros_support_t support = nros_support_get_zero_initialized();
    nros_ret_t ret = nros_support_init_named(
        &support,
        CONFIG_NROS_XRCE_AGENT_ADDR ":" STRINGIFY(CONFIG_NROS_XRCE_AGENT_PORT),
        CONFIG_NROS_DOMAIN_ID,
        "xrce_talker");
    if (ret != NROS_RET_OK) {
        LOG_ERR("Support init failed: %d", ret);
        return 1;
    }

    /* Create node */
    nros_node_t node = nros_node_get_zero_initialized();
    ret = nros_node_init(&node, &support, "zephyr_xrce_talker", "/");
    if (ret != NROS_RET_OK) {
        LOG_ERR("Node init failed: %d", ret);
        return 1;
    }

    /* Create publisher using generated type support */
    nros_publisher_t pub = nros_publisher_get_zero_initialized();
    ret = nros_publisher_init(
        &pub, &node, std_msgs_msg_int32_get_type_support(), "/chatter");
    if (ret != NROS_RET_OK) {
        LOG_ERR("Publisher init failed: %d", ret);
        return 1;
    }

    /* Publish messages */
    int32_t count = 0;
    uint8_t buffer[64];
    std_msgs_msg_int32 msg;
    std_msgs_msg_int32_init(&msg);

    LOG_INF("Publishing messages...");

    while (1) {
        count++;
        msg.data = count;

        size_t serialized_size = 0;
        int32_t ser_ret = std_msgs_msg_int32_serialize(
            &msg, buffer, sizeof(buffer), &serialized_size);

        if (ser_ret == 0 && serialized_size > 0) {
            ret = nros_publish_raw(&pub, buffer, serialized_size);
            if (ret == NROS_RET_OK) {
                LOG_INF("Published: %d", count);
            } else {
                LOG_ERR("Publish failed: %d", ret);
            }
        } else {
            LOG_ERR("Serialize failed: %d", ser_ret);
        }

        k_sleep(K_SECONDS(1));
    }

    /* Cleanup (unreachable in this example) */
    nros_publisher_fini(&pub);
    nros_node_fini(&node);
    nros_support_fini(&support);

    return 0;
}

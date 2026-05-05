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

LOG_MODULE_REGISTER(nros_listener, LOG_LEVEL_INF);

#define NROS_CHECK_LOG(file, line, expr, ret) \
    LOG_ERR("%s:%d %s -> %d", (file), (line), (expr), (int)(ret))

#include <nros/check.h>
#include <nros/executor.h>
#include <nros/init.h>
#include <nros/node.h>
#include <nros/subscription.h>

// Generated message bindings
#include "std_msgs.h"

/* ============================================================================
 * Subscription Callback
 * ============================================================================ */

static int message_count = 0;

static void on_message(const uint8_t *data, size_t len, void *context)
{
    (void)context;

    std_msgs_msg_int32 msg;
    std_msgs_msg_int32_init(&msg);

    if (std_msgs_msg_int32_deserialize(&msg, data, len) == 0) {
        message_count++;
        LOG_INF("Received: %d", msg.data);
    } else {
        LOG_ERR("Failed to deserialize message (len=%zu)", len);
    }
}

/* ============================================================================
 * Application
 * ============================================================================ */

int main(void)
{
    LOG_INF("nros Zephyr C Listener");
    LOG_INF("=======================");

    /* Initialize support context */
    nros_support_t support = nros_support_get_zero_initialized();
    NROS_CHECK_RET(nros_support_init(&support, "", CONFIG_NROS_DOMAIN_ID), 1);

    /* Create node */
    nros_node_t node = nros_node_get_zero_initialized();
    NROS_CHECK_RET(nros_node_init(&node, &support, "zephyr_listener", "/"), 1);

    /* Create subscription using generated type support */
    nros_subscription_t sub = nros_subscription_get_zero_initialized();
    NROS_CHECK_RET(nros_subscription_init(
        &sub, &node, std_msgs_msg_int32_get_type_support(), "/chatter",
        on_message, NULL), 1);

    /* Create executor and add subscription */
    nros_executor_t executor = nros_executor_get_zero_initialized();
    NROS_CHECK_RET(nros_executor_init(&executor, &support, 1), 1);
    NROS_CHECK_RET(nros_executor_add_subscription(
        &executor, &sub, NROS_EXECUTOR_ON_NEW_DATA), 1);

    LOG_INF("Waiting for messages...");

    /* Spin forever — executor dispatches callbacks */
    nros_executor_spin(&executor);

    /* Cleanup (unreachable in this example) */
    nros_executor_fini(&executor);
    nros_subscription_fini(&sub);
    nros_node_fini(&node);
    nros_support_fini(&support);

    return 0;
}

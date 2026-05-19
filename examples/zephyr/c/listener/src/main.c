/**
 * @file main.c
 * @brief Zephyr C listener example (Phase 168.4 collapsed shape).
 */

#include <zephyr/kernel.h>
#include <zephyr/logging/log.h>
#include <zephyr/autoconf.h>

#if defined(CONFIG_NROS_RMW_ZENOH)
#include <zpico_zephyr.h>
#elif defined(CONFIG_NROS_RMW_XRCE)
#include <xrce_zephyr.h>
#endif

LOG_MODULE_REGISTER(nros_listener, LOG_LEVEL_INF);

#define NROS_CHECK_LOG(file, line, expr, ret) \
    LOG_ERR("%s:%d %s -> %d", (file), (line), (expr), (int)(ret))

#include <nros/app_main.h>
#include <nros/check.h>
#include <nros/executor.h>
#include <nros/init.h>
#include <nros/log.h>
#include <nros/node.h>
#include <nros/subscription.h>

#include "std_msgs.h"

static nros_logger_t g_logger = NULL;
static int message_count = 0;

static void on_message(const uint8_t *data, size_t len, void *context)
{
    (void)context;
    std_msgs_msg_int32 msg;
    std_msgs_msg_int32_init(&msg);
    if (std_msgs_msg_int32_deserialize(&msg, data, len) == 0) {
        message_count++;
        NROS_LOG_INFO(g_logger, "Received: %d", msg.data);
    } else {
        LOG_ERR("Failed to deserialize message (len=%zu)", len);
    }
}

int nros_app_main(int argc, char **argv)
{
    (void)argc;
    (void)argv;

    LOG_INF("nros Zephyr C Listener");

#if defined(CONFIG_NROS_RMW_ZENOH)
    if (zpico_zephyr_wait_network(CONFIG_NROS_INIT_DELAY_MS) != 0) {
        LOG_ERR("Network not ready"); return 1;
    }
#elif defined(CONFIG_NROS_RMW_XRCE)
    if (xrce_zephyr_wait_network(CONFIG_NROS_INIT_DELAY_MS) != 0) { return 1; }
#endif

    nros_support_t support = nros_support_get_zero_initialized();
#if defined(CONFIG_NROS_RMW_ZENOH)
    NROS_CHECK_RET(nros_support_init(&support, CONFIG_NROS_ZENOH_LOCATOR, CONFIG_NROS_DOMAIN_ID), 1);
#elif defined(CONFIG_NROS_RMW_XRCE)
    NROS_CHECK_RET(nros_support_init_named(&support,
        CONFIG_NROS_XRCE_AGENT_ADDR ":" STRINGIFY(CONFIG_NROS_XRCE_AGENT_PORT),
        CONFIG_NROS_DOMAIN_ID, "xrce_listener"), 1);
#elif defined(CONFIG_NROS_RMW_CYCLONEDDS)
    NROS_CHECK_RET(nros_support_init(&support, "", CONFIG_NROS_DOMAIN_ID), 1);
#else
#error "Phase 168.4 requires CONFIG_NROS_RMW_{ZENOH,XRCE,CYCLONEDDS}=y via prj-<rmw>.conf"
#endif

    nros_node_t node = nros_node_get_zero_initialized();
    NROS_CHECK_RET(nros_node_init(&node, &support, "zephyr_listener", "/"), 1);
    g_logger = nros_node_get_logger(&node);

    nros_subscription_t sub = nros_subscription_get_zero_initialized();
    NROS_CHECK_RET(nros_subscription_init(&sub, &node,
        std_msgs_msg_int32_get_type_support(), "/chatter", on_message, NULL), 1);

    nros_executor_t executor = nros_executor_get_zero_initialized();
    NROS_CHECK_RET(nros_executor_init(&executor, &support, 1), 1);
    NROS_CHECK_RET(nros_executor_register_subscription(&executor, &sub, NROS_EXECUTOR_ON_NEW_DATA), 1);

    LOG_INF("Waiting for messages...");
    nros_executor_spin(&executor);

    nros_executor_fini(&executor);
    nros_subscription_fini(&sub);
    nros_node_fini(&node);
    nros_support_fini(&support);
    return 0;
}

NROS_APP_MAIN_REGISTER_ZEPHYR()

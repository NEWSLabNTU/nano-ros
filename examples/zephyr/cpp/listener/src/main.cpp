/**
 * @file main.cpp
 * @brief Zephyr C++ listener (Phase 168.4 collapsed shape).
 */

#include <zephyr/kernel.h>
#include <zephyr/logging/log.h>
#include <zephyr/autoconf.h>

extern "C" {
#if defined(CONFIG_NROS_RMW_ZENOH)
#include <zpico_zephyr.h>
#elif defined(CONFIG_NROS_RMW_XRCE)
#include <xrce_zephyr.h>
#endif
}

LOG_MODULE_REGISTER(nros_cpp_listener, LOG_LEVEL_INF);

#define NROS_TRY_LOG(file, line, expr, ret) \
    LOG_ERR("%s:%d %s -> %d", (file), (line), (expr), (int)(ret))

#include <nros/app_main.h>
#include <nros/log.hpp>
#include <nros/nros.hpp>

#include "std_msgs.hpp"

static nros_logger_t g_logger = nullptr;

int nros_app_main(int argc, char **argv)
{
    (void)argc; (void)argv;
    LOG_INF("nros Zephyr C++ Listener");

#if defined(CONFIG_NROS_RMW_ZENOH)
    if (zpico_zephyr_wait_network(CONFIG_NROS_INIT_DELAY_MS) != 0) { LOG_ERR("Network not ready"); return 1; }
#elif defined(CONFIG_NROS_RMW_XRCE)
    if (xrce_zephyr_wait_network(CONFIG_NROS_INIT_DELAY_MS) != 0) { return 1; }
#endif

#if defined(CONFIG_NROS_RMW_ZENOH)
    NROS_TRY_RET(nros::init(CONFIG_NROS_ZENOH_LOCATOR, CONFIG_NROS_DOMAIN_ID), 1);
#elif defined(CONFIG_NROS_RMW_XRCE)
    NROS_TRY_RET(nros::init(CONFIG_NROS_XRCE_AGENT_ADDR ":" STRINGIFY(CONFIG_NROS_XRCE_AGENT_PORT),
                            CONFIG_NROS_DOMAIN_ID), 1);
#elif defined(CONFIG_NROS_RMW_CYCLONEDDS)
    NROS_TRY_RET(nros::init("", CONFIG_NROS_DOMAIN_ID), 1);
#else
#error "Phase 168.4 requires CONFIG_NROS_RMW_{ZENOH,XRCE,CYCLONEDDS}=y"
#endif

    nros::Node node;
    NROS_TRY_RET(nros::create_node(node, "zephyr_cpp_listener"), 1);
    g_logger = node.get_logger();

    nros::Subscription<std_msgs::msg::Int32> sub;
    NROS_TRY_RET(node.create_subscription(sub, "/chatter"), 1);

    LOG_INF("Waiting for messages...");
    int message_count = 0;
    while (true) {
        nros::spin_once(100);
        std_msgs::msg::Int32 msg;
        while (sub.try_recv(msg)) {
            message_count++;
            NROS_LOG_INFO(g_logger, "Received: %d", msg.data);
        }
    }
    nros::shutdown();
    return 0;
}

NROS_APP_MAIN_REGISTER_ZEPHYR()

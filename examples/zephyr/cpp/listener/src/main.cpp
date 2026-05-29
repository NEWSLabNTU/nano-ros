/**
 * @file main.cpp
 * @brief Zephyr C++ listener (Phase 168.4 collapsed shape).
 */

#include <zephyr/kernel.h>
#include <zephyr/logging/log.h>
#include <zephyr/autoconf.h>

extern "C" {
#include <nros/platform_zephyr.h>
}

LOG_MODULE_REGISTER(nros_cpp_listener, LOG_LEVEL_INF);

#define NROS_TRY_LOG(file, line, expr, ret)                                                        \
    LOG_ERR("%s:%d %s -> %d", (file), (line), (expr), (int)(ret))

#include <nros/app_main.h>
#include <nros/log.hpp>
#include <nros/nros.hpp>

#include "std_msgs.hpp"

static nros_logger_t g_logger = nullptr;

int nros_app_main(int argc, char** argv) {
    (void)argc;
    (void)argv;
    LOG_INF("nros Zephyr C++ Listener");

    if (nros_platform_zephyr_wait_network(CONFIG_NROS_INIT_DELAY_MS) != 0) {
        LOG_ERR("Network not ready");
        return 1;
    }

#if defined(CONFIG_NROS_RMW_ZENOH)
    NROS_TRY_RET(nros::init(CONFIG_NROS_ZENOH_LOCATOR, CONFIG_NROS_DOMAIN_ID), 1);
#elif defined(CONFIG_NROS_RMW_XRCE)
    /* Pass a distinct session name (Phase 177.9.F). The XRCE client key is
     * `hash_session_key(session_name)`; the 2-arg `nros::init` defaults it to
     * "nros_cpp", so a talker and listener on the same Agent would share a
     * client key, and the Agent resets the shared client when the second
     * connects — dropping this listener's DataReader (0 messages received).
     * Give each process a unique session name (its node name here). */
    NROS_TRY_RET(nros::init(CONFIG_NROS_XRCE_AGENT_ADDR ":" STRINGIFY(CONFIG_NROS_XRCE_AGENT_PORT),
                            CONFIG_NROS_DOMAIN_ID, "zephyr_cpp_listener"),
                 1);
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

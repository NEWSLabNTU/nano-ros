/**
 * @file main.cpp
 * @brief Zephyr C++ listener example using nros-cpp API
 *
 * This example demonstrates subscribing to Int32 messages on Zephyr RTOS
 * using the nros C++ API (nros::init, nros::Node, nros::Subscription<M>).
 * Uses manual-poll with spin_once() + try_recv().
 * The nros module handles zenoh initialization and platform support.
 */

#include <zephyr/kernel.h>
#include <zephyr/logging/log.h>

LOG_MODULE_REGISTER(nros_cpp_listener, LOG_LEVEL_INF);

#define NROS_TRY_LOG(file, line, expr, ret) \
    LOG_ERR("%s:%d %s -> %d", (file), (line), (expr), (int)(ret))

extern "C" {
#include <zpico_zephyr.h>
}

#include <nros/app_main.h>
#include <nros/nros.hpp>

// Generated C++ message bindings
#include "std_msgs.hpp"

/* ============================================================================
 * Application
 * ============================================================================ */

int nros_app_main(int argc, char **argv) {
    (void)argc;
    (void)argv;

    LOG_INF("nros Zephyr C++ Listener");
    LOG_INF("=========================");

    /* Wait for network interface */
    if (zpico_zephyr_wait_network(CONFIG_NROS_INIT_DELAY_MS) != 0) {
        LOG_ERR("Network not ready");
        return 1;
    }

    NROS_TRY_RET(nros::init(CONFIG_NROS_ZENOH_LOCATOR, CONFIG_NROS_DOMAIN_ID), 1);

    nros::Node node;
    NROS_TRY_RET(nros::create_node(node, "zephyr_cpp_listener"), 1);

    nros::Subscription<std_msgs::msg::Int32> sub;
    NROS_TRY_RET(node.create_subscription(sub, "/chatter"), 1);

    /* Alternative: use Stream::wait_next for blocking reception */
    // std_msgs::msg::Int32 msg;
    // sub.stream().wait_next(executor_handle, 1000, msg);

    /* Spin + poll loop */
    LOG_INF("Waiting for messages...");

    int message_count = 0;

    while (true) {
        nros::spin_once(100);

        std_msgs::msg::Int32 msg;
        while (sub.try_recv(msg)) {
            message_count++;
            LOG_INF("Received: %d", msg.data);
        }
    }

    /* Cleanup (unreachable in this example) */
    nros::shutdown();

    return 0;
}

NROS_APP_MAIN_REGISTER_ZEPHYR()

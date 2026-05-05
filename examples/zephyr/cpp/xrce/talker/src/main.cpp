/**
 * @file main.cpp
 * @brief Zephyr C++ talker example using nros-cpp API
 *
 * This example demonstrates publishing Int32 messages on Zephyr RTOS
 * using the nros C++ API (nros::init, nros::Node, nros::Publisher<M>).
 * The nros module handles zenoh initialization and platform support.
 */

#include <zephyr/kernel.h>
#include <zephyr/logging/log.h>

LOG_MODULE_REGISTER(nros_cpp_xrce_talker, LOG_LEVEL_INF);

#define NROS_TRY_LOG(file, line, expr, ret) \
    LOG_ERR("%s:%d %s -> %d", (file), (line), (expr), (int)(ret))

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

    LOG_INF("nros Zephyr C++ Talker");
    LOG_INF("=======================");

    NROS_TRY_RET(nros::init(CONFIG_NROS_XRCE_AGENT_ADDR ":" STRINGIFY(CONFIG_NROS_XRCE_AGENT_PORT),
                            CONFIG_NROS_DOMAIN_ID, "zephyr_cpp_talker"), 1);

    nros::Node node;
    NROS_TRY_RET(nros::create_node(node, "zephyr_cpp_talker"), 1);

    nros::Publisher<std_msgs::msg::Int32> pub;
    NROS_TRY_RET(node.create_publisher(pub, "/chatter"), 1);

    LOG_INF("Publishing messages...");

    int32_t count = 0;
    while (true) {
        count++;
        std_msgs::msg::Int32 msg;
        msg.data = count;
        nros::Result ret = pub.publish(msg);
        if (ret.ok()) LOG_INF("Published: %d", count);
        else LOG_ERR("Publish failed: %d", ret.raw());
        k_sleep(K_SECONDS(1));
    }

    /* Cleanup (unreachable in this example) */
    nros::shutdown();

    return 0;
}

NROS_APP_MAIN_REGISTER_ZEPHYR()

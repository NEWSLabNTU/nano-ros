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

LOG_MODULE_REGISTER(nros_cpp_talker, LOG_LEVEL_INF);

#define NROS_TRY_LOG(file, line, expr, ret) \
    LOG_ERR("%s:%d %s -> %d", (file), (line), (expr), (int)(ret))

extern "C" {
#include <zpico_zephyr.h>
}

#include <nros/app_main.h>
#include <nros/log.hpp>
#include <nros/nros.hpp>

// Generated C++ message bindings
#include "std_msgs.hpp"

/* ============================================================================
 * Application
 * ============================================================================ */

// Phase 88.16.G — set after `nros::create_node`; used by post-init
// diagnostics. nullptr before init = `NROS_LOG_*` silently drops.
static nros_logger_t g_logger = nullptr;

int nros_app_main(int argc, char **argv) {
    (void)argc;
    (void)argv;

    LOG_INF("nros Zephyr C++ Talker");
    LOG_INF("=======================");

    /* Wait for network interface */
    if (zpico_zephyr_wait_network(CONFIG_NROS_INIT_DELAY_MS) != 0) {
        LOG_ERR("Network not ready");
        return 1;
    }

    NROS_TRY_RET(nros::init(CONFIG_NROS_ZENOH_LOCATOR, CONFIG_NROS_DOMAIN_ID), 1);

    nros::Node node;
    NROS_TRY_RET(nros::create_node(node, "zephyr_cpp_talker"), 1);
    g_logger = node.get_logger();

    nros::Publisher<std_msgs::msg::Int32> pub;
    NROS_TRY_RET(node.create_publisher(pub, "/chatter"), 1);

    LOG_INF("Publishing messages...");

    int32_t count = 0;
    while (true) {
        count++;
        std_msgs::msg::Int32 msg;
        msg.data = count;
        nros::Result ret = pub.publish(msg);
        if (ret.ok()) NROS_LOG_INFO(g_logger, "Published: %d", count);
        else NROS_LOG_ERROR(g_logger, "Publish failed: %d", ret.raw());
        k_sleep(K_SECONDS(1));
    }

    /* Cleanup (unreachable in this example) */
    nros::shutdown();

    return 0;
}

NROS_APP_MAIN_REGISTER_ZEPHYR()

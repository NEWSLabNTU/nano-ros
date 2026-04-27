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

#include <nros/nros.hpp>

// Generated C++ message bindings
#include "std_msgs.hpp"

LOG_MODULE_REGISTER(nros_cpp_xrce_talker, LOG_LEVEL_INF);

/* ============================================================================
 * Application
 * ============================================================================ */

int main(void)
{
    LOG_INF("nros Zephyr C++ Talker");
    LOG_INF("=======================");

    /* Initialize nros session */
    nros::Result ret = nros::init(CONFIG_NROS_XRCE_AGENT_ADDR ":" STRINGIFY(CONFIG_NROS_XRCE_AGENT_PORT), CONFIG_NROS_DOMAIN_ID);
    if (!ret.ok()) {
        LOG_ERR("Init failed: %d", ret.raw());
        return 1;
    }

    /* Create node */
    nros::Node node;
    ret = nros::create_node(node, "zephyr_cpp_talker");
    if (!ret.ok()) {
        LOG_ERR("Node creation failed: %d", ret.raw());
        nros::shutdown();
        return 1;
    }

    /* Create publisher */
    nros::Publisher<std_msgs::msg::Int32> pub;
    ret = node.create_publisher(pub, "/chatter");
    if (!ret.ok()) {
        LOG_ERR("Publisher creation failed: %d", ret.raw());
        nros::shutdown();
        return 1;
    }

    /* Publish messages */
    LOG_INF("Publishing messages...");

    int32_t count = 0;

    while (true) {
        count++;

        std_msgs::msg::Int32 msg;
        msg.data = count;

        ret = pub.publish(msg);
        if (ret.ok()) {
            LOG_INF("Published: %d", count);
        } else {
            LOG_ERR("Publish failed: %d", ret.raw());
        }

        k_sleep(K_SECONDS(1));
    }

    /* Cleanup (unreachable in this example) */
    nros::shutdown();

    return 0;
}

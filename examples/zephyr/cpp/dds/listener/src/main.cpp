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

#include <nros/nros.hpp>

// Generated C++ message bindings
#include "std_msgs.hpp"

LOG_MODULE_REGISTER(nros_cpp_dds_listener, LOG_LEVEL_INF);

/* ============================================================================
 * Application
 * ============================================================================ */

int main(void)
{
    LOG_INF("nros Zephyr C++ Listener");
    LOG_INF("=========================");

    /* Initialize nros session */
    nros::Result ret = nros::init("", CONFIG_NROS_DOMAIN_ID);
    if (!ret.ok()) {
        LOG_ERR("Init failed: %d", ret.raw());
        return 1;
    }

    /* Create node */
    nros::Node node;
    ret = nros::create_node(node, "zephyr_cpp_listener");
    if (!ret.ok()) {
        LOG_ERR("Node creation failed: %d", ret.raw());
        nros::shutdown();
        return 1;
    }

    /* Create subscription (manual-poll) */
    nros::Subscription<std_msgs::msg::Int32> sub;
    ret = node.create_subscription(sub, "/chatter");
    if (!ret.ok()) {
        LOG_ERR("Subscription creation failed: %d", ret.raw());
        nros::shutdown();
        return 1;
    }

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
